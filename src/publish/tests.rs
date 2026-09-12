//! Tests for publishing a recorded run.
//!
//! The fixtures here are written as JSON rather than built from
//! `report::Report`, because that is what publishing actually reads: a file
//! on disk that some build of the harness wrote.

use super::*;

/// A minimal but realistic `report.json`.
fn report_json(valid: bool, extra_summary: &str) -> String {
    let reasons = if valid {
        "[]".to_string()
    } else {
        "[\"1 recorded failure(s)\"]".to_string()
    };
    format!(
        r#"{{
  "schema_version": {schema},
  "validity": {{ "valid": {valid}, "reasons": {reasons} }},
  "run_id": "smoke-1789000000000000000",
  "harness_version": "0.1.0",
  "harness_commit": "abcdef1234567890abcdef1234567890abcdef12",
  "harness_dirty": false,
  "started_unix_nanos": 1789000000000000000,
  "finished_unix_nanos": 1789000123000000000,
  "command_line": "amber-cas-bench run --profile smoke",
  "profile": {{ "name": "smoke", "description": "small", "segment_size": 8388608 }},
  "seed": 7,
  "order_seed": 9,
  "jobs": 4,
  "repeats": 3,
  "host": {{
    "os": "linux", "arch": "x86_64", "kernel": "Linux 6.18",
    "cpu_model": "Test CPU", "cpu_logical_cores": 32,
    "memory_total_bytes": 1073741824,
    "load_average_at_start": [0.5, 0.4, 0.3],
    "cache_policy": "The page cache was NOT dropped."
  }},
  "tools": {{
    "tools": {{
      "amber-rust": {{ "name": "amber-rust", "path": "/nix/store/x/bin/amber-store",
        "version": "", "sha256": "aa", "source_commit": "141df2b", "source_dirty": false }},
      "git": {{ "name": "git", "path": "/nix/store/y/bin/git",
        "version": "git version 2.54.0", "sha256": "bb" }}
    }},
    "missing": {{}}
  }},
  "blob": {{ "service": "garage", "gateway_endpoint": "http://127.0.0.1:1",
             "shaping": "no shaping", "measurement": "counted at the socket" }},
  "statistics_method": "median = lower middle sample",
  "runs": [
    {{ "group": "tree", "scenario": "tree/lifecycle", "backend": "amber-rust", "rep": 0,
       "semantics": {{ "compression": "zstd. Always.", "encryption": "none",
                      "durability": "fsync", "concurrency": "single" }} }},
    {{ "group": "tree", "scenario": "tree/lifecycle", "backend": "git", "rep": 0,
       "semantics": {{ "compression": "zlib", "encryption": "none",
                      "durability": "none", "concurrency": "threads" }} }}
  ],
  "summary": [
    {{ "group": "tree", "scenario": "tree/lifecycle", "backend": "amber-rust",
       "op": "ingest_fresh", "ok": 3, "excluded_invalid_samples": 0,
       "unsupported": 0, "failed": 0,
       "wall_ns": {{ "n": 3, "unit": "ns", "min": 1.0, "median": 2000000000.0,
                    "p95": 2500000000.0, "max": 3.0, "mean": 2.0, "cv": 0.1 }},
       "store_allocated_bytes": {{ "n": 3, "unit": "bytes", "min": 1.0,
                    "median": 1048576.0, "p95": 1048576.0, "max": 1.0, "mean": 1.0 }},
       "counters_median": {{ "s3_bytes_up": 1024.0 }} }},
    {{ "group": "tree", "scenario": "tree/lifecycle", "backend": "git",
       "op": "ingest_fresh", "ok": 3, "excluded_invalid_samples": 0,
       "unsupported": 0, "failed": 0,
       "wall_ns": {{ "n": 3, "unit": "ns", "min": 1.0, "median": 4000000000.0,
                    "p95": 4000000000.0, "max": 3.0, "mean": 2.0 }} }},
    {{ "group": "tree", "scenario": "tree/lifecycle", "backend": "git",
       "op": "gc", "ok": 0, "excluded_invalid_samples": 0,
       "unsupported": 3, "failed": 0, "reason": "git has no equivalent" }}{extra_summary}
  ],
  "unsupported": [],
  "cross_checks": [],
  "errors": [],
  "skipped_backends": {{}}
}}"#,
        schema = crate::report::SCHEMA_VERSION,
    )
}

