//! Turning a finished run into a portable, committable results directory.
//!
//! A `--out` directory is a working artefact: it is large, it sits next to
//! scratch data, and its paths belong to the machine that produced it. What
//! goes into a repository is something else — a dated directory of Markdown,
//! static SVG and the CSV/JSON behind them, readable on a page that runs no
//! code.
//!
//! The rules this module enforces:
//!
//! * Measured values are copied, never recomputed. `report.json` and the
//!   CSVs are the ones the run wrote.
//! * A run whose own verdict is *invalid* is not turned into charts. It can
//!   still be recorded, with `--allow-invalid`, and then every artefact says
//!   so and no comparison plot is written.
//! * An existing results directory for the same identifier is refused, not
//!   overwritten.
//! * Nothing that identifies the workspace it was produced in — scratch
//!   paths, home directories, credentials — is copied into the page. The
//!   full `report.json` is copied verbatim, so the run itself must not put
//!   secrets there; it does not.

pub mod plot;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::report::markdown::{bytes, duration};
use crate::util::fsx;
use plot::{BarChart, Cell};

/// Artefacts copied verbatim from the run directory into the results
/// directory. `REPORT.md` is listed first because it is what a reader opens.
pub const ARTEFACTS: [&str; 6] = [
    "REPORT.md",
    "report.json",
    "samples.csv",
    "summary.csv",
    "counters.csv",
    "verifications.csv",
];

/// What `publish` was asked to do.
#[derive(Debug, Clone)]
pub struct Options {
    /// The finished run directory, as written by `run --out`.
    pub report_dir: PathBuf,
    /// The results directory to add to, e.g. `./results`.
    pub results_dir: PathBuf,
    /// Directory name inside `results_dir`; derived from the run when absent.
    pub id: Option<String>,
    /// A one-line human note recorded with the run.
    pub label: Option<String>,
    /// Record a run that declared itself invalid. No charts are produced and
    /// every artefact says why.
    pub allow_invalid: bool,
    /// Extra absolute paths to replace with a placeholder in everything
    /// published, beyond the ones derived from the run itself.
    pub redact: Vec<PathBuf>,
}

/// One path that is replaced by a placeholder everywhere it is published.
///
/// The published artefacts are committed to a public repository, and the
/// directories a run happened to use are nobody else's business and help no
/// reader reproduce anything. Only the placeholder is recorded; writing the
/// original down beside it would defeat the whole exercise.
#[derive(Debug, Clone)]
struct Redaction {
    from: String,
    placeholder: String,
}

/// The paths publishing replaces, longest first so a nested path is
/// rewritten before the directory that contains it.
fn redactions(report: &Loaded, opts: &Options) -> Vec<Redaction> {
    let mut out: Vec<Redaction> = Vec::new();
    let mut add = |path: Option<PathBuf>, placeholder: &str| {
        let Some(p) = path else { return };
        let s = p.display().to_string();
        // "/" would rewrite the whole file; an empty string would loop.
        if s.len() < 2 || out.iter().any(|r| r.from == s) {
            return;
        }
        out.push(Redaction {
            from: s,
            placeholder: placeholder.to_string(),
        });
    };
    add(canon(&opts.report_dir), "<run-output-dir>");
    add(Some(PathBuf::from(&report.host.scratch_path)), "<scratch>");
    // The results directory lives inside the repository being published to,
    // so its parent is the checkout path that appears in --harness-repo,
    // --flake-dir and the harness's own command line.
    add(
        canon(&opts.results_dir).and_then(|p| p.parent().map(Path::to_path_buf)),
        "<repo>",
    );
    // Publishing is normally run from the checkout, which is also what
    // `--harness-repo` and `--flake-dir` named in the run's command line.
    add(std::env::current_dir().ok(), "<repo>");
    for (i, p) in opts.redact.iter().enumerate() {
        add(
            canon(p).or_else(|| Some(p.clone())),
            &format!("<redacted-{i}>"),
        );
    }
    add(std::env::var_os("HOME").map(PathBuf::from), "<home>");
    out.sort_by_key(|r| std::cmp::Reverse(r.from.len()));
    out
}

fn canon(p: &Path) -> Option<PathBuf> {
    p.canonicalize().ok()
}

