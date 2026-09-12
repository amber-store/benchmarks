//! Writing the results out.
//!
//! Three artefacts, from one structure: `report.json` carries everything,
//! including every raw sample and every recorded failure; `samples.csv`,
//! `summary.csv` and `counters.csv` carry the same numbers in a form a
//! spreadsheet can read; and `REPORT.md` is the readable comparison. The
//! Markdown never says more than the JSON does, and neither of them invents a
//! number for an operation a backend cannot perform.

pub mod csv;
pub mod markdown;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::blob::s3::TargetInfo;
use crate::config::Profile;
use crate::dataset::corpus::Corpus;
use crate::dataset::gitgen::HistorySummary;
use crate::hostinfo::HostInfo;
use crate::metrics::{OpStatus, RunRecord};
use crate::stats::{self, Summary};
use crate::toolbox::Toolbox;

/// Bumped whenever the shape of `report.json` changes.
pub const SCHEMA_VERSION: u32 = 1;

/// What the Nix workload's closures turned out to be.
#[derive(Debug, Clone, Serialize)]
pub struct NixFixtureInfo {
    /// Where the closures came from, in words: the built fixture, or the
    /// store paths the run was pointed at.
    pub source: String,
    /// The `nix-fixtures-*` output that was built, absent when the closures
    /// were supplied instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flake_output: Option<String>,
    pub root: String,
    pub gen1: String,
    pub gen2: String,
    pub gen1_closure_paths: usize,
    pub gen2_closure_paths: usize,
    pub shared_paths: usize,
    /// False when only one closure was supplied, in which case every
    /// operation that needs a changed generation is unsupported rather than
    /// invented.
    pub has_second_generation: bool,
}

/// The object-storage setup, with no credentials in it.
#[derive(Debug, Clone, Serialize)]
pub struct BlobInfo {
    pub service: String,
    pub gateway_endpoint: String,
    pub shaping: String,
    pub target: TargetInfo,
    /// Exactly what the gateway does and does not observe.
    pub measurement: String,
}

/// One entry of the randomised execution order.
#[derive(Debug, Clone, Serialize)]
pub struct OrderEntry {
    pub rep: usize,
    pub index: usize,
    pub scenario: String,
    pub backend: String,
}

/// One aggregated comparison row.
#[derive(Debug, Clone, Serialize)]
pub struct SummaryRow {
    pub group: String,
    pub scenario: String,
    pub backend: String,
    pub op: String,
    /// Repetitions that produced a usable sample: the operation succeeded
    /// *and* every correctness check of its repetition passed.
    pub ok: usize,
    /// Successful timings that were deliberately left out of the statistics
    /// below because something else in the same repetition failed. The raw
    /// samples are still in `report.json` and `samples.csv`.
    pub excluded_invalid_samples: usize,
    /// Repetitions where the backend could not perform the operation.
    pub unsupported: usize,
    /// Repetitions where it tried and failed.
    pub failed: usize,
    /// Why, when it could not or did not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_ns: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_ns: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rss_bytes: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_bytes_per_s: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_bytes: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_allocated_bytes: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_apparent_bytes: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reclaimed_bytes: Option<Summary>,
    /// Median of every integer counter the operation recorded.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub counters_median: BTreeMap<String, f64>,
}

/// An operation a backend cannot perform, and why.
#[derive(Debug, Clone, Serialize)]
pub struct UnsupportedRow {
    pub scenario: String,
    pub backend: String,
    pub op: String,
    pub reason: String,
}

/// Something that went wrong.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorRow {
    pub scenario: String,
    pub backend: String,
    pub rep: usize,
    /// `operation`, `verification` or `repetition`.
    pub kind: String,
    pub name: String,
    pub detail: String,
}

/// Whether this run's aggregated comparison may be read as a result, and
/// why not when it may not.
///
/// It defaults to *invalid*: a report that was never finished cannot claim
/// otherwise.
#[derive(Debug, Clone, Serialize)]
pub struct Validity {
    pub valid: bool,
    pub reasons: Vec<String>,
}

impl Default for Validity {
    fn default() -> Validity {
        Validity {
            valid: false,
            reasons: vec!["validity was never computed for this report".into()],
        }
    }
}

