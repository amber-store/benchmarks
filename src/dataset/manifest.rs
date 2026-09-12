//! Independent manifests.
//!
//! A manifest is produced by walking a directory tree and hashing what is
//! actually on disk — never from the generator's intentions. The same routine
//! produces the manifest of the generated corpus and the manifest of whatever
//! a backend restored, so a comparison is a comparison of two independent
//! observations. Any missing, extra or differing entry fails the run.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::metrics::Verification;
use crate::util::fsx;

/// What a manifest entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Dir,
    File,
    Symlink,
}

/// One observed filesystem object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Slash-separated path relative to the tree root.
    pub path: String,
    pub kind: Kind,
    /// Permission bits (`st_mode & 0o7777`).
    pub mode: u32,
    /// File size in bytes; for a symlink, the length of its target.
    pub size: u64,
    /// Modification time, whole seconds since the Unix epoch.
    pub mtime_unix_secs: i64,
    /// SHA-256 of the file's bytes. Files only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Symlink target. Symlinks only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
}

/// The observed contents of a tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// A label naming what was observed, e.g. `corpus/gen0`.
    pub label: String,
    /// Entries sorted by path.
    pub entries: Vec<Entry>,
    pub file_count: u64,
    pub dir_count: u64,
    pub symlink_count: u64,
    /// Sum of every file's size: the corpus's logical byte count.
    pub total_file_bytes: u64,
}

/// How closely permission bits are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeCheck {
    /// All twelve permission bits.
    Full,
    /// Only whether the file is executable. Git records exactly two file
    /// modes, `100644` and `100755`, so nothing finer can survive it.
    ExecBit,
    /// Not compared.
    Ignore,
}

/// Which attributes a comparison insists on.
///
/// Not every backend stores every attribute, and holding one to another's
/// contract would either flatter it or libel it. The scenario picks the
/// strictness, the report records it, and any attribute *not* compared is
/// listed as a recorded limitation rather than quietly dropped.
#[derive(Debug, Clone, Copy)]
pub struct Strictness {
    pub mode: ModeCheck,
    /// Compare whole-second modification times.
    pub mtime: bool,
    /// Compare symlink targets.
    pub symlinks: bool,
    /// Require directories that contain nothing to come back. Git has no
    /// representation for them at all.
    pub empty_dirs: bool,
}

impl Strictness {
    /// Everything: bytes, all permission bits, mtimes, symlink targets and
    /// empty directories. What both Amber cores, restic, Nix and the
    /// SHA-256 baseline are held to.
    pub fn full() -> Strictness {
        Strictness {
            mode: ModeCheck::Full,
            mtime: true,
            symlinks: true,
            empty_dirs: true,
        }
    }

    /// What Git can actually round-trip: bytes, the executable bit and
    /// symlink targets. Git stores no modification times, no permission bits
    /// beyond `x`, and no empty directories.
    pub fn git() -> Strictness {
        Strictness {
            mode: ModeCheck::ExecBit,
            mtime: false,
            symlinks: true,
            empty_dirs: false,
        }
    }

    /// The attributes this strictness does *not* check, for the report.
    pub fn omissions(&self) -> Vec<String> {
        let mut out = Vec::new();
        match self.mode {
            ModeCheck::Full => {}
            ModeCheck::ExecBit => out.push("permission bits other than the executable bit".into()),
            ModeCheck::Ignore => out.push("permission bits".into()),
        }
        if !self.mtime {
            out.push("modification times".into());
        }
        if !self.symlinks {
            out.push("symlink targets".into());
        }
        if !self.empty_dirs {
            out.push("empty directories".into());
        }
        out
    }
}

/// What a comparison found.
#[derive(Debug, Clone, Default)]
pub struct Diff {
    /// In the reference manifest, absent from the observed tree.
    pub missing: Vec<String>,
    /// In the observed tree, absent from the reference manifest.
    pub extra: Vec<String>,
    /// Present in both but different; each string names the path and the
    /// attribute that disagreed.
    pub corrupt: Vec<String>,
}

impl Diff {
    /// True when nothing disagreed.
    pub fn clean(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty() && self.corrupt.is_empty()
    }
}

