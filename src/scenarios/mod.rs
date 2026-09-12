//! The scenario groups.
//!
//! A scenario is one workload run by one backend in one repetition against a
//! store created for that repetition alone. The groups mirror the three
//! things these stores are actually asked to replace — a Git object database,
//! a Nix store, a backup repository — plus the generic filesystem-tree
//! workload and the object-storage transfers each group needs.

pub mod backup;
pub mod blobs;
pub mod gitsc;
pub mod nixsc;
pub mod tree;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::Profile;
use crate::dataset::corpus::Corpus;
use crate::dataset::gitgen::History;
use crate::metrics::{Op, Phase, RunRecord, Semantics, Verification};
use crate::toolbox::Toolbox;
use crate::util::fsx::{self, DirSizes};
use crate::util::proc::{Output, Run, Usage};

/// Everything a scenario needs that is the same for every backend.
pub struct Ctx {
    pub profile: Profile,
    pub seed: u64,
    pub jobs: usize,
    /// Root of the harness-owned scratch tree.
    pub scratch: PathBuf,
    /// Report directory.
    pub out: PathBuf,
    pub tools: Toolbox,
    /// The shared filesystem corpus.
    pub corpus: Corpus,
    /// The generated source history.
    pub history: History,
    /// Per-commit manifests observed from a staging tree, in step order.
    pub history_manifests: Vec<PathBuf>,
    /// The Nix fixture closures, when the Nix group is enabled.
    pub nix: Option<NixFixture>,
    /// Per-command deadline.
    pub timeout: Duration,
    /// Whether the page cache was dropped before reads.
    pub dropped_caches: bool,
}

/// The built Nix fixture and what is known about it independently of any
/// store under test.
#[derive(Debug, Clone)]
pub struct NixFixture {
    /// The `nix-fixtures-*` output path, or the first supplied root.
    pub root: PathBuf,
    /// Store path of generation 1's top of closure.
    pub gen1: String,
    /// Store path of generation 2's top of closure.
    pub gen2: String,
    /// Closure members of each generation, sorted.
    pub gen1_closure: Vec<String>,
    pub gen2_closure: Vec<String>,
    /// NAR hash and references of every path in either closure, as reported
    /// by the *source* store. Restored closures are checked against this.
    pub details: BTreeMap<String, (String, Vec<String>)>,
    /// Where these closures came from; see [`NixOrigin`].
    pub origin: NixOrigin,
}

/// Where a Nix workload's closures came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NixOrigin {
    /// The deterministic fixture this flake builds: two generations that
    /// share most of their closure, so there is a real incremental
    /// publication and a real retention step to measure. This is the default
    /// and is what the smoke profile and CI use.
    Fixture,
    /// Closures rooted at store paths supplied on the command line.
    ///
    /// These belong to whoever supplied them, so the harness only ever reads
    /// them: it queries their NAR hashes and references from the store they
    /// are already in, copies them into stores of its own, and never writes
    /// to them.
    Supplied {
        /// The roots, in the order they were given.
        roots: Vec<String>,
        /// True when a second, different root was supplied, so generation 2
        /// really is a different generation.
        ///
        /// When it is false there is no churn, and every operation that
        /// needs a *changed* generation — the incremental import, the
        /// incremental publication and pull, the retention step — is
        /// recorded as unsupported with that reason. Editing a user's store
        /// paths to manufacture churn is not an option: they are read-only,
        /// and a fabricated second generation would not be a measurement of
        /// anything.
        second_generation: bool,
    },
}

impl NixOrigin {
    /// True when the two generations really differ, so the operations that
    /// depend on a change can be measured.
    pub fn has_churn(&self) -> bool {
        match self {
            NixOrigin::Fixture => true,
            NixOrigin::Supplied {
                second_generation, ..
            } => *second_generation,
        }
    }

    /// Why an operation that needs a changed generation cannot run.
    pub fn no_churn_reason(&self) -> String {
        "only one closure root was supplied with --nix-closure-root, so there \
         is no second generation to change to. Supplied store paths are \
         read-only: the harness will not edit them to manufacture churn, and \
         it will not report a repeat of the same closure as if it were an \
         incremental one. Supply the option twice, or use the deterministic \
         fixture, to measure this operation."
            .to_string()
    }

