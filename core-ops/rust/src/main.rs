//! Measures the exported operations of the Rust Amber-Store core in process,
//! through its library APIs.
//!
//! The driver is deliberately small: it owns fixture construction, a case
//! registry and a timing loop, and writes one JSON document with the shared
//! raw-sample schema (see ../SCHEMA.md). Every counterpart case in the Go
//! driver (../go) carries the same op and workload names, so the report pairs
//! them without any per-language table.

mod cases;
mod env;
mod fixtures;
mod fixtures_build;
mod harness;
mod sha256;
mod stores;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;

use crate::env::{Env, Environment, Identity, Profile, quick_profile, standard_profile};
use crate::harness::{Check, Counter, Recorder, SAMPLE_SCHEMA, Sample, Unsupported};

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    core: &'static str,
    profile: String,
    config: Profile,
    identity: Identity,
    environment: Environment,
    started_at: String,
    finished_at: String,
    checks: Vec<Check>,
    samples: Vec<Sample>,
    counters: Vec<Counter>,
    unsupported: Vec<Unsupported>,
    blackhole: u64,
}

fn rfc3339(t: SystemTime) -> String {
    // A plain, stable UTC rendering: seconds since the epoch with
    // nanoseconds, formatted the way both drivers' reports are read.
    let d = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let nanos = d.subsec_nanos();
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, dd) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{dd:02}T{:02}:{:02}:{:02}.{nanos:09}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Howard Hinnant's civil-from-days, so the report carries a readable date
/// without pulling in a calendar crate.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const USAGE: &str = "\
amber-core-ops-rs --out <file.json> --scratch <dir> [options]

Measures the Rust Amber-Store core's exported operations in process and
writes one JSON document with the shared raw-sample schema. Driven by
core-ops/run.sh, which also builds the Go counterpart and the report.

  --profile quick|standard   fixture sizes and repetition counts (default quick)
  --out <file>               where to write the sample document (required)
  --scratch <dir>            scratch directory for on-disk fixtures (required)
  --seed <u64>               fixture seed; both cores must be given the same one
  --core-repo <label>        recorded provenance of the linked core source
  --core-revision <sha>      recorded revision of that core source
  --core-dirty               record that core source as dirty
  --harness-revision <sha>   recorded revision of this benchmark repository
  --harness-dirty            record the benchmark repository as dirty
  --xattrs                   the scratch filesystem accepts user.* xattrs
  --scratch-fs <type>        recorded filesystem type of the scratch directory
  --only <a,b,c>             run only these module groups";

struct Args {
    profile: String,
    out: PathBuf,
    scratch: PathBuf,
    seed: u64,
    core_repo: String,
    core_revision: String,
    core_dirty: bool,
    harness_revision: String,
    harness_dirty: bool,
    xattrs: bool,
    scratch_fs: String,
    only: Option<BTreeSet<String>>,
}

