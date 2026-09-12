//! A deterministic source-history fixture.
//!
//! The Git comparison is not "ingest a checkout": both Git and the Amber
//! cores are handed the *same* sequence of working-tree states and asked to
//! retain all of them. This module produces that sequence — a branching,
//! tagged history with the shapes a real source repository has:
//!
//! * `src/` — hundreds of small text files, a handful rewritten per commit
//!   (the "small changes" case Git's delta packing is built for);
//! * `vendor/` — a large subtree that never changes, so every commit after
//!   the first shares its trees;
//! * `assets/` — binary blobs; one of them is rewritten wholesale part-way
//!   through, which is the case Git handles worst;
//! * `docs/` — text that changes on its own, slower cadence.
//!
//! Two side branches fork from the main line; one is merged back with a real
//! two-parent commit, the other is left unmerged so its objects stay
//! reachable only from its own ref. Tags, one lightweight and one annotated,
//! pin two points of the main line.
//!
//! Every commit is made with fixed author and committer identities and fixed
//! timestamps, so the commit object hashes are a function of the seed alone.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::corpus;
use crate::util::fsx;

/// Author, committer and base timestamp of every generated commit. Changing
/// any of these changes every commit id.
pub const IDENTITY_NAME: &str = "Amber Bench";
/// Committer e-mail of every generated commit.
pub const IDENTITY_EMAIL: &str = "bench@example.invalid";
/// Timestamp of the first commit; each later commit adds 60 seconds.
pub const BASE_COMMIT_TIME: i64 = 1_700_000_000;

/// Shape and size of the generated history.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct GitSpec {
    /// Small text files under `src/`.
    pub src_files: u32,
    pub src_file_bytes: u64,
    /// Text files under `docs/`.
    pub docs_files: u32,
    pub docs_file_bytes: u64,
    /// Binary blobs under `assets/`.
    pub asset_files: u32,
    pub asset_file_bytes: u64,
    /// Never-changing files under `vendor/`.
    pub vendor_files: u32,
    pub vendor_file_bytes: u64,
    /// Commits on the main line, excluding the initial import.
    pub main_commits: u32,
    /// `src/` files rewritten by each commit.
    pub changed_src_files_per_commit: u32,
    /// Commits on each of the two side branches.
    pub branch_commits: u32,
}

/// One file of one working-tree state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    /// Content label: the deterministic byte stream identifier.
    pub label: String,
    pub bytes: u64,
    /// Whether the bytes are text-like (compressible) or incompressible.
    pub text: bool,
    /// Whether the file is executable.
    pub exec: bool,
}

/// A complete working-tree state: every path and its content.
pub type TreeState = BTreeMap<String, Blob>;

/// One commit in the generated history.
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    /// Position in the order the history is built.
    pub index: usize,
    /// Branch this commit lands on.
    pub branch: String,
    /// The branch this commit's parent is on, when it differs (a fork point).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_from: Option<String>,
    /// Branch merged into this commit, if it is a merge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_of: Option<String>,
    /// Commit subject line.
    pub subject: String,
    /// A tag to create here, and whether it is annotated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<(String, bool)>,
    /// Commit timestamp, seconds since the epoch.
    pub timestamp: i64,
    /// Reference name the Amber backends use for this snapshot.
    pub ref_name: String,
}

/// The generated history: an ordered list of commits and the working-tree
/// state each one records.
#[derive(Debug, Clone)]
pub struct History {
    pub spec: GitSpec,
    pub seed: u64,
    pub steps: Vec<Step>,
    states: Vec<TreeState>,
}

/// Summary of a history, for the report.
#[derive(Debug, Clone, Serialize)]
pub struct HistorySummary {
    pub spec: GitSpec,
    pub seed: u64,
    pub commits: usize,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
    /// Sum of file sizes of the final main-line state.
    pub head_state_bytes: u64,
    /// Sum of file sizes across every retained state.
    pub all_states_bytes: u64,
}

impl History {
    /// The working-tree state recorded by step `index`.
    pub fn state(&self, index: usize) -> &TreeState {
        &self.states[index]
    }

