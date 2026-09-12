//! The readable comparison.
//!
//! Organised by scenario, because that is the only level at which the numbers
//! mean the same thing. There is deliberately no overall score and no single
//! winner: a fast plaintext tree store and an encrypted backup repository are
//! not two entries in one league table, and a report that ranked them would
//! be lying about what it measured.

use std::collections::BTreeSet;

use crate::metrics::OpStatus;
use crate::stats::Summary;

use super::{Report, SummaryRow};

/// Renders the whole report.
pub fn render(r: &Report) -> String {
    let mut s = String::new();
    header(&mut s, r);
    how_to_read(&mut s, r);
    for scenario in scenarios(r) {
        scenario_section(&mut s, r, &scenario);
    }
    semantics_section(&mut s, r);
    unsupported_section(&mut s, r);
    verification_section(&mut s, r);
    error_section(&mut s, r);
    configuration_section(&mut s, r);
    s
}

fn scenarios(r: &Report) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for run in &r.runs {
        if seen.insert(run.scenario.clone()) {
            out.push(run.scenario.clone());
        }
    }
    out
}

fn header(s: &mut String, r: &Report) {
    s.push_str("# Amber-Store CAS comparison benchmarks\n\n");
    s.push_str(&format!(
        "Run `{}`, profile **{}**, seed `{}`, {} repetition(s), {} worker(s).\n\n",
        r.run_id, r.profile.name, r.seed, r.repeats, r.jobs
    ));
    s.push_str(&format!("{}\n\n", r.profile.description));
    if !r.clean() {
        s.push_str(
            "> ## ⚠ This run is NOT a valid comparison\n>\n\
             > Something failed, so the tables below are diagnostic output \
             rather than a result:\n>\n",
        );
        for reason in r.invalid_reasons() {
            s.push_str(&format!("> * {reason}\n"));
        }
        for check in r.cross_checks.iter().filter(|c| !c.passed) {
            s.push_str(&format!("> * `{}`: {}\n", check.name, check.detail));
        }
        s.push_str(
            ">\n> Repetitions that failed a correctness check contribute no \
             timings to any median below; their raw samples are kept in \
             `report.json` and `samples.csv` for diagnosis only. See \
             [Failures](#failures) and [Correctness checks](#correctness-checks).\n\n",
        );
    }
}

fn how_to_read(s: &mut String, r: &Report) {
    s.push_str("## How to read this\n\n");
    s.push_str(
        "* Every row is a **median over repetitions** of one operation by one \
         backend. The raw samples are in `samples.csv`; the dispersion is in \
         `summary.csv`.\n",
    );
    s.push_str(
        "* An empty cell means **not measured**. An operation a backend cannot \
         perform is listed under [Unsupported operations](#unsupported-operations) \
         with the reason, and is never shown as a zero.\n",
    );
    s.push_str(
        "* Comparisons are **within a scenario only**. Backends in different \
         scenarios publish different data and make different guarantees; see \
         [What each backend guarantees](#what-each-backend-guarantees) before \
         comparing anything across sections.\n",
    );
    s.push_str(&format!("* Cache policy: {}\n", r.host.cache_policy));
    s.push_str(&format!("* Statistics: {}\n", r.statistics_method));
    s.push_str(
        "* Setup (generating the corpus, materialising a working tree) and \
         verification (re-hashing a restored tree) are timed separately and \
         are never part of a measured number.\n",
    );
    s.push_str(
        "* Only repetitions in which **everything** succeeded contribute to a \
         median: a fast operation in a repetition whose restored bytes did \
         not match its manifest is not a measurement of anything. Such \
         samples are counted as `excluded_invalid_samples` in `summary.csv` \
         and kept in full in `samples.csv`.\n\n",
    );
    let excluded: usize = r.summary.iter().map(|x| x.excluded_invalid_samples).sum();
    if excluded > 0 {
        s.push_str(&format!(
            "**{excluded} successful timing(s) were excluded** from the \
             statistics below because another part of the same repetition \
             failed.\n\n"
        ));
    }
}

