//! Nix, driven through `nix(1)` against *isolated* stores.
//!
//! Each repetition gets its own store directory (`--store /path`), so nothing
//! measured here touches the host's `/nix/store`. Closures are produced by
//! building the pinned fixture flake into the host store once, as setup; the
//! measured operations are the ones a Nix store performs on them: import and
//! copy, closure queries, read-back, garbage collection, and binary-cache
//! export and substitution.
//!
//! This compares *storage layers*. A Nix store additionally enforces
//! references, signatures, immutability and a build model that no
//! filesystem-tree store implements, and nothing here should be read as
//! saying otherwise.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::util::proc::Run;

/// A configured `nix` command line against one store.
#[derive(Debug, Clone)]
pub struct NixCli {
    pub bin: PathBuf,
    /// The store to operate on: a directory for an isolated store, or `None`
    /// for the host store the fixtures are built into.
    pub store: Option<PathBuf>,
    pub timeout: Duration,
    /// What this client remembers between commands; see [`ClientCache`].
    pub cache: ClientCache,
}

/// A Nix client's *other* state: the narinfo cache.
///
/// A store directory is not the whole of what a client knows. Nix also keeps
/// a per-user narinfo cache under `XDG_CACHE_HOME`, and a binary-cache query
/// is answered out of it while the entry is inside its TTL. A brand new
/// store in the same environment is therefore not a new client: it can be
/// told a path is available by a narinfo that was deleted from the cache
/// minutes ago, and then fetch the NAR it names, which is still there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCache {
    /// Whatever the environment provides. Used for commands that act on the
    /// host store, where there is no client freshness question.
    Inherited,
    /// A cache directory of this client's own, with Nix's ordinary caching
    /// inside it. This is what a cold client looks like: it starts knowing
    /// nothing, and it is allowed to remember what it learns — which is what
    /// makes the incremental pull that follows a real incremental pull.
    Private(PathBuf),
    /// A cache directory of its own *and* no narinfo caching at all. For a
    /// probe that has to answer "is this really gone from the cache?" from
    /// the cache itself rather than from anything remembered locally.
    Disabled(PathBuf),
}

impl NixCli {
    /// The bare base command: experimental features, the deadline, and this
    /// client's cache isolation when it has any.
    fn base(&self) -> Run {
        let r = Run::new(&self.bin)
            .args(["--extra-experimental-features", "nix-command flakes"])
            .timeout(self.timeout);
        match &self.cache {
            ClientCache::Inherited => r,
            ClientCache::Private(dir) => r.env("XDG_CACHE_HOME", dir),
            ClientCache::Disabled(dir) => r
                .env("XDG_CACHE_HOME", dir)
                // The empty directory alone is not enough: an entry written
                // during this command would still be reused, so both TTLs
                // are zero as well.
                .args(["--option", "narinfo-cache-positive-ttl", "0"])
                .args(["--option", "narinfo-cache-negative-ttl", "0"]),
        }
    }

    /// The same client, cold: its own cache directory, ordinary caching
    /// inside it. See [`ClientCache::Private`].
    pub fn cold_client(&self, dir: impl AsRef<Path>) -> NixCli {
        NixCli {
            cache: ClientCache::Private(dir.as_ref().to_path_buf()),
            ..self.clone()
        }
    }

    /// The same client with nothing remembered and nothing cacheable. See
    /// [`ClientCache::Disabled`].
    pub fn uncached_client(&self, dir: impl AsRef<Path>) -> NixCli {
        NixCli {
            cache: ClientCache::Disabled(dir.as_ref().to_path_buf()),
            ..self.clone()
        }
    }

    /// The base command with every substituter the host has configured
    /// switched off, so nothing a measurement or a verification needs can
    /// arrive from a cache that is not part of the benchmark.
    fn no_substituters(&self) -> Run {
        self.base()
            .args(["--option", "substituters", ""])
            .args(["--option", "extra-substituters", ""])
            .args(["--option", "trusted-substituters", ""])
            .args(["--option", "builders", ""])
            // A daemon would apply its own substituter list rather than these
            // options, so the store is operated on directly.
            .env_remove("NIX_REMOTE")
            .env_remove("NIX_SUBSTITUTERS")
    }

    /// The base command for the measured, isolated store: substitution off
    /// and the network off, so a measurement can never be served from a
    /// cache.
    pub fn nix(&self) -> Run {
        let mut r = self.no_substituters().arg("--no-net");
        if let Some(store) = &self.store {
            r = r.arg("--store").arg(store);
        }
        r
    }