impl Manifest {
    /// Walks `root` and records what is there. Symlinks are never followed.
    pub fn observe(label: &str, root: &Path) -> io::Result<Manifest> {
        let mut entries = Vec::new();
        let mut m = Manifest {
            label: label.to_string(),
            entries: Vec::new(),
            file_count: 0,
            dir_count: 0,
            symlink_count: 0,
            total_file_bytes: 0,
        };
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let mut children: Vec<_> = fs::read_dir(&dir)?.collect::<Result<_, _>>()?;
            children.sort_by_key(|e| e.file_name());
            for child in children {
                let path = child.path();
                let rel = path
                    .strip_prefix(root)
                    .map_err(io::Error::other)?
                    .to_string_lossy()
                    .into_owned();
                let md = fs::symlink_metadata(&path)?;
                let ft = md.file_type();
                let (kind, size, sha256, link_target) = if ft.is_dir() {
                    stack.push(path.clone());
                    m.dir_count += 1;
                    (Kind::Dir, 0, None, None)
                } else if ft.is_symlink() {
                    let t = fs::read_link(&path)?.to_string_lossy().into_owned();
                    m.symlink_count += 1;
                    (Kind::Symlink, t.len() as u64, None, Some(t))
                } else {
                    m.file_count += 1;
                    m.total_file_bytes += md.len();
                    (Kind::File, md.len(), Some(fsx::sha256_file(&path)?), None)
                };
                entries.push(Entry {
                    path: rel,
                    kind,
                    mode: md.mode() & 0o7777,
                    size,
                    mtime_unix_secs: md.mtime(),
                    sha256,
                    link_target,
                });
            }
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        m.entries = entries;
        Ok(m)
    }

    /// Like [`Manifest::observe`] but skipping any path whose first
    /// component is in `exclude`. Used to describe a Git working tree
    /// without its `.git` directory.
    pub fn observe_excluding(label: &str, root: &Path, exclude: &[&str]) -> io::Result<Manifest> {
        let mut m = Manifest::observe(label, root)?;
        m.entries.retain(|e| {
            let first = e.path.split('/').next().unwrap_or("");
            !exclude.contains(&first)
        });
        m.file_count = 0;
        m.dir_count = 0;
        m.symlink_count = 0;
        m.total_file_bytes = 0;
        for e in &m.entries {
            match e.kind {
                Kind::Dir => m.dir_count += 1,
                Kind::Symlink => m.symlink_count += 1,
                Kind::File => {
                    m.file_count += 1;
                    m.total_file_bytes += e.size;
                }
            }
        }
        Ok(m)
    }