fn scenario_section(s: &mut String, r: &Report, scenario: &str) {
    let rows: Vec<&SummaryRow> = r
        .summary
        .iter()
        .filter(|x| x.scenario == scenario)
        .collect();
    if rows.is_empty() {
        return;
    }
    s.push_str(&format!("## Scenario `{scenario}`\n\n"));

    let backends = ordered(rows.iter().map(|x| x.backend.clone()));
    let ops = ordered(rows.iter().map(|x| x.op.clone()));

    section(
        s,
        "### Elapsed time (median, lower is better)\n\n",
        &ops,
        &backends,
        &rows,
        |row| row.wall_ns.as_ref().map(|x| duration(x.median)),
    );
    section(
        s,
        "\n### Throughput over the logical bytes processed (median)\n\n",
        &ops,
        &backends,
        &rows,
        |row| {
            row.throughput_bytes_per_s
                .as_ref()
                .map(|x| format!("{}/s", bytes(x.median)))
        },
    );
    section(
        s,
        "\n### CPU time, user + system (median)\n\n",
        &ops,
        &backends,
        &rows,
        |row| row.cpu_ns.as_ref().map(|x| duration(x.median)),
    );
    section(
        s,
        "\n### Peak resident set size (median)\n\n",
        &ops,
        &backends,
        &rows,
        |row| row.max_rss_bytes.as_ref().map(|x| bytes(x.median)),
    );
    section(
        s,
        "\n### Storage allocated after each operation (median)\n\n",
        &ops,
        &backends,
        &rows,
        |row| row.store_allocated_bytes.as_ref().map(|x| bytes(x.median)),
    );
    section(
        s,
        "\n### Space reclaimed (median; positive means released)\n\n",
        &ops,
        &backends,
        &rows,
        |row| row.reclaimed_bytes.as_ref().map(|x| bytes(x.median)),
    );
    s.push_str("\n### Dispersion of elapsed time\n\n");
    s.push_str("| backend | operation | n | median | p95 | min | max | stddev | cv |\n");
    s.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for row in &rows {
        let Some(w) = &row.wall_ns else { continue };
        s.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            row.backend,
            row.op,
            w.n,
            duration(w.median),
            duration(w.p95),
            duration(w.min),
            duration(w.max),
            w.stddev.map(duration).unwrap_or_else(|| "—".into()),
            w.cv.map(|c| format!("{:.1}%", c * 100.0))
                .unwrap_or_else(|| "—".into()),
        ));
    }

    notes_section(s, r, scenario);

    let counters: Vec<&SummaryRow> = rows
        .iter()
        .copied()
        .filter(|x| !x.counters_median.is_empty())
        .collect();
    if !counters.is_empty() {
        s.push_str("\n### Recorded counters (median per operation)\n\n");
        s.push_str("| backend | operation | counter | median |\n|---|---|---|---|\n");
        for row in counters {
            for (k, v) in &row.counters_median {
                s.push_str(&format!(
                    "| {} | `{}` | `{k}` | {} |\n",
                    row.backend,
                    row.op,
                    if k.contains("bytes") {
                        bytes(*v)
                    } else {
                        format!("{v:.0}")
                    }
                ));
            }
        }
    }
    s.push('\n');
}

/// The caveats attached to individual operations.
///
/// This is where a number's meaning usually lives: what a transfer actually
/// delivered and how many references that was, why a counter is absent for
/// one operation, what a retention step did and did not reclaim. A reader
/// comparing two cells in the tables above has to be able to see, in the
/// same section, whether the two cells describe the same delivered outcome —
/// so the notes are rendered per scenario rather than collected into a
/// footnote nobody reaches.
fn notes_section(s: &mut String, r: &Report, scenario: &str) {
    // Deduplicated: repetitions of the same (backend, operation) attach the
    // same note, and printing it once per repetition would bury it.
    let mut rows: BTreeSet<(String, String, String)> = BTreeSet::new();
    for run in r.runs.iter().filter(|x| x.scenario == scenario) {
        for op in &run.ops {
            if op.phase != crate::metrics::Phase::Measured {
                continue;
            }
            for note in &op.notes {
                rows.insert((run.backend.clone(), op.name.clone(), note.clone()));
            }
        }
    }
    if rows.is_empty() {
        return;
    }
    s.push_str("\n### What these numbers mean, operation by operation\n\n");
    s.push_str(
        "Read these before comparing two cells above: they state what each \
         measured operation actually delivered.\n\n",
    );
    s.push_str("| backend | operation | note |\n|---|---|---|\n");
    for (backend, op, note) in rows {
        s.push_str(&format!("| {backend} | `{op}` | {} |\n", cell_text(&note)));
    }
}

