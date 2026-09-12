//! The orchestrator: set the world up, run everything in a randomised order,
//! write the results, and take the world down again.
//!
//! Three rules shape this file:
//!
//! * Nothing is written outside a directory the harness created itself, and
//!   the output directory must not already exist.
//! * A backend that was asked for and cannot run is a recorded failure with a
//!   reason and a non-zero exit, never a quietly missing row.
//! * Corpus generation, tool discovery and verification all happen outside the
//!   measured windows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapters::nixstore::{ClientCache, NixCli};
use crate::blob::gateway::{Gateway, Shaping};
use crate::blob::s3;
use crate::blob::service::{self, LocalService};
use crate::config::{self, Profile};
use crate::dataset::{corpus, gitgen, manifest::Manifest};
use crate::hostinfo::{self, HostInfo};
use crate::metrics::{RunRecord, Verification};
use crate::report::{self, BlobInfo, NixFixtureInfo, OrderEntry, Report};
use crate::scenarios::{self, Ctx, blobs, gitsc, nixsc, tree};
use crate::toolbox::{self, Toolbox};
use crate::util::fsx;
use crate::util::rng::Rng;

/// Version of this harness, from its manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A remote S3 endpoint, used instead of the local service.
#[derive(Debug, Clone)]
pub struct RemoteS3 {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub prefix: String,
}

/// Everything the command line can set.
#[derive(Debug, Clone)]
pub struct Options {
    pub profile: String,
    pub out: PathBuf,
    pub seed: u64,
    pub order_seed: Option<u64>,
    pub repeats: Option<usize>,
    pub jobs: Option<usize>,
    pub groups: Option<Vec<String>>,
    pub scenarios: Option<Vec<String>>,
    pub backends: Option<Vec<String>>,
    /// Checkout of this benchmark repository; its revision and dirty state
    /// are the harness's own.
    pub harness_repo: PathBuf,
    /// Development overrides: the local checkouts the core binaries were
    /// built from, when they were not built from the pinned revisions.
    pub amber_rust_repo: Option<PathBuf>,
    pub amber_go_repo: Option<PathBuf>,
    pub amber_rust_bin: Option<PathBuf>,
    pub amber_go_bin: Option<PathBuf>,
    /// Source revisions of pinned (non-checkout) core builds, recorded in
    /// the report when there is no checkout to read them from.
    pub amber_rust_rev: Option<String>,
    pub amber_go_rev: Option<String>,
    pub flake_dir: PathBuf,
    /// Store paths to use as the Nix workload's closure roots instead of the
    /// deterministic fixture. Read-only; empty means "build the fixture".
    pub nix_closure_roots: Vec<String>,
    pub blob_latency_ms: u64,
    pub blob_bandwidth_bytes_per_s: u64,
    pub remote_s3: Option<RemoteS3>,
    pub drop_caches: bool,
    pub keep_scratch: bool,
    pub timeout_secs: Option<u64>,
    pub command_line: String,
}

/// Every scenario the harness knows, with its group and backends.
fn catalogue() -> Vec<(&'static str, &'static str, Vec<&'static str>)> {
    let mut out: Vec<(&'static str, &'static str, Vec<&'static str>)> = vec![
        (tree::GROUP, tree::SCENARIO, tree::BACKENDS.to_vec()),
        (gitsc::GROUP, gitsc::SCENARIO, gitsc::BACKENDS.to_vec()),
        (nixsc::GROUP, nixsc::SCENARIO, nixsc::BACKENDS.to_vec()),
        (
            scenarios::backup::GROUP,
            scenarios::backup::SCENARIO,
            scenarios::backup::BACKENDS.to_vec(),
        ),
    ];
    for s in blobs::SCENARIOS {
        out.push((blobs::GROUP, s, blobs::backends(s)));
    }
    out
}

/// Which tool each backend needs.
fn required_tool(backend: &str) -> &'static str {
    match backend {
        "amber-rust" | "amber-rust-s3" => "amber-rust",
        "amber-go" | "amber-go-s3" => "amber-go",
        "git" | "git-bundle-s3" => "git",
        "restic" | "restic-s3" => "restic",
        "nix" | "nix-binary-cache" => "nix",
        // The SHA-256 baseline is the harness binary itself.
        "fs-sha256" => "",
        _ => "",
    }
}