/// Writes a finished-looking run directory.
fn run_dir(root: &Path, valid: bool) -> PathBuf {
    let dir = root.join("run");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("report.json"), report_json(valid, "")).unwrap();
    std::fs::write(dir.join("REPORT.md"), "# report\n").unwrap();
    for csv in [
        "samples.csv",
        "summary.csv",
        "counters.csv",
        "verifications.csv",
    ] {
        std::fs::write(dir.join(csv), "a,b\n1,2\n").unwrap();
    }
    dir
}

fn opts(report_dir: PathBuf, results_dir: PathBuf) -> Options {
    Options {
        report_dir,
        results_dir,
        id: None,
        label: None,
        allow_invalid: false,
        redact: Vec::new(),
    }
}

#[test]
fn a_valid_run_becomes_a_readable_directory_with_every_artefact() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), true);
    let results = tmp.path().join("results");
    let dir = publish(&opts(run, results.clone())).unwrap();

    // Dated and identified, from the run's own start time.
    assert!(
        dir.file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-smoke"),
        "{dir:?}"
    );
    for name in ARTEFACTS {
        assert!(dir.join(name).is_file(), "{name} was not copied");
    }
    assert!(dir.join("run.json").is_file());
    assert!(dir.join("README.md").is_file());
    assert!(results.join("README.md").is_file());

    // The measured values are the ones the run wrote. Nothing in this
    // fixture is a workspace path, so the copy is byte for byte.
    assert_eq!(
        std::fs::read_to_string(dir.join("report.json")).unwrap(),
        report_json(true, "")
    );

    let page = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(page.contains("141df2b"), "the core revision is on the page");
    assert!(page.contains("Test CPU"), "the host is on the page");
    assert!(page.contains("page cache was NOT dropped"));
    assert!(page.contains("declared itself **valid**"));

    let idx = std::fs::read_to_string(results.join("README.md")).unwrap();
    assert!(idx.contains("`smoke`"));
    assert!(idx.contains("not a leaderboard"), "{idx}");
}

#[test]
fn charts_are_written_per_scenario_and_state_their_context() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), true);
    let dir = publish(&opts(run, tmp.path().join("results"))).unwrap();

    let elapsed = dir.join("plots/tree-lifecycle-elapsed.svg");
    assert!(elapsed.is_file(), "no elapsed chart");
    let svg = std::fs::read_to_string(&elapsed).unwrap();
    assert!(svg.contains("milliseconds"), "the unit is stated");
    assert!(svg.contains("n=3"), "the sample count is stated");
    assert!(svg.contains("page cache"), "the cache policy is stated");
    assert!(svg.contains("compression zstd"), "semantics are stated");
    assert!(svg.contains("Small sample"), "the caveat is stated");
    // An unsupported operation is named, never drawn as a zero bar.
    assert!(svg.contains("git — unsupported"), "{svg}");
    assert!(dir.join("plots/tree-lifecycle-store-bytes.svg").is_file());
    // A non-blob scenario gets no transfer charts.
    assert!(!dir.join("plots/tree-lifecycle-requests.svg").exists());
}

#[test]
fn an_invalid_run_is_refused_and_never_charted() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), false);
    let results = tmp.path().join("results");

    let err = publish(&opts(run.clone(), results.clone())).unwrap_err();
    assert!(err.contains("declares itself invalid"), "{err}");
    assert!(err.contains("--allow-invalid"), "{err}");
    assert!(!results.exists(), "nothing was created for a refused run");

    let mut o = opts(run, results);
    o.allow_invalid = true;
    let dir = publish(&o).unwrap();
    assert!(!dir.join("plots").exists(), "an invalid run gets no charts");
    let page = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(page.contains("INVALID"), "{page}");
    assert!(page.contains("1 recorded failure(s)"), "{page}");
    let idx = std::fs::read_to_string(dir.parent().unwrap().join("README.md")).unwrap();
    assert!(
        idx.contains("**no**"),
        "the index says it is invalid: {idx}"
    );
}

#[test]
fn an_existing_result_directory_is_never_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), true);
    let results = tmp.path().join("results");
    let mut o = opts(run, results.clone());
    o.id = Some("keepme".into());
    publish(&o).unwrap();
    std::fs::write(results.join("keepme/precious"), b"x").unwrap();

    let err = publish(&o).unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    assert!(results.join("keepme/precious").is_file(), "it survived");
}