    /// Every branch the history creates, in creation order.
    pub fn branches(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for s in &self.steps {
            if !seen.contains(&s.branch) {
                seen.push(s.branch.clone());
            }
        }
        seen
    }

    /// Every tag the history creates.
    pub fn tags(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter_map(|s| s.tag.as_ref().map(|(n, _)| n.clone()))
            .collect()
    }

    /// Bytes of the state recorded by step `index`.
    pub fn state_bytes(&self, index: usize) -> u64 {
        self.states[index].values().map(|b| b.bytes).sum()
    }

    /// A serialisable summary.
    pub fn summary(&self) -> HistorySummary {
        HistorySummary {
            spec: self.spec,
            seed: self.seed,
            commits: self.steps.len(),
            branches: self.branches(),
            tags: self.tags(),
            head_state_bytes: self.state_bytes(self.steps.len() - 1),
            all_states_bytes: (0..self.steps.len()).map(|i| self.state_bytes(i)).sum(),
        }
    }

    /// Materialises the state of step `index` into `dir`, adding, rewriting
    /// and deleting exactly the files that differ from what `previous`
    /// already left there. Passing `None` writes the whole tree.
    ///
    /// Doing it incrementally matters: it is what makes `git add -A` see a
    /// handful of modified paths rather than a fresh import, which is the
    /// workload the comparison is about.
    pub fn apply(&self, index: usize, previous: Option<usize>, dir: &Path) -> io::Result<()> {
        let want = &self.states[index];
        let have = previous.map(|p| &self.states[p]);
        if let Some(have) = have {
            for path in have.keys() {
                if !want.contains_key(path) {
                    let p = dir.join(path);
                    if p.exists() {
                        std::fs::remove_file(&p)?;
                    }
                }
            }
        }
        for (path, blob) in want {
            if have.and_then(|h| h.get(path)) == Some(blob) {
                continue;
            }
            let p = dir.join(path);
            corpus::write_blob(&p, self.seed, &blob.label, blob.bytes, blob.text)?;
            let mode = if blob.exec { 0o755 } else { 0o644 };
            std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(mode))?;
        }
        Ok(())
    }

    /// Writes the state of step `index` into an empty `dir` and returns the
    /// manifest observed from disk afterwards.
    pub fn materialize_fresh(&self, index: usize, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        self.apply(index, None, dir)?;
        Ok(())
    }
}

