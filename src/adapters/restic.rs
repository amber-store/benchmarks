//! restic, driven through `restic(1)`.
//!
//! restic is the backup comparison and the native S3 client of the blob
//! group. Its repository password lives in a file inside the run's scratch
//! directory and is passed with `--password-file`, so it never appears in a
//! command line or in a report. Caches are pinned to the run's scratch
//! directory too: a shared `~/.cache/restic` would make one repetition's
//! numbers depend on the previous one's.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::util::proc::Run;

/// A configured `restic` command line.
#[derive(Debug, Clone)]
pub struct ResticCli {
    pub bin: PathBuf,
    /// Repository location: a path, or `s3:ENDPOINT/BUCKET/PREFIX`.
    pub repo: String,
    /// File holding the repository password.
    pub password_file: PathBuf,
    /// Cache directory for this run.
    pub cache_dir: PathBuf,
    pub timeout: Duration,
    /// Credentials for an S3 repository, passed in the environment.
    pub s3_credentials: Option<(String, String)>,
}

impl ResticCli {
    /// The base command.
    pub fn restic(&self) -> Run {
        let mut r = Run::new(&self.bin)
            .args(["-r", &self.repo])
            .arg("--password-file")
            .arg(&self.password_file)
            .arg("--cache-dir")
            .arg(&self.cache_dir)
            .timeout(self.timeout)
            .env("RESTIC_PROGRESS_FPS", "0")
            .env_remove("RESTIC_PASSWORD")
            .env_remove("RESTIC_REPOSITORY");
        if let Some((key, secret)) = &self.s3_credentials {
            r = r
                .env("AWS_ACCESS_KEY_ID", key)
                .env("AWS_SECRET_ACCESS_KEY", secret);
        }
        r
    }

    /// `restic init`.
    pub fn init(&self) -> Run {
        let mut r = Run::new(&self.bin)
            .args(["-r", &self.repo])
            .arg("--password-file")
            .arg(&self.password_file)
            .arg("--cache-dir")
            .arg(&self.cache_dir)
            .arg("init")
            .timeout(self.timeout);
        if let Some((key, secret)) = &self.s3_credentials {
            r = r
                .env("AWS_ACCESS_KEY_ID", key)
                .env("AWS_SECRET_ACCESS_KEY", secret);
        }
        r
    }

    /// `restic backup`, tagged so snapshots can be selected later.
    pub fn backup(&self, path: &Path, tag: &str, jobs: usize) -> Run {
        self.restic()
            .arg("backup")
            .args(["--tag", tag])
            .args(["--read-concurrency", &jobs.to_string()])
            .arg("--no-scan")
            .arg(path)
    }

    /// `restic snapshots --json`.
    pub fn snapshots(&self) -> Run {
        self.restic().args(["snapshots", "--json"])
    }

    /// `restic snapshots --json --tag TAG`: the snapshots carrying exactly
    /// one tag. Every snapshot the harness takes is tagged with the name the
    /// scenario knows it by, so this resolves an id without depending on the
    /// order of an unrelated listing.
    pub fn snapshots_tagged(&self, tag: &str) -> Run {
        self.restic().args(["snapshots", "--json", "--tag", tag])
    }

    /// `restic ls --recursive`: the listing comparison.
    pub fn ls(&self, snapshot: &str) -> Run {
        self.restic().args(["ls", "--recursive", snapshot])
    }

    /// `restic restore` into a target directory.
    pub fn restore(&self, snapshot: &str, target: &Path) -> Run {
        self.restic()
            .args(["restore", snapshot])
            .arg("--target")
            .arg(target)
    }

    /// `restic forget` of one snapshot: the reference-deletion comparison.
    /// Pruning is a separate, separately measured step.
    pub fn forget(&self, snapshot: &str) -> Run {
        self.restic().args(["forget", snapshot])
    }

    /// `restic forget --keep-last N`: retention.
    pub fn forget_keep_last(&self, keep: usize) -> Run {
        self.restic()
            .args(["forget", "--keep-last", &keep.to_string()])
    }

    /// `restic prune`: the garbage-collection comparison.
    pub fn prune(&self) -> Run {
        self.restic().args(["prune", "--max-unused", "0"])
    }

    /// `restic check --read-data`: the integrity check, which re-reads and
    /// re-authenticates every pack rather than only the metadata.
    pub fn check(&self, read_data: bool) -> Run {
        let r = self.restic().arg("check");
        if read_data { r.arg("--read-data") } else { r }
    }

    /// `restic stats --mode raw-data --json`: bytes actually stored.
    pub fn stats_raw(&self) -> Run {
        self.restic()
            .args(["stats", "--mode", "raw-data", "--json"])
    }
}

/// Snapshot ids from `restic snapshots --json`, oldest first.
pub fn parse_snapshot_ids(json: &str) -> Result<Vec<String>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("restic snapshots: {e}"))?;
    let arr = v
        .as_array()
        .ok_or_else(|| "restic snapshots: expected a JSON array".to_string())?;
    Ok(arr
        .iter()
        .filter_map(|s| s.get("id").and_then(|i| i.as_str()).map(|s| s.to_string()))
        .collect())
}

