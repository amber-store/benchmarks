//! Resolving, identifying and pinning the executables under test.
//!
//! Every binary the harness runs is recorded by absolute path, by the version
//! it reports, and by the SHA-256 of the executable file itself, so a report
//! names exactly what was measured. A tool that cannot be found is recorded
//! as missing with the reason; the runner then refuses to pretend the
//! backends that need it simply had nothing to do.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::util::fsx;
use crate::util::proc::Run;

/// One resolved executable.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub path: String,
    /// Whatever the tool prints for its version, first line only.
    pub version: String,
    /// SHA-256 of the executable file.
    pub sha256: String,
    /// For the two Amber cores: the source revision they were built from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    /// Set when the source tree had uncommitted changes at build time.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub source_dirty: bool,
}

/// Where a built executable's source came from, which is what the report
/// records beside it.
#[derive(Debug, Clone, Copy)]
pub enum Source<'a> {
    /// A working checkout: its `HEAD` and dirty state are read with git.
    Checkout(&'a Path),
    /// A revision pinned by the build (the flake's `amber-go-src` input).
    PinnedRevision(&'a str),
    /// Nothing is known about the source.
    Unknown,
}

/// Every tool the harness could and could not find.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Toolbox {
    pub tools: BTreeMap<String, Tool>,
    /// Tool name to the reason it is unavailable.
    pub missing: BTreeMap<String, String>,
}

impl Toolbox {
    /// The tool, or the reason it is not available.
    pub fn get(&self, name: &str) -> Result<&Tool, String> {
        if let Some(t) = self.tools.get(name) {
            return Ok(t);
        }
        Err(self
            .missing
            .get(name)
            .cloned()
            .unwrap_or_else(|| format!("{name}: not resolved")))
    }

    /// Path of a tool, or the reason it is not available.
    pub fn path(&self, name: &str) -> Result<PathBuf, String> {
        self.get(name).map(|t| PathBuf::from(&t.path))
    }