/// Builds the history plan. Nothing is written to disk here.
pub fn plan(spec: &GitSpec, seed: u64) -> History {
    let mut steps = Vec::new();
    let mut states: Vec<TreeState> = Vec::new();

    // Revision counters per path; a file's content label is its path plus the
    // number of times it has been rewritten, so an untouched file keeps its
    // bytes and its Git blob id forever.
    let mut rev: BTreeMap<String, u32> = BTreeMap::new();
    let mut tree = initial_tree(spec, &mut rev);

    let push = |steps: &mut Vec<Step>,
                states: &mut Vec<TreeState>,
                branch: &str,
                fork_from: Option<String>,
                merge_of: Option<String>,
                subject: String,
                tag: Option<(String, bool)>,
                tree: &TreeState| {
        let index = steps.len();
        steps.push(Step {
            index,
            branch: branch.to_string(),
            fork_from,
            merge_of,
            subject,
            tag,
            timestamp: BASE_COMMIT_TIME + index as i64 * 60,
            ref_name: format!("history-{index:04}"),
        });
        states.push(tree.clone());
    };

    push(
        &mut steps,
        &mut states,
        "main",
        None,
        None,
        "import: initial tree".into(),
        None,
        &tree,
    );

    let fork_a = spec.main_commits / 3 + 1;
    let fork_b = spec.main_commits * 2 / 3 + 1;
    let rewrite_asset_at = spec.main_commits / 2 + 1;
    let mut branch_a_tip: Option<(TreeState, BTreeMap<String, u32>)> = None;

    for n in 1..=spec.main_commits {
        advance_src(spec, &mut tree, &mut rev, "main", n);
        if n % 5 == 0 {
            advance_docs(spec, &mut tree, &mut rev, "main", n);
        }
        if n == rewrite_asset_at && spec.asset_files > 0 {
            // A binary asset rewritten in full: no delta helps here.
            let path = format!("assets/a{:03}.bin", 0);
            bump(&mut tree, &mut rev, &path);
        }
        if n == spec.main_commits / 4 + 1 {
            tree.remove(&format!("src/pkg00/f{:04}.txt", 0));
        }
        if n == spec.main_commits / 4 + 2 {
            let path = "src/pkg00/new_module.txt".to_string();
            insert(
                spec,
                &mut tree,
                &mut rev,
                &path,
                spec.src_file_bytes,
                true,
                false,
            );
        }
        let tag = if n == 1 {
            Some(("v0.1".to_string(), false))
        } else if n == spec.main_commits / 2 {
            Some(("v1.0".to_string(), true))
        } else {
            None
        };
        push(
            &mut steps,
            &mut states,
            "main",
            None,
            None,
            format!("main: change set {n}"),
            tag,
            &tree,
        );

        if n == fork_a {
            let mut side = tree.clone();
            let mut side_rev = rev.clone();
            for k in 1..=spec.branch_commits {
                advance_src(spec, &mut side, &mut side_rev, "feat-a", k);
                push(
                    &mut steps,
                    &mut states,
                    "feature/a",
                    if k == 1 { Some("main".into()) } else { None },
                    None,
                    format!("feature/a: change set {k}"),
                    None,
                    &side,
                );
            }
            branch_a_tip = Some((side, side_rev));
        }
        if n == fork_b {
            let mut side = tree.clone();
            let mut side_rev = rev.clone();
            for k in 1..=spec.branch_commits {
                advance_src(spec, &mut side, &mut side_rev, "feat-b", k);
                push(
                    &mut steps,
                    &mut states,
                    "feature/b",
                    if k == 1 { Some("main".into()) } else { None },
                    None,
                    format!("feature/b: change set {k}"),
                    None,
                    &side,
                );
            }
            // feature/b is deliberately never merged.
        }
        if n == fork_b
            && let Some((side, side_rev)) = branch_a_tip.take()
        {
            // Merge feature/a. A file the side branch advanced further
            // than the main line wins; everything else keeps the main
            // line's version, which is what an ordinary merge does.
            for (path, blob) in side {
                let theirs = side_rev.get(&path).copied().unwrap_or(0);
                let ours = rev.get(&path).copied().unwrap_or(0);
                if theirs > ours || !tree.contains_key(&path) {
                    rev.insert(path.clone(), theirs);
                    tree.insert(path, blob);
                }
            }
            push(
                &mut steps,
                &mut states,
                "main",
                None,
                Some("feature/a".into()),
                "merge feature/a into main".into(),
                None,
                &tree,
            );
        }
    }

    History {
        spec: *spec,
        seed,
        steps,
        states,
    }
}

fn initial_tree(spec: &GitSpec, rev: &mut BTreeMap<String, u32>) -> TreeState {
    let mut tree = TreeState::new();
    for i in 0..spec.src_files {
        let path = format!("src/pkg{:02}/f{:04}.txt", i % 16, i);
        insert(
            spec,
            &mut tree,
            rev,
            &path,
            spec.src_file_bytes,
            true,
            i % 64 == 0,
        );
    }
    for i in 0..spec.docs_files {
        let path = format!("docs/d{i:03}.md");
        insert(
            spec,
            &mut tree,
            rev,
            &path,
            spec.docs_file_bytes,
            true,
            false,
        );
    }
    for i in 0..spec.asset_files {
        let path = format!("assets/a{i:03}.bin");
        insert(
            spec,
            &mut tree,
            rev,
            &path,
            spec.asset_file_bytes,
            false,
            false,
        );
    }
    for i in 0..spec.vendor_files {
        let path = format!("vendor/lib{:02}/v{:04}.txt", i % 8, i);
        insert(
            spec,
            &mut tree,
            rev,
            &path,
            spec.vendor_file_bytes,
            true,
            false,
        );
    }
    tree
}

