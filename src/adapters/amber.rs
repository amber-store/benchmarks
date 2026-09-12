//! Both Amber cores, driven through the `amber-store` command line.
//!
//! The Rust core's dev CLI (`cargo run --example amber-store`) deliberately
//! mirrors the Go core's `cmd/amber-store` flag for flag, so a single builder
//! here can drive either one and the only difference between the two rows of
//! a comparison is the implementation. Chunking and segment size are passed
//! explicitly on every invocation rather than left to defaults.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::ChunkSettings;
use crate::util::proc::{Output, Run, Usage};

/// A configured `amber-store` command line.
#[derive(Debug, Clone)]
pub struct AmberCli {
    /// The executable: the Rust example binary or the Go binary.
    pub bin: PathBuf,
    /// Store directory (`<dir>/packstore`, `<dir>/refs`).
    pub store: PathBuf,
    pub segment_size: u64,
    pub chunk: ChunkSettings,
    pub jobs: usize,
    pub timeout: Duration,
}

/// One entry of a directory listing, as `ls --keys` prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LsEntry {
    /// `ls -l` style mode string, e.g. `drwxr-xr-x`.
    pub mode: String,
    pub name: String,
    /// Content key, absent for entries that have none (symlinks, devices).
    pub key: Option<String>,
    pub is_dir: bool,
}

impl AmberCli {
    /// A command with the global flags already set.
    fn base(&self) -> Run {
        Run::new(&self.bin)
            .arg("--store")
            .arg(&self.store)
            .arg("--segment-size")
            .arg(self.segment_size.to_string())
            .timeout(self.timeout)
    }

    /// `ingest`, with every chunking parameter stated explicitly and the
    /// progress UI off, recording the root under `reference`.
    pub fn ingest(&self, path: &Path, reference: Option<&str>) -> Run {
        let mut r = self
            .base()
            .arg("ingest")
            .arg("--no-progress")
            .args(["--min", &self.chunk.min.to_string()])
            .args(["--avg", &self.chunk.avg.to_string()])
            .args(["--max", &self.chunk.max.to_string()])
            .args(["--item-bits", &self.chunk.item_bits.to_string()])
            .args([
                "--xattr-inline-max",
                &self.chunk.xattr_inline_max.to_string(),
            ])
            .args(["--jobs", &self.jobs.to_string()]);
        if let Some(name) = reference {
            r = r.args(["--ref", name]);
        }
        r.arg(path)
    }

    /// `ls` for one directory object or reference.
    pub fn ls(&self, spec: &str, keys: bool) -> Run {
        let mut r = self.base().arg("ls");
        if keys {
            r = r.arg("--keys");
        }
        r.arg(spec)
    }

    /// `export`: the whole tree as a PAX tar on standard output, redirected
    /// to `to`. Reading it end to end also
    /// re-reads and CRC-checks every stored record, which is what the
    /// integrity check uses it for.
    pub fn export_stdout(&self, spec: &str, to: &Path) -> Run {
        self.base().arg("export").arg(spec).stdout_to(to)
    }

    /// `export -o FILE`: the same tar, written to a file.
    pub fn export_to_file(&self, spec: &str, to: &Path) -> Run {
        self.base().arg("export").arg("-o").arg(to).arg(spec)
    }

    /// `restore` into a directory.
    pub fn restore(&self, spec: &str, dir: &Path) -> Run {
        self.base().arg("restore").arg(spec).arg(dir)
    }

    /// `ref list`.
    pub fn ref_list(&self) -> Run {
        self.base().args(["ref", "list"])
    }

    /// `ref get`.
    pub fn ref_get(&self, name: &str) -> Run {
        self.base().args(["ref", "get", name])
    }

    /// `ref rm`.
    pub fn ref_rm(&self, name: &str) -> Run {
        self.base().args(["ref", "rm", name])
    }

    /// `gc status`.
    pub fn gc_status(&self) -> Run {
        self.base().args(["gc", "status"])
    }

    /// `gc run`, forced to reap everything reapable: the selection line is
    /// dropped to zero and the grace period removed, because a benchmark that
    /// waits an hour for a pack to age measures nothing.
    pub fn gc_run(&self) -> Run {
        self.base()
            .args(["gc", "run", "--garbage", "0", "--grace", "0s"])
    }

    /// Backdates every pack segment file by `by`, so a collection can
    /// actually reap the segments a dropped reference made dead.
    ///
    /// Both cores gate reaping on a sealed segment's **file mtime** against
    /// `now - grace`, and both treat a grace of zero as "use the default",
    /// which is one hour — there is no command-line value that disables it.
    /// A benchmark that did not age the segments would therefore measure
    /// every collection reclaiming nothing, for both cores, however much
    /// garbage it had created: a vacuous number that looks like a result.
    ///
    /// Every other backend in the suite prunes immediately (`git gc
    /// --prune=now`, `restic prune --max-unused 0`, `nix store gc`), so
    /// satisfying the grace period makes the comparison more equal rather
    /// than less. It is done identically for both cores, outside every
    /// measured window, and recorded in the report as a setup step.
    pub fn age_sealed_segments(&self, by: Duration) -> Result<usize, String> {
        let dir = self.store.join("packstore");
        if !dir.exists() {
            return Ok(0);
        }
        let when = std::time::SystemTime::now()
            .checked_sub(by)
            .ok_or_else(|| "cannot age segments that far back".to_string())?;
        let secs = when
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs() as i64;
        let mut aged = 0;
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if std::fs::symlink_metadata(&path)
                .map(|m| m.is_file())
                .unwrap_or(false)
            {
                crate::util::fsx::set_mtime(&path, secs, 0)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                aged += 1;
            }
        }
        Ok(aged)
    }

    /// Walks the whole tree by repeatedly calling `ls --keys`, one child
    /// process per directory.
    ///
    /// Neither core's CLI has a recursive listing, so this is the only way to
    /// enumerate a stored tree through the shipped interface. It is
    /// *expensive in a way the store is not responsible for* — one process
    /// spawn per directory — so the caller records the invocation count and
    /// the report says what the number includes.
    pub fn list_recursive(&self, spec: &str) -> Result<RecursiveListing, String> {
        let mut listing = RecursiveListing::default();
        let mut queue = vec![spec.to_string()];
        while let Some(next) = queue.pop() {
            let out: Output = self.ls(&next, true).run()?;
            listing.invocations += 1;
            listing.usage = listing.usage.add(out.usage);
            let out = out.require_success()?;
            for entry in parse_ls(&out.stdout_text()) {
                if entry.is_dir {
                    listing.directories += 1;
                    match &entry.key {
                        Some(k) => queue.push(k.clone()),
                        None => {
                            return Err(format!(
                                "ls --keys printed no content key for directory {}",
                                entry.name
                            ));
                        }
                    }
                } else {
                    listing.entries += 1;
                }
            }
        }
        Ok(listing)
    }
}