    /// True when the tool is usable.
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Records a resolved tool.
    pub fn insert(&mut self, tool: Tool) {
        self.missing.remove(&tool.name);
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Records a tool that could not be resolved, with the reason.
    pub fn miss(&mut self, name: &str, reason: impl Into<String>) {
        self.missing.insert(name.to_string(), reason.into());
    }

    /// Resolves `name` on `PATH` (or at `explicit`), asks it for its version
    /// with `version_args`, and records the result either way.
    pub fn probe(
        &mut self,
        name: &str,
        explicit: Option<&Path>,
        version_args: &[&str],
    ) -> Option<&Tool> {
        let path = match explicit {
            Some(p) if p.exists() => p.to_path_buf(),
            Some(p) => {
                self.miss(name, format!("{}: no such file", p.display()));
                return None;
            }
            None => match which(name) {
                Some(p) => p,
                None => {
                    self.miss(name, format!("{name}: not found on PATH"));
                    return None;
                }
            },
        };
        self.record(name, &path, version_args, None, false);
        self.tools.get(name)
    }

    /// Records an executable the harness built itself.
    pub fn record_built(&mut self, name: &str, path: &Path, version_args: &[&str], source: Source) {
        if !path.exists() {
            self.miss(name, format!("{}: was not built", path.display()));
            return;
        }
        let (commit, dirty) = match source {
            // A local checkout: its revision *and* whether it had
            // uncommitted changes, because a dirty tree is not the revision
            // it claims to be.
            Source::Checkout(repo) => git_revision(repo),
            // A source revision pinned by the flake: there is no working
            // tree, so it cannot be dirty.
            Source::PinnedRevision(rev) => (Some(rev.to_string()), false),
            Source::Unknown => (None, false),
        };
        self.record(name, path, version_args, commit, dirty);
    }

    fn record(
        &mut self,
        name: &str,
        path: &Path,
        version_args: &[&str],
        source_commit: Option<String>,
        source_dirty: bool,
    ) {
        let sha256 = fsx::sha256_file(path).unwrap_or_else(|e| format!("unavailable: {e}"));
        let version = if version_args.is_empty() {
            String::from("not reported by this executable")
        } else {
            Run::new(path)
                .args(version_args)
                .timeout(Duration::from_secs(60))
                .run()
                .ok()
                .map(|o| {
                    let text = o.stdout_text();
                    let text = if text.is_empty() {
                        String::from_utf8_lossy(&o.stderr).trim_end().to_string()
                    } else {
                        text
                    };
                    text.lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .trim()
                        .to_string()
                })
                .unwrap_or_else(|| "unavailable".to_string())
        };
        self.insert(Tool {
            name: name.to_string(),
            path: path.display().to_string(),
            version,
            sha256,
            source_commit,
            source_dirty,
        });
    }
}

/// Finds `name` on `PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// `HEAD` of a Git checkout, plus whether the working tree is dirty.
pub fn git_revision(repo: &Path) -> (Option<String>, bool) {
    let Some(git) = which("git") else {
        return (None, false);
    };
    let head = Run::new(&git)
        .args(["-C", &repo.display().to_string(), "rev-parse", "HEAD"])
        .timeout(Duration::from_secs(60))
        .run()
        .ok()
        .filter(|o| o.success())
        .map(|o| o.stdout_text());
    let dirty = Run::new(&git)
        .args(["-C", &repo.display().to_string(), "status", "--porcelain"])
        .timeout(Duration::from_secs(120))
        .run()
        .ok()
        .map(|o| !o.stdout_text().is_empty())
        .unwrap_or(false);
    (head, dirty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_a_standard_tool_and_rejects_a_nonexistent_one() {
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-tool-xyzzy").is_none());
    }

    #[test]
    fn a_missing_tool_is_recorded_with_its_reason_not_dropped() {
        let mut tb = Toolbox::default();
        assert!(
            tb.probe("definitely-not-a-real-tool-xyzzy", None, &["--version"])
                .is_none()
        );
        assert!(!tb.has("definitely-not-a-real-tool-xyzzy"));
        let err = tb.get("definitely-not-a-real-tool-xyzzy").unwrap_err();
        assert!(err.contains("not found on PATH"), "{err}");
        let json = serde_json::to_string(&tb).unwrap();
        assert!(json.contains("not found on PATH"), "{json}");
    }

    #[test]
    fn an_explicit_path_that_does_not_exist_is_an_error_not_a_path_lookup() {
        let mut tb = Toolbox::default();
        tb.probe("sh", Some(Path::new("/nonexistent/sh")), &["--version"]);
        let err = tb.get("sh").unwrap_err();
        assert!(err.contains("no such file"), "{err}");
    }

    #[test]
    fn a_resolved_tool_carries_a_version_and_a_hash() {
        let mut tb = Toolbox::default();
        let t = tb.probe("sh", None, &["--version"]).expect("sh").clone();
        assert!(!t.sha256.is_empty() && t.sha256.len() == 64, "{t:?}");
        assert!(!t.version.is_empty());
        assert_eq!(t.sha256, fsx::sha256_file(Path::new(&t.path)).unwrap());
    }

    #[test]
    fn a_pinned_source_revision_is_recorded_without_a_checkout() {
        let mut tb = Toolbox::default();
        let sh = which("sh").expect("sh");
        tb.record_built(
            "amber-go",
            &sh,
            &[],
            Source::PinnedRevision("4ed4660657b12421a534ab0b08cfd717ae3d2291"),
        );
        let t = tb.get("amber-go").expect("recorded");
        assert_eq!(
            t.source_commit.as_deref(),
            Some("4ed4660657b12421a534ab0b08cfd717ae3d2291")
        );
        assert!(
            !t.source_dirty,
            "a pinned source has no working tree to dirty"
        );
        // With nothing known, nothing is claimed.
        let mut tb = Toolbox::default();
        tb.record_built("amber-go", &sh, &[], Source::Unknown);
        assert!(tb.get("amber-go").unwrap().source_commit.is_none());
    }

    #[test]
    fn a_built_binary_that_is_absent_is_reported_as_not_built() {
        let mut tb = Toolbox::default();
        tb.record_built(
            "amber-rust",
            Path::new("/nonexistent/x"),
            &[],
            Source::Unknown,
        );
        assert!(tb.get("amber-rust").unwrap_err().contains("was not built"));
    }
}