fn parse_args() -> Args {
    let mut a = Args {
        profile: "quick".into(),
        out: PathBuf::new(),
        scratch: PathBuf::new(),
        seed: 0x5EED_C0DE,
        core_repo: String::new(),
        core_revision: String::new(),
        core_dirty: false,
        harness_revision: String::new(),
        harness_dirty: false,
        xattrs: false,
        scratch_fs: String::new(),
        only: None,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let (flag, inline) = match argv[i].split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (argv[i].clone(), None),
        };
        let mut value = || -> String {
            if let Some(v) = inline.clone() {
                return v;
            }
            i += 1;
            argv.get(i)
                .cloned()
                .unwrap_or_else(|| panic!("missing value for {}", argv[i - 1]))
        };
        match flag.as_str() {
            "--profile" => a.profile = value(),
            "--out" => a.out = PathBuf::from(value()),
            "--scratch" => a.scratch = PathBuf::from(value()),
            "--seed" => {
                let v = value();
                a.seed = v.parse().unwrap_or_else(|_| panic!("bad --seed {v}"));
            }
            "--core-repo" => a.core_repo = value(),
            "--core-revision" => a.core_revision = value(),
            "--core-dirty" => a.core_dirty = true,
            "--harness-revision" => a.harness_revision = value(),
            "--harness-dirty" => a.harness_dirty = true,
            "--xattrs" => a.xattrs = true,
            "--scratch-fs" => a.scratch_fs = value(),
            "--only" => {
                a.only = Some(
                    value()
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect(),
                )
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => {
                eprintln!("amber-core-ops-rs: unknown flag {other}");
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    if a.out.as_os_str().is_empty() || a.scratch.as_os_str().is_empty() {
        eprintln!("amber-core-ops-rs: --out and --scratch are required");
        std::process::exit(2);
    }
    a
}

fn main() {
    let args = parse_args();
    let profile = match args.profile.as_str() {
        "quick" => quick_profile(args.seed),
        "standard" => standard_profile(args.seed),
        other => {
            eprintln!("amber-core-ops-rs: unknown profile {other}");
            std::process::exit(2);
        }
    };
    std::fs::create_dir_all(&args.scratch).unwrap();

    let exe = std::env::current_exe().unwrap_or_default();
    let identity = Identity {
        core: "rust".into(),
        core_repo: args.core_repo.clone(),
        core_revision: args.core_revision.clone(),
        core_dirty: args.core_dirty,
        core_module: "amber-store-core".into(),
        core_version: env!("CARGO_PKG_VERSION").into(),
        driver_path: exe.to_string_lossy().to_string(),
        driver_sha256: sha256::file_hex(&exe),
        toolchain: env!("AMBER_CORE_OPS_RUSTC").to_string(),
        harness_revision: args.harness_revision.clone(),
        harness_dirty: args.harness_dirty,
        build_settings: [
            ("target".to_string(), std::env::consts::ARCH.to_string()),
            ("os".to_string(), std::env::consts::OS.to_string()),
            ("profile".to_string(), "release".to_string()),
        ]
        .into_iter()
        .collect(),
    };
    let environment = Environment {
        host: std::fs::read_to_string("/proc/sys/kernel/hostname")
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        num_cpu: Env::auto_jobs(),
        available_parallelism: Env::auto_jobs(),
        scratch: args.scratch.to_string_lossy().to_string(),
        scratch_fs: args.scratch_fs.clone(),
        xattrs: args.xattrs,
    };

    let started = SystemTime::now();
    // Correctness first: the fixtures are built and validated before any
    // timing runs.
    let fx = fixtures_build::build_fixtures(&args.scratch, &profile, args.xattrs);
    let env = Env {
        profile: profile.clone(),
        scratch: args.scratch.clone(),
        xattrs: args.xattrs,
        fx,
    };

    let mut rec = Recorder::default();
    let case_list = cases::registry(&env, &args.only);
    cases::run_checks(&env, &mut rec, &args.only);

    let finish = |rec: &Recorder, env: &Env, code: i32| -> ! {
        if let Some(ro) = env.fx.ro.as_ref() {
            let _ = ro.close();
        }
        let report = Report {
            schema: SAMPLE_SCHEMA,
            core: "rust",
            profile: profile.name.clone(),
            config: profile.clone(),
            identity: identity.clone(),
            environment: environment.clone(),
            started_at: rfc3339(started),
            finished_at: rfc3339(SystemTime::now()),
            checks: rec.checks.clone(),
            samples: rec.samples.clone(),
            counters: rec.counters.clone(),
            unsupported: rec.unsupported.clone(),
            blackhole: rec.sink,
        };
        std::fs::write(
            &args.out,
            format!("{}\n", serde_json::to_string_pretty(&report).unwrap()),
        )
        .unwrap();
        let failed = rec.checks.iter().filter(|c| !c.passed).count();
        for c in rec.checks.iter().filter(|c| !c.passed) {
            eprintln!("FAIL {}: {}", c.id, c.detail);
        }
        println!(
            "rust: {} samples, {} checks ({failed} failed), {} unsupported, blackhole {}",
            rec.samples.len(),
            rec.checks.len(),
            rec.unsupported.len(),
            rec.sink
        );
        std::process::exit(if failed > 0 || code != 0 { 1 } else { 0 });
    };

    if rec.checks.iter().any(|c| !c.passed) {
        finish(&rec, &env, 1);
    }

    harness::run_all(&env, &mut rec, case_list);
    finish(&rec, &env, 0);
}