    /// A short description for the report.
    pub fn describe(&self) -> String {
        match self {
            NixOrigin::Fixture => "the deterministic fixture closure built by this flake".into(),
            NixOrigin::Supplied {
                roots,
                second_generation,
            } => format!(
                "{} store path(s) supplied with --nix-closure-root, read only: {}{}",
                roots.len(),
                roots.join(", "),
                if *second_generation {
                    ". The first is generation 1 and the second is generation 2"
                } else {
                    ". Only one root was supplied, so every operation that \
                     needs a changed generation is recorded as unsupported"
                }
            ),
        }
    }
}

impl Ctx {
    /// A fresh, empty directory for one backend's one repetition.
    pub fn workdir(&self, group: &str, backend: &str, rep: usize, name: &str) -> PathBuf {
        self.scratch
            .join("runs")
            .join(format!("{group}-{backend}-rep{rep}"))
            .join(name)
    }

    /// Creates the parent run directory, removing any previous attempt.
    pub fn prepare_run_dir(
        &self,
        group: &str,
        backend: &str,
        rep: usize,
    ) -> Result<PathBuf, String> {
        let dir = self
            .scratch
            .join("runs")
            .join(format!("{group}-{backend}-rep{rep}"));
        if dir.exists() {
            fsx::make_writable_tree(&dir).map_err(|e| e.to_string())?;
            std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(dir)
    }
}

/// Collects the operations, verifications and notes of one repetition.
pub struct Recorder {
    record: RunRecord,
}

impl Recorder {
    /// Starts a record for one backend's repetition.
    pub fn new(
        group: &str,
        scenario: &str,
        backend: &str,
        rep: usize,
        order_index: usize,
        semantics: Semantics,
    ) -> Recorder {
        Recorder {
            record: RunRecord {
                group: group.to_string(),
                scenario: scenario.to_string(),
                backend: backend.to_string(),
                rep,
                order_index,
                semantics,
                ops: Vec::new(),
                roots: std::collections::BTreeMap::new(),
                verifications: Vec::new(),
                error: None,
            },
        }
    }

    /// Adds an operation.
    pub fn push(&mut self, op: Op) {
        self.record.ops.push(op);
    }

    /// Records a content identifier the backend produced.
    pub fn root(&mut self, name: &str, key: &str) {
        self.record.roots.insert(name.to_string(), key.to_string());
    }

    /// Adds a verification.
    pub fn verify(&mut self, v: Verification) {
        self.record.verifications.push(v);
    }

    /// True when every operation so far succeeded.
    pub fn ok_so_far(&self) -> bool {
        self.record
            .ops
            .iter()
            .all(|o| o.status != crate::metrics::OpStatus::Failed)
    }

    /// Aborts the repetition with an error, keeping what was recorded.
    pub fn abort(mut self, error: impl Into<String>) -> RunRecord {
        self.record.error = Some(error.into());
        self.record
    }

    /// Finishes the record.
    pub fn finish(self) -> RunRecord {
        self.record
    }

    /// Adds a note to the backend's semantics.
    pub fn note(&mut self, note: impl Into<String>) {
        self.record.semantics.notes.push(note.into());
    }