fn insert(
    _spec: &GitSpec,
    tree: &mut TreeState,
    rev: &mut BTreeMap<String, u32>,
    path: &str,
    bytes: u64,
    text: bool,
    exec: bool,
) {
    let r = rev.entry(path.to_string()).or_insert(0);
    tree.insert(
        path.to_string(),
        Blob {
            label: format!("git:{path}@{r}"),
            bytes,
            text,
            exec,
        },
    );
}

/// Rewrites a file already in the tree, advancing its revision.
fn bump(tree: &mut TreeState, rev: &mut BTreeMap<String, u32>, path: &str) {
    let r = rev.entry(path.to_string()).or_insert(0);
    *r += 1;
    if let Some(blob) = tree.get_mut(path) {
        blob.label = format!("git:{path}@{r}");
    }
}

/// Rewrites this commit's window of `src/` files.
fn advance_src(
    spec: &GitSpec,
    tree: &mut TreeState,
    rev: &mut BTreeMap<String, u32>,
    line: &str,
    n: u32,
) {
    if spec.src_files == 0 || spec.changed_src_files_per_commit == 0 {
        return;
    }
    // A different, non-overlapping window per line, so the branches diverge.
    let offset = match line {
        "feat-a" => spec.src_files / 3,
        "feat-b" => spec.src_files * 2 / 3,
        _ => 0,
    };
    for k in 0..spec.changed_src_files_per_commit {
        let i = (offset + (n - 1) * spec.changed_src_files_per_commit + k) % spec.src_files;
        let path = format!("src/pkg{:02}/f{:04}.txt", i % 16, i);
        if tree.contains_key(&path) {
            bump(tree, rev, &path);
        }
    }
}

fn advance_docs(
    spec: &GitSpec,
    tree: &mut TreeState,
    rev: &mut BTreeMap<String, u32>,
    _line: &str,
    n: u32,
) {
    if spec.docs_files == 0 {
        return;
    }
    let path = format!("docs/d{:03}.md", n % spec.docs_files);
    if tree.contains_key(&path) {
        bump(tree, rev, &path);
    }
}