#[test]
fn a_report_from_another_schema_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), true);
    let bumped = report_json(true, "").replace(
        &format!("\"schema_version\": {}", crate::report::SCHEMA_VERSION),
        &format!("\"schema_version\": {}", crate::report::SCHEMA_VERSION + 7),
    );
    std::fs::write(run.join("report.json"), bumped).unwrap();
    let err = publish(&opts(run, tmp.path().join("results"))).unwrap_err();
    assert!(err.contains("schema version"), "{err}");
}

#[test]
fn an_unfinished_run_directory_is_refused_before_anything_is_created() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), true);
    std::fs::remove_file(run.join("counters.csv")).unwrap();
    let results = tmp.path().join("results");
    let err = publish(&opts(run, results.clone())).unwrap_err();
    assert!(err.contains("counters.csv"), "{err}");
    assert!(!results.join("20260910T002640Z-smoke").exists());

    // And a directory with no report at all.
    let empty = tmp.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(publish(&opts(empty, results)).is_err());
}

#[test]
fn the_index_is_derived_from_the_recorded_runs_not_appended_to() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), true);
    let results = tmp.path().join("results");
    for id in ["r-one", "r-two"] {
        let mut o = opts(run.clone(), results.clone());
        o.id = Some(id.into());
        o.label = Some(format!("the {id} run"));
        publish(&o).unwrap();
    }
    let idx = std::fs::read_to_string(results.join("README.md")).unwrap();
    assert!(idx.contains("r-one") && idx.contains("r-two"), "{idx}");
    assert!(idx.contains("the r-one run"), "labels are shown");

    // Removing a run removes it from the index.
    std::fs::remove_dir_all(results.join("r-one")).unwrap();
    assert_eq!(reindex(&results).unwrap(), 1);
    let idx = std::fs::read_to_string(results.join("README.md")).unwrap();
    assert!(!idx.contains("r-one"), "{idx}");
    assert!(idx.contains("r-two"), "{idx}");
}

/// What gets committed must not carry the directories the run happened to
/// use, in any artefact — and the numbers must survive that untouched.
#[test]
fn workspace_paths_are_replaced_everywhere_and_values_are_not() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_dir(tmp.path(), true);
    let secret = tmp.path().join("private-workspace");
    std::fs::create_dir(&secret).unwrap();
    let scratch = format!("{}/scratch", run.display());

    // A report whose command line, scratch path and CSVs mention the
    // directories involved, exactly as a real one does.
    let json = report_json(true, "")
        .replace(
            "amber-cas-bench run --profile smoke",
            &format!(
                "amber-cas-bench run --harness-repo {} --out {} --profile smoke",
                secret.display(),
                run.display()
            ),
        )
        .replace(
            "\"cpu_model\": \"Test CPU\"",
            &format!("\"cpu_model\": \"Test CPU\", \"scratch_path\": \"{scratch}\""),
        );
    std::fs::write(run.join("report.json"), &json).unwrap();
    std::fs::write(run.join("samples.csv"), format!("path\n{scratch}\n")).unwrap();

    let results = tmp.path().join("results");
    let mut o = opts(run.clone(), results);
    o.redact = vec![secret.clone()];
    let dir = publish(&o).unwrap();

    for name in ARTEFACTS {
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        assert!(
            !text.contains(&run.display().to_string()),
            "{name} still names the run directory"
        );
        assert!(
            !text.contains(&secret.display().to_string()),
            "{name} still names the redacted path"
        );
    }
    let published = std::fs::read_to_string(dir.join("report.json")).unwrap();
    assert!(published.contains("<run-output-dir>"), "{published}");
    assert!(published.contains("<scratch>"), "{published}");
    assert!(published.contains("<redacted-0>"), "{published}");
    // Redaction rewrites paths and nothing else.
    assert!(published.contains("\"median\": 2000000000.0"));
    assert!(published.contains("\"source_commit\": \"141df2b\""));
    assert!(
        std::fs::read_to_string(dir.join("samples.csv"))
            .unwrap()
            .contains("<scratch>")
    );

    let page = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(
        page.contains("`<scratch>`"),
        "the page says what it replaced"
    );
    let meta: Entry =
        serde_json::from_str(&std::fs::read_to_string(dir.join("run.json")).unwrap()).unwrap();
    assert!(meta.redacted_paths.contains(&"<scratch>".to_string()));
    // The placeholders are recorded; the paths behind them are not.
    assert!(!meta.redacted_paths.iter().any(|p| p.contains('/')));
}

