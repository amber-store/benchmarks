//! S3 client plumbing.
//!
//! The harness never signs a request itself: it drives the MinIO client
//! (`mc`), which is a real S3 client with real retry behaviour, and points
//! restic and Nix at their own native S3 support. Credentials are passed in
//! the environment (`MC_HOST_*`, `AWS_*`), never in argv, so no command line
//! recorded in a report can leak one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::util::proc::{Output, Run};

/// Where objects go and how to authenticate.
#[derive(Debug, Clone)]
pub struct Target {
    /// Endpoint the *backends* use. In the local case this is the measuring
    /// gateway, not the object store itself.
    pub endpoint: String,
    /// Endpoint that bypasses the gateway, for the harness's own bookkeeping
    /// calls — listing, cleanup and fault injection must not pollute the
    /// counters of the operation under measurement.
    pub admin_endpoint: String,
    pub region: String,
    pub bucket: String,
    /// Prefix every object of this run lives under. Nothing outside it is
    /// ever listed, overwritten or deleted.
    pub prefix: String,
    pub access_key: String,
    pub secret_key: String,
    /// True for a user-supplied remote endpoint rather than the local
    /// service.
    pub remote: bool,
}

/// The publishable description of a target: everything except the secrets.
#[derive(Debug, Clone, Serialize)]
pub struct TargetInfo {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub prefix: String,
    pub remote: bool,
    /// Stated explicitly so a reader knows the omission is deliberate.
    pub credentials: &'static str,
}

impl Target {
    /// The report-safe view.
    pub fn info(&self) -> TargetInfo {
        TargetInfo {
            endpoint: self.endpoint.clone(),
            region: self.region.clone(),
            bucket: self.bucket.clone(),
            prefix: self.prefix.clone(),
            remote: self.remote,
            credentials: "not recorded: access key and secret are passed to \
                          child processes in the environment and never written \
                          to any output file",
        }
    }

    /// `s3:<endpoint>/<bucket>/<prefix>/<suffix>`, the repository form restic
    /// takes.
    pub fn restic_repo(&self, suffix: &str) -> String {
        format!(
            "s3:{}/{}/{}/{}",
            self.endpoint, self.bucket, self.prefix, suffix
        )
    }

    /// The `s3://` store URI Nix takes: the bucket, then the key prefix this
    /// run owns, then the endpoint, region and scheme as parameters.
    ///
    /// Nix's S3 store carries its key prefix in the path after the bucket
    /// (`s3://bucket/prefix`), not in a URI parameter — there is no `root=`
    /// option, and an unrecognised parameter is only warned about, so
    /// trusting one for isolation would be trusting nothing. The pinned Nix
    /// release does honour the path form: it requests
    /// `<scheme>://<endpoint>/<bucket>/<prefix>/nix-cache-info`. The harness
    /// does not take that on faith either — after the first publication it
    /// lists the keys that actually appeared and fails the run if any of
    /// them lies outside this namespace (the
    /// `published_keys_stay_inside_the_namespace` check).
    ///
    /// The scheme follows the endpoint: a remote HTTPS endpoint stays HTTPS.
    pub fn nix_store_uri(&self, suffix: &str) -> String {
        format!(
            "s3://{}?endpoint={}&region={}&scheme={}",
            self.namespace(suffix),
            self.host_port(),
            self.region,
            self.scheme(),
        )
    }

    /// `bucket/prefix/suffix`: where this run's objects live.
    pub fn namespace(&self, suffix: &str) -> String {
        let suffix = suffix.trim_matches('/');
        if suffix.is_empty() {
            format!("{}/{}", self.bucket, self.prefix)
        } else {
            format!("{}/{}/{}", self.bucket, self.prefix, suffix)
        }
    }

    /// The endpoint without its scheme, which is how Nix and restic want it.
    pub fn host_port(&self) -> String {
        self.endpoint
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string()
    }