/// A digest of the whole plan: two runs with the same seed and spec must
/// agree, and it changes whenever any file of any state changes.
pub fn plan_digest(h: &History) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&h.seed.to_le_bytes());
    for (i, step) in h.steps.iter().enumerate() {
        hasher.update(step.branch.as_bytes());
        hasher.update(step.subject.as_bytes());
        hasher.update(&step.timestamp.to_le_bytes());
        for (path, blob) in &h.states[i] {
            hasher.update(path.as_bytes());
            hasher.update(blob.label.as_bytes());
            hasher.update(&blob.bytes.to_le_bytes());
            hasher.update(&[blob.text as u8, blob.exec as u8]);
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// Writes the manifest of a materialised state next to the working tree.
pub fn save_state_manifest(dir: &Path, out: &Path, label: &str) -> io::Result<PathBuf> {
    let m = super::manifest::Manifest::observe(label, dir)?;
    m.save(out)?;
    Ok(out.to_path_buf())
}

/// Normalises a materialised working tree's timestamps, so an Amber ingest of
/// it is reproducible. Git stores no mtimes, so this affects only the Amber
/// side and is applied identically for every repetition.
pub fn normalize_mtimes(dir: &Path) -> io::Result<()> {
    let mut paths = fsx::walk_relative(dir)?;
    paths.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for rel in &paths {
        if rel.starts_with(".git") {
            continue;
        }
        fsx::set_mtime(&dir.join(rel), BASE_COMMIT_TIME, 0)?;
    }
    fsx::set_mtime(dir, BASE_COMMIT_TIME, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GitSpec {
        GitSpec {
            src_files: 48,
            src_file_bytes: 900,
            docs_files: 4,
            docs_file_bytes: 2048,
            asset_files: 3,
            asset_file_bytes: 64 * 1024,
            vendor_files: 12,
            vendor_file_bytes: 4096,
            main_commits: 9,
            changed_src_files_per_commit: 3,
            branch_commits: 2,
        }
    }

    #[test]
    fn the_plan_is_a_function_of_the_seed_and_spec() {
        assert_eq!(
            plan_digest(&plan(&spec(), 5)),
            plan_digest(&plan(&spec(), 5))
        );
        assert_ne!(
            plan_digest(&plan(&spec(), 5)),
            plan_digest(&plan(&spec(), 6))
        );
    }

    #[test]
    fn the_history_branches_tags_and_merges() {
        let h = plan(&spec(), 1);
        let branches = h.branches();
        assert!(branches.contains(&"main".to_string()));
        assert!(branches.contains(&"feature/a".to_string()));
        assert!(branches.contains(&"feature/b".to_string()));
        assert_eq!(h.tags(), vec!["v0.1".to_string(), "v1.0".to_string()]);
        assert_eq!(
            h.steps.iter().filter(|s| s.merge_of.is_some()).count(),
            1,
            "exactly one merge commit"
        );
        assert!(
            h.steps
                .iter()
                .any(|s| s.tag.as_ref().is_some_and(|(_, ann)| *ann)),
            "one tag must be annotated"
        );
    }

    #[test]
    fn most_of_the_tree_is_shared_between_consecutive_commits() {
        let h = plan(&spec(), 1);
        let a = h.state(1);
        let b = h.state(2);
        let changed = a
            .iter()
            .filter(|(p, blob)| b.get(*p) != Some(*blob))
            .count();
        assert!(changed > 0, "a commit must change something");
        assert!(
            changed * 4 < a.len(),
            "{changed} of {} files changed; that is not a small change",
            a.len()
        );
        // vendor/ never moves.
        for (p, blob) in a.iter().filter(|(p, _)| p.starts_with("vendor/")) {
            assert_eq!(b.get(p), Some(blob), "{p} must be shared");
        }
    }

    #[test]
    fn a_binary_asset_is_rewritten_somewhere_in_the_history() {
        let h = plan(&spec(), 1);
        let first = h.state(0).get("assets/a000.bin").unwrap().clone();
        let last = h.state(h.steps.len() - 1).get("assets/a000.bin").unwrap();
        assert_ne!(first.label, last.label, "no asset was ever rewritten");
        assert!(!first.text, "assets must be incompressible");
    }

    #[test]
    fn files_are_added_and_deleted_along_the_way() {
        let h = plan(&spec(), 1);
        let first = h.state(0);
        let last = h.state(h.steps.len() - 1);
        assert!(
            first.contains_key("src/pkg00/f0000.txt") && !last.contains_key("src/pkg00/f0000.txt"),
            "no file was deleted"
        );
        assert!(
            last.contains_key("src/pkg00/new_module.txt"),
            "no file was added"
        );
    }

    #[test]
    fn applying_states_incrementally_equals_applying_them_fresh() {
        let h = plan(&spec(), 2);
        let inc = tempfile::tempdir().unwrap();
        let fresh = tempfile::tempdir().unwrap();
        let target = h.steps.len() - 1;
        h.apply(0, None, inc.path()).unwrap();
        for i in 1..=target {
            h.apply(i, Some(i - 1), inc.path()).unwrap();
        }
        h.materialize_fresh(target, fresh.path()).unwrap();
        normalize_mtimes(inc.path()).unwrap();
        normalize_mtimes(fresh.path()).unwrap();
        let a = super::super::manifest::Manifest::observe("inc", inc.path()).unwrap();
        let b = super::super::manifest::Manifest::observe("fresh", fresh.path()).unwrap();
        let diff = a.compare(&b, super::super::manifest::Strictness::full());
        assert!(
            diff.clean(),
            "incremental and fresh disagree: {:?} {:?} {:?}",
            diff.missing,
            diff.extra,
            diff.corrupt
        );
    }

    #[test]
    fn executable_bits_survive_materialisation() {
        let h = plan(&spec(), 2);
        let d = tempfile::tempdir().unwrap();
        h.materialize_fresh(0, d.path()).unwrap();
        let m = super::super::manifest::Manifest::observe("t", d.path()).unwrap();
        assert!(
            m.entries.iter().any(|e| e.mode == 0o755),
            "no executable file materialised"
        );
    }
}
