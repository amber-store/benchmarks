//! The measurement record: what an operation cost, what it left on disk, and
//! — just as importantly — when the harness has nothing to report.
//!
//! Two rules are enforced by the types here rather than by convention:
//!
//! * An operation a backend cannot perform is [`OpStatus::Unsupported`] with
//!   a reason. It never becomes a zero, and it never disappears.
//! * Setup and verification carry a [`Phase`] of their own, so the cost of
//!   generating a corpus or of re-hashing a restored tree can never leak into
//!   the number a reader compares.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::util::fsx::DirSizes;
use crate::util::proc::Usage;

/// Which part of a run an operation belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Work done to put a backend into the state the measurement needs. Timed
    /// for transparency, never compared.
    Setup,
    /// The operation under comparison.
    Measured,
    /// Integrity and correctness checking after the fact. Timed for
    /// transparency, never compared.
    Verify,
}

/// How an operation ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OpStatus {
    /// Ran to completion; the timings are valid.
    Ok,
    /// The backend has no equivalent of this operation. `reason` says why.
    Unsupported,
    /// The backend tried and failed. `reason` carries the error.
    Failed,
}

/// One operation of one backend in one repetition.
#[derive(Debug, Clone, Serialize)]
pub struct Op {
    /// Stable identifier, e.g. `ingest_fresh`; the same name across backends
    /// is what makes them comparable.
    pub name: String,
    /// Human-readable description of what was actually executed.
    pub description: String,
    pub phase: Phase,
    pub status: OpStatus,
    /// Present for [`OpStatus::Unsupported`] and [`OpStatus::Failed`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Elapsed wall-clock nanoseconds. `None` unless `status` is `Ok`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_ns: Option<u64>,
    /// Child user CPU nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_user_ns: Option<u64>,
    /// Child system CPU nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_sys_ns: Option<u64>,
    /// Peak resident set size of the child process, in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rss_bytes: Option<u64>,

    /// Bytes of source data the operation logically processed, used for
    /// throughput. `None` when the notion does not apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_bytes: Option<u64>,
    /// Occupancy of the backend's store directory after the operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_after: Option<DirSizes>,
    /// Allocated bytes released by the operation (positive means freed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reclaimed_bytes: Option<i64>,