/// Every tag of every snapshot in `restic snapshots --json` output, sorted
/// and deduplicated.
///
/// The harness tags each snapshot with the name the scenario knows it by, so
/// this is the backend's own answer to "which snapshots do you still have?".
pub fn parse_snapshot_tags(json: &str) -> Result<Vec<String>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("restic snapshots: {e}"))?;
    let arr = v
        .as_array()
        .ok_or_else(|| "restic snapshots: expected a JSON array".to_string())?;
    let mut out: Vec<String> = Vec::new();
    for snap in arr {
        let Some(tags) = snap.get("tags") else {
            continue;
        };
        let tags = tags
            .as_array()
            .ok_or_else(|| "restic snapshots: tags is not a list".to_string())?;
        for t in tags {
            if let Some(t) = t.as_str() {
                out.push(t.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// The single snapshot id carrying `tag`.
///
/// Exactly one is required: none means the snapshot is not there, and more
/// than one means the harness's tagging is ambiguous, and either way
/// guessing would be worse than failing.
pub fn resolve_tagged_snapshot(json: &str, tag: &str) -> Result<String, String> {
    let ids = parse_snapshot_ids(json)?;
    match ids.len() {
        1 => Ok(ids[0].clone()),
        0 => Err(format!("restic has no snapshot tagged {tag:?}")),
        n => Err(format!(
            "restic has {n} snapshots tagged {tag:?}; the harness cannot tell \
             which one it meant"
        )),
    }
}

/// `total_size` from `restic stats --json`.
pub fn parse_stats_bytes(json: &str) -> Result<u64, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("restic stats: {e}"))?;
    v.get("total_size")
        .and_then(|s| s.as_u64())
        .ok_or_else(|| "restic stats: no total_size".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli() -> ResticCli {
        ResticCli {
            bin: PathBuf::from("/bin/restic"),
            repo: "s3:http://127.0.0.1:9001/b/run/backup".into(),
            password_file: PathBuf::from("/scratch/pw"),
            cache_dir: PathBuf::from("/scratch/cache"),
            timeout: Duration::from_secs(60),
            s3_credentials: Some(("KEY".into(), "SECRET".into())),
        }
    }

    #[test]
    fn the_password_is_never_on_the_command_line() {
        for cmd in [
            cli().init(),
            cli().backup(Path::new("/data"), "t", 4),
            cli().prune(),
            cli().check(true),
        ] {
            let d = cmd.display();
            assert!(!d.contains("SECRET"), "{d}");
            assert!(d.contains("--password-file /scratch/pw"), "{d}");
        }
    }

    #[test]
    fn credentials_go_in_the_environment_not_argv() {
        let d = cli().backup(Path::new("/data"), "t", 4).display();
        assert!(!d.contains("KEY"), "{d}");
        let dbg = format!("{:?}", cli().backup(Path::new("/data"), "t", 4));
        assert!(dbg.contains("AWS_ACCESS_KEY_ID"), "{dbg}");
    }

    #[test]
    fn the_cache_is_pinned_to_the_run_so_repetitions_do_not_warm_each_other() {
        assert!(
            cli()
                .backup(Path::new("/data"), "t", 4)
                .display()
                .contains("--cache-dir /scratch/cache")
        );
        assert!(
            cli()
                .init()
                .display()
                .contains("--cache-dir /scratch/cache")
        );
    }

    #[test]
    fn the_integrity_check_can_be_asked_to_re_read_the_data() {
        assert!(cli().check(true).display().contains("--read-data"));
        assert!(!cli().check(false).display().contains("--read-data"));
    }

    #[test]
    fn snapshot_ids_are_parsed_in_order() {
        let json = r#"[{"id":"aaa","time":"2026-01-01T00:00:00Z"},{"id":"bbb","time":"2026-01-02T00:00:00Z"}]"#;
        assert_eq!(
            parse_snapshot_ids(json).unwrap(),
            vec!["aaa".to_string(), "bbb".to_string()]
        );
        assert!(parse_snapshot_ids("not json").is_err());
        assert!(parse_snapshot_ids("{}").is_err());
    }

    #[test]
    fn a_snapshot_is_resolved_from_its_own_tag_not_from_listing_order() {
        let d = cli().snapshots_tagged("snap1").display();
        assert!(d.contains("--tag snap1"), "{d}");

        let one = r#"[{"id":"abc","tags":["snap1"]}]"#;
        assert_eq!(resolve_tagged_snapshot(one, "snap1").unwrap(), "abc");

        // No snapshot with that tag is an error, not "latest".
        let err = resolve_tagged_snapshot("[]", "snap1").unwrap_err();
        assert!(err.contains("no snapshot tagged"), "{err}");

        // Two is ambiguous, and guessing would be worse than failing.
        let two = r#"[{"id":"abc","tags":["snap1"]},{"id":"def","tags":["snap1"]}]"#;
        let err = resolve_tagged_snapshot(two, "snap1").unwrap_err();
        assert!(err.contains("2 snapshots tagged"), "{err}");
    }

    #[test]
    fn retained_snapshot_tags_are_read_from_the_repository_itself() {
        let json = r#"[{"id":"a","tags":["snap1","extra"]},{"id":"b","tags":["snap2"]},
                       {"id":"c"}]"#;
        assert_eq!(
            parse_snapshot_tags(json).unwrap(),
            vec![
                "extra".to_string(),
                "snap1".to_string(),
                "snap2".to_string()
            ]
        );
        assert!(parse_snapshot_tags("nope").is_err());
        assert!(parse_snapshot_tags(r#"[{"tags":"x"}]"#).is_err());
        assert!(parse_snapshot_tags("[]").unwrap().is_empty());
    }

    #[test]
    fn raw_data_stats_are_parsed() {
        assert_eq!(
            parse_stats_bytes(r#"{"total_size":12345,"total_file_count":7}"#).unwrap(),
            12345
        );
        assert!(parse_stats_bytes(r#"{"other":1}"#).is_err());
    }
}