/// Applies every redaction to `text`.
fn redact_text(text: &str, redactions: &[Redaction]) -> String {
    let mut out = text.to_string();
    for r in redactions {
        if out.contains(&r.from) {
            out = out.replace(&r.from, &r.placeholder);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Reading back what the run wrote.
//
// These mirror the fields of `report::Report` that this module needs. They
// are a separate, tolerant set of types on purpose: publishing reads a JSON
// file that some *other* build of the harness may have written, so it names
// the fields it depends on and ignores everything else.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Validity {
    pub valid: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stat {
    pub n: usize,
    pub unit: String,
    pub median: f64,
    pub p95: f64,
    #[serde(default)]
    pub cv: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SummaryRow {
    pub group: String,
    pub scenario: String,
    pub backend: String,
    pub op: String,
    pub ok: usize,
    #[serde(default)]
    pub excluded_invalid_samples: usize,
    #[serde(default)]
    pub unsupported: usize,
    #[serde(default)]
    pub failed: usize,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub wall_ns: Option<Stat>,
    #[serde(default)]
    pub store_allocated_bytes: Option<Stat>,
    #[serde(default)]
    pub counters_median: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Semantics {
    #[serde(default)]
    pub compression: String,
    #[serde(default)]
    pub encryption: String,
    #[serde(default)]
    pub durability: String,
    #[serde(default)]
    pub concurrency: String,
    #[serde(default)]
    pub transport: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunRecord {
    pub scenario: String,
    pub backend: String,
    #[serde(default)]
    pub semantics: Semantics,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tool {
    pub version: String,
    pub sha256: String,
    #[serde(default)]
    pub source_commit: Option<String>,
    #[serde(default)]
    pub source_dirty: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Toolbox {
    #[serde(default)]
    pub tools: BTreeMap<String, Tool>,
    #[serde(default)]
    pub missing: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Host {
    pub os: String,
    pub arch: String,
    #[serde(default)]
    pub kernel: String,
    #[serde(default)]
    pub cpu_model: String,
    #[serde(default)]
    pub cpu_logical_cores: usize,
    #[serde(default)]
    pub memory_total_bytes: Option<u64>,
    /// Where the run's scratch data lived. Published only as a placeholder.
    #[serde(default)]
    pub scratch_path: String,
    #[serde(default)]
    pub load_average_at_start: Option<(f64, f64, f64)>,
    pub cache_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub segment_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlobInfo {
    pub service: String,
    #[serde(default)]
    pub shaping: String,
    #[serde(default)]
    pub measurement: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorRow {
    pub scenario: String,
    pub backend: String,
    pub kind: String,
    pub name: String,
    pub detail: String,
}

/// The parts of `report.json` publishing depends on.
#[derive(Debug, Clone, Deserialize)]
pub struct Loaded {
    pub schema_version: u32,
    pub validity: Validity,
    pub run_id: String,
    pub harness_version: String,
    #[serde(default)]
    pub harness_commit: Option<String>,
    #[serde(default)]
    pub harness_dirty: bool,
    pub started_unix_nanos: u128,
    pub finished_unix_nanos: u128,
    pub command_line: String,
    pub profile: ProfileInfo,
    pub seed: u64,
    pub order_seed: u64,
    pub jobs: usize,
    pub repeats: usize,
    pub host: Host,
    pub tools: Toolbox,
    #[serde(default)]
    pub blob: Option<BlobInfo>,
    pub statistics_method: String,
    #[serde(default)]
    pub runs: Vec<RunRecord>,
    #[serde(default)]
    pub summary: Vec<SummaryRow>,
    #[serde(default)]
    pub errors: Vec<ErrorRow>,
    #[serde(default)]
    pub skipped_backends: BTreeMap<String, String>,
}

/// The index entry written beside every published run, and the only thing
/// the results index is built from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub started_utc: String,
    pub profile: String,
    pub repeats: usize,
    pub seed: u64,
    pub order_seed: u64,
    pub valid: bool,
    #[serde(default)]
    pub invalid_reasons: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
    pub harness_version: String,
    #[serde(default)]
    pub harness_commit: Option<String>,
    pub harness_dirty: bool,
    /// Source revision of each measured core, by tool name.
    #[serde(default)]
    pub core_revisions: BTreeMap<String, String>,
    pub host_cpu: String,
    pub host_cores: usize,
    pub cache_policy: String,
    pub scenarios: Vec<String>,
    pub backends: Vec<String>,
    /// Charts written for this run, relative to its directory.
    #[serde(default)]
    pub plots: Vec<String>,
    /// Placeholders that replaced workspace paths in every published
    /// artefact. The paths themselves are deliberately not recorded.
    #[serde(default)]
    pub redacted_paths: Vec<String>,
}

/// Publishes one finished run. Returns the directory it created.
pub fn publish(opts: &Options) -> Result<PathBuf, String> {
    let json_path = opts.report_dir.join("report.json");
    let raw =
        std::fs::read_to_string(&json_path).map_err(|e| format!("{}: {e}", json_path.display()))?;
    let report: Loaded = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "{} is not a report this command can read: {e}",
            json_path.display()
        )
    })?;
    if report.schema_version != crate::report::SCHEMA_VERSION {
        return Err(format!(
            "{} has report schema version {}, and this build of the harness \
             publishes version {}. Publish it with the build that wrote it.",
            json_path.display(),
            report.schema_version,
            crate::report::SCHEMA_VERSION
        ));
    }
    if !report.validity.valid && !opts.allow_invalid {
        return Err(format!(
            "{} declares itself invalid ({}), so it is not published as a \
             result. Pass --allow-invalid to record it anyway: it is then \
             marked invalid everywhere and no comparison chart is drawn for \
             it.",
            json_path.display(),
            if report.validity.reasons.is_empty() {
                "no reason recorded".to_string()
            } else {
                report.validity.reasons.join("; ")
            }
        ));
    }

    let id = match &opts.id {
        Some(id) => {
            check_id(id)?;
            id.clone()
        }
        None => default_id(&report),
    };
    let dir = opts.results_dir.join(&id);
    if dir.exists() {
        return Err(format!(
            "{} already exists; publishing never overwrites a recorded run. \
             Choose another --id, or remove that directory yourself.",
            dir.display()
        ));
    }
    // Every artefact has to be there before anything is created, so a run
    // directory missing half its files does not leave half a result behind.
    for name in ARTEFACTS {
        let p = opts.report_dir.join(name);
        if !p.is_file() {
            return Err(format!(
                "{} has no {name}: that is not a finished run directory",
                opts.report_dir.display()
            ));
        }
    }

    // Copy the artefacts, rewriting the directories this particular
    // workspace happened to use. Only paths are touched: no number, no
    // revision and no counter is altered by this.
    let redactions = redactions(&report, opts);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for name in ARTEFACTS {
        let from = opts.report_dir.join(name);
        let text =
            std::fs::read_to_string(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        let text = redact_text(&text, &redactions);
        for r in &redactions {
            if text.contains(&r.from) {
                return Err(format!(
                    "{name} still contains a path that should have been \
                     replaced by {}; refusing to publish it",
                    r.placeholder
                ));
            }
        }
        fsx::write_file(&dir.join(name), text.as_bytes())
            .map_err(|e| format!("{}: {e}", dir.join(name).display()))?;
    }
    let redacted: Vec<String> = redactions.iter().map(|r| r.placeholder.clone()).collect();

    // Charts, but only for a run that may be read as a result.
    let mut plots = Vec::new();
    if report.validity.valid {
        let plot_dir = dir.join("plots");
        std::fs::create_dir_all(&plot_dir).map_err(|e| format!("{}: {e}", plot_dir.display()))?;
        for (name, svg) in charts(&report) {
            let svg = redact_text(&svg, &redactions);
            fsx::write_file(&plot_dir.join(&name), svg.as_bytes())
                .map_err(|e| format!("{name}: {e}"))?;
            plots.push(format!("plots/{name}"));
        }
    }

    let entry = entry(&report, &id, opts.label.clone(), plots, redacted);
    let entry_json = serde_json::to_string_pretty(&entry).map_err(|e| e.to_string())?;
    fsx::write_file(
        &dir.join("run.json"),
        redact_text(&entry_json, &redactions).as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    fsx::write_file(
        &dir.join("README.md"),
        redact_text(&page(&report, &entry), &redactions).as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    reindex(&opts.results_dir)?;
    Ok(dir)
}

/// Rebuilds `results/README.md` from every `run.json` under `results_dir`.
///
/// The index is derived, never appended to, so removing a run directory is
/// enough to remove it from the index.
pub fn reindex(results_dir: &Path) -> Result<usize, String> {
    let mut entries: Vec<Entry> = Vec::new();
    let read =
        std::fs::read_dir(results_dir).map_err(|e| format!("{}: {e}", results_dir.display()))?;
    for item in read {
        let item = item.map_err(|e| e.to_string())?;
        let meta = item.path().join("run.json");
        if !meta.is_file() {
            continue;
        }
        let text =
            std::fs::read_to_string(&meta).map_err(|e| format!("{}: {e}", meta.display()))?;
        let entry: Entry =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", meta.display()))?;
        entries.push(entry);
    }
    // Newest first, and stable when two runs share a timestamp.
    entries.sort_by(|a, b| {
        b.started_utc
            .cmp(&a.started_utc)
            .then_with(|| a.id.cmp(&b.id))
    });
    fsx::write_file(&results_dir.join("README.md"), index(&entries).as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(entries.len())
}

/// A directory name that is safe to create and to link to.
fn check_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !id.starts_with('.');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "--id {id:?} is not usable as a directory name: use ASCII \
             letters, digits, '-', '_' and '.', and do not start with '.'"
        ))
    }
}

/// `YYYYMMDDTHHMMSSZ-<profile>`: dated, sortable, and unique per run.
fn default_id(r: &Loaded) -> String {
    let (y, m, d, hh, mm, ss) = civil(r.started_unix_nanos);
    format!(
        "{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z-{}",
        r.profile.name
    )
}

/// Splits a Unix nanosecond timestamp into UTC civil time.
///
/// Days-to-civil is Howard Hinnant's algorithm; leap seconds do not exist in
/// Unix time, so this is exact.
fn civil(nanos: u128) -> (i64, u32, u32, u32, u32, u32) {
    let secs = (nanos / 1_000_000_000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// `YYYY-MM-DDTHH:MM:SSZ`.
pub fn stamp(nanos: u128) -> String {
    let (y, m, d, hh, mm, ss) = civil(nanos);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn entry(
    r: &Loaded,
    id: &str,
    label: Option<String>,
    plots: Vec<String>,
    redacted_paths: Vec<String>,
) -> Entry {
    let mut core_revisions = BTreeMap::new();
    for name in ["amber-rust", "amber-go"] {
        if let Some(t) = r.tools.tools.get(name)
            && let Some(rev) = &t.source_commit
        {
            core_revisions.insert(
                name.to_string(),
                if t.source_dirty {
                    format!("{rev} (dirty working tree)")
                } else {
                    rev.clone()
                },
            );
        }
    }
    Entry {
        id: id.to_string(),
        started_utc: stamp(r.started_unix_nanos),
        profile: r.profile.name.clone(),
        repeats: r.repeats,
        seed: r.seed,
        order_seed: r.order_seed,
        valid: r.validity.valid,
        invalid_reasons: r.validity.reasons.clone(),
        label,
        harness_version: r.harness_version.clone(),
        harness_commit: r.harness_commit.clone(),
        harness_dirty: r.harness_dirty,
        core_revisions,
        host_cpu: r.host.cpu_model.clone(),
        host_cores: r.host.cpu_logical_cores,
        cache_policy: r.host.cache_policy.clone(),
        scenarios: ordered(r.summary.iter().map(|s| s.scenario.clone())),
        backends: ordered(r.summary.iter().map(|s| s.backend.clone())),
        plots,
        redacted_paths,
    }
}

fn ordered(values: impl Iterator<Item = String>) -> Vec<String> {
    let set: BTreeSet<String> = values.collect();
    set.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

/// Every chart for a run, as `(file name, svg)`.
///
/// One chart never spans two scenarios: the operations of `tree/lifecycle`
/// and of `blob/nix-closure` are different work, and a picture that put them
/// on one axis would be a ranking of unlike things.
pub fn charts(r: &Loaded) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for scenario in ordered(r.summary.iter().map(|s| s.scenario.clone())) {
        let slug = scenario.replace('/', "-");
        let rows: Vec<&SummaryRow> = r
            .summary
            .iter()
            .filter(|s| s.scenario == scenario)
            .collect();

        if let Some(svg) = chart(
            r,
            &scenario,
            &rows,
            "elapsed time",
            "median elapsed wall-clock time per operation, milliseconds \
             (shorter is faster); the tick is p95",
            |row| {
                row.wall_ns
                    .as_ref()
                    .map(|w| (w.median / 1e6, Some(w.p95 / 1e6), duration(w.median), w.n))
            },
            &[],
        ) {
            out.push((format!("{slug}-elapsed.svg"), svg));
        }

        if let Some(svg) = chart(
            r,
            &scenario,
            &rows,
            "store size on disk",
            "median allocated bytes of the backend's own store directory \
             after each operation (smaller is denser)",
            |row| {
                row.store_allocated_bytes
                    .as_ref()
                    .map(|s| (s.median, Some(s.p95), bytes(s.median), s.n))
            },
            &[
                "Allocated bytes, i.e. what the filesystem charges, not the \
                 sum of file lengths. Both are in counters.csv.",
            ],
        ) {
            out.push((format!("{slug}-store-bytes.svg"), svg));
        }

        if scenario.starts_with("blob/") {
            for (counter, file, what, note) in [
                (
                    "s3_bytes_up",
                    "upload-bytes",
                    "bytes sent to the object store",
                    "Counted at the socket by the measuring gateway, in both \
                     directions, including retries and all protocol overhead.",
                ),
                (
                    "s3_bytes_down",
                    "download-bytes",
                    "bytes received from the object store",
                    "Counted at the socket by the measuring gateway, in both \
                     directions, including retries and all protocol overhead.",
                ),
                (
                    "s3_requests_total",
                    "requests",
                    "HTTP requests to the object store",
                    "Every request the gateway saw, by method in counters.csv; \
                     retries are counted, not collapsed.",
                ),
            ] {
                if let Some(svg) = chart(
                    r,
                    &scenario,
                    &rows,
                    what,
                    &format!("median {what} per operation"),
                    |row| {
                        row.counters_median.get(counter).map(|v| {
                            let label = if counter.ends_with("total") {
                                format!("{v:.0}")
                            } else {
                                bytes(*v)
                            };
                            (*v, None, label, row.ok)
                        })
                    },
                    &[note],
                ) {
                    out.push((format!("{slug}-{file}.svg"), svg));
                }
            }
        }
    }
    out
}

/// Builds one chart, or `None` when no backend produced the metric.
fn chart(
    r: &Loaded,
    scenario: &str,
    rows: &[&SummaryRow],
    what: &str,
    subtitle: &str,
    pick: impl Fn(&SummaryRow) -> Option<(f64, Option<f64>, String, usize)>,
    extra_notes: &[&str],
) -> Option<String> {
    let ops = ordered_ops(rows);
    let backends = ordered(rows.iter().map(|s| s.backend.clone()));
    if ops.is_empty() || backends.is_empty() {
        return None;
    }
    let mut cells = Vec::new();
    let mut any = false;
    for op in &ops {
        let mut row_cells = Vec::new();
        for backend in &backends {
            let found = rows
                .iter()
                .find(|s| &s.op == op && &s.backend == backend)
                .copied();
            let cell = match found {
                None => Cell::absent("not run"),
                Some(row) => match pick(row) {
                    Some((v, hi, label, n)) if row.ok > 0 => {
                        any = true;
                        Cell::measured(v, hi, label, n)
                    }
                    // A value with no valid repetition behind it is not a
                    // measurement; nor is a missing one. Both say why.
                    _ => Cell::absent(absence(row)),
                },
            };
            row_cells.push(cell);
        }
        cells.push(row_cells);
    }
    if !any {
        return None;
    }

    let mut footer = Vec::new();
    for line in extra_notes {
        footer.extend(plot::wrap(line, 150));
    }
    footer.extend(plot::wrap(
        &format!(
            "Sample count is on every bar. {} Repetitions requested: {}.",
            r.statistics_method, r.repeats
        ),
        150,
    ));
    if r.repeats < 5 {
        footer.extend(plot::wrap(
            &format!(
                "Small sample: {} repetition(s) per bar. Dispersion is in \
                 summary.csv (cv) and every raw sample is in samples.csv. \
                 Treat a difference of the same order as the spread as \
                 indicative only.",
                r.repeats
            ),
            150,
        ));
    }
    footer.extend(plot::wrap(
        &format!("Cache policy: {}", r.host.cache_policy),
        150,
    ));
    if scenario.starts_with("blob/")
        && let Some(b) = &r.blob
    {
        footer.extend(plot::wrap(
            &format!("Object store: {} ({}).", b.service, b.shaping),
            150,
        ));
    }
    for backend in &backends {
        if let Some(s) = semantics(r, scenario, backend) {
            let mut line = format!(
                "{backend}: compression {} | encryption {} | durability {} | concurrency {}",
                first_clause(&s.compression),
                first_clause(&s.encryption),
                first_clause(&s.durability),
                first_clause(&s.concurrency)
            );
            if let Some(t) = &s.transport {
                line.push_str(&format!(" | transport {}", first_clause(t)));
            }
            footer.extend(plot::wrap(&line, 150));
        }
    }
    if ops.iter().any(|o| o == "retention_cleanup") {
        footer.extend(plot::wrap(
            "retention_cleanup is the one operation here where the backends \
             are not asked for the same thing; what each one kept is in \
             REPORT.md and in counters.csv (retained_references). These bars \
             are not a ranking.",
            150,
        ));
    }
    footer.extend(plot::wrap(
        &format!(
            "Run {} on {} ({} logical cores), profile {}, seed {}.",
            r.run_id, r.host.cpu_model, r.host.cpu_logical_cores, r.profile.name, r.seed
        ),
        150,
    ));

    BarChart {
        title: format!("{scenario} — {what}"),
        subtitle: vec![subtitle.to_string()],
        footer,
        categories: ops,
        series: backends,
        cells,
    }
    .render()
}

/// Operation names in the order the report lists them, deduplicated.
fn ordered_ops(rows: &[&SummaryRow]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in rows {
        if !out.contains(&r.op) {
            out.push(r.op.clone());
        }
    }
    out
}

/// Why a bar is absent, in the fewest words that stay true.
fn absence(row: &SummaryRow) -> String {
    if row.unsupported > 0 {
        "unsupported".into()
    } else if row.failed > 0 {
        "failed".into()
    } else if row.ok == 0 && row.excluded_invalid_samples > 0 {
        "no valid repetition".into()
    } else {
        "not measured".into()
    }
}

fn semantics<'a>(r: &'a Loaded, scenario: &str, backend: &str) -> Option<&'a Semantics> {
    r.runs
        .iter()
        .find(|x| x.scenario == scenario && x.backend == backend)
        .map(|x| &x.semantics)
}

/// The first sentence of a semantics field, for a one-line chart footer.
fn first_clause(s: &str) -> String {
    let s = s.trim();
    match s.find(". ") {
        Some(i) => s[..i].to_string(),
        None => s.trim_end_matches('.').to_string(),
    }
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

/// The page for one published run.
fn page(r: &Loaded, e: &Entry) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Benchmark run `{}`\n\n", e.id));
    if let Some(label) = &e.label {
        s.push_str(&format!("{label}\n\n"));
    }
    if e.valid {
        s.push_str(
            "This run declared itself **valid**: no operation failed, no \
             correctness check failed, no cross-backend check failed and no \
             requested backend went unrun. Only repetitions that were healthy \
             throughout contribute to the statistics below.\n\n",
        );
    } else {
        s.push_str(
            "> **This run is INVALID and is recorded as a diagnostic, not as a result.**\n>\n",
        );
        for reason in &e.invalid_reasons {
            s.push_str(&format!("> * {reason}\n"));
        }
        s.push_str(
            ">\n> No comparison chart is drawn for it, and its timings must \
             not be quoted as a measurement.\n\n",
        );
    }

    s.push_str("## What produced these numbers\n\n");
    s.push_str("| | |\n|---|---|\n");
    let row = |s: &mut String, k: &str, v: String| s.push_str(&format!("| {k} | {v} |\n"));
    row(&mut s, "started (UTC)", e.started_utc.clone());
    row(
        &mut s,
        "wall clock",
        duration((r.finished_unix_nanos - r.started_unix_nanos) as f64),
    );
    row(
        &mut s,
        "profile",
        format!("`{}` — {}", r.profile.name, r.profile.description),
    );
    row(
        &mut s,
        "repetitions",
        format!("{} per (scenario, backend)", r.repeats),
    );
    row(
        &mut s,
        "seeds",
        format!(
            "corpus and fixtures `{}`, randomised backend order `{}`",
            r.seed, r.order_seed
        ),
    );
    row(&mut s, "worker threads offered", r.jobs.to_string());
    row(
        &mut s,
        "Amber segment size",
        format!(
            "{} (both cores, explicitly)",
            bytes(r.profile.segment_size as f64)
        ),
    );
    row(
        &mut s,
        "harness",
        format!(
            "`amber-cas-bench` {}{}{}",
            r.harness_version,
            r.harness_commit
                .as_deref()
                .map(|c| format!(", commit `{c}`"))
                .unwrap_or_default(),
            if r.harness_dirty {
                " — **built from a dirty working tree**"
            } else {
                ""
            }
        ),
    );
    row(
        &mut s,
        "host",
        format!(
            "{} {}, {} ({} logical cores){}",
            r.host.os,
            r.host.arch,
            r.host.cpu_model,
            r.host.cpu_logical_cores,
            r.host
                .memory_total_bytes
                .map(|m| format!(", {} RAM", bytes(m as f64)))
                .unwrap_or_default()
        ),
    );
    if !r.host.kernel.is_empty() {
        row(&mut s, "kernel", format!("`{}`", r.host.kernel));
    }
    if let Some((one, five, fifteen)) = r.host.load_average_at_start {
        row(
            &mut s,
            "load average at start",
            format!("{one:.2}, {five:.2}, {fifteen:.2} (1/5/15 min)"),
        );
    }
    row(&mut s, "cache policy", r.host.cache_policy.clone());
    if let Some(b) = &r.blob {
        row(
            &mut s,
            "object store",
            format!("{} — {}", b.service, b.shaping),
        );
        row(&mut s, "transfer measurement", b.measurement.clone());
    }
    row(&mut s, "command", format!("`{}`", r.command_line));
    s.push('\n');

    s.push_str("### Executables measured\n\n");
    s.push_str("| tool | version | source revision | SHA-256 of the executable |\n");
    s.push_str("|---|---|---|---|\n");
    for (name, t) in &r.tools.tools {
        s.push_str(&format!(
            "| `{name}` | {} | {} | `{}` |\n",
            if t.version.is_empty() {
                "—".into()
            } else {
                t.version.clone()
            },
            match (&t.source_commit, t.source_dirty) {
                (Some(c), true) => format!("`{c}` **dirty**"),
                (Some(c), false) => format!("`{c}`"),
                (None, _) => "—".into(),
            },
            t.sha256
        ));
    }
    s.push('\n');

    if !r.errors.is_empty() {
        s.push_str("### Recorded failures\n\n| scenario | backend | kind | what | detail |\n");
        s.push_str("|---|---|---|---|---|\n");
        for e in &r.errors {
            s.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` | {} |\n",
                e.scenario,
                e.backend,
                e.kind,
                e.name,
                e.detail.replace('|', "\\|").replace('\n', " ")
            ));
        }
        s.push('\n');
    }
    if !r.skipped_backends.is_empty() {
        s.push_str("### Backends that could not be run\n\n");
        for (b, why) in &r.skipped_backends {
            s.push_str(&format!("* `{b}`: {why}\n"));
        }
        s.push('\n');
    }

    if !e.plots.is_empty() {
        s.push_str("## Charts\n\n");
        s.push_str(
            "One scenario per chart, because that is the only level at which \
             two bars describe the same work. An operation a backend cannot \
             perform is written out as such: it is never drawn as a bar of \
             length zero. Each chart states its unit, the sample count behind \
             every bar, this run's cache policy and the storage semantics of \
             each backend in it.\n\n",
        );
        let mut current = String::new();
        for p in &e.plots {
            let scenario = plot_scenario(p, &e.scenarios);
            if scenario != current {
                s.push_str(&format!("### `{scenario}`\n\n"));
                current = scenario;
            }
            s.push_str(&format!("![{p}]({p})\n\n"));
        }
    }

    s.push_str("## The full result\n\n");
    s.push_str("| file | what it is |\n|---|---|\n");
    for (name, what) in [
        (
            "REPORT.md",
            "the readable comparison, scenario by scenario, with every correctness check and every caveat",
        ),
        (
            "report.json",
            "everything, including every raw sample, every executed command and every recorded failure",
        ),
        (
            "samples.csv",
            "one row per operation per repetition, with `rep_valid`",
        ),
        (
            "summary.csv",
            "the aggregated statistics, with `run_valid` on every row",
        ),
        (
            "counters.csv",
            "every integer counter: S3 requests by method, bytes by direction, store component sizes, delivered references",
        ),
        ("verifications.csv", "one row per correctness check"),
        (
            "run.json",
            "this run's index entry, which is what the results index is built from",
        ),
    ] {
        s.push_str(&format!("| [`{name}`]({name}) | {what} |\n"));
    }
    s.push_str(&format!("\nStatistics: {}\n", r.statistics_method));
    if !e.redacted_paths.is_empty() {
        s.push_str(&format!(
            "\nThe directories this run used are not part of the result, so \
             publishing replaced them everywhere they appeared — in the \
             command line, the tool paths and the charts — with {}. Nothing \
             else was rewritten: every measured value, counter, revision and \
             executable hash is the one the run recorded.\n",
            e.redacted_paths
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    s.push_str("\n[Back to the results index](../README.md)\n");
    s
}

/// Which scenario a chart file belongs to, for grouping headings.
fn plot_scenario(path: &str, scenarios: &[String]) -> String {
    let name = path.trim_start_matches("plots/");
    scenarios
        .iter()
        .filter(|s| name.starts_with(&s.replace('/', "-")))
        .max_by_key(|s| s.len())
        .cloned()
        .unwrap_or_else(|| "other".into())
}

/// The results index.
fn index(entries: &[Entry]) -> String {
    let mut s = String::new();
    s.push_str("# Recorded benchmark results\n\n");
    s.push_str(
        "Each directory below is one complete run of the suite, exported with\n\
         `amber-cas-bench publish` from the report that run wrote. The Markdown\n\
         and the SVG charts are readable here with nothing installed; the JSON\n\
         and CSV beside them are the same numbers in machine-readable form.\n\n",
    );
    s.push_str(
        "**These are not a leaderboard.** A number is only comparable with\n\
         another number from the same scenario, the same run and the same host.\n\
         Runs recorded here differ in profile, host and configuration, so rows\n\
         of this table must not be compared with each other; open a run and read\n\
         its scenarios. Every run states its repetition count, and the small\n\
         sample sizes used here mean differences of the same order as the\n\
         recorded dispersion are indicative only.\n\n",
    );
    if entries.is_empty() {
        s.push_str("_No run has been published yet._\n");
    } else {
        s.push_str(&table(entries));
    }
    s.push_str(
        "\n## Exploring the data\n\n\
         `explore.ipynb` in this directory loads any run recorded here and\n\
         compares its scenarios. It needs nothing that is not pinned by this\n\
         repository's flake:\n\n\
         ```\n\
         nix develop --command jupyter lab results/explore.ipynb\n\
         ```\n\n\
         It refuses to draw a performance comparison from a run whose own\n\
         verdict is invalid, and it prints its tables as text so a saved copy\n\
         is readable without re-running it.\n",
    );
    s
}

/// The index table itself, plus a line per labelled run.
fn table(entries: &[Entry]) -> String {
    let mut s = String::new();
    s.push_str(
        "| run | started (UTC) | profile | repeats | valid | host | harness | cores measured |\n",
    );
    s.push_str("|---|---|---|---|---|---|---|---|\n");
    for e in entries {
        let cores = if e.core_revisions.is_empty() {
            "—".to_string()
        } else {
            e.core_revisions
                .iter()
                .map(|(k, v)| format!("{k} `{}`", &v[..v.len().min(12)]))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        s.push_str(&format!(
            "| [`{}`]({}/README.md) | {} | `{}` | {} | {} | {} × {} | {}{} | {cores} |\n",
            e.id,
            e.id,
            e.started_utc,
            e.profile,
            e.repeats,
            if e.valid { "yes" } else { "**no**" },
            e.host_cores,
            e.host_cpu,
            e.harness_commit
                .as_deref()
                .map(|c| format!("`{}`", &c[..c.len().min(12)]))
                .unwrap_or_else(|| e.harness_version.clone()),
            if e.harness_dirty { " (dirty)" } else { "" },
        ));
    }
    s.push('\n');
    for e in entries {
        if let Some(label) = &e.label {
            s.push_str(&format!("* [`{}`]({}/README.md) — {label}\n", e.id, e.id));
        }
    }
    s
}

#[cfg(test)]
mod tests;