    /// A query against whichever store the fixture was built into (the host
    /// store), with substitution off: `path-info` must report what is really
    /// there, and must never pull a missing path from a cache.
    pub fn host_query(&self) -> Run {
        self.no_substituters()
    }

    /// The base command with substitution left enabled, for the setup step
    /// that builds the fixtures. This is the *only* command that is allowed
    /// to reach a host-configured cache, and it never measures anything.
    pub fn nix_online(&self) -> Run {
        self.base()
    }

    /// `nix build` of a flake output, printing the resulting store path.
    pub fn build(&self, flake_ref: &str, out_link: &Path) -> Run {
        self.nix_online()
            .args(["build", "--print-out-paths"])
            .arg("--out-link")
            .arg(out_link)
            .arg(flake_ref)
    }

    /// `nix copy` between two stores, both named explicitly.
    ///
    /// Substituters are switched off: a path that is missing from `from` must
    /// make the copy fail, not silently arrive from a cache the host happens
    /// to trust. The network is *not* switched off, because `from` or `to`
    /// may legitimately be an S3 binary cache — but only the one this
    /// benchmark named.
    pub fn copy(&self, from: &str, to: &str, paths: &[String]) -> Run {
        self.no_substituters()
            .args(["copy", "--no-check-sigs"])
            .args(["--from", from])
            .args(["--to", to])
            .args(paths)
    }

    /// `nix hash path`: the NAR hash of a path on disk, computed with no
    /// store involved at all.
    ///
    /// This is what lets the harness check a *tree* store's restored bytes
    /// against the NAR hashes Nix recorded for the source closure, without
    /// either Amber core knowing anything about Nix.
    pub fn hash_path(&self, path: &Path) -> Run {
        self.base()
            .args(["hash", "path", "--type", "sha256", "--sri"])
            .arg(path)
    }

    /// `nix path-info -r --json`: the closure query.
    pub fn path_info_closure(&self, path: &str) -> Run {
        self.nix()
            .args(["path-info", "-r", "--json", "--json-format", "1", path])
    }

    /// `nix path-info -S`: closure size.
    pub fn closure_size(&self, path: &str) -> Run {
        self.nix().args(["path-info", "-S", path])
    }

    /// `nix store verify --all`: re-hash every path in the store and compare
    /// with its registered NAR hash.
    pub fn verify_all(&self) -> Run {
        self.nix().args(["store", "verify", "--all", "--no-trust"])
    }

    /// `nix store gc`: delete everything not reachable from a GC root.
    pub fn gc(&self) -> Run {
        self.nix().args(["store", "gc"])
    }

    /// `nix store delete`: drop one path (the reference-deletion comparison).
    pub fn delete(&self, path: &str) -> Run {
        self.nix().args(["store", "delete", path])
    }

    /// `nix store dump-path`: stream a path's NAR, which reads every byte.
    pub fn dump_path(&self, path: &str, to: &Path) -> Run {
        self.nix().args(["store", "dump-path", path]).stdout_to(to)
    }
}

/// A store URI for an isolated local store directory.
pub fn local_store_uri(dir: &Path) -> String {
    dir.display().to_string()
}

/// Store paths from `nix path-info -r --json`, sorted.
pub fn parse_closure(json: &str) -> Result<Vec<String>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("nix path-info: {e}"))?;
    let mut out: Vec<String> = match &v {
        serde_json::Value::Object(map) => map.keys().cloned().collect(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|e| {
                e.get("path")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            })
            .collect(),
        _ => return Err("nix path-info: unexpected JSON shape".into()),
    };
    out.sort();
    Ok(out)
}