/// The result of a full recursive walk.
#[derive(Debug, Clone, Default)]
pub struct RecursiveListing {
    /// Non-directory entries seen.
    pub entries: u64,
    /// Directories descended into.
    pub directories: u64,
    /// Child processes spawned.
    pub invocations: u64,
    /// Summed usage of every child.
    pub usage: Usage,
}

/// Parses `ls --keys` output.
///
/// The format is `MODE UID GID SIZE MON DAY TIME NAME [-> TARGET] [KEY]`;
/// the timestamp is always three whitespace-separated tokens in both cores.
pub fn parse_ls(text: &str) -> Vec<LsEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 8 {
            continue;
        }
        let mode = tokens[0].to_string();
        let name = tokens[7].to_string();
        let last = tokens[tokens.len() - 1];
        let key = if tokens.len() > 8 && is_key(last) {
            Some(last.to_string())
        } else {
            None
        };
        out.push(LsEntry {
            is_dir: mode.starts_with('d'),
            mode,
            name,
            key,
        });
    }
    out
}

fn is_key(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The one line `ref list` prints per reference: `NAME KEY CREATED CREATOR`.
pub fn parse_ref_list(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli() -> AmberCli {
        AmberCli {
            bin: PathBuf::from("/bin/amber-store"),
            store: PathBuf::from("/scratch/store"),
            segment_size: 8 * 1024 * 1024,
            chunk: ChunkSettings::shared(),
            jobs: 4,
            timeout: Duration::from_secs(60),
        }
    }

    #[test]
    fn ingest_states_every_chunking_parameter_explicitly() {
        let cmd = cli().ingest(Path::new("/data/gen0"), Some("v0")).display();
        for expected in [
            "--segment-size 8388608",
            "--min 32768",
            "--avg 524288",
            "--max 1048576",
            "--item-bits 7",
            "--xattr-inline-max 256",
            "--jobs 4",
            "--ref v0",
            "--no-progress",
        ] {
            assert!(cmd.contains(expected), "{expected} missing from {cmd}");
        }
    }

    #[test]
    fn gc_is_forced_so_it_actually_reaps() {
        let cmd = cli().gc_run().display();
        assert!(cmd.contains("--garbage 0"), "{cmd}");
        assert!(cmd.contains("--grace 0s"), "{cmd}");
    }

    #[test]
    fn ls_output_is_parsed_with_keys_dirs_and_symlinks() {
        let text = "\
drwxr-xr-x 992 989    117 Sep 12 15:22 sub 2075bb3429d1172bec103652c0513d03555f929da8f051002d1b06d7d41a2943
-rw-r--r-- 992 989 300000 Sep 12 15:22 a.bin 120493e096489e48d6402077e8293d2635c547e95b9a0ee2427a9dfc7ea35484
lrwxrwxrwx 992 989      6 Jan  1  2023 link -> a.bin
-rw-r--r-- 992 989      5 Jan  1  2023 plain";
        let entries = parse_ls(text);
        assert_eq!(entries.len(), 4);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "sub");
        assert!(entries[0].key.is_some());
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].name, "a.bin");
        assert_eq!(entries[2].name, "link");
        assert_eq!(entries[2].key, None, "a symlink has no content key");
        assert_eq!(entries[3].name, "plain");
        assert_eq!(entries[3].key, None);
    }

    #[test]
    fn a_token_that_only_looks_like_a_key_is_not_one() {
        assert!(is_key(&"a".repeat(64)));
        assert!(!is_key(&"z".repeat(64)));
        assert!(!is_key(&"a".repeat(63)));
    }

    #[test]
    fn ref_list_names_are_extracted() {
        let text =
            "v0 2075bb34 2026-09-12T15:22:00Z bench\nv1 120493e0 2026-09-12T15:23:00Z bench\n";
        assert_eq!(
            parse_ref_list(text),
            vec!["v0".to_string(), "v1".to_string()]
        );
        assert!(parse_ref_list("").is_empty());
    }

    #[test]
    fn the_store_directory_is_passed_to_every_subcommand() {
        let c = cli();
        for cmd in [
            c.ls("ref:v0", false),
            c.export_stdout("ref:v0", Path::new("/dev/null")),
            c.restore("ref:v0", Path::new("/tmp/out")),
            c.ref_list(),
            c.ref_rm("v0"),
            c.gc_status(),
            c.gc_run(),
        ] {
            assert!(
                cmd.display().contains("--store /scratch/store"),
                "{}",
                cmd.display()
            );
        }
    }
}