    /// `https` for anything but an explicitly plain-HTTP endpoint. The local
    /// service and the gateway in front of it speak HTTP; a remote endpoint
    /// keeps whatever it was given, and an endpoint with no scheme at all is
    /// treated as HTTPS rather than silently downgraded.
    pub fn scheme(&self) -> &'static str {
        if self.endpoint.starts_with("http://") {
            "http"
        } else {
            "https"
        }
    }

    /// An `mc` path under this run's prefix. Refuses to escape the prefix.
    pub fn mc_path(&self, alias: &str, suffix: &str) -> Result<String, String> {
        if suffix.contains("..") {
            return Err(format!("refusing to build an mc path from {suffix:?}"));
        }
        Ok(format!(
            "{alias}/{}/{}/{}",
            self.bucket,
            self.prefix,
            suffix.trim_start_matches('/')
        ))
    }
}

/// Alias name used for every `mc` invocation.
pub const MC_ALIAS: &str = "bench";

/// Builds an `mc` invocation against `target`, with credentials in the
/// environment and a config directory of its own.
pub fn mc(mc_bin: &Path, config_dir: &Path, target: &Target, admin: bool) -> Run {
    let endpoint = if admin {
        &target.admin_endpoint
    } else {
        &target.endpoint
    };
    Run::new(mc_bin)
        .arg("--config-dir")
        .arg(config_dir)
        .arg("--no-color")
        .env(
            format!("MC_HOST_{MC_ALIAS}"),
            format!(
                "{}://{}:{}@{}",
                scheme(endpoint),
                urlencode(&target.access_key),
                urlencode(&target.secret_key),
                endpoint
                    .trim_start_matches("http://")
                    .trim_start_matches("https://")
            ),
        )
        .timeout(Duration::from_secs(1800))
}

fn scheme(endpoint: &str) -> &'static str {
    if endpoint.starts_with("http://") {
        "http"
    } else {
        "https"
    }
}

/// Percent-encodes the characters that would otherwise break a URL userinfo
/// field. Secret keys are base64-ish and can legitimately contain `/` and `+`.
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// One object as `mc ls --json` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEntry {
    pub key: String,
    pub size: u64,
}

/// Parses the newline-delimited JSON `mc ls --recursive --json` emits.
pub fn parse_mc_ls(stdout: &str) -> Result<Vec<ObjectEntry>, String> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("mc ls: {e}: {line}"))?;
        if v.get("status").and_then(|s| s.as_str()) == Some("error") {
            return Err(format!("mc ls: {line}"));
        }
        let Some(key) = v.get("key").and_then(|k| k.as_str()) else {
            continue;
        };
        if key.ends_with('/') {
            continue;
        }
        out.push(ObjectEntry {
            key: key.to_string(),
            size: v.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}

/// Objects currently stored under `prefix/suffix`, via the admin endpoint so
/// the listing is not charged to whatever is being measured.
pub fn list(
    mc_bin: &Path,
    config_dir: &Path,
    target: &Target,
    suffix: &str,
) -> Result<Vec<ObjectEntry>, String> {
    let path = target.mc_path(MC_ALIAS, suffix)?;
    let out: Output = mc(mc_bin, config_dir, target, true)
        .args(["ls", "--recursive", "--json"])
        .arg(&path)
        .run()?;
    if !out.success() {
        // An absent prefix lists as an error; that is an empty store, not a
        // failure.
        let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
        let text = out.stdout_text().to_lowercase();
        if err.contains("does not exist") || text.contains("does not exist") {
            return Ok(Vec::new());
        }
        return Err(out.require_success().unwrap_err());
    }
    parse_mc_ls(&out.stdout_text())
}

/// Total size and count of the objects under `suffix`.
///
/// An error here is returned, never flattened into `(0, 0)`: a failed
/// listing is not an empty store, and reporting one as the other would
/// fabricate a storage measurement.
pub fn usage(
    mc_bin: &Path,
    config_dir: &Path,
    target: &Target,
    suffix: &str,
) -> Result<(u64, u64), String> {
    let objects = list(mc_bin, config_dir, target, suffix)?;
    Ok((objects.iter().map(|o| o.size).sum(), objects.len() as u64))
}

/// Every key in the whole bucket, relative to the bucket root.
///
/// Listing the bucket root removes the ambiguity of a prefixed listing: `mc`
/// reports keys relative to what it was asked for, so only a listing of the
/// root is unambiguously bucket-relative. This is the listing the namespace
/// check needs, because the question it answers is "did anything appear
/// *outside* where we said it would".
pub fn list_bucket(
    mc_bin: &Path,
    config_dir: &Path,
    target: &Target,
) -> Result<Vec<ObjectEntry>, String> {
    let path = format!("{MC_ALIAS}/{}", target.bucket);
    let out: Output = mc(mc_bin, config_dir, target, true)
        .args(["ls", "--recursive", "--json"])
        .arg(&path)
        .run()?;
    if !out.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
        let text = out.stdout_text().to_lowercase();
        if err.contains("does not exist") || text.contains("does not exist") {
            return Ok(Vec::new());
        }
        return Err(out.require_success().unwrap_err());
    }
    parse_mc_ls(&out.stdout_text())
}