/// The whole result of one run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: u32,
    /// Computed when the report is written; see [`Validity`].
    pub validity: Validity,
    pub run_id: String,
    pub harness_version: String,
    /// Commit of the repository this harness was built from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness_commit: Option<String>,
    pub harness_dirty: bool,
    pub started_unix_nanos: u128,
    pub finished_unix_nanos: u128,
    pub command_line: String,
    pub profile: Profile,
    pub seed: u64,
    /// Seed used to randomise the backend order; recorded so the order can be
    /// reproduced exactly.
    pub order_seed: u64,
    pub jobs: usize,
    pub repeats: usize,
    pub host: HostInfo,
    pub tools: Toolbox,
    pub corpus: Corpus,
    pub history: HistorySummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nix_fixture: Option<NixFixtureInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobInfo>,
    pub statistics_method: String,
    pub execution_order: Vec<OrderEntry>,
    pub runs: Vec<RunRecord>,
    pub summary: Vec<SummaryRow>,
    pub unsupported: Vec<UnsupportedRow>,
    /// Checks the harness performs across backends rather than within
    /// one, such as comparing the two Amber cores' root keys.
    pub cross_checks: Vec<crate::metrics::Verification>,
    pub errors: Vec<ErrorRow>,
    /// Backends that were requested but could not be run at all, with the
    /// reason. Never empty just because a tool was missing: a missing tool is
    /// an error, not a silent omission.
    pub skipped_backends: BTreeMap<String, String>,
}

impl Report {
    /// True when nothing failed anywhere: no failed operation, no failed
    /// correctness check inside a repetition, no failed check *across*
    /// backends, and no requested backend left unrun.
    ///
    /// The cross-backend checks are part of this. A run where the two Amber
    /// cores disagreed about a root key has discovered something important
    /// and must not exit zero just because every command happened to
    /// succeed.
    pub fn clean(&self) -> bool {
        self.errors.is_empty()
            && self.skipped_backends.is_empty()
            && self.cross_checks.iter().all(|c| c.passed)
    }

    /// Whether the aggregated comparison may be read as a valid result.
    ///
    /// This is the same condition as [`Report::clean`], published as a field
    /// of every machine-readable artefact so an invalid run cannot produce a
    /// summary that looks valid.
    pub fn summary_valid(&self) -> bool {
        self.clean()
    }