    /// Sets the transport description (blob scenarios).
    pub fn transport(&mut self, t: impl Into<String>) {
        self.record.semantics.transport = Some(t.into());
    }
}

/// Runs `cmd` and turns it into a measured operation. A failure becomes a
/// failed operation carrying the error, never a missing row.
pub fn measured(name: &str, description: &str, cmd: &Run) -> Op {
    let display = cmd.display();
    match cmd.run() {
        Ok(out) if out.success() => Op::measured(name, out.usage)
            .describe(description)
            .command(display),
        Ok(out) => {
            let usage = out.usage;
            let err = out.require_success().unwrap_err();
            Op::failed(name, err)
                .describe(description)
                .command(display)
                .note(format!(
                    "the child still consumed {} ms of wall time before failing",
                    usage.wall_ns / 1_000_000
                ))
        }
        Err(e) => Op::failed(name, e).describe(description).command(display),
    }
}

/// Runs several commands and charges them all to one operation.
pub fn measured_many(name: &str, description: &str, cmds: &[Run]) -> Op {
    let mut usage = Usage::default();
    let mut commands = Vec::new();
    for cmd in cmds {
        commands.push(cmd.display());
        match cmd.run() {
            Ok(out) => {
                usage = usage.add(out.usage);
                if !out.success() {
                    let err = out.require_success().unwrap_err();
                    return Op::failed(name, err)
                        .describe(description)
                        .commands(commands);
                }
            }
            Err(e) => {
                return Op::failed(name, e).describe(description).commands(commands);
            }
        }
    }
    Op::measured(name, usage)
        .describe(description)
        .commands(commands)
}

/// Runs a command as setup or verification: timed for transparency, never
/// compared.
pub fn aside(name: &str, phase: Phase, description: &str, cmd: &Run) -> Op {
    let display = cmd.display();
    match cmd.run() {
        Ok(out) if out.success() => Op::aside(name, phase, out.usage)
            .describe(description)
            .command(display),
        Ok(out) => Op::failed(name, out.require_success().unwrap_err())
            .describe(description)
            .command(display),
        Err(e) => Op::failed(name, e).describe(description).command(display),
    }
}

/// Runs a command for its output, failing the whole repetition if it errors.
pub fn capture(cmd: &Run) -> Result<Output, String> {
    cmd.ok()
}

/// Measures a store directory, syncing first so the numbers describe what is
/// on the device rather than what is still in the page cache. The sync
/// happens *after* the measured window, so it is charged to nobody.
pub fn store_sizes(dir: &Path) -> DirSizes {
    fsx::sync_all();
    fsx::measure_dir(dir).unwrap_or_default()
}

/// Occupancy of a store's named components, as counters.
///
/// It matters: an Amber store is a `packstore/` directory of pack
/// segments plus a `refs/` reference database, and the two cores use
/// different databases (redb in Rust, Pebble in Go). redb preallocates
/// several megabytes, which dominates a whole-store size comparison on
/// small profiles and says nothing about either core's storage
/// efficiency. Reporting the components separately lets a reader see
/// the pack bytes, which is the number the comparison is about.
pub fn store_counters(dir: &Path) -> std::collections::BTreeMap<String, i64> {
    let mut out = std::collections::BTreeMap::new();
    for name in [
        "packstore",
        "refs",
        "closures",
        "data",
        "index",
        "snapshots",
    ] {
        let sub = dir.join(name);
        if !sub.exists() {
            continue;
        }
        if let Ok(sizes) = fsx::measure_dir(&sub) {
            out.insert(
                format!("store_{name}_allocated_bytes"),
                sizes.allocated_bytes as i64,
            );
            out.insert(
                format!("store_{name}_apparent_bytes"),
                sizes.apparent_bytes as i64,
            );
        }
    }
    out
}

/// Difference in allocated bytes, positive when space was released.
pub fn reclaimed(before: &DirSizes, after: &DirSizes) -> i64 {
    before.allocated_bytes as i64 - after.allocated_bytes as i64
}

/// How far a pack segment is backdated before a measured collection, so the
/// cores' one-hour minimum pack age cannot make every collection reclaim
/// nothing. See [`crate::adapters::amber::AmberCli::age_sealed_segments`].
pub const SEGMENT_AGE: Duration = Duration::from_secs(2 * 60 * 60);

/// Ages an Amber store's sealed segments and records it as a setup step.
///
/// Returns the operation to push, so the report shows that it happened and
/// why. Nothing here is charged to a measured operation.
pub fn age_segments(cli: &crate::adapters::amber::AmberCli) -> Op {
    match cli.age_sealed_segments(SEGMENT_AGE) {
        Ok(n) => Op::aside("age_sealed_segments", Phase::Setup, Usage::default())
            .describe("backdate the sealed pack segments so collection can reap them")
            .counter("segments_aged", n as i64)
            .note(format!(
                "both cores refuse to reap a sealed segment younger than \
                 their grace period, and a grace of zero selects the default \
                 of one hour in both, so {n} segment file(s) were backdated \
                 by {} hour(s). Without this every collection would be \
                 measured reclaiming nothing however much garbage it had. \
                 Git, restic and Nix all prune immediately, so this makes the \
                 comparison more equal, not less.",
                SEGMENT_AGE.as_secs() / 3600
            )),
        Err(e) => Op::failed("age_sealed_segments", e),
    }
}

/// What is known about a store's contents after a retention step.
pub struct RetentionEvidence {
    /// What the backend's own listing says it still holds, or the error the
    /// listing command failed with.
    pub listing: Result<Vec<String>, String>,
    /// Names that were supposed to have been dropped but could still be read
    /// back in full. A successful read is proof of presence; a *failed* read
    /// is proof of nothing, which is why absence is never concluded from one.
    pub still_readable: Vec<String>,
}

/// Checks what a store retains against what retention was supposed to do.
///
/// The asymmetry is deliberate and is the whole point of this function:
///
/// * A **successful** listing that lacks a name is evidence the name is gone.
/// * A listing that **failed** is evidence of nothing at all — a timeout, a
///   missing executable or an unrelated error says nothing about whether a
///   snapshot is still there — so it is reported as an infrastructure
///   failure rather than as a passing absence check.
/// * A dropped name that can still be *read back* fails the check outright.
pub fn check_retention(
    name: &str,
    evidence: RetentionEvidence,
    kept: &[&str],
    dropped: &[&str],
) -> Verification {
    let names = match evidence.listing {
        Ok(n) => n,
        Err(e) => {
            return Verification::fail(
                name,
                format!(
                    "the store's own listing failed, so nothing can be \
                     concluded about what it retains: {e}"
                ),
            );
        }
    };
    let mut problems = Vec::new();
    for want in kept {
        if !names.iter().any(|n| n == want) {
            problems.push(format!(
                "{want} should have been retained but is not listed"
            ));
        }
    }
    for gone in dropped {
        if names.iter().any(|n| n == gone) {
            problems.push(format!("{gone} was dropped but the store still lists it"));
        }
    }
    for readable in &evidence.still_readable {
        problems.push(format!(
            "{readable} was dropped but its contents could still be read back in full"
        ));
    }
    let mut v = if problems.is_empty() {
        Verification::pass(
            name,
            format!(
                "the store lists exactly what retention should have left: \
                 retained {} ({}), dropped {} ({}); absence was read from the \
                 backend's own successful listing, not inferred from a failed \
                 command",
                kept.len(),
                kept.join(", "),
                dropped.len(),
                dropped.join(", "),
            ),
        )
    } else {
        Verification::fail(name, format!("{} problem(s)", problems.len()))
    };
    v.corrupt = problems;
    v
}

/// Picks which retained versions to verify: all of them when there are few,
/// otherwise the first, the last and an even spread between, so a long
/// history is still checked end to end without the verification dominating
/// the run.
pub fn versions_to_verify(total: usize, budget: usize) -> Vec<usize> {
    if total == 0 {
        return Vec::new();
    }
    if total <= budget {
        return (0..total).collect();
    }
    let mut picked: Vec<usize> = (0..budget)
        .map(|i| i * (total - 1) / (budget - 1).max(1))
        .collect();
    picked.push(0);
    picked.push(total - 1);
    picked.sort_unstable();
    picked.dedup();
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::OpStatus;

    #[test]
    fn a_failing_command_becomes_a_failed_operation_not_a_missing_row() {
        let cmd = Run::new("/bin/sh").arg("-c").arg("echo bad >&2; exit 7");
        let op = measured("ingest", "ingest the corpus", &cmd);
        assert_eq!(op.status, OpStatus::Failed);
        assert!(op.reason.as_ref().unwrap().contains("bad"));
        assert!(op.wall_ns.is_none(), "a failed op has no comparable timing");
        assert_eq!(op.commands.len(), 1);
    }

    #[test]
    fn a_successful_command_is_measured_and_records_its_command_line() {
        let cmd = Run::new("/bin/sh").arg("-c").arg("true");
        let op = measured("noop", "do nothing", &cmd);
        assert_eq!(op.status, OpStatus::Ok);
        assert!(op.wall_ns.is_some());
        assert!(op.commands[0].contains("/bin/sh"));
    }

    #[test]
    fn measured_many_sums_usage_and_stops_at_the_first_failure() {
        let ok = Run::new("/bin/sh").arg("-c").arg("true");
        let bad = Run::new("/bin/sh").arg("-c").arg("exit 3");
        let op = measured_many(
            "multi",
            "three steps",
            &[ok.clone(), ok.clone(), ok.clone()],
        );
        assert_eq!(op.status, OpStatus::Ok);
        assert_eq!(op.commands.len(), 3);

        let op = measured_many("multi", "three steps", &[ok.clone(), bad, ok]);
        assert_eq!(op.status, OpStatus::Failed);
        assert_eq!(
            op.commands.len(),
            2,
            "it must not keep going after a failure"
        );
    }

    #[test]
    fn setup_and_verification_never_become_comparable_rows() {
        let cmd = Run::new("/bin/sh").arg("-c").arg("true");
        for phase in [Phase::Setup, Phase::Verify] {
            let op = aside("stage", phase, "stage the corpus", &cmd);
            assert_eq!(op.status, OpStatus::Ok);
            assert!(!op.is_comparable());
        }
    }

    #[test]
    fn reclaim_is_positive_when_space_was_released() {
        let before = DirSizes {
            allocated_bytes: 1000,
            ..Default::default()
        };
        let after = DirSizes {
            allocated_bytes: 400,
            ..Default::default()
        };
        assert_eq!(reclaimed(&before, &after), 600);
        assert_eq!(reclaimed(&after, &before), -600);
    }

    #[test]
    fn a_single_supplied_closure_has_no_churn_and_says_why() {
        let one = NixOrigin::Supplied {
            roots: vec!["/nix/store/aaa-x".into()],
            second_generation: false,
        };
        assert!(!one.has_churn());
        let why = one.no_churn_reason();
        assert!(why.contains("read-only"), "{why}");
        assert!(why.contains("--nix-closure-root"), "{why}");
        let what = one.describe();
        assert!(what.contains("read only"), "{what}");
        assert!(what.contains("unsupported"), "{what}");

        let two = NixOrigin::Supplied {
            roots: vec!["/nix/store/aaa-x".into(), "/nix/store/bbb-y".into()],
            second_generation: true,
        };
        assert!(two.has_churn());
        assert!(
            two.describe().contains("generation 2"),
            "{}",
            two.describe()
        );

        // The built fixture always has two generations.
        assert!(NixOrigin::Fixture.has_churn());
        assert!(NixOrigin::Fixture.describe().contains("deterministic"));
    }

    #[test]
    fn version_sampling_always_includes_the_ends() {
        assert_eq!(versions_to_verify(0, 8), Vec::<usize>::new());
        assert_eq!(versions_to_verify(5, 8), vec![0, 1, 2, 3, 4]);
        let picked = versions_to_verify(100, 8);
        assert!(picked.len() <= 9, "{picked:?}");
        assert_eq!(*picked.first().unwrap(), 0);
        assert_eq!(*picked.last().unwrap(), 99);
        assert!(picked.windows(2).all(|w| w[0] < w[1]), "{picked:?}");
    }

    #[test]
    fn a_recorder_keeps_the_operations_recorded_before_an_abort() {
        let mut r = Recorder::new(
            "tree",
            "tree/lifecycle",
            "amber-rust",
            0,
            0,
            crate::adapters::semantics("amber-rust"),
        );
        r.push(Op::measured("a", Usage::default()));
        assert!(r.ok_so_far());
        r.push(Op::failed("b", "boom"));
        assert!(!r.ok_so_far());
        let rec = r.abort("the store vanished");
        assert_eq!(rec.ops.len(), 2);
        assert_eq!(rec.error.as_deref(), Some("the store vanished"));
        assert!(!rec.healthy());
    }
}