/// Keys that appeared between two bucket listings and lie outside
/// `prefix/suffix`.
///
/// Comparing with a listing taken beforehand is what makes this usable
/// against a user-supplied remote bucket that legitimately holds other
/// objects: only what *this* run created is judged.
pub fn keys_written_outside(
    before: &[ObjectEntry],
    after: &[ObjectEntry],
    prefix: &str,
    suffix: &str,
) -> Vec<String> {
    let suffix = suffix.trim_matches('/');
    let inside = if suffix.is_empty() {
        format!("{}/", prefix.trim_matches('/'))
    } else {
        format!("{}/{}/", prefix.trim_matches('/'), suffix)
    };
    let known: std::collections::BTreeSet<&str> = before.iter().map(|o| o.key.as_str()).collect();
    after
        .iter()
        .filter(|o| !known.contains(o.key.as_str()))
        .map(|o| o.key.trim_start_matches('/').to_string())
        .filter(|key| !key.starts_with(&inside))
        .collect()
}

/// Deletes everything under `prefix/suffix`. The path is built through
/// [`Target::mc_path`], which cannot escape the run's own prefix, so a
/// cleanup can never reach an unrelated object.
pub fn remove_tree(
    mc_bin: &Path,
    config_dir: &Path,
    target: &Target,
    suffix: &str,
) -> Result<(), String> {
    if suffix.trim().is_empty() {
        return Err("refusing to remove the whole run prefix without a suffix".into());
    }
    let path = target.mc_path(MC_ALIAS, suffix)?;
    let out = mc(mc_bin, config_dir, target, true)
        .args(["rm", "--recursive", "--force"])
        .arg(&path)
        .run()?;
    if out.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
    if err.contains("does not exist") || err.contains("no such object") {
        return Ok(());
    }
    out.require_success().map(|_| ())
}