    /// Writes the manifest as pretty JSON.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fsx::write_file(path, &bytes)
    }

    /// Reads a manifest written by [`Manifest::save`].
    pub fn load(path: &Path) -> io::Result<Manifest> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }

    /// A stable digest of the manifest's content, used to assert that two
    /// independent generations produced the same corpus.
    pub fn digest(&self) -> String {
        let mut h = blake3::Hasher::new();
        for e in &self.entries {
            h.update(e.path.as_bytes());
            h.update(&[0]);
            h.update(format!("{:?}", e.kind).as_bytes());
            h.update(&e.mode.to_le_bytes());
            h.update(&e.size.to_le_bytes());
            h.update(&e.mtime_unix_secs.to_le_bytes());
            h.update(e.sha256.as_deref().unwrap_or("").as_bytes());
            h.update(e.link_target.as_deref().unwrap_or("").as_bytes());
        }
        h.finalize().to_hex().to_string()
    }

    /// Paths of directories in this manifest that contain nothing.
    fn empty_dirs(&self) -> std::collections::BTreeSet<&str> {
        let mut parents = std::collections::BTreeSet::new();
        for e in &self.entries {
            let mut p = e.path.as_str();
            while let Some(i) = p.rfind('/') {
                p = &p[..i];
                parents.insert(p);
            }
        }
        self.entries
            .iter()
            .filter(|e| e.kind == Kind::Dir && !parents.contains(e.path.as_str()))
            .map(|e| e.path.as_str())
            .collect()
    }

    /// Compares `self` (the reference) with `observed`.
    pub fn compare(&self, observed: &Manifest, strict: Strictness) -> Diff {
        let skip: std::collections::BTreeSet<&str> = if strict.empty_dirs {
            std::collections::BTreeSet::new()
        } else {
            self.empty_dirs()
        };
        let mine: BTreeMap<&str, &Entry> = self
            .entries
            .iter()
            .filter(|e| !skip.contains(e.path.as_str()))
            .map(|e| (e.path.as_str(), e))
            .collect();
        let theirs: BTreeMap<&str, &Entry> = observed
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e))
            .collect();
        let mut diff = Diff::default();
        for (path, want) in &mine {
            let Some(got) = theirs.get(path) else {
                diff.missing.push((*path).to_string());
                continue;
            };
            if want.kind != got.kind {
                diff.corrupt
                    .push(format!("{path}: kind {:?} != {:?}", want.kind, got.kind));
                continue;
            }
            if want.kind == Kind::File {
                if want.size != got.size {
                    diff.corrupt
                        .push(format!("{path}: size {} != {}", want.size, got.size));
                }
                if want.sha256 != got.sha256 {
                    diff.corrupt.push(format!(
                        "{path}: sha256 {} != {}",
                        want.sha256.as_deref().unwrap_or("-"),
                        got.sha256.as_deref().unwrap_or("-")
                    ));
                }
            }
            if strict.symlinks && want.kind == Kind::Symlink && want.link_target != got.link_target
            {
                diff.corrupt.push(format!(
                    "{path}: link target {:?} != {:?}",
                    want.link_target, got.link_target
                ));
            }
            // A symlink's own mode is not meaningful on Linux.
            if want.kind != Kind::Symlink {
                match strict.mode {
                    ModeCheck::Full if want.mode != got.mode => diff
                        .corrupt
                        .push(format!("{path}: mode {:o} != {:o}", want.mode, got.mode)),
                    ModeCheck::ExecBit if (want.mode & 0o111 != 0) != (got.mode & 0o111 != 0) => {
                        diff.corrupt.push(format!(
                            "{path}: executable bit {:o} != {:o}",
                            want.mode & 0o111,
                            got.mode & 0o111
                        ))
                    }
                    _ => {}
                }
            }
            if strict.mtime && want.mtime_unix_secs != got.mtime_unix_secs {
                diff.corrupt.push(format!(
                    "{path}: mtime {} != {}",
                    want.mtime_unix_secs, got.mtime_unix_secs
                ));
            }
        }
        for (path, got) in &theirs {
            if mine.contains_key(path) {
                continue;
            }
            // A directory this strictness does not require is allowed to come
            // back anyway; anything else is an extra entry.
            if !strict.empty_dirs && got.kind == Kind::Dir && skip.contains(path) {
                continue;
            }
            diff.extra.push((*path).to_string());
        }
        diff
    }

    /// Compares against a tree on disk and turns the result into a reportable
    /// [`Verification`]. A tree that cannot even be walked fails the check
    /// rather than skipping it.
    pub fn verify_tree(&self, name: &str, root: &Path, strict: Strictness) -> Verification {
        let observed = match Manifest::observe("observed", root) {
            Ok(o) => o,
            Err(e) => {
                return Verification::fail(name, format!("cannot read {}: {e}", short_path(root)));
            }
        };
        let diff = self.compare(&observed, strict);
        let mode_label = format!("{:?}", strict.mode);
        let detail = format!(
            "{} entries expected under {}; {} missing, {} extra, {} differing (mode={}, mtime={}, symlinks={})",
            self.entries.len(),
            short_path(root),
            diff.missing.len(),
            diff.extra.len(),
            diff.corrupt.len(),
            mode_label,
            strict.mtime,
            strict.symlinks,
        );
        let mut v = if diff.clean() {
            Verification::pass(name, detail)
        } else {
            Verification::fail(name, detail)
        };
        v.missing = truncate(diff.missing);
        v.extra = truncate(diff.extra);
        v.corrupt = truncate(diff.corrupt);
        v
    }
}