/// NAR hash and references of every path in `nix path-info -r --json` output.
/// This is what a restored closure is verified against.
///
/// Every field this evidence rests on is required. An entry with no
/// `narHash`, or whose `references` is missing or is not a list of strings,
/// is a parse error rather than an empty value: a verification that compared
/// an empty hash with an empty hash would pass while checking nothing.
pub fn parse_closure_details(
    json: &str,
) -> Result<std::collections::BTreeMap<String, (String, Vec<String>)>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("nix path-info: {e}"))?;
    let mut out = std::collections::BTreeMap::new();
    let entries: Vec<(String, &serde_json::Value)> = match &v {
        serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v)).collect(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|e| {
                e.get("path")
                    .and_then(|p| p.as_str())
                    .map(|s| (s.to_string(), e))
                    .ok_or_else(|| "nix path-info: an entry has no \"path\"".to_string())
            })
            .collect::<Result<Vec<_>, String>>()?,
        _ => return Err("nix path-info: unexpected JSON shape".into()),
    };
    for (path, info) in entries {
        let hash = info
            .get("narHash")
            .and_then(|h| h.as_str())
            .filter(|h| !h.trim().is_empty())
            .ok_or_else(|| format!("nix path-info: {path} reports no narHash"))?
            .to_string();
        let refs_value = info
            .get("references")
            .ok_or_else(|| format!("nix path-info: {path} reports no references"))?;
        let refs_array = refs_value
            .as_array()
            .ok_or_else(|| format!("nix path-info: {path} references is not a list"))?;
        let mut refs: Vec<String> = Vec::with_capacity(refs_array.len());
        for r in refs_array {
            let r = r
                .as_str()
                .ok_or_else(|| format!("nix path-info: {path} has a non-string reference"))?;
            refs.push(r.to_string());
        }
        refs.sort();
        out.insert(path, (hash, refs));
    }
    if out.is_empty() {
        return Err("nix path-info: reported no paths at all".into());
    }
    Ok(out)
}