/// Makes arbitrary prose safe to put inside a Markdown table cell.
fn cell_text(note: &str) -> String {
    note.replace('|', "\\|")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Renders a comparison table, or nothing at all when no backend
/// produced the metric: a table of dashes tells a reader nothing and
/// invites them to read absence as zero.
fn section(
    s: &mut String,
    heading: &str,
    ops: &[String],
    backends: &[String],
    rows: &[&SummaryRow],
    cell: impl Fn(&SummaryRow) -> Option<String> + Copy,
) {
    if !rows.iter().any(|r| cell(r).is_some()) {
        return;
    }
    s.push_str(heading);
    table(s, ops, backends, rows, cell);
}

fn table(
    s: &mut String,
    ops: &[String],
    backends: &[String],
    rows: &[&SummaryRow],
    cell: impl Fn(&SummaryRow) -> Option<String>,
) {
    s.push_str("| operation |");
    for b in backends {
        s.push_str(&format!(" {b} |"));
    }
    s.push_str("\n|---|");
    for _ in backends {
        s.push_str("---|");
    }
    s.push('\n');
    for op in ops {
        s.push_str(&format!("| `{op}` |"));
        for b in backends {
            let value = rows
                .iter()
                .find(|r| &r.op == op && &r.backend == b)
                .map(|r| match cell(r) {
                    Some(v) => v,
                    None if r.unsupported > 0 => "unsupported".to_string(),
                    None if r.failed > 0 => "**failed**".to_string(),
                    None if r.excluded_invalid_samples > 0 => "**invalid**".to_string(),
                    None => "—".to_string(),
                })
                .unwrap_or_else(|| "—".to_string());
            s.push_str(&format!(" {value} |"));
        }
        s.push('\n');
    }
}

fn semantics_section(s: &mut String, r: &Report) {
    s.push_str("## What each backend guarantees\n\n");
    s.push_str(
        "These differences are the reason a single ranking would be \
         meaningless. Read them before comparing any two rows.\n\n",
    );
    let mut seen = BTreeSet::new();
    for run in &r.runs {
        if !seen.insert((run.scenario.clone(), run.backend.clone())) {
            continue;
        }
        s.push_str(&format!("### `{}` in `{}`\n\n", run.backend, run.scenario));
        s.push_str(&format!(
            "* **Compression:** {}\n",
            run.semantics.compression
        ));
        s.push_str(&format!("* **Encryption:** {}\n", run.semantics.encryption));
        s.push_str(&format!("* **Durability:** {}\n", run.semantics.durability));
        s.push_str(&format!(
            "* **Concurrency:** {}\n",
            run.semantics.concurrency
        ));
        if let Some(t) = &run.semantics.transport {
            s.push_str(&format!("* **Transport:** {t}\n"));
        }
        for note in &run.semantics.notes {
            s.push_str(&format!("* {note}\n"));
        }
        s.push('\n');
    }
}

fn unsupported_section(s: &mut String, r: &Report) {
    s.push_str("## Unsupported operations\n\n");
    if r.unsupported.is_empty() {
        s.push_str("Every backend performed every operation asked of it.\n\n");
        return;
    }
    s.push_str(
        "These are not zeroes and not omissions: the backend has no \
         equivalent of the operation.\n\n",
    );
    s.push_str("| scenario | backend | operation | why |\n|---|---|---|---|\n");
    for row in &r.unsupported {
        s.push_str(&format!(
            "| `{}` | {} | `{}` | {} |\n",
            row.scenario, row.backend, row.op, row.reason
        ));
    }
    s.push('\n');
}

fn verification_section(s: &mut String, r: &Report) {
    s.push_str("## Correctness checks\n\n");
    let mut names: BTreeSet<(String, String, String)> = BTreeSet::new();
    for run in &r.runs {
        for v in &run.verifications {
            names.insert((run.scenario.clone(), run.backend.clone(), v.name.clone()));
        }
    }
    if names.is_empty() {
        s.push_str("No correctness checks ran.\n\n");
        return;
    }
    s.push_str("| scenario | backend | check | result | detail |\n|---|---|---|---|---|\n");
    for (scenario, backend, name) in names {
        let results: Vec<&crate::metrics::Verification> = r
            .runs
            .iter()
            .filter(|x| x.scenario == scenario && x.backend == backend)
            .flat_map(|x| x.verifications.iter())
            .filter(|v| v.name == name)
            .collect();
        let failed = results.iter().filter(|v| !v.passed).count();
        let detail = results
            .iter()
            .find(|v| !v.passed)
            .or_else(|| results.first())
            .map(|v| v.detail.clone())
            .unwrap_or_default();
        s.push_str(&format!(
            "| `{scenario}` | {backend} | `{name}` | {} | {} |\n",
            if failed == 0 {
                format!("pass ({}/{})", results.len(), results.len())
            } else {
                format!("**FAIL ({failed}/{})**", results.len())
            },
            detail.replace('|', "\\|")
        ));
    }
    s.push('\n');
}

fn error_section(s: &mut String, r: &Report) {
    s.push_str("## Failures\n\n");
    if r.errors.is_empty() && r.skipped_backends.is_empty() {
        s.push_str("None.\n\n");
        return;
    }
    if !r.skipped_backends.is_empty() {
        s.push_str("### Backends that could not be run\n\n");
        s.push_str("| backend | why |\n|---|---|\n");
        for (backend, reason) in &r.skipped_backends {
            s.push_str(&format!("| {backend} | {} |\n", reason.replace('|', "\\|")));
        }
        s.push('\n');
    }
    if !r.errors.is_empty() {
        s.push_str(
            "| scenario | backend | rep | kind | name | detail |\n|---|---|---|---|---|---|\n",
        );
        for e in &r.errors {
            s.push_str(&format!(
                "| `{}` | {} | {} | {} | `{}` | {} |\n",
                e.scenario,
                e.backend,
                e.rep,
                e.kind,
                e.name,
                first_line(&e.detail).replace('|', "\\|")
            ));
        }
        s.push('\n');
    }
}

fn configuration_section(s: &mut String, r: &Report) {
    s.push_str("## Configuration and environment\n\n");
    s.push_str(&format!("* Command line: `{}`\n", r.command_line));
    s.push_str(&format!(
        "* Harness: version {}{}{}\n",
        r.harness_version,
        r.harness_commit
            .as_ref()
            .map(|c| format!(", commit `{c}`"))
            .unwrap_or_default(),
        if r.harness_dirty {
            " (working tree dirty)"
        } else {
            ""
        }
    ));
    s.push_str(&format!(
        "* Chunking, identical for both Amber cores: min {}, average {}, max {}, \
         item-bits {}, xattr-inline-max {}\n",
        bytes(r.profile.chunk.min as f64),
        bytes(r.profile.chunk.avg as f64),
        bytes(r.profile.chunk.max as f64),
        r.profile.chunk.item_bits,
        r.profile.chunk.xattr_inline_max,
    ));
    s.push_str(&format!(
        "* Pack segment size, identical for both Amber cores: {} (the cores \
         default to 2 GiB; a smaller, explicit value is used here because a \
         segment is the unit collection reaps, and a profile that stores less \
         than one segment could never reclaim anything)\n",
        bytes(r.profile.segment_size as f64)
    ));
    match r.corpus.generations.first() {
        Some(g) => s.push_str(&format!(
            "* Corpus: seed {}, {} generation(s), {} logical bytes in generation 0, \
             manifest digest `{}`\n",
            r.corpus.seed,
            r.corpus.generations.len(),
            bytes(g.logical_bytes as f64),
            g.manifest_digest,
        )),
        None => s.push_str(&format!(
            "* Corpus: seed {}, no generation was materialised\n",
            r.corpus.seed
        )),
    }
    s.push_str(&format!(
        "* Source history: {} commits on {} branches with {} tags\n",
        r.history.commits,
        r.history.branches.len(),
        r.history.tags.len()
    ));
    if let Some(n) = &r.nix_fixture {
        s.push_str(&format!(
            "* Nix closures: {}{}. Generation 1 closure {} paths, generation 2 \
             {} paths, {} shared\n",
            n.source,
            n.flake_output
                .as_ref()
                .map(|f| format!(" (flake output `{f}`)"))
                .unwrap_or_default(),
            n.gen1_closure_paths,
            n.gen2_closure_paths,
            n.shared_paths
        ));
        if !n.has_second_generation {
            s.push_str(
                "* There is **no second Nix generation**: every operation that \
                 needs a changed closure is recorded as unsupported rather \
                 than measured against a repeat of the same one.\n",
            );
        }
    }
    if let Some(b) = &r.blob {
        s.push_str(&format!("* Object store: {}\n", b.service));
        s.push_str(&format!("* Gateway: {}\n", b.measurement));
        s.push_str(&format!("* Network shaping: {}\n", b.shaping));
        s.push_str(&format!(
            "* Bucket `{}`, prefix `{}`, region `{}`, remote: {}. {}\n",
            b.target.bucket,
            b.target.prefix,
            b.target.region,
            b.target.remote,
            b.target.credentials
        ));
    }
    s.push_str(&format!(
        "* Execution order was randomised per repetition with seed `{}`; the \
         exact order is in `report.json` under `execution_order`.\n",
        r.order_seed
    ));
    s.push_str(&format!(
        "* Host: {} {} on {}, {} logical core(s){}\n",
        r.host.os,
        r.host.arch,
        r.host.cpu_model,
        r.host.cpu_logical_cores,
        r.host
            .memory_total_bytes
            .map(|m| format!(", {} of memory", bytes(m as f64)))
            .unwrap_or_default()
    ));
    s.push_str(&format!("* Kernel: {}\n", r.host.kernel));
    s.push_str(&format!(
        "* Load average when the run started: {}\n",
        r.host
            .load_average_at_start
            .map(|(a, b, c)| format!("{a:.2} {b:.2} {c:.2} (1, 5, 15 minutes)"))
            .unwrap_or_else(|| "not available on this system".into())
    ));
    s.push_str(&format!(
        "* Scratch: `{}`, {} free at the start\n\n",
        r.host.scratch_path,
        bytes(r.host.scratch_free_bytes_at_start as f64)
    ));

    s.push_str("### Executables measured\n\n");
    s.push_str("| tool | version | path | sha256 | source commit |\n|---|---|---|---|---|\n");
    for (name, t) in &r.tools.tools {
        s.push_str(&format!(
            "| {name} | {} | `{}` | `{}` | {} |\n",
            t.version.replace('|', "\\|"),
            t.path,
            &t.sha256[..t.sha256.len().min(16)],
            t.source_commit
                .as_ref()
                .map(|c| format!(
                    "`{}`{}",
                    &c[..c.len().min(12)],
                    if t.source_dirty { " (dirty)" } else { "" }
                ))
                .unwrap_or_else(|| "—".into())
        ));
    }
    if !r.tools.missing.is_empty() {
        s.push_str("\n### Tools that could not be resolved\n\n");
        s.push_str("| tool | why |\n|---|---|\n");
        for (name, why) in &r.tools.missing {
            s.push_str(&format!("| {name} | {} |\n", why.replace('|', "\\|")));
        }
    }
    s.push('\n');
}

fn ordered(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for v in values {
        if seen.insert(v.clone()) {
            out.push(v);
        }
    }
    out
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// Renders nanoseconds in whichever unit keeps three significant figures.
pub fn duration(ns: f64) -> String {
    if ns < 1_000.0 {
        format!("{ns:.0} ns")
    } else if ns < 1_000_000.0 {
        format!("{:.1} µs", ns / 1_000.0)
    } else if ns < 1_000_000_000.0 {
        format!("{:.1} ms", ns / 1_000_000.0)
    } else if ns < 60_000_000_000.0 {
        format!("{:.2} s", ns / 1_000_000_000.0)
    } else {
        format!("{:.1} min", ns / 60_000_000_000.0)
    }
}

/// Renders a byte count in binary units.
pub fn bytes(b: f64) -> String {
    let negative = b < 0.0;
    let v = b.abs();
    let (value, unit) = if v < 1024.0 {
        (v, "B")
    } else if v < 1024.0 * 1024.0 {
        (v / 1024.0, "KiB")
    } else if v < 1024.0 * 1024.0 * 1024.0 {
        (v / (1024.0 * 1024.0), "MiB")
    } else {
        (v / (1024.0 * 1024.0 * 1024.0), "GiB")
    };
    let rendered = if unit == "B" {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.2} {unit}")
    };
    if negative {
        format!("-{rendered}")
    } else {
        rendered
    }
}

/// The dispersion of a summary, as the Markdown renders it.
pub fn dispersion(s: &Summary) -> String {
    match (s.stddev, s.cv) {
        (Some(sd), Some(cv)) => format!("{} ({:.1}%)", duration(sd), cv * 100.0),
        _ => "single sample".into(),
    }
}

/// True when the operation produced nothing comparable.
pub fn blank(row: &SummaryRow) -> bool {
    row.ok == 0
}

/// Whether a status should be shown as a failure in a table cell.
pub fn is_failure(status: &OpStatus) -> bool {
    matches!(status, OpStatus::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_pick_a_readable_unit() {
        assert_eq!(duration(500.0), "500 ns");
        assert_eq!(duration(1_500.0), "1.5 µs");
        assert_eq!(duration(2_500_000.0), "2.5 ms");
        assert_eq!(duration(2_500_000_000.0), "2.50 s");
        assert_eq!(duration(120_000_000_000.0), "2.0 min");
    }

    #[test]
    fn byte_counts_are_binary_and_signed() {
        assert_eq!(bytes(512.0), "512 B");
        assert_eq!(bytes(1536.0), "1.50 KiB");
        assert_eq!(bytes(1024.0 * 1024.0 * 3.0), "3.00 MiB");
        assert_eq!(bytes(-1024.0), "-1.00 KiB");
    }

    #[test]
    fn a_cell_with_no_sample_says_which_kind_of_nothing_it_is() {
        let base = SummaryRow {
            group: "g".into(),
            scenario: "s".into(),
            backend: "b".into(),
            op: "o".into(),
            ok: 0,
            excluded_invalid_samples: 0,
            unsupported: 1,
            failed: 0,
            reason: Some("no protocol".into()),
            wall_ns: None,
            cpu_ns: None,
            max_rss_bytes: None,
            throughput_bytes_per_s: None,
            logical_bytes: None,
            store_allocated_bytes: None,
            store_apparent_bytes: None,
            reclaimed_bytes: None,
            counters_median: Default::default(),
        };
        let mut s = String::new();
        table(
            &mut s,
            &["o".to_string()],
            &["b".to_string()],
            &[&base],
            |r| r.wall_ns.as_ref().map(|x| duration(x.median)),
        );
        assert!(s.contains("unsupported"), "{s}");

        let failed = SummaryRow {
            unsupported: 0,
            failed: 1,
            ..base.clone()
        };
        let mut s = String::new();
        table(
            &mut s,
            &["o".to_string()],
            &["b".to_string()],
            &[&failed],
            |r| r.wall_ns.as_ref().map(|x| duration(x.median)),
        );
        assert!(s.contains("**failed**"), "{s}");
        assert!(blank(&failed));
    }

    #[test]
    fn dispersion_admits_when_there_is_only_one_sample() {
        let one = crate::stats::summarize(&[5.0], "ns").unwrap();
        assert_eq!(dispersion(&one), "single sample");
        let many = crate::stats::summarize(&[5.0, 7.0, 9.0], "ns").unwrap();
        assert!(dispersion(&many).contains('%'));
    }

    #[test]
    fn failure_detection_matches_the_status() {
        assert!(is_failure(&OpStatus::Failed));
        assert!(!is_failure(&OpStatus::Unsupported));
    }
}