/// The tail of a path, so a report names the destination without
/// embedding the absolute layout of whoever ran it.
fn short_path(p: &Path) -> String {
    let parts: Vec<String> = p
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

/// Keeps a report readable when a whole tree is wrong.
fn truncate(mut v: Vec<String>) -> Vec<String> {
    const MAX: usize = 25;
    if v.len() > MAX {
        let rest = v.len() - MAX;
        v.truncate(MAX);
        v.push(format!("... and {rest} more"));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn sample(root: &Path) {
        fs::create_dir_all(root.join("d")).unwrap();
        fs::write(root.join("d/a.txt"), b"hello").unwrap();
        fs::write(root.join("d/b.bin"), vec![1u8; 100]).unwrap();
        fs::set_permissions(root.join("d/b.bin"), fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("a.txt", root.join("d/l")).unwrap();
        for p in ["d/a.txt", "d/b.bin", "d/l"] {
            fsx::set_mtime(&root.join(p), 1_700_000_000, 0).unwrap();
        }
        fsx::set_mtime(&root.join("d"), 1_700_000_000, 0).unwrap();
    }

    #[test]
    fn observe_records_kinds_sizes_modes_and_targets() {
        let d = tempfile::tempdir().unwrap();
        sample(d.path());
        let m = Manifest::observe("t", d.path()).unwrap();
        assert_eq!(m.file_count, 2);
        assert_eq!(m.dir_count, 1);
        assert_eq!(m.symlink_count, 1);
        assert_eq!(m.total_file_bytes, 105);
        let paths: Vec<&str> = m.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["d", "d/a.txt", "d/b.bin", "d/l"]);
        let bin = m.entries.iter().find(|e| e.path == "d/b.bin").unwrap();
        assert_eq!(bin.mode, 0o755);
        let link = m.entries.iter().find(|e| e.path == "d/l").unwrap();
        assert_eq!(link.link_target.as_deref(), Some("a.txt"));
    }

    #[test]
    fn identical_trees_compare_clean_and_share_a_digest() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        let ma = Manifest::observe("a", a.path()).unwrap();
        let mb = Manifest::observe("b", b.path()).unwrap();
        assert!(ma.compare(&mb, Strictness::full()).clean());
        assert_eq!(ma.digest(), mb.digest());
    }

    #[test]
    fn a_corrupted_byte_fails_verification() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        fs::write(b.path().join("d/a.txt"), b"hellO").unwrap();
        fsx::set_mtime(&b.path().join("d/a.txt"), 1_700_000_000, 0).unwrap();
        let v = Manifest::observe("a", a.path()).unwrap().verify_tree(
            "restore",
            b.path(),
            Strictness::full(),
        );
        assert!(!v.passed);
        assert_eq!(v.corrupt.len(), 1);
        assert!(v.corrupt[0].contains("sha256"), "{:?}", v.corrupt);
    }

    #[test]
    fn a_missing_file_fails_verification() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        fs::remove_file(b.path().join("d/b.bin")).unwrap();
        let v = Manifest::observe("a", a.path()).unwrap().verify_tree(
            "restore",
            b.path(),
            Strictness::full(),
        );
        assert!(!v.passed);
        assert_eq!(v.missing, vec!["d/b.bin".to_string()]);
    }

    #[test]
    fn an_extra_file_fails_verification() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        fs::write(b.path().join("d/surprise"), b"!").unwrap();
        let v = Manifest::observe("a", a.path()).unwrap().verify_tree(
            "restore",
            b.path(),
            Strictness::full(),
        );
        assert!(!v.passed);
        assert_eq!(v.extra, vec!["d/surprise".to_string()]);
    }

    #[test]
    fn mtime_strictness_is_honoured_in_both_directions() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        fsx::set_mtime(&b.path().join("d/a.txt"), 1_600_000_000, 0).unwrap();
        let ma = Manifest::observe("a", a.path()).unwrap();
        assert!(!ma.verify_tree("v", b.path(), Strictness::full()).passed);
        assert!(ma.verify_tree("v", b.path(), Strictness::git()).passed);
    }

    #[test]
    fn a_changed_symlink_target_is_corruption() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        fs::remove_file(b.path().join("d/l")).unwrap();
        std::os::unix::fs::symlink("b.bin", b.path().join("d/l")).unwrap();
        fsx::set_mtime(&b.path().join("d/l"), 1_700_000_000, 0).unwrap();
        fsx::set_mtime(&b.path().join("d"), 1_700_000_000, 0).unwrap();
        let v = Manifest::observe("a", a.path()).unwrap().verify_tree(
            "v",
            b.path(),
            Strictness::full(),
        );
        assert!(!v.passed);
        assert!(v.corrupt[0].contains("link target"), "{:?}", v.corrupt);
    }

    #[test]
    fn observe_excluding_drops_a_whole_subtree_and_fixes_the_totals() {
        let d = tempfile::tempdir().unwrap();
        sample(d.path());
        fs::create_dir_all(d.path().join(".git/objects")).unwrap();
        fs::write(d.path().join(".git/objects/x"), vec![0u8; 999]).unwrap();
        let all = Manifest::observe("t", d.path()).unwrap();
        let some = Manifest::observe_excluding("t", d.path(), &[".git"]).unwrap();
        assert!(all.entries.iter().any(|e| e.path.starts_with(".git")));
        assert!(!some.entries.iter().any(|e| e.path.starts_with(".git")));
        assert_eq!(some.file_count, 2);
        assert_eq!(some.dir_count, 1);
        assert_eq!(some.symlink_count, 1);
        assert_eq!(some.total_file_bytes, 105);
    }

    #[test]
    fn git_strictness_tolerates_what_git_cannot_store_and_nothing_else() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        // Git drops empty directories, mtimes and fine-grained modes.
        fs::create_dir(a.path().join("d/empty")).unwrap();
        fs::set_permissions(b.path().join("d/b.bin"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(a.path().join("d/b.bin"), fs::Permissions::from_mode(0o750)).unwrap();
        let ma = Manifest::observe("a", a.path()).unwrap();
        assert!(!ma.verify_tree("v", b.path(), Strictness::full()).passed);
        let v = ma.verify_tree("v", b.path(), Strictness::git());
        assert!(v.passed, "{v:?}");

        // But a wrong executable bit is still corruption.
        fs::set_permissions(b.path().join("d/b.bin"), fs::Permissions::from_mode(0o644)).unwrap();
        let v = ma.verify_tree("v", b.path(), Strictness::git());
        assert!(!v.passed);
        assert!(v.corrupt[0].contains("executable bit"), "{:?}", v.corrupt);
    }

    #[test]
    fn a_missing_non_empty_directory_fails_even_under_git_strictness() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        sample(a.path());
        sample(b.path());
        fs::remove_dir_all(b.path().join("d")).unwrap();
        let v =
            Manifest::observe("a", a.path())
                .unwrap()
                .verify_tree("v", b.path(), Strictness::git());
        assert!(!v.passed);
        assert!(
            v.missing.contains(&"d/a.txt".to_string()),
            "{:?}",
            v.missing
        );
    }

    #[test]
    fn omissions_are_listed_so_a_report_can_state_them() {
        assert!(Strictness::full().omissions().is_empty());
        let o = Strictness::git().omissions();
        assert!(o.iter().any(|s| s.contains("modification times")), "{o:?}");
        assert!(o.iter().any(|s| s.contains("empty directories")), "{o:?}");
    }

    #[test]
    fn manifests_round_trip_through_json() {
        let d = tempfile::tempdir().unwrap();
        sample(d.path());
        let m = Manifest::observe("t", d.path()).unwrap();
        let p = d.path().join("m.json");
        m.save(&p).unwrap();
        assert_eq!(Manifest::load(&p).unwrap(), m);
    }

    #[test]
    fn verifying_a_missing_tree_fails_rather_than_passing_vacuously() {
        let d = tempfile::tempdir().unwrap();
        sample(d.path());
        let m = Manifest::observe("t", d.path()).unwrap();
        let v = m.verify_tree("v", &d.path().join("absent"), Strictness::full());
        assert!(!v.passed);
        assert!(v.detail.contains("cannot read"), "{}", v.detail);
    }
}