/// The SRI NAR hash `nix hash path` printed, e.g. `sha256-…`.
///
/// Anything that is not a single `<algo>-<digest>` line is refused: an empty
/// or unexpected value must never be compared as if it were a hash.
pub fn parse_hash_path(stdout: &str) -> Result<String, String> {
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| "nix hash path: printed nothing".to_string())?;
    let (algo, digest) = line
        .split_once('-')
        .ok_or_else(|| format!("nix hash path: {line:?} is not an SRI hash"))?;
    if algo != "sha256" || digest.len() < 40 {
        return Err(format!("nix hash path: {line:?} is not a SHA-256 SRI hash"));
    }
    Ok(line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli() -> NixCli {
        NixCli {
            bin: PathBuf::from("/bin/nix"),
            store: Some(PathBuf::from("/scratch/store")),
            timeout: Duration::from_secs(60),
            cache: ClientCache::Inherited,
        }
    }

    #[test]
    fn measured_commands_run_against_the_isolated_store_with_substitution_off() {
        for cmd in [
            cli().gc(),
            cli().verify_all(),
            cli().closure_size("/nix/store/x"),
        ] {
            let d = cmd.display();
            assert!(d.contains("--store /scratch/store"), "{d}");
            assert!(d.contains("--option substituters"), "{d}");
        }
    }

    #[test]
    fn the_setup_build_is_allowed_to_use_the_network() {
        let d = cli()
            .build(".#nix-fixtures-smoke", Path::new("/tmp/result"))
            .display();
        assert!(!d.contains("--no-net"), "{d}");
        assert!(d.contains("--print-out-paths"), "{d}");
    }

    #[test]
    fn a_closure_query_is_parsed_from_the_object_form() {
        let json = r#"{"/nix/store/b-x":{"narHash":"sha256-B","references":["/nix/store/a-y"]},
                       "/nix/store/a-y":{"narHash":"sha256-A","references":[]}}"#;
        assert_eq!(
            parse_closure(json).unwrap(),
            vec!["/nix/store/a-y".to_string(), "/nix/store/b-x".to_string()]
        );
        let details = parse_closure_details(json).unwrap();
        assert_eq!(details["/nix/store/b-x"].0, "sha256-B");
        assert_eq!(
            details["/nix/store/b-x"].1,
            vec!["/nix/store/a-y".to_string()]
        );
        assert!(details["/nix/store/a-y"].1.is_empty());
    }

    #[test]
    fn a_closure_query_is_parsed_from_the_array_form() {
        let json = r#"[{"path":"/nix/store/a-y","narHash":"sha256-A","references":[]}]"#;
        assert_eq!(
            parse_closure(json).unwrap(),
            vec!["/nix/store/a-y".to_string()]
        );
        assert_eq!(
            parse_closure_details(json).unwrap()["/nix/store/a-y"].0,
            "sha256-A"
        );
    }

    #[test]
    fn malformed_closure_output_is_an_error() {
        assert!(parse_closure("nope").is_err());
        assert!(parse_closure("42").is_err());
    }

    #[test]
    fn closure_evidence_with_a_missing_hash_or_references_is_refused() {
        // No narHash at all.
        let err = parse_closure_details(r#"{"/nix/store/a-y":{"references":[]}}"#).unwrap_err();
        assert!(err.contains("narHash"), "{err}");
        // An empty narHash is not a hash.
        let err = parse_closure_details(r#"{"/nix/store/a-y":{"narHash":"","references":[]}}"#)
            .unwrap_err();
        assert!(err.contains("narHash"), "{err}");
        // No references field.
        let err =
            parse_closure_details(r#"{"/nix/store/a-y":{"narHash":"sha256-A"}}"#).unwrap_err();
        assert!(err.contains("references"), "{err}");
        // References that are not a list of store paths.
        let err =
            parse_closure_details(r#"{"/nix/store/a-y":{"narHash":"sha256-A","references":"x"}}"#)
                .unwrap_err();
        assert!(err.contains("not a list"), "{err}");
        let err = parse_closure_details(r#"[{"narHash":"sha256-A","references":[]}]"#).unwrap_err();
        assert!(err.contains("path"), "{err}");
        // An empty closure is evidence of nothing.
        assert!(parse_closure_details("{}").is_err());
    }

    #[test]
    fn a_recomputed_nar_hash_is_parsed_and_a_non_hash_is_refused() {
        assert_eq!(
            parse_hash_path("sha256-XFTOsrkX5a+IDPCF7/Rx/Sth/IOd/i8Gf2IQm69ygKc=\n").unwrap(),
            "sha256-XFTOsrkX5a+IDPCF7/Rx/Sth/IOd/i8Gf2IQm69ygKc="
        );
        assert!(parse_hash_path("").is_err());
        assert!(parse_hash_path("no-hash-here").is_err());
        assert!(parse_hash_path("sha1-0123456789012345678901234567890123456789").is_err());
    }

    #[test]
    fn a_copy_disables_every_host_substituter_but_keeps_the_network() {
        let d = cli()
            .copy("daemon", "/scratch/dest", &["/nix/store/a".into()])
            .display();
        assert!(d.contains("--option substituters"), "{d}");
        assert!(d.contains("--option extra-substituters"), "{d}");
        assert!(d.contains("--no-check-sigs"), "{d}");
        // A copy to or from an S3 binary cache needs the network; what it
        // must not have is a substituter the benchmark did not name.
        assert!(!d.contains("--no-net"), "{d}");
        let dbg = format!("{:?}", cli().copy("daemon", "/scratch/dest", &[]));
        assert!(
            dbg.contains("NIX_REMOTE"),
            "the daemon must not be used: {dbg}"
        );
    }

    #[test]
    fn a_client_carries_its_own_narinfo_cache_or_none_at_all() {
        let host = cli();
        // A command against the host store inherits the environment: there
        // is no client-freshness question to answer there.
        let dbg = format!("{:?}", host.copy("s3://b", "/dest", &[]));
        assert!(!dbg.contains("XDG_CACHE_HOME"), "{dbg}");

        // A cold client starts knowing nothing, and may then remember what
        // it learns — the incremental pull that follows depends on it.
        let cold = host.cold_client("/scratch/nix-cache-client");
        let cmd = cold.copy("s3://b", "/dest", &[]);
        let dbg = format!("{cmd:?}");
        assert!(dbg.contains("XDG_CACHE_HOME"), "{dbg}");
        assert!(dbg.contains("nix-cache-client"), "{dbg}");
        assert!(
            !cmd.display().contains("narinfo-cache"),
            "a cold client is still allowed to cache: {}",
            cmd.display()
        );

        // A probe asking whether something is really gone may remember
        // nothing at all.
        let probe = host.uncached_client("/scratch/nix-cache-probe");
        let cmd = probe.copy("s3://b", "/dest", &[]);
        let d = cmd.display();
        assert!(d.contains("narinfo-cache-positive-ttl 0"), "{d}");
        assert!(d.contains("narinfo-cache-negative-ttl 0"), "{d}");
        assert!(format!("{cmd:?}").contains("nix-cache-probe"));

        // Everything else about the client is unchanged either way.
        assert_eq!(probe.store, host.store);
        assert_eq!(cold.bin, host.bin);
    }

    #[test]
    fn a_host_store_query_still_refuses_to_substitute() {
        let d = cli()
            .host_query()
            .args(["path-info", "/nix/store/a"])
            .display();
        assert!(d.contains("--option substituters"), "{d}");
        assert!(!d.contains("--store"), "{d}");
    }

    #[test]
    fn hashing_a_path_needs_no_store_and_asks_for_the_nar_hash_form() {
        let d = cli().hash_path(Path::new("/restored/abc-x")).display();
        assert!(d.contains("hash path"), "{d}");
        assert!(d.contains("--type sha256"), "{d}");
        assert!(d.contains("--sri"), "{d}");
        assert!(!d.contains("--store"), "{d}");
    }
}