#[test]
fn an_unusable_identifier_is_refused() {
    for bad in ["", "..", "a/b", ".hidden", "with space"] {
        assert!(check_id(bad).is_err(), "{bad:?} should be refused");
    }
    for good in ["20260912T101010Z-standard", "run_1", "v0.1"] {
        assert!(check_id(good).is_ok(), "{good:?} should be accepted");
    }
}

#[test]
fn timestamps_are_rendered_in_utc() {
    // 2026-09-10T00:26:40Z.
    assert_eq!(stamp(1_789_000_000_000_000_000), "2026-09-10T00:26:40Z");
    assert_eq!(stamp(0), "1970-01-01T00:00:00Z");
    // A leap day, which the civil-from-days conversion has to get right.
    assert_eq!(stamp(1_709_164_800_000_000_000), "2024-02-29T00:00:00Z");
}

#[test]
fn a_summary_row_with_no_valid_repetition_is_not_a_bar() {
    let row = |ok, unsupported, failed, excluded| SummaryRow {
        group: "g".into(),
        scenario: "s".into(),
        backend: "b".into(),
        op: "o".into(),
        ok,
        excluded_invalid_samples: excluded,
        unsupported,
        failed,
        reason: None,
        wall_ns: None,
        store_allocated_bytes: None,
        counters_median: BTreeMap::new(),
    };
    assert_eq!(absence(&row(0, 3, 0, 0)), "unsupported");
    assert_eq!(absence(&row(0, 0, 3, 0)), "failed");
    assert_eq!(absence(&row(0, 0, 0, 3)), "no valid repetition");
    assert_eq!(absence(&row(0, 0, 0, 0)), "not measured");
}

/// A run whose only measured operation had no healthy repetition must not
/// produce a chart at all, rather than a chart of nothing.
#[test]
fn a_scenario_with_no_usable_sample_produces_no_chart() {
    let json = report_json(true, "").replace("\"ok\": 3", "\"ok\": 0");
    let loaded: Loaded = serde_json::from_str(&json).unwrap();
    assert!(charts(&loaded).is_empty());
}

#[test]
fn blob_scenarios_get_transfer_and_request_charts() {
    let extra = r#",
    { "group": "blob", "scenario": "blob/nix-closure", "backend": "amber-rust-s3",
      "op": "push_initial", "ok": 2, "excluded_invalid_samples": 0,
      "unsupported": 0, "failed": 0,
      "wall_ns": { "n": 2, "unit": "ns", "min": 1.0, "median": 1000000.0,
                   "p95": 1000000.0, "max": 1.0, "mean": 1.0 },
      "counters_median": { "s3_bytes_up": 4096.0, "s3_bytes_down": 512.0,
                           "s3_requests_total": 17.0 } },
    { "group": "blob", "scenario": "blob/nix-closure", "backend": "amber-rust-s3",
      "op": "retention_cleanup", "ok": 2, "excluded_invalid_samples": 0,
      "unsupported": 0, "failed": 0,
      "wall_ns": { "n": 2, "unit": "ns", "min": 1.0, "median": 2000000.0,
                   "p95": 2000000.0, "max": 1.0, "mean": 1.0 } }"#;
    let loaded: Loaded = serde_json::from_str(&report_json(true, extra)).unwrap();
    let names: Vec<String> = charts(&loaded).into_iter().map(|(n, _)| n).collect();
    assert!(
        names.contains(&"blob-nix-closure-upload-bytes.svg".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"blob-nix-closure-download-bytes.svg".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"blob-nix-closure-requests.svg".to_string()),
        "{names:?}"
    );
    // And the tree scenario is still its own chart, not merged in.
    assert!(
        names.contains(&"tree-lifecycle-elapsed.svg".to_string()),
        "{names:?}"
    );

    // The retention caveat travels with the chart that shows it.
    let (_, svg) = charts(&loaded)
        .into_iter()
        .find(|(n, _)| n == "blob-nix-closure-elapsed.svg")
        .expect("blob elapsed chart");
    assert!(svg.contains("not asked for the same thing"), "{svg}");
    assert!(svg.contains("garage"), "the object store is named");
}

#[test]
fn a_chart_file_is_attributed_to_its_own_scenario() {
    let scenarios = vec!["blob/nix-closure".to_string(), "nix/closure".to_string()];
    assert_eq!(
        plot_scenario("plots/blob-nix-closure-elapsed.svg", &scenarios),
        "blob/nix-closure"
    );
    assert_eq!(
        plot_scenario("plots/nix-closure-elapsed.svg", &scenarios),
        "nix/closure"
    );
}