    /// Backend-specific integer counters, e.g. S3 requests by method.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub counters: BTreeMap<String, i64>,
    /// The command lines that were executed, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
    /// Caveats a reader needs in order to interpret the number.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Op {
    fn base(name: &str, phase: Phase, status: OpStatus) -> Op {
        Op {
            name: name.to_string(),
            description: String::new(),
            phase,
            status,
            reason: None,
            wall_ns: None,
            cpu_user_ns: None,
            cpu_sys_ns: None,
            max_rss_bytes: None,
            logical_bytes: None,
            store_after: None,
            reclaimed_bytes: None,
            counters: BTreeMap::new(),
            commands: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// A measured operation that succeeded, charged `usage`.
    pub fn measured(name: &str, usage: Usage) -> Op {
        let mut op = Op::base(name, Phase::Measured, OpStatus::Ok);
        op.wall_ns = Some(usage.wall_ns);
        op.cpu_user_ns = Some(usage.user_ns);
        op.cpu_sys_ns = Some(usage.sys_ns);
        op.max_rss_bytes = Some(usage.max_rss_bytes);
        op
    }

    /// A setup or verification step that succeeded. Timed, but excluded from
    /// every comparison.
    pub fn aside(name: &str, phase: Phase, usage: Usage) -> Op {
        let mut op = Op::measured(name, usage);
        op.phase = phase;
        op
    }

    /// An operation this backend genuinely cannot perform.
    pub fn unsupported(name: &str, reason: impl Into<String>) -> Op {
        let mut op = Op::base(name, Phase::Measured, OpStatus::Unsupported);
        op.reason = Some(reason.into());
        op
    }

    /// An operation that was attempted and failed.
    pub fn failed(name: &str, reason: impl Into<String>) -> Op {
        let mut op = Op::base(name, Phase::Measured, OpStatus::Failed);
        op.reason = Some(reason.into());
        op
    }

    /// Describes what the operation actually did.
    pub fn describe(mut self, d: impl Into<String>) -> Op {
        self.description = d.into();
        self
    }

    /// Records the logical byte count for throughput.
    pub fn logical(mut self, bytes: u64) -> Op {
        self.logical_bytes = Some(bytes);
        self
    }

    /// Records the store occupancy after the operation.
    pub fn store(mut self, sizes: DirSizes) -> Op {
        self.store_after = Some(sizes);
        self
    }

    /// Records how many allocated bytes the operation released.
    pub fn reclaimed(mut self, bytes: i64) -> Op {
        self.reclaimed_bytes = Some(bytes);
        self
    }

    /// Adds a named counter.
    pub fn counter(mut self, key: &str, value: i64) -> Op {
        self.counters.insert(key.to_string(), value);
        self
    }

    /// Adds several named counters.
    pub fn counters(mut self, more: BTreeMap<String, i64>) -> Op {
        self.counters.extend(more);
        self
    }

    /// Records an executed command line.
    pub fn command(mut self, c: impl Into<String>) -> Op {
        self.commands.push(c.into());
        self
    }

    /// Records several executed command lines.
    pub fn commands(mut self, c: impl IntoIterator<Item = String>) -> Op {
        self.commands.extend(c);
        self
    }

    /// Adds an interpretation caveat.
    pub fn note(mut self, n: impl Into<String>) -> Op {
        self.notes.push(n.into());
        self
    }

    /// Throughput in bytes per second, or `None` when either input is absent
    /// or the operation took no measurable time.
    pub fn throughput_bytes_per_s(&self) -> Option<f64> {
        match (self.logical_bytes, self.wall_ns) {
            (Some(b), Some(ns)) if ns > 0 => Some(b as f64 * 1e9 / ns as f64),
            _ => None,
        }
    }

    /// True when this operation contributes to a comparison.
    pub fn is_comparable(&self) -> bool {
        self.phase == Phase::Measured && self.status == OpStatus::Ok
    }
}

/// The result of one correctness check.
#[derive(Debug, Clone, Serialize)]
pub struct Verification {
    /// Stable identifier, e.g. `restore_matches_manifest`.
    pub name: String,
    pub passed: bool,
    /// What was compared and what came out.
    pub detail: String,
    /// Entries present in the manifest but absent from the restored tree.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    /// Entries present in the restored tree but absent from the manifest.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<String>,
    /// Entries whose bytes or metadata disagree with the manifest.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub corrupt: Vec<String>,
}

impl Verification {
    /// A check that passed.
    pub fn pass(name: &str, detail: impl Into<String>) -> Verification {
        Verification {
            name: name.to_string(),
            passed: true,
            detail: detail.into(),
            missing: Vec::new(),
            extra: Vec::new(),
            corrupt: Vec::new(),
        }
    }

    /// A check that failed.
    pub fn fail(name: &str, detail: impl Into<String>) -> Verification {
        Verification {
            name: name.to_string(),
            passed: false,
            detail: detail.into(),
            missing: Vec::new(),
            extra: Vec::new(),
            corrupt: Vec::new(),
        }
    }
}

/// What a backend guarantees, recorded next to every number it produces so a
/// reader is never invited to read unlike things as like.
#[derive(Debug, Clone, Serialize)]
pub struct Semantics {
    /// Whether and how stored bytes are compressed.
    pub compression: String,
    /// Whether stored bytes are encrypted, and by whom.
    pub encryption: String,
    /// What is guaranteed to survive a crash at the end of the operation.
    pub durability: String,
    /// What the backend parallelises, and under whose control.
    pub concurrency: String,
    /// For blob backends: how bytes reach the object store, and whether that
    /// is the backend's own protocol or a harness construction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Anything else a comparison must not ignore.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Everything one backend did in one repetition of one scenario.
#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    /// Scenario group: `tree`, `git`, `nix`, `backup` or `blob`.
    pub group: String,
    /// Scenario within the group.
    pub scenario: String,
    /// Backend identifier, e.g. `amber-rust`.
    pub backend: String,
    /// Zero-based repetition index.
    pub rep: usize,
    /// Position of this (scenario, backend) pair in the randomised execution
    /// order of its repetition.
    pub order_index: usize,
    pub semantics: Semantics,
    pub ops: Vec<Op>,
    /// Content identifiers the backend produced, keyed by snapshot
    /// name. For the two Amber cores these are root keys, and the
    /// runner compares them across the cores: byte-compatible
    /// addressing is a claim the benchmark is in a position to check.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub roots: BTreeMap<String, String>,
    pub verifications: Vec<Verification>,
    /// Set when the repetition aborted; the ops recorded before the abort are
    /// still present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RunRecord {
    /// True when nothing failed: no failed operation, no failed verification,
    /// no abort.
    pub fn healthy(&self) -> bool {
        self.error.is_none()
            && self.verifications.iter().all(|v| v.passed)
            && self.ops.iter().all(|o| o.status != OpStatus::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_never_carries_a_zero_timing() {
        let op = Op::unsupported("gc", "this backend has no garbage collector");
        assert_eq!(op.status, OpStatus::Unsupported);
        assert!(op.wall_ns.is_none());
        assert!(op.cpu_user_ns.is_none());
        assert!(op.logical_bytes.is_none());
        assert!(!op.is_comparable());
        let json = serde_json::to_string(&op).unwrap();
        assert!(!json.contains("wall_ns"), "{json}");
        assert!(json.contains("no garbage collector"), "{json}");
    }

    #[test]
    fn failed_operations_are_not_comparable_and_keep_their_error() {
        let op = Op::failed("ingest_fresh", "exited 2");
        assert!(!op.is_comparable());
        assert_eq!(op.reason.as_deref(), Some("exited 2"));
    }

    #[test]
    fn setup_and_verify_are_timed_but_never_compared() {
        let u = Usage {
            wall_ns: 5,
            user_ns: 1,
            sys_ns: 1,
            max_rss_bytes: 10,
        };
        for phase in [Phase::Setup, Phase::Verify] {
            let op = Op::aside("gen", phase, u);
            assert_eq!(op.wall_ns, Some(5));
            assert!(!op.is_comparable(), "{phase:?} must not be comparable");
        }
        assert!(Op::measured("x", u).is_comparable());
    }

    #[test]
    fn throughput_needs_both_bytes_and_time() {
        let u = Usage {
            wall_ns: 1_000_000_000,
            ..Usage::default()
        };
        assert!(Op::measured("x", u).throughput_bytes_per_s().is_none());
        let op = Op::measured("x", u).logical(1024);
        assert_eq!(op.throughput_bytes_per_s(), Some(1024.0));
        let zero = Op::measured("x", Usage::default()).logical(1024);
        assert!(zero.throughput_bytes_per_s().is_none());
    }

    #[test]
    fn a_record_with_a_failed_verification_is_not_healthy() {
        let mut r = RunRecord {
            group: "tree".into(),
            scenario: "mixed".into(),
            backend: "amber-rust".into(),
            rep: 0,
            order_index: 0,
            semantics: Semantics {
                compression: "per-record zstd".into(),
                encryption: "none".into(),
                durability: "fsync on seal".into(),
                concurrency: "jobs workers".into(),
                transport: None,
                notes: vec![],
            },
            ops: vec![],
            roots: BTreeMap::new(),
            verifications: vec![Verification::pass("a", "ok")],
            error: None,
        };
        assert!(r.healthy());
        r.verifications.push(Verification::fail("b", "mismatch"));
        assert!(!r.healthy());
    }
}