/// Where `mc`'s own configuration lives for a run.
pub fn config_dir(scratch: &Path) -> PathBuf {
    scratch.join("mc-config")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            endpoint: "http://127.0.0.1:9001".into(),
            admin_endpoint: "http://127.0.0.1:9000".into(),
            region: "garage".into(),
            bucket: "bench".into(),
            prefix: "run-abc".into(),
            access_key: "GKKEY".into(),
            secret_key: "sec/ret+key".into(),
            remote: false,
        }
    }

    #[test]
    fn the_publishable_view_carries_no_credentials() {
        let json = serde_json::to_string(&target().info()).unwrap();
        assert!(!json.contains("GKKEY"), "{json}");
        assert!(!json.contains("sec/ret+key"), "{json}");
        assert!(json.contains("not recorded"), "{json}");
    }

    #[test]
    fn repository_urls_stay_inside_the_run_prefix() {
        let t = target();
        assert_eq!(
            t.restic_repo("backup"),
            "s3:http://127.0.0.1:9001/bench/run-abc/backup"
        );
        let uri = t.nix_store_uri("cache");
        assert!(uri.starts_with("s3://bench/run-abc/cache?"), "{uri}");
        assert!(uri.contains("endpoint=127.0.0.1:9001"), "{uri}");
        assert!(uri.contains("scheme=http"), "{uri}");
        assert!(
            !uri.contains("root="),
            "there is no root option to rely on: {uri}"
        );
        assert_eq!(
            t.mc_path(MC_ALIAS, "amber/segments").unwrap(),
            "bench/bench/run-abc/amber/segments"
        );
        assert_eq!(t.namespace("cache"), "bench/run-abc/cache");
        assert_eq!(t.namespace(""), "bench/run-abc");
    }

    #[test]
    fn a_remote_endpoint_keeps_https() {
        let mut t = target();
        t.endpoint = "https://s3.example.com".into();
        t.remote = true;
        let uri = t.nix_store_uri("cache");
        assert!(uri.contains("scheme=https"), "{uri}");
        assert!(uri.contains("endpoint=s3.example.com"), "{uri}");
        assert_eq!(t.scheme(), "https");
        // An endpoint given without a scheme is treated as HTTPS rather than
        // silently downgraded to plain HTTP.
        t.endpoint = "s3.example.com".into();
        assert_eq!(t.scheme(), "https");
        assert!(t.nix_store_uri("c").contains("scheme=https"));
        assert_eq!(t.host_port(), "s3.example.com");
    }

    #[test]
    fn a_key_written_outside_the_namespace_is_reported() {
        let before = vec![ObjectEntry {
            key: "someone-elses/data".into(),
            size: 10,
        }];
        let after = vec![
            ObjectEntry {
                key: "someone-elses/data".into(),
                size: 10,
            },
            ObjectEntry {
                key: "run-abc/nix-closure/cache/nix-cache-info".into(),
                size: 20,
            },
            ObjectEntry {
                key: "nix-cache-info".into(),
                size: 20,
            },
            ObjectEntry {
                key: "run-other/cache/x.narinfo".into(),
                size: 30,
            },
        ];
        let escaped = keys_written_outside(&before, &after, "run-abc", "nix-closure/cache");
        assert_eq!(
            escaped,
            vec![
                "nix-cache-info".to_string(),
                "run-other/cache/x.narinfo".to_string()
            ],
            "a key at the bucket root, or under another prefix, is an escape"
        );
        // Pre-existing objects of a user-supplied bucket are not this run's
        // doing and are not reported.
        assert!(
            !escaped.contains(&"someone-elses/data".to_string()),
            "{escaped:?}"
        );
        assert!(keys_written_outside(&after, &after, "run-abc", "x").is_empty());
    }

    #[test]
    fn a_traversing_suffix_is_refused() {
        assert!(target().mc_path(MC_ALIAS, "../../other").is_err());
    }

    #[test]
    fn credentials_are_percent_encoded_for_the_mc_host_variable() {
        assert_eq!(urlencode("sec/ret+key"), "sec%2Fret%2Bkey");
        assert_eq!(urlencode("plain-key_1.0~"), "plain-key_1.0~");
    }

    #[test]
    fn mc_ls_json_is_parsed_and_directories_are_skipped() {
        let out = r#"
{"status":"success","type":"folder","key":"run-abc/amber/","size":0}
{"status":"success","type":"file","key":"run-abc/amber/0001.seg","size":1024}
{"status":"success","type":"file","key":"run-abc/amber/0000.seg","size":2048}
"#;
        let objects = parse_mc_ls(out).unwrap();
        assert_eq!(
            objects,
            vec![
                ObjectEntry {
                    key: "run-abc/amber/0000.seg".into(),
                    size: 2048
                },
                ObjectEntry {
                    key: "run-abc/amber/0001.seg".into(),
                    size: 1024
                },
            ]
        );
    }

    #[test]
    fn an_error_line_from_mc_is_an_error_not_an_empty_listing() {
        let out = r#"{"status":"error","error":{"message":"Access Denied"}}"#;
        assert!(parse_mc_ls(out).is_err());
    }

    #[test]
    fn removing_the_whole_prefix_without_a_suffix_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let err = remove_tree(
            std::path::Path::new("/nonexistent/mc"),
            d.path(),
            &target(),
            "  ",
        )
        .unwrap_err();
        assert!(err.contains("without a suffix"), "{err}");
    }
}
