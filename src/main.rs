//! `amber-cas-bench`: the command line.
//!
//! `run` is the whole suite; `clean` removes a run's scratch data while
//! keeping its reports; `profiles` prints the sizes each profile uses;
//! `fs-sha256` is the SHA-256 baseline store, invoked as a child process by
//! the harness itself rather than by a user.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use amber_cas_bench::adapters::fscas;
use amber_cas_bench::config;
use amber_cas_bench::publish;
use amber_cas_bench::run::{self, Options, RemoteS3};
use amber_cas_bench::util::fsx;

fn main() {
    // The baseline store is dispatched before clap, because its argument
    // grammar is its own and it is never typed by a person.
    let raw: Vec<String> = std::env::args().collect();
    if raw.get(1).map(|s| s.as_str()) == Some("fs-sha256") {
        if let Err(e) = fscas::main(&raw[2..]) {
            eprintln!("amber-cas-bench: {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = dispatch(Cli::parse(), &raw.join(" ")) {
        eprintln!("amber-cas-bench: {e}");
        std::process::exit(1);
    }
}

/// Reproducible comparison benchmarks for the Amber-Store cores.
#[derive(Parser)]
#[command(name = "amber-cas-bench", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Cmd {
    /// Run the benchmark suite and write a report.
    Run(RunArgs),
    /// Remove a run's scratch data, keeping every report it produced.
    Clean(CleanArgs),
    /// Export a finished run into the committable results directory.
    Publish(PublishArgs),
    /// Print the sizes each profile uses.
    Profiles,
}

#[derive(Args)]
struct PublishArgs {
    /// The output directory of a finished run. Not needed with --reindex.
    #[arg(long, required_unless_present = "reindex")]
    report: Option<PathBuf>,
    /// The results directory to add the run to.
    #[arg(long, default_value = "results")]
    results: PathBuf,
    /// Rebuild the results index from the runs already recorded, and do
    /// nothing else. The index is derived, so this is how a run that has
    /// been deleted leaves it.
    #[arg(long, conflicts_with = "report")]
    reindex: bool,
    /// Name of the run's directory inside --results. Defaults to the run's
    /// UTC start time and profile.
    #[arg(long)]
    id: Option<String>,
    /// A one-line note recorded with the run and shown in the index.
    #[arg(long)]
    label: Option<String>,
    /// Record a run that declared itself invalid. It is then marked invalid
    /// everywhere and no comparison chart is drawn for it.
    #[arg(long)]
    allow_invalid: bool,
    /// An extra absolute path to replace with a placeholder everywhere it
    /// appears in the published artefacts. The run's own output, scratch,
    /// repository and home directories are replaced anyway. Repeatable.
    #[arg(long, value_name = "PATH")]
    redact: Vec<PathBuf>,
}

#[derive(Args)]
struct RunArgs {
    /// Sizes to use: smoke, standard or large.
    #[arg(long, default_value = "smoke")]
    profile: String,
    /// Output directory. It must not already exist.
    #[arg(long)]
    out: PathBuf,
    /// Seed for the corpus and every generated fixture.
    #[arg(long, default_value_t = 20260912)]
    seed: u64,
    /// Seed for the randomised backend order (defaults to a function of
    /// --seed, and is always recorded in the report).
    #[arg(long)]
    order_seed: Option<u64>,
    /// Repetitions of every (scenario, backend) pair; defaults to the
    /// profile's.
    #[arg(long)]
    repeats: Option<usize>,
    /// Worker threads offered to backends that take a count.
    #[arg(long, short = 'j')]
    jobs: Option<usize>,
    /// Scenario groups to run: tree, git, nix, backup, blob.
    #[arg(long, value_delimiter = ',')]
    groups: Option<Vec<String>>,
    /// Scenarios to run, by full name or suffix.
    #[arg(long, value_delimiter = ',')]
    scenarios: Option<Vec<String>>,
    /// Backends to run. A name that appears in no selected scenario is an
    /// error, not a silent omission.
    #[arg(long, value_delimiter = ',')]
    backends: Option<Vec<String>>,
    /// The built Rust `amber-store` binary. `run.sh` builds it from the
    /// revision this repository's flake pins and passes it here.
    #[arg(long, env = "AMBER_RUST_BIN")]
    amber_rust_bin: Option<PathBuf>,
    /// The built Go `amber-store` binary. `run.sh` builds it from the
    /// revision this repository's flake pins and passes it here.
    #[arg(long, env = "AMBER_GO_BIN")]
    amber_go_bin: Option<PathBuf>,
    /// Source revision of the Rust binary when it was built from pinned
    /// source rather than from a checkout. `run.sh` passes the revision its
    /// flake pins; with a local checkout the revision and dirty state are
    /// read from the checkout instead.
    #[arg(long)]
    amber_rust_rev: Option<String>,
    /// Source revision of the Go binary when it was built from pinned
    /// source rather than from a checkout.
    #[arg(long)]
    amber_go_rev: Option<String>,
    /// Development override: the local checkout the Rust binary was built
    /// from. Its revision *and* dirty state are then recorded in place of a
    /// pinned revision.
    #[arg(long, alias = "core-rs-repo", env = "AMBER_RUST_REPO")]
    amber_rust_repo: Option<PathBuf>,
    /// Development override: the local checkout the Go binary was built
    /// from. Its revision *and* dirty state are then recorded in place of a
    /// pinned revision.
    #[arg(long, alias = "go-repo", env = "AMBER_GO_REPO")]
    amber_go_repo: Option<PathBuf>,
    /// Checkout of this benchmark repository. Its revision and dirty state
    /// are recorded in the report as the harness's own.
    #[arg(long, default_value = ".")]
    harness_repo: PathBuf,
    /// Directory holding the benchmark flake, used to build the Nix fixture.
    #[arg(long, default_value = ".")]
    flake_dir: PathBuf,
    /// Measure an existing closure instead of the deterministic fixture.
    ///
    /// Give a store path; give the option twice to supply a second
    /// generation. The paths are read only: the harness queries their NAR
    /// hashes and references, copies them into stores of its own, and never
    /// writes to them. With a single root there is no churn, and every
    /// operation that needs a changed generation is recorded as unsupported
    /// rather than invented.
    #[arg(long = "nix-closure-root", value_name = "STORE_PATH")]
    nix_closure_roots: Vec<PathBuf>,
    /// Milliseconds of simulated delay injected per HTTP message per
    /// direction by the measuring gateway.
    #[arg(long, default_value_t = 0)]
    blob_latency_ms: u64,
    /// Simulated bandwidth cap in bytes per second per direction.
    #[arg(long, default_value_t = 0)]
    blob_bandwidth_bytes_per_s: u64,
    /// Use a remote S3 endpoint instead of the local service. Credentials
    /// come from AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY.
    #[arg(long)]
    remote_s3_endpoint: Option<String>,
    /// Bucket for --remote-s3-endpoint.
    #[arg(long)]
    remote_s3_bucket: Option<String>,
    /// Region for --remote-s3-endpoint.
    #[arg(long, default_value = "us-east-1")]
    remote_s3_region: String,
    /// Unique object prefix for --remote-s3-endpoint. Required: the harness
    /// never touches an object outside it.
    #[arg(long)]
    remote_s3_prefix: Option<String>,
    /// Drop the page cache before the run. Needs write access to
    /// /proc/sys/vm/drop_caches; the report records whether it worked.
    #[arg(long)]
    drop_caches: bool,
    /// Keep the scratch data instead of deleting it when the run finishes.
    #[arg(long)]
    keep_scratch: bool,
    /// Per-command deadline in seconds; defaults to the profile's.
    #[arg(long)]
    timeout_secs: Option<u64>,
}

#[derive(Args)]
struct CleanArgs {
    /// The output directory of a previous run.
    #[arg(long)]
    out: PathBuf,
}

fn dispatch(cli: Cli, command_line: &str) -> Result<(), String> {
    match cli.cmd {
        Cmd::Profiles => {
            for name in config::PROFILES {
                let p = config::profile(name).expect("listed profile");
                println!("{name}: {}", p.description);
                println!(
                    "  corpus: {} generation(s), about {} each",
                    p.corpus.generations,
                    amber_cas_bench::report::markdown::bytes(
                        p.corpus.approx_generation_bytes() as f64
                    )
                );
                println!(
                    "  tiny files {} (<= {} each), large random {} x {}, compressible {} x {}, \
                     duplicates {}, shifted {}",
                    p.corpus.tiny_files,
                    amber_cas_bench::report::markdown::bytes(p.corpus.tiny_max_bytes as f64),
                    p.corpus.random_files,
                    amber_cas_bench::report::markdown::bytes(p.corpus.random_file_bytes as f64),
                    p.corpus.compressible_files,
                    amber_cas_bench::report::markdown::bytes(
                        p.corpus.compressible_file_bytes as f64
                    ),
                    p.corpus.duplicate_files,
                    p.corpus.shifted_files,
                );
                println!(
                    "  source history: {} main commits, {} files changed each, {} vendor files",
                    p.git.main_commits, p.git.changed_src_files_per_commit, p.git.vendor_files
                );
                println!("  nix fixture: {}", p.nix_fixture);
                println!(
                    "  segment size {}, repeats {}, free space wanted about {}",
                    amber_cas_bench::report::markdown::bytes(p.segment_size as f64),
                    p.repeats,
                    amber_cas_bench::report::markdown::bytes(p.approx_disk_bytes as f64),
                );
            }
            Ok(())
        }
        Cmd::Clean(args) => clean(&args.out),
        Cmd::Publish(args) if args.reindex => {
            std::fs::create_dir_all(&args.results)
                .map_err(|e| format!("{}: {e}", args.results.display()))?;
            let n = publish::reindex(&args.results)?;
            println!(
                "{}/README.md rebuilt from {n} recorded run(s)",
                args.results.display()
            );
            Ok(())
        }
        Cmd::Publish(args) => {
            let dir = publish::publish(&publish::Options {
                report_dir: args.report.expect("clap requires it without --reindex"),
                results_dir: args.results.clone(),
                id: args.id,
                label: args.label,
                allow_invalid: args.allow_invalid,
                redact: args.redact,
            })?;
            println!("published to {}", dir.display());
            println!("index rewritten: {}/README.md", args.results.display());
            Ok(())
        }
        Cmd::Run(args) => {
            let opts = options(args, command_line)?;
            let (report, clean) = run::run(&opts)?;
            println!();
            println!("report written to {}", opts.out.display());
            println!(
                "  {} operation samples, {} correctness check(s), {} failure(s)",
                report.runs.iter().map(|r| r.ops.len()).sum::<usize>(),
                report
                    .runs
                    .iter()
                    .map(|r| r.verifications.len())
                    .sum::<usize>(),
                report.errors.len()
            );
            for check in &report.cross_checks {
                println!(
                    "  {}: {}",
                    if check.passed { "pass" } else { "FAIL" },
                    check.detail
                );
            }
            if !clean {
                return Err(format!(
                    "the run completed but was not clean: {} failure(s) and {} \
                     unrunnable backend(s) are recorded in {}/REPORT.md",
                    report.errors.len(),
                    report.skipped_backends.len(),
                    opts.out.display()
                ));
            }
            Ok(())
        }
    }
}

/// Validates the supplied Nix closure roots.
///
/// They are checked here rather than deep in the run, so a mistyped path
/// fails before anything is created: each has to be an existing absolute
/// path, and there are at most two of them because the workload has exactly
/// two generations. Whether the store really holds them — and whether they
/// still hash to what it recorded — is settled later, by asking Nix.
fn nix_closure_roots(paths: &[PathBuf]) -> Result<Vec<String>, String> {
    if paths.len() > 2 {
        return Err(format!(
            "--nix-closure-root was given {} times; the workload has exactly \
             two generations, so it takes at most two (the first is \
             generation 1, the second is generation 2)",
            paths.len()
        ));
    }
    let mut out = Vec::new();
    for p in paths {
        if !p.is_absolute() {
            return Err(format!(
                "--nix-closure-root {} is not an absolute store path",
                p.display()
            ));
        }
        if !p.exists() {
            return Err(format!("--nix-closure-root {} does not exist", p.display()));
        }
        let s = p.display().to_string();
        if out.contains(&s) {
            return Err(format!(
                "--nix-closure-root {s} was given twice; two identical roots \
                 are not two generations"
            ));
        }
        out.push(s);
    }
    Ok(out)
}

fn options(args: RunArgs, command_line: &str) -> Result<Options, String> {
    let remote_s3 = match (&args.remote_s3_endpoint, &args.remote_s3_bucket) {
        (Some(endpoint), Some(bucket)) => Some(RemoteS3 {
            endpoint: endpoint.clone(),
            bucket: bucket.clone(),
            region: args.remote_s3_region.clone(),
            prefix: args.remote_s3_prefix.clone().ok_or_else(|| {
                "--remote-s3-prefix is required with --remote-s3-endpoint: a \
                 remote run must name a unique namespace of its own, so it can \
                 never remove an unrelated object"
                    .to_string()
            })?,
        }),
        (Some(_), None) => {
            return Err("--remote-s3-endpoint also needs --remote-s3-bucket".into());
        }
        (None, Some(_)) => {
            return Err("--remote-s3-bucket also needs --remote-s3-endpoint".into());
        }
        (None, None) => None,
    };
    Ok(Options {
        profile: args.profile,
        out: args.out,
        seed: args.seed,
        order_seed: args.order_seed,
        repeats: args.repeats,
        jobs: args.jobs,
        groups: args.groups,
        scenarios: args.scenarios,
        backends: args.backends,
        harness_repo: args.harness_repo,
        amber_rust_repo: args.amber_rust_repo,
        amber_go_repo: args.amber_go_repo,
        amber_rust_bin: args.amber_rust_bin,
        amber_go_bin: args.amber_go_bin,
        amber_rust_rev: args.amber_rust_rev,
        amber_go_rev: args.amber_go_rev,
        flake_dir: args.flake_dir,
        nix_closure_roots: nix_closure_roots(&args.nix_closure_roots)?,
        blob_latency_ms: args.blob_latency_ms,
        blob_bandwidth_bytes_per_s: args.blob_bandwidth_bytes_per_s,
        remote_s3,
        drop_caches: args.drop_caches,
        keep_scratch: args.keep_scratch,
        timeout_secs: args.timeout_secs,
        command_line: command_line.to_string(),
    })
}

/// Removes the scratch directory of a previous run and nothing else.
///
/// The reports stay. The scratch directory is identified by the marker file
/// the harness wrote when it created it, so `clean` cannot be pointed at an
/// arbitrary path and made to delete it.
fn clean(out: &std::path::Path) -> Result<(), String> {
    if !out.exists() {
        return Err(format!("{} does not exist", out.display()));
    }
    let out = out
        .canonicalize()
        .map_err(|e| format!("{}: {e}", out.display()))?;
    let scratch = out.join("scratch");
    if !scratch.exists() {
        println!("nothing to clean: {} has no scratch data", out.display());
        return Ok(());
    }
    fsx::remove_owned(&scratch)?;
    println!("removed {}", scratch.display());
    let kept: Vec<String> = std::fs::read_dir(&out)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    println!("kept: {}", kept.join(", "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn supplied_closure_roots_must_be_existing_absolute_paths() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();

        // None is the default: the deterministic fixture.
        assert!(nix_closure_roots(&[]).unwrap().is_empty());
        // One root is generation 1; two are the two generations.
        assert_eq!(
            nix_closure_roots(std::slice::from_ref(&a)).unwrap().len(),
            1
        );
        assert_eq!(nix_closure_roots(&[a.clone(), b.clone()]).unwrap().len(), 2);

        let err = nix_closure_roots(&[a.clone(), b.clone(), a.clone()]).unwrap_err();
        assert!(err.contains("at most two"), "{err}");
        let err = nix_closure_roots(&[a.clone(), a.clone()]).unwrap_err();
        assert!(err.contains("twice"), "{err}");
        let err = nix_closure_roots(&[dir.path().join("missing")]).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
        let err = nix_closure_roots(&[PathBuf::from("relative/path")]).unwrap_err();
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn a_remote_run_must_name_a_namespace_of_its_own() {
        let args = |f: fn(&mut RunArgs)| -> Result<Options, String> {
            let mut a = RunArgs {
                profile: "smoke".into(),
                out: PathBuf::from("/tmp/out"),
                seed: 1,
                order_seed: None,
                repeats: None,
                jobs: None,
                groups: None,
                scenarios: None,
                backends: None,
                harness_repo: PathBuf::from("."),
                amber_rust_repo: None,
                amber_go_repo: None,
                amber_rust_bin: None,
                amber_go_bin: None,
                amber_rust_rev: None,
                amber_go_rev: None,
                flake_dir: PathBuf::from("."),
                nix_closure_roots: Vec::new(),
                blob_latency_ms: 0,
                blob_bandwidth_bytes_per_s: 0,
                remote_s3_endpoint: None,
                remote_s3_bucket: None,
                remote_s3_region: "us-east-1".into(),
                remote_s3_prefix: None,
                drop_caches: false,
                keep_scratch: false,
                timeout_secs: None,
            };
            f(&mut a);
            options(a, "amber-cas-bench run")
        };

        let err = args(|a| {
            a.remote_s3_endpoint = Some("https://s3.example.com".into());
            a.remote_s3_bucket = Some("b".into());
        })
        .unwrap_err();
        assert!(err.contains("--remote-s3-prefix is required"), "{err}");

        let err =
            args(|a| a.remote_s3_endpoint = Some("https://s3.example.com".into())).unwrap_err();
        assert!(err.contains("--remote-s3-bucket"), "{err}");
        let err = args(|a| a.remote_s3_bucket = Some("b".into())).unwrap_err();
        assert!(err.contains("--remote-s3-endpoint"), "{err}");

        let ok = args(|a| {
            a.remote_s3_endpoint = Some("https://s3.example.com".into());
            a.remote_s3_bucket = Some("b".into());
            a.remote_s3_prefix = Some("run-1".into());
        })
        .unwrap();
        assert_eq!(ok.remote_s3.unwrap().prefix, "run-1");
    }
}