/// Runs everything. Returns the report and whether the run was clean.
pub fn run(opts: &Options) -> Result<(Report, bool), String> {
    let started = fsx::now_unix_nanos();
    let profile: Profile = config::profile(&opts.profile).ok_or_else(|| {
        format!(
            "unknown profile {:?}; known profiles: {}",
            opts.profile,
            config::PROFILES.join(", ")
        )
    })?;
    let run_id = format!("{}-{}", profile.name, started);

    // --- what to run -------------------------------------------------------
    //
    // Settled *before* anything is created. A mistyped backend name is an
    // argument error, and an argument error that had already made the output
    // directory would make the very next attempt fail with "already exists"
    // as well.
    let selected = select(opts, &catalogue())?;
    let groups: Vec<&str> = {
        let mut g: Vec<&str> = selected.iter().map(|s| s.group).collect();
        g.sort_unstable();
        g.dedup();
        g
    };

    // --- the output directory ---------------------------------------------
    if opts.out.exists() {
        return Err(format!(
            "{} already exists; the harness refuses to write into an existing \
             output directory. Choose another --out, or remove it yourself.",
            opts.out.display()
        ));
    }
    std::fs::create_dir_all(&opts.out).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    let out = opts
        .out
        .canonicalize()
        .map_err(|e| format!("{}: {e}", opts.out.display()))?;
    let scratch = out.join("scratch");
    fsx::create_owned(&scratch, &run_id).map_err(|e| format!("{}: {e}", scratch.display()))?;

    let repeats = opts.repeats.unwrap_or(profile.repeats).max(1);
    let jobs = opts
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
        .max(1);
    let timeout = Duration::from_secs(opts.timeout_secs.unwrap_or(profile.command_timeout_secs));

    // --- space -------------------------------------------------------------
    let free = fsx::free_bytes(&scratch).unwrap_or(0);
    if free < profile.approx_disk_bytes {
        eprintln!(
            "warning: profile {} expects about {} of free space and {} has {}",
            profile.name,
            report::markdown::bytes(profile.approx_disk_bytes as f64),
            scratch.display(),
            report::markdown::bytes(free as f64)
        );
    }

    // --- tools -------------------------------------------------------------
    let mut tools = discover(opts, &groups, &selected);
    let mut skipped: BTreeMap<String, String> = BTreeMap::new();
    for entry in &selected {
        for backend in &entry.backends {
            let tool = required_tool(backend);
            if !tool.is_empty()
                && let Err(why) = tools.get(tool)
            {
                skipped.insert((*backend).to_string(), why);
            }
        }
    }

    let dropped = if opts.drop_caches {
        match hostinfo::drop_caches() {
            Ok(()) => true,
            Err(e) => {
                eprintln!("warning: --drop-caches requested but not possible: {e}");
                false
            }
        }
    } else {
        false
    };
    let host = HostInfo::collect(&scratch, dropped);

    // --- the shared corpus -------------------------------------------------
    eprintln!("generating the corpus under {}", scratch.display());
    let corpus_dir = scratch.join("corpus");
    std::fs::create_dir_all(&corpus_dir).map_err(|e| e.to_string())?;
    let corpus = corpus::generate(&profile.corpus, opts.seed, &corpus_dir, jobs)
        .map_err(|e| format!("generating the corpus: {e}"))?;
    for g in &corpus.generations {
        eprintln!(
            "  generation {}: {} files, {} — manifest {}",
            g.index,
            g.file_count,
            report::markdown::bytes(g.logical_bytes as f64),
            &g.manifest_digest[..16]
        );
    }

    // --- the source history ------------------------------------------------
    let history = gitgen::plan(&profile.git, opts.seed);
    eprintln!(
        "planning the source history: {} commits, {} branches, {} tags",
        history.steps.len(),
        history.branches().len(),
        history.tags().len()
    );
    let history_manifests = stage_history(&history, &scratch)?;

    // --- the Nix fixture ---------------------------------------------------
    let mut nix_fixture_info = None;
    let nix = if groups.contains(&nixsc::GROUP)
        || selected.iter().any(|s| s.scenario == blobs::SCENARIO_NIX)
    {
        match build_nix_fixture(opts, &profile, &tools, &scratch, timeout) {
            Ok((fixture, info)) => {
                nix_fixture_info = Some(info);
                Some(fixture)
            }
            Err(e) => {
                for entry in &selected {
                    if entry.group == nixsc::GROUP || entry.scenario == blobs::SCENARIO_NIX {
                        for backend in &entry.backends {
                            skipped.insert(
                                format!("{backend} in {}", entry.scenario),
                                format!("the Nix fixture closure could not be built: {e}"),
                            );
                        }
                    }
                }
                None
            }
        }
    } else {
        None
    };

    // --- object storage ----------------------------------------------------
    let wants_blob = selected.iter().any(|s| s.group == blobs::GROUP);
    let mut service_guard: Option<LocalService> = None;
    let mut blob_ctx: Option<blobs::BlobCtx> = None;
    let mut blob_info: Option<BlobInfo> = None;
    if wants_blob {
        match start_blob(opts, &tools, &scratch, &run_id) {
            Ok((svc, ctx, info)) => {
                service_guard = svc;
                blob_info = Some(info);
                blob_ctx = Some(ctx);
            }
            Err(e) => {
                for entry in selected.iter().filter(|s| s.group == blobs::GROUP) {
                    for backend in &entry.backends {
                        skipped.insert(
                            format!("{backend} in {}", entry.scenario),
                            format!("the object store could not be started: {e}"),
                        );
                    }
                }
            }
        }
    }

    let ctx = Ctx {
        profile: profile.clone(),
        seed: opts.seed,
        jobs,
        scratch: scratch.clone(),
        out: out.clone(),
        tools: std::mem::take(&mut tools),
        corpus,
        history,
        history_manifests,
        nix,
        timeout,
        dropped_caches: dropped,
    };

    // --- run ---------------------------------------------------------------
    let order_seed = opts.order_seed.unwrap_or(opts.seed ^ 0x5bf0_3635_9d1a_77e1);
    let mut order = Vec::new();
    let mut runs: Vec<RunRecord> = Vec::new();
    for rep in 0..repeats {
        let mut pairs: Vec<(&'static str, &'static str, &'static str)> = Vec::new();
        for entry in &selected {
            for backend in &entry.backends {
                if skipped.contains_key(*backend)
                    || skipped.contains_key(&format!("{backend} in {}", entry.scenario))
                {
                    continue;
                }
                pairs.push((entry.group, entry.scenario, backend));
            }
        }
        // Randomising the order per repetition keeps a systematic advantage
        // (a warm cache, a cooler CPU, a quieter disk) from always landing on
        // the same backend. The seed is recorded so the order can be replayed.
        Rng::new(order_seed, &format!("order-{rep}")).shuffle(&mut pairs);
        for (index, (group, scenario, backend)) in pairs.into_iter().enumerate() {
            order.push(OrderEntry {
                rep,
                index,
                scenario: scenario.to_string(),
                backend: backend.to_string(),
            });
            eprintln!("[rep {rep} #{index}] {scenario} / {backend}");
            let record = match group {
                tree::GROUP => tree::run(&ctx, backend, rep, index),
                gitsc::GROUP => gitsc::run(&ctx, backend, rep, index),
                nixsc::GROUP => nixsc::run(&ctx, backend, rep, index),
                scenarios::backup::GROUP => scenarios::backup::run(&ctx, backend, rep, index),
                blobs::GROUP => match &blob_ctx {
                    Some(blob) => blobs::run(&ctx, blob, scenario, backend, rep, index),
                    None => continue,
                },
                other => {
                    return Err(format!("unknown scenario group {other}"));
                }
            };
            if !record.healthy() {
                eprintln!(
                    "    ! {} operation failure(s), {} verification failure(s){}",
                    record
                        .ops
                        .iter()
                        .filter(|o| o.status == crate::metrics::OpStatus::Failed)
                        .count(),
                    record.verifications.iter().filter(|v| !v.passed).count(),
                    record
                        .error
                        .as_ref()
                        .map(|e| format!(", aborted: {}", e.lines().next().unwrap_or("")))
                        .unwrap_or_default()
                );
            }
            runs.push(record);
            // Reclaim the repetition's scratch as soon as it is verified, so
            // a long profile does not need room for every store at once.
            if !opts.keep_scratch {
                let dir = scratch
                    .join("runs")
                    .join(format!("{}-{backend}-rep{rep}", group_dir(group, scenario)));
                let _ = fsx::make_writable_tree(&dir);
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
    }

    let cross_checks = cross_check_amber_roots(&runs);

    // --- tear down ---------------------------------------------------------
    if let Some(blob) = &blob_ctx {
        blob.gateway.shutdown();
    }
    drop(blob_ctx);
    if let Some(svc) = &mut service_guard {
        svc.stop();
    }
    drop(service_guard);

    let summary = report::summarize(&runs);
    let unsupported = report::unsupported(&runs);
    let errors = report::errors(&runs);
    let (harness_commit, harness_dirty) = toolbox::git_revision(&opts.harness_repo);

    let mut report = Report {
        schema_version: report::SCHEMA_VERSION,
        // Recomputed from the contents when the report is written.
        validity: Default::default(),
        run_id,
        harness_version: VERSION.to_string(),
        harness_commit,
        harness_dirty,
        started_unix_nanos: started,
        finished_unix_nanos: fsx::now_unix_nanos(),
        command_line: opts.command_line.clone(),
        profile,
        seed: opts.seed,
        order_seed,
        jobs,
        repeats,
        host,
        tools: ctx.tools,
        corpus: ctx.corpus,
        history: ctx.history.summary(),
        nix_fixture: nix_fixture_info,
        blob: blob_info,
        statistics_method: crate::stats::METHOD.to_string(),
        execution_order: order,
        runs,
        summary,
        unsupported,
        errors,
        cross_checks,
        skipped_backends: skipped,
    };
    report
        .write_all(&out)
        .map_err(|e| format!("writing the report: {e}"))?;

    if !opts.keep_scratch
        && let Err(e) = fsx::remove_owned(&scratch)
    {
        eprintln!("warning: could not remove the scratch directory: {e}");
    }
    // `Report::clean` already accounts for the cross-backend checks.
    let clean = report.clean();
    Ok((report, clean))
}

fn group_dir(group: &str, scenario: &str) -> String {
    if group == blobs::GROUP {
        format!(
            "{group}-{}",
            scenario.rsplit('/').next().unwrap_or(scenario)
        )
    } else {
        group.to_string()
    }
}

/// One selected scenario and the backends to run in it.
#[derive(Debug)]
struct Selection {
    group: &'static str,
    scenario: &'static str,
    backends: Vec<&'static str>,
}

fn select(
    opts: &Options,
    catalogue: &[(&'static str, &'static str, Vec<&'static str>)],
) -> Result<Vec<Selection>, String> {
    let want_group = |g: &str| {
        opts.groups
            .as_ref()
            .is_none_or(|list| list.iter().any(|x| x == g))
    };
    let want_scenario = |s: &str| {
        opts.scenarios
            .as_ref()
            .is_none_or(|list| list.iter().any(|x| x == s || s.ends_with(x.as_str())))
    };
    let want_backend = |b: &str| {
        opts.backends
            .as_ref()
            .is_none_or(|list| list.iter().any(|x| x == b))
    };

    let mut out = Vec::new();
    for (group, scenario, backends) in catalogue {
        if !want_group(group) || !want_scenario(scenario) {
            continue;
        }
        let picked: Vec<&'static str> = backends
            .iter()
            .copied()
            .filter(|b| want_backend(b))
            .collect();
        if picked.is_empty() {
            continue;
        }
        out.push(Selection {
            group,
            scenario,
            backends: picked,
        });
    }
    // A requested backend that exists nowhere in the selection is a mistake,
    // not something to run silently without.
    if let Some(list) = &opts.backends {
        for wanted in list {
            if !out.iter().any(|s| s.backends.contains(&wanted.as_str())) {
                let known: Vec<&str> = catalogue
                    .iter()
                    .flat_map(|(_, _, b)| b.iter().copied())
                    .collect();
                return Err(format!(
                    "backend {wanted:?} is not part of any selected scenario. \
                     Known backends: {}",
                    dedup(known).join(", ")
                ));
            }
        }
    }
    if let Some(list) = &opts.scenarios {
        for wanted in list {
            if !out
                .iter()
                .any(|s| s.scenario == wanted || s.scenario.ends_with(wanted.as_str()))
            {
                let known: Vec<&str> = catalogue.iter().map(|(_, s, _)| *s).collect();
                return Err(format!(
                    "scenario {wanted:?} is unknown. Known scenarios: {}",
                    known.join(", ")
                ));
            }
        }
    }
    if out.is_empty() {
        return Err("the selection matched no scenario at all".into());
    }
    Ok(out)
}

fn dedup(mut v: Vec<&str>) -> Vec<&str> {
    v.sort_unstable();
    v.dedup();
    v
}

/// Records one of the two Amber cores, with where its source came from.
///
/// This repository does not contain either core and does not guess where a
/// build of one might be: the binary is passed in, having been built from
/// the revision the flake pins (`run.sh`) or from a local checkout given as
/// a development override. A checkout wins over a pin, because its dirty
/// state is part of the answer to "what was measured"; with neither, nothing
/// about the source is claimed. No binary at all is a recorded miss with a
/// reason, which the runner turns into a refusal rather than a blank row.
fn record_core(
    tools: &mut Toolbox,
    name: &str,
    bin: Option<&Path>,
    repo: Option<&Path>,
    rev: Option<&str>,
) {
    let Some(path) = bin else {
        tools.miss(
            name,
            format!(
                "{name}: no executable was given. Pass --{name}-bin (run.sh \
                 builds it from the revision the flake pins and does this \
                 for you)."
            ),
        );
        return;
    };
    let source = match (repo, rev) {
        (Some(repo), _) if repo.join(".git").exists() => toolbox::Source::Checkout(repo),
        (_, Some(rev)) => toolbox::Source::PinnedRevision(rev),
        _ => toolbox::Source::Unknown,
    };
    tools.record_built(name, path, &[], source);
}

fn discover(opts: &Options, groups: &[&str], selected: &[Selection]) -> Toolbox {
    let mut tools = Toolbox::default();
    let backends: Vec<&str> = selected
        .iter()
        .flat_map(|s| s.backends.iter().copied())
        .collect();
    let needs = |b: &str| backends.contains(&b);

    if needs("amber-rust") || needs("amber-rust-s3") {
        record_core(
            &mut tools,
            "amber-rust",
            opts.amber_rust_bin.as_deref(),
            opts.amber_rust_repo.as_deref(),
            opts.amber_rust_rev.as_deref(),
        );
    }
    if needs("amber-go") || needs("amber-go-s3") {
        record_core(
            &mut tools,
            "amber-go",
            opts.amber_go_bin.as_deref(),
            opts.amber_go_repo.as_deref(),
            opts.amber_go_rev.as_deref(),
        );
    }
    if needs("git") || needs("git-bundle-s3") {
        tools.probe("git", None, &["--version"]);
    }
    if needs("restic") || needs("restic-s3") {
        tools.probe("restic", None, &["version"]);
    }
    if needs("nix") || needs("nix-binary-cache") {
        tools.probe("nix", None, &["--version"]);
    }
    if groups.contains(&blobs::GROUP) {
        tools.probe("mc", None, &["--version"]);
        if opts.remote_s3.is_none() {
            tools.probe("garage", None, &["--version"]);
        }
    }
    tools
}

/// Materialises every history state once into a staging tree and records a
/// manifest of each, observed from disk. Setup: never part of a measurement.
fn stage_history(history: &gitgen::History, scratch: &Path) -> Result<Vec<PathBuf>, String> {
    let staging = scratch.join("history-staging");
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let manifests = scratch.join("history-manifests");
    std::fs::create_dir_all(&manifests).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut previous = None;
    for i in 0..history.steps.len() {
        history
            .apply(i, previous, &staging)
            .map_err(|e| format!("staging history state {i}: {e}"))?;
        gitgen::normalize_mtimes(&staging).map_err(|e| e.to_string())?;
        previous = Some(i);
        let m = Manifest::observe(&format!("history/{i:04}"), &staging)
            .map_err(|e| format!("observing history state {i}: {e}"))?;
        let path = manifests.join(format!("{i:04}.json"));
        m.save(&path).map_err(|e| e.to_string())?;
        out.push(path);
    }
    Ok(out)
}

fn build_nix_fixture(
    opts: &Options,
    profile: &Profile,
    tools: &Toolbox,
    scratch: &Path,
    timeout: Duration,
) -> Result<(scenarios::NixFixture, NixFixtureInfo), String> {
    let bin = tools.path("nix")?;
    let cli = NixCli {
        bin,
        store: None,
        cache: ClientCache::Inherited,
        timeout,
    };
    // Supplied roots take precedence, and then nothing is built at all: the
    // point of the option is to measure a closure that already exists.
    let (fixture, flake_output) = if opts.nix_closure_roots.is_empty() {
        let link = scratch.join("nix-fixture");
        let flake_ref = format!("{}#{}", opts.flake_dir.display(), profile.nix_fixture);
        eprintln!("building the Nix fixture closure: {flake_ref}");
        let out = cli.build(&flake_ref, &link).ok()?;
        let root = PathBuf::from(out.last_stdout_line());
        if !root.exists() {
            return Err(format!(
                "nix build printed {:?}, which does not exist",
                root.display()
            ));
        }
        (
            nixsc::inspect_fixture(&cli, &root)?,
            Some(profile.nix_fixture.clone()),
        )
    } else {
        let roots = opts.nix_closure_roots.clone();
        eprintln!(
            "using {} supplied Nix closure root(s), read only: {}",
            roots.len(),
            roots.join(", ")
        );
        (nixsc::inspect_supplied(&cli, &roots)?, None)
    };
    let shared = fixture
        .gen1_closure
        .iter()
        .filter(|p| fixture.gen2_closure.contains(p))
        .count();
    let info = NixFixtureInfo {
        source: fixture.origin.describe(),
        flake_output,
        root: fixture.root.display().to_string(),
        gen1: fixture.gen1.clone(),
        gen2: fixture.gen2.clone(),
        gen1_closure_paths: fixture.gen1_closure.len(),
        gen2_closure_paths: fixture.gen2_closure.len(),
        shared_paths: shared,
        has_second_generation: fixture.origin.has_churn(),
    };
    Ok((fixture, info))
}

#[allow(clippy::type_complexity)]
fn start_blob(
    opts: &Options,
    tools: &Toolbox,
    scratch: &Path,
    run_id: &str,
) -> Result<(Option<LocalService>, blobs::BlobCtx, BlobInfo), String> {
    let mc = tools.path("mc")?;
    let mc_config = s3::config_dir(scratch);
    std::fs::create_dir_all(&mc_config).map_err(|e| e.to_string())?;
    let shaping = Shaping {
        latency_ms: opts.blob_latency_ms,
        bandwidth_bytes_per_s: opts.blob_bandwidth_bytes_per_s,
    };

    let (service_guard, target, service_desc, upstream) = match &opts.remote_s3 {
        Some(remote) => {
            let target = service::remote_target(
                &remote.endpoint,
                &remote.region,
                &remote.bucket,
                &remote.prefix,
                service::credentials_from_env()?,
            )?;
            let desc = format!(
                "user-supplied remote endpoint {} (bucket {}, region {})",
                target.endpoint, target.bucket, target.region
            );
            (None, target, desc, None)
        }
        None => {
            let garage = tools.path("garage")?;
            let dir = scratch.join("garage");
            eprintln!("starting the local S3 service");
            let svc = LocalService::start(&garage, &dir, opts.seed)?;
            let api = svc.api_addr;
            let desc = format!("local {} started by the harness", svc.version);
            let target = svc.target("", &format!("bench/{run_id}"));
            (Some(svc), target, desc, Some(api))
        }
    };

    // A remote endpoint is reached over TLS, which the gateway cannot look
    // inside; it is therefore used only for the local service, and a remote
    // run says so rather than reporting request counts it did not observe.
    let (endpoint, measurement) = match upstream {
        Some(addr) => {
            let gw = Gateway::start(addr, shaping).map_err(|e| format!("gateway: {e}"))?;
            let endpoint = gw.endpoint();
            let measurement = format!(
                "every backend's S3 traffic is forwarded verbatim through a \
                 counting gateway at {endpoint}; requests are counted by \
                 method and bodies by direction, including retries. The \
                 harness's own listing and cleanup calls bypass it."
            );
            (Some(gw), measurement)
        }
        None => {
            // Point the gateway at a dead loopback address: it is never used,
            // but keeping the type simple costs nothing and the counters stay
            // visibly zero rather than fabricated.
            let gw = Gateway::start("127.0.0.1:9".parse().unwrap(), shaping)
                .map_err(|e| format!("gateway: {e}"))?;
            (
                Some(gw),
                "no gateway: a remote endpoint is reached over TLS, which the \
                 harness cannot inspect. Request and byte counters are absent \
                 for this run rather than estimated."
                    .to_string(),
            )
        }
    };
    let gateway = endpoint.expect("a gateway is always constructed");
    let target = if opts.remote_s3.is_none() {
        let mut t = target;
        t.endpoint = gateway.endpoint();
        t
    } else {
        target
    };

    let info = BlobInfo {
        service: service_desc.clone(),
        gateway_endpoint: gateway.endpoint(),
        shaping: gateway.shaping().describe(),
        target: target.info(),
        measurement,
    };
    Ok((
        service_guard,
        blobs::BlobCtx {
            gateway,
            target,
            mc,
            mc_config,
            service: service_desc,
        },
        info,
    ))
}

/// Compares the root keys the two Amber cores produced for the same input.
///
/// The cores claim byte-identical content addressing; a benchmark that feeds
/// both of them the same tree is in a position to check that claim, so it
/// does.
#[allow(clippy::type_complexity)]
pub fn cross_check_amber_roots(runs: &[RunRecord]) -> Vec<Verification> {
    let mut out = Vec::new();
    let mut pairs: BTreeMap<(String, usize, String), (Option<String>, Option<String>)> =
        BTreeMap::new();
    for r in runs {
        let slot = match r.backend.as_str() {
            "amber-rust" => 0,
            "amber-go" => 1,
            _ => continue,
        };
        for (name, key) in &r.roots {
            let entry = pairs
                .entry((r.scenario.clone(), r.rep, name.clone()))
                .or_default();
            if slot == 0 {
                entry.0 = Some(key.clone());
            } else {
                entry.1 = Some(key.clone());
            }
        }
    }
    let mut agreed = 0;
    let mut disagreed = Vec::new();
    let mut unpaired = Vec::new();
    for ((scenario, rep, name), (rust, go)) in pairs {
        match (rust, go) {
            (Some(a), Some(b)) if a == b => agreed += 1,
            (Some(a), Some(b)) => {
                disagreed.push(format!("{scenario} rep{rep} {name}: rust {a} != go {b}"))
            }
            _ => unpaired.push(format!("{scenario} rep{rep} {name}")),
        }
    }
    if agreed == 0 && disagreed.is_empty() {
        return out;
    }
    let mut v = if disagreed.is_empty() {
        Verification::pass(
            "amber_cores_agree_on_root_keys",
            format!(
                "{agreed} tree(s) produced an identical root key in both cores, \
                 which is the byte-compatibility claim the two implementations \
                 make about their content addressing"
            ),
        )
    } else {
        Verification::fail(
            "amber_cores_agree_on_root_keys",
            format!(
                "{} of {} trees produced different root keys in the two cores",
                disagreed.len(),
                agreed + disagreed.len()
            ),
        )
    };
    v.corrupt = disagreed;
    v.missing = unpaired;
    out.push(v);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{OpStatus, Semantics};

    fn opts() -> Options {
        Options {
            profile: "smoke".into(),
            out: PathBuf::from("/tmp/out"),
            seed: 1,
            order_seed: None,
            repeats: None,
            jobs: None,
            groups: None,
            scenarios: None,
            backends: None,
            harness_repo: PathBuf::from("/repo"),
            amber_rust_repo: None,
            amber_go_repo: None,
            amber_rust_bin: None,
            amber_go_bin: None,
            amber_rust_rev: None,
            amber_go_rev: None,
            flake_dir: PathBuf::from("/repo"),
            nix_closure_roots: Vec::new(),
            blob_latency_ms: 0,
            blob_bandwidth_bytes_per_s: 0,
            remote_s3: None,
            drop_caches: false,
            keep_scratch: false,
            timeout_secs: None,
            command_line: "amber-cas-bench run".into(),
        }
    }

    #[test]
    fn the_default_selection_covers_every_group() {
        let sel = select(&opts(), &catalogue()).unwrap();
        let groups = dedup(sel.iter().map(|s| s.group).collect());
        assert_eq!(groups, vec!["backup", "blob", "git", "nix", "tree"]);
        assert!(sel.iter().filter(|s| s.group == "blob").count() >= 3);
    }

    #[test]
    fn an_unknown_backend_is_an_error_not_a_silent_omission() {
        let mut o = opts();
        o.backends = Some(vec!["not-a-backend".into()]);
        let err = select(&o, &catalogue()).unwrap_err();
        assert!(err.contains("not-a-backend"), "{err}");
        assert!(err.contains("Known backends"), "{err}");
    }

    /// An argument error must leave the filesystem exactly as it found it.
    /// Creating the output directory first would make the corrected command
    /// fail too, with a confusing "already exists".
    #[test]
    fn a_bad_selection_creates_nothing_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("report");
        let mut o = opts();
        o.out = out.clone();
        o.backends = Some(vec!["not-a-backend".into()]);
        let err = run(&o).unwrap_err();
        assert!(err.contains("not-a-backend"), "{err}");
        assert!(!out.exists(), "{} was created anyway", out.display());

        // The same for an unknown profile.
        let mut o = opts();
        o.out = out.clone();
        o.profile = "enormous".into();
        assert!(run(&o).unwrap_err().contains("unknown profile"));
        assert!(!out.exists());

        // And an existing output directory is refused without touching it.
        std::fs::create_dir_all(out.join("keep")).unwrap();
        let mut o = opts();
        o.out = out.clone();
        let err = run(&o).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert!(out.join("keep").exists());
        assert!(!out.join("scratch").exists());
    }

    #[test]
    fn an_unknown_scenario_is_an_error() {
        let mut o = opts();
        o.scenarios = Some(vec!["tree/nonsense".into()]);
        let err = select(&o, &catalogue()).unwrap_err();
        assert!(err.contains("Known scenarios"), "{err}");
    }

    #[test]
    fn selecting_a_group_keeps_only_that_group() {
        let mut o = opts();
        o.groups = Some(vec!["nix".into()]);
        let sel = select(&o, &catalogue()).unwrap();
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0].scenario, "nix/closure");
        assert_eq!(sel[0].backends, vec!["nix", "amber-rust", "amber-go"]);
    }

    #[test]
    fn selecting_a_backend_keeps_every_scenario_it_appears_in() {
        let mut o = opts();
        o.backends = Some(vec!["amber-rust".into()]);
        let sel = select(&o, &catalogue()).unwrap();
        assert!(sel.iter().all(|s| s.backends == vec!["amber-rust"]));
        assert!(sel.iter().any(|s| s.scenario == "tree/lifecycle"));
        assert!(sel.iter().any(|s| s.scenario == "nix/closure"));
    }

    #[test]
    fn every_backend_declares_which_tool_it_needs() {
        for (_, _, backends) in catalogue() {
            for b in backends {
                if b == "fs-sha256" {
                    assert_eq!(required_tool(b), "", "the baseline needs no external tool");
                } else {
                    assert!(!required_tool(b).is_empty(), "{b} declares no tool");
                }
            }
        }
    }

    fn amber_record(backend: &str, key: &str) -> RunRecord {
        let mut roots = BTreeMap::new();
        roots.insert("gen0".to_string(), key.to_string());
        RunRecord {
            group: "tree".into(),
            scenario: "tree/lifecycle".into(),
            backend: backend.into(),
            rep: 0,
            order_index: 0,
            semantics: Semantics {
                compression: String::new(),
                encryption: String::new(),
                durability: String::new(),
                concurrency: String::new(),
                transport: None,
                notes: vec![],
            },
            ops: vec![],
            roots,
            verifications: vec![],
            error: None,
        }
    }

    #[test]
    fn matching_root_keys_pass_the_cross_check() {
        let runs = vec![
            amber_record("amber-rust", "2204"),
            amber_record("amber-go", "2204"),
        ];
        let checks = cross_check_amber_roots(&runs);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].passed, "{:?}", checks[0]);
    }

    #[test]
    fn differing_root_keys_fail_the_cross_check_and_name_the_tree() {
        let runs = vec![
            amber_record("amber-rust", "2204"),
            amber_record("amber-go", "ffff"),
        ];
        let checks = cross_check_amber_roots(&runs);
        assert!(!checks[0].passed);
        assert!(checks[0].corrupt[0].contains("gen0"), "{:?}", checks[0]);
    }

    #[test]
    fn the_cross_check_is_absent_when_only_one_core_ran() {
        assert!(cross_check_amber_roots(&[amber_record("amber-rust", "2204")]).is_empty());
    }

    #[test]
    fn blob_run_directories_are_distinct_per_scenario() {
        assert_eq!(group_dir("tree", "tree/lifecycle"), "tree");
        assert_eq!(
            group_dir("blob", "blob/backup-corpus"),
            "blob-backup-corpus"
        );
        assert_ne!(
            group_dir("blob", "blob/backup-corpus"),
            group_dir("blob", "blob/nix-closure")
        );
    }

    /// The standalone repository holds neither core, so it never guesses at
    /// a build of one: without an explicit binary the backend is a recorded
    /// miss, and with one the recorded source is the checkout if there is
    /// one, otherwise the pin, otherwise nothing.
    #[test]
    fn a_core_records_its_source_and_refuses_to_guess_at_a_binary() {
        let mut tools = Toolbox::default();
        record_core(&mut tools, "amber-rust", None, None, Some("deadbeef"));
        let err = tools.get("amber-rust").unwrap_err();
        assert!(err.contains("no executable was given"), "{err}");
        assert!(err.contains("--amber-rust-bin"), "{err}");

        let sh = toolbox::which("sh").expect("sh");
        let mut tools = Toolbox::default();
        record_core(&mut tools, "amber-go", Some(&sh), None, Some("deadbeef"));
        let t = tools.get("amber-go").unwrap();
        assert_eq!(t.source_commit.as_deref(), Some("deadbeef"));
        assert!(!t.source_dirty);

        // A checkout override wins over the pin: its dirty state is part of
        // the answer, and a pinned revision cannot describe it.
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        assert!(
            std::process::Command::new(toolbox::which("git").expect("git"))
                .args(["init", "-q"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(repo.join("dirty"), b"x").unwrap();
        let mut tools = Toolbox::default();
        record_core(
            &mut tools,
            "amber-rust",
            Some(&sh),
            Some(repo),
            Some("deadbeef"),
        );
        let t = tools.get("amber-rust").unwrap();
        assert_ne!(t.source_commit.as_deref(), Some("deadbeef"));
        assert!(t.source_dirty, "an untracked file makes the tree dirty");

        // Neither: nothing is claimed about the source.
        let mut tools = Toolbox::default();
        record_core(&mut tools, "amber-rust", Some(&sh), None, None);
        assert!(tools.get("amber-rust").unwrap().source_commit.is_none());
    }

    #[test]
    fn a_status_is_only_ok_when_it_really_is() {
        assert_ne!(OpStatus::Ok, OpStatus::Unsupported);
    }
}