    /// Why the comparison is invalid, when it is.
    pub fn invalid_reasons(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.errors.is_empty() {
            out.push(format!("{} recorded failure(s)", self.errors.len()));
        }
        if !self.skipped_backends.is_empty() {
            out.push(format!(
                "{} requested backend(s) could not be run at all",
                self.skipped_backends.len()
            ));
        }
        let failed_cross = self.cross_checks.iter().filter(|c| !c.passed).count();
        if failed_cross > 0 {
            out.push(format!(
                "{failed_cross} cross-backend check(s) failed: {}",
                self.cross_checks
                    .iter()
                    .filter(|c| !c.passed)
                    .map(|c| c.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        out
    }

    /// Writes every artefact into `dir`.
    ///
    /// The validity verdict is recomputed here rather than stored earlier, so
    /// what the artefacts say about themselves cannot drift from what they
    /// contain.
    pub fn write_all(&mut self, dir: &Path) -> std::io::Result<()> {
        self.validity = Validity {
            valid: self.clean(),
            reasons: self.invalid_reasons(),
        };
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::util::fsx::write_file(&dir.join("report.json"), &json)?;
        crate::util::fsx::write_file(&dir.join("samples.csv"), csv::samples(self).as_bytes())?;
        crate::util::fsx::write_file(&dir.join("summary.csv"), csv::summary(self).as_bytes())?;
        crate::util::fsx::write_file(&dir.join("counters.csv"), csv::counters(self).as_bytes())?;
        crate::util::fsx::write_file(
            &dir.join("verifications.csv"),
            csv::verifications(self).as_bytes(),
        )?;
        crate::util::fsx::write_file(&dir.join("REPORT.md"), markdown::render(self).as_bytes())?;
        Ok(())
    }
}

/// Builds the aggregated comparison rows from the raw records.
///
/// Two rules decide what reaches a comparison:
///
/// * A row is keyed by *group, scenario, backend and operation* — all four,
///   so two scenarios that happen to share an operation name are never
///   merged.
/// * Only repetitions that were healthy — no failed operation, no failed
///   correctness check, no abort — contribute samples. A repetition whose
///   restored tree did not match its manifest may well have produced a fast
///   `ingest_fresh` timing, and that timing is not a measurement of anything
///   anyone wants: it is preserved as a raw sample and counted in
///   `excluded_invalid_samples`, but it never becomes a median.
pub fn summarize(runs: &[RunRecord]) -> Vec<SummaryRow> {
    let mut keys: Vec<(String, String, String, String)> = Vec::new();
    let mut seen = BTreeSet::new();
    for r in runs {
        for op in &r.ops {
            if op.phase != crate::metrics::Phase::Measured {
                continue;
            }
            let key = (
                r.group.clone(),
                r.scenario.clone(),
                r.backend.clone(),
                op.name.clone(),
            );
            if seen.insert(key.clone()) {
                keys.push(key);
            }
        }
    }
    keys.into_iter()
        .map(|(group, scenario, backend, op_name)| {
            let matching =
                |r: &&RunRecord| r.group == group && r.scenario == scenario && r.backend == backend;
            // Every sample, for the diagnostic counts.
            let all: Vec<&crate::metrics::Op> = runs
                .iter()
                .filter(matching)
                .flat_map(|r| r.ops.iter())
                .filter(|o| o.name == op_name && o.phase == crate::metrics::Phase::Measured)
                .collect();
            // Only the samples a comparison may use.
            let ops: Vec<&crate::metrics::Op> = runs
                .iter()
                .filter(|r| matching(r) && r.healthy())
                .flat_map(|r| r.ops.iter())
                .filter(|o| o.name == op_name && o.phase == crate::metrics::Phase::Measured)
                .collect();
            let pick = |f: fn(&crate::metrics::Op) -> Option<f64>| -> Vec<f64> {
                ops.iter().filter_map(|o| f(o)).collect()
            };
            let mut counters: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            for o in ops.iter().filter(|o| o.status == OpStatus::Ok) {
                for (k, v) in &o.counters {
                    counters.entry(k.clone()).or_default().push(*v as f64);
                }
            }
            let usable = ops.iter().filter(|o| o.status == OpStatus::Ok).count();
            SummaryRow {
                group,
                scenario,
                backend,
                op: op_name,
                ok: usable,
                excluded_invalid_samples: all.iter().filter(|o| o.status == OpStatus::Ok).count()
                    - usable,
                unsupported: all
                    .iter()
                    .filter(|o| o.status == OpStatus::Unsupported)
                    .count(),
                failed: all.iter().filter(|o| o.status == OpStatus::Failed).count(),
                reason: all
                    .iter()
                    .find(|o| o.status != OpStatus::Ok)
                    .and_then(|o| o.reason.clone()),
                wall_ns: stats::summarize(&pick(|o| o.wall_ns.map(|v| v as f64)), "ns"),
                cpu_ns: stats::summarize(
                    &pick(|o| match (o.cpu_user_ns, o.cpu_sys_ns) {
                        (Some(u), Some(s)) => Some((u + s) as f64),
                        _ => None,
                    }),
                    "ns",
                ),
                max_rss_bytes: stats::summarize(
                    &pick(|o| o.max_rss_bytes.map(|v| v as f64)),
                    "bytes",
                ),
                throughput_bytes_per_s: stats::summarize(
                    &pick(|o| o.throughput_bytes_per_s()),
                    "bytes/s",
                ),
                logical_bytes: stats::summarize(
                    &pick(|o| o.logical_bytes.map(|v| v as f64)),
                    "bytes",
                ),
                store_allocated_bytes: stats::summarize(
                    &pick(|o| o.store_after.map(|s| s.allocated_bytes as f64)),
                    "bytes",
                ),
                store_apparent_bytes: stats::summarize(
                    &pick(|o| o.store_after.map(|s| s.apparent_bytes as f64)),
                    "bytes",
                ),
                reclaimed_bytes: stats::summarize(
                    &pick(|o| o.reclaimed_bytes.map(|v| v as f64)),
                    "bytes",
                ),
                counters_median: counters
                    .into_iter()
                    .filter_map(|(k, v)| stats::summarize(&v, "count").map(|s| (k, s.median)))
                    .collect(),
            }
        })
        .collect()
}

/// Collects every unsupported operation, deduplicated.
pub fn unsupported(runs: &[RunRecord]) -> Vec<UnsupportedRow> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for r in runs {
        for op in &r.ops {
            if op.status != OpStatus::Unsupported {
                continue;
            }
            let key = (r.scenario.clone(), r.backend.clone(), op.name.clone());
            if seen.insert(key) {
                out.push(UnsupportedRow {
                    scenario: r.scenario.clone(),
                    backend: r.backend.clone(),
                    op: op.name.clone(),
                    reason: op.reason.clone().unwrap_or_default(),
                });
            }
        }
    }
    out
}

/// Collects every failure: failed operations, failed verifications and
/// aborted repetitions.
pub fn errors(runs: &[RunRecord]) -> Vec<ErrorRow> {
    let mut out = Vec::new();
    for r in runs {
        if let Some(e) = &r.error {
            out.push(ErrorRow {
                scenario: r.scenario.clone(),
                backend: r.backend.clone(),
                rep: r.rep,
                kind: "repetition".into(),
                name: "aborted".into(),
                detail: e.clone(),
            });
        }
        for op in &r.ops {
            if op.status == OpStatus::Failed {
                out.push(ErrorRow {
                    scenario: r.scenario.clone(),
                    backend: r.backend.clone(),
                    rep: r.rep,
                    kind: "operation".into(),
                    name: op.name.clone(),
                    detail: op.reason.clone().unwrap_or_default(),
                });
            }
        }
        for v in &r.verifications {
            if !v.passed {
                let mut detail = v.detail.clone();
                for extra in v.missing.iter().chain(&v.extra).chain(&v.corrupt).take(5) {
                    detail.push_str("; ");
                    detail.push_str(extra);
                }
                out.push(ErrorRow {
                    scenario: r.scenario.clone(),
                    backend: r.backend.clone(),
                    rep: r.rep,
                    kind: "verification".into(),
                    name: v.name.clone(),
                    detail,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{Op, Semantics, Verification};
    use crate::util::proc::Usage;

    fn record(
        backend: &str,
        rep: usize,
        ops: Vec<Op>,
        verifications: Vec<Verification>,
    ) -> RunRecord {
        RunRecord {
            group: "tree".into(),
            scenario: "tree/lifecycle".into(),
            backend: backend.into(),
            rep,
            order_index: 0,
            semantics: Semantics {
                compression: "x".into(),
                encryption: "x".into(),
                durability: "x".into(),
                concurrency: "x".into(),
                transport: None,
                notes: vec![],
            },
            ops,
            roots: BTreeMap::new(),
            verifications,
            error: None,
        }
    }

    fn timed(name: &str, wall: u64) -> Op {
        Op::measured(
            name,
            Usage {
                wall_ns: wall,
                user_ns: wall / 2,
                sys_ns: wall / 4,
                max_rss_bytes: 1024,
            },
        )
        .logical(1_000_000)
    }

    #[test]
    fn summaries_are_grouped_by_scenario_backend_and_operation() {
        let runs = vec![
            record("a", 0, vec![timed("ingest", 100)], vec![]),
            record("a", 1, vec![timed("ingest", 300)], vec![]),
            record("b", 0, vec![timed("ingest", 200)], vec![]),
        ];
        let rows = summarize(&runs);
        assert_eq!(rows.len(), 2);
        let a = rows.iter().find(|r| r.backend == "a").unwrap();
        assert_eq!(a.ok, 2);
        assert_eq!(a.wall_ns.as_ref().unwrap().n, 2);
        assert_eq!(a.wall_ns.as_ref().unwrap().median, 100.0);
        assert_eq!(a.cpu_ns.as_ref().unwrap().median, 75.0);
        assert!(a.throughput_bytes_per_s.is_some());
    }

    #[test]
    fn an_unsupported_operation_has_no_timing_summary_but_keeps_its_reason() {
        let runs = vec![record(
            "a",
            0,
            vec![Op::unsupported("clone", "no protocol")],
            vec![],
        )];
        let rows = summarize(&runs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ok, 0);
        assert_eq!(rows[0].unsupported, 1);
        assert!(rows[0].wall_ns.is_none(), "no timing may be invented");
        assert_eq!(rows[0].reason.as_deref(), Some("no protocol"));

        let rows = unsupported(&runs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].reason, "no protocol");
    }

    #[test]
    fn a_failed_operation_is_counted_and_reported_not_averaged_in() {
        let runs = vec![
            record("a", 0, vec![timed("ingest", 100)], vec![]),
            record("a", 1, vec![Op::failed("ingest", "exit 2")], vec![]),
        ];
        let rows = summarize(&runs);
        assert_eq!(rows[0].ok, 1);
        assert_eq!(rows[0].failed, 1);
        assert_eq!(rows[0].wall_ns.as_ref().unwrap().n, 1);
        let errs = errors(&runs);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].kind, "operation");
        assert_eq!(errs[0].detail, "exit 2");
    }

    #[test]
    fn a_failed_verification_becomes_an_error_row_with_its_examples() {
        let mut v = Verification::fail("restore", "3 differing");
        v.corrupt = vec!["a: sha256".into(), "b: size".into()];
        let runs = vec![record("a", 0, vec![], vec![v])];
        let errs = errors(&runs);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].kind, "verification");
        assert!(errs[0].detail.contains("a: sha256"), "{}", errs[0].detail);
    }

    /// A report with nothing in it but the fields every report has, so the
    /// validity rules can be tested without running a benchmark.
    fn empty_report(runs: Vec<RunRecord>, cross: Vec<Verification>) -> Report {
        let profile = crate::config::profile("smoke").unwrap();
        let history = crate::dataset::gitgen::plan(&profile.git, 1).summary();
        Report {
            schema_version: SCHEMA_VERSION,
            validity: Default::default(),
            run_id: "test".into(),
            harness_version: "0".into(),
            harness_commit: None,
            harness_dirty: false,
            started_unix_nanos: 0,
            finished_unix_nanos: 1,
            command_line: "amber-cas-bench run".into(),
            corpus: crate::dataset::corpus::Corpus {
                root: std::path::PathBuf::from("/scratch/corpus"),
                seed: 1,
                spec: profile.corpus,
                generations: vec![],
            },
            profile,
            seed: 1,
            order_seed: 2,
            jobs: 1,
            repeats: runs.len().max(1),
            host: crate::hostinfo::HostInfo::collect(std::path::Path::new("."), false),
            tools: Default::default(),
            history,
            nix_fixture: None,
            blob: None,
            statistics_method: crate::stats::METHOD.into(),
            execution_order: vec![],
            summary: summarize(&runs),
            unsupported: unsupported(&runs),
            errors: errors(&runs),
            runs,
            cross_checks: cross,
            skipped_backends: BTreeMap::new(),
        }
    }

    #[test]
    fn a_failed_cross_backend_check_makes_the_whole_report_invalid() {
        // Every command succeeded and every per-backend check passed; what
        // failed is the comparison *between* the two cores.
        let runs = vec![
            record("amber-rust", 0, vec![timed("ingest", 10)], vec![]),
            record("amber-go", 0, vec![timed("ingest", 11)], vec![]),
        ];
        let mut failed = Verification::fail(
            "amber_cores_agree_on_root_keys",
            "1 of 1 trees produced different root keys in the two cores",
        );
        failed.corrupt = vec!["tree/lifecycle rep0 gen0: rust 2204 != go ffff".into()];

        let mut report = empty_report(runs.clone(), vec![failed]);
        assert!(report.errors.is_empty(), "no operation failed");
        assert!(!report.clean(), "a failed cross-check must not be clean");
        assert!(!report.summary_valid());
        let why = report.invalid_reasons().join("; ");
        assert!(why.contains("amber_cores_agree_on_root_keys"), "{why}");

        let dir = tempfile::tempdir().unwrap();
        report.write_all(dir.path()).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("report.json")).unwrap())
                .unwrap();
        assert_eq!(json["validity"]["valid"], serde_json::Value::Bool(false));
        let md = std::fs::read_to_string(dir.path().join("REPORT.md")).unwrap();
        assert!(md.contains("NOT a valid comparison"), "{md}");
        assert!(md.contains("amber_cores_agree_on_root_keys"), "{md}");
        let csv = std::fs::read_to_string(dir.path().join("summary.csv")).unwrap();
        assert!(csv.lines().next().unwrap().contains("run_valid"));
        assert!(
            csv.lines().nth(1).unwrap().contains("false"),
            "every summary row must carry the invalid verdict: {csv}"
        );

        // And with the cores agreeing, the same run is valid.
        let mut ok = empty_report(
            runs,
            vec![Verification::pass(
                "amber_cores_agree_on_root_keys",
                "1 tree agreed",
            )],
        );
        assert!(ok.clean());
        ok.write_all(dir.path().join("second").as_path()).unwrap();
        let md = std::fs::read_to_string(dir.path().join("second/REPORT.md")).unwrap();
        assert!(!md.contains("NOT a valid comparison"), "{md}");
    }

    #[test]
    fn an_unrunnable_backend_also_invalidates_the_comparison() {
        let mut report = empty_report(
            vec![record("a", 0, vec![timed("ingest", 1)], vec![])],
            vec![],
        );
        assert!(report.clean());
        report
            .skipped_backends
            .insert("restic".into(), "restic: not found on PATH".into());
        assert!(!report.clean());
        assert!(
            report
                .invalid_reasons()
                .iter()
                .any(|r| r.contains("backend"))
        );
    }

    #[test]
    fn an_aborted_repetition_is_reported() {
        let mut r = record("a", 0, vec![], vec![]);
        r.error = Some("the store vanished".into());
        let errs = errors(&[r]);
        assert_eq!(errs[0].kind, "repetition");
    }

    #[test]
    fn two_groups_that_share_an_operation_name_are_never_merged() {
        let mut tree = record("amber-rust", 0, vec![timed("ingest_fresh", 100)], vec![]);
        tree.group = "tree".into();
        tree.scenario = "tree/lifecycle".into();
        let mut blob = record("amber-rust", 0, vec![timed("ingest_fresh", 900)], vec![]);
        blob.group = "blob".into();
        // Deliberately the same scenario *name* in a different group, which is
        // what a filter that ignores the group would collapse.
        blob.scenario = "tree/lifecycle".into();

        let rows = summarize(&[tree, blob]);
        assert_eq!(rows.len(), 2, "{rows:#?}");
        let t = rows.iter().find(|r| r.group == "tree").unwrap();
        let b = rows.iter().find(|r| r.group == "blob").unwrap();
        assert_eq!(t.wall_ns.as_ref().unwrap().n, 1);
        assert_eq!(t.wall_ns.as_ref().unwrap().median, 100.0);
        assert_eq!(b.wall_ns.as_ref().unwrap().median, 900.0);
    }

    #[test]
    fn a_repetition_with_a_failed_check_contributes_no_timing_to_the_comparison() {
        let good = record(
            "a",
            0,
            vec![timed("ingest", 100)],
            vec![Verification::pass("v", "ok")],
        );
        let bad = record(
            "a",
            1,
            // The command succeeded; what it produced was wrong.
            vec![timed("ingest", 1)],
            vec![Verification::fail("v", "the restored tree did not match")],
        );
        let rows = summarize(&[good, bad]);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.ok, 1, "only the healthy repetition is usable");
        assert_eq!(row.excluded_invalid_samples, 1);
        let w = row.wall_ns.as_ref().unwrap();
        assert_eq!(w.n, 1);
        assert_eq!(
            w.median, 100.0,
            "the invalid repetition must not set the median"
        );
        assert_eq!(w.min, 100.0);
    }

    #[test]
    fn an_operation_whose_every_repetition_was_invalid_still_appears_but_has_no_statistics() {
        let bad = record(
            "a",
            0,
            vec![timed("ingest", 5)],
            vec![Verification::fail("v", "mismatch")],
        );
        let rows = summarize(&[bad]);
        assert_eq!(rows.len(), 1, "the operation must not disappear");
        assert_eq!(rows[0].ok, 0);
        assert_eq!(rows[0].excluded_invalid_samples, 1);
        assert!(rows[0].wall_ns.is_none());
    }

    #[test]
    fn setup_and_verification_operations_never_reach_the_summary() {
        let runs = vec![record(
            "a",
            0,
            vec![
                Op::aside("gen", crate::metrics::Phase::Setup, Usage::default()),
                Op::aside("check", crate::metrics::Phase::Verify, Usage::default()),
                timed("ingest", 10),
            ],
            vec![],
        )];
        let rows = summarize(&runs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].op, "ingest");
    }
}
