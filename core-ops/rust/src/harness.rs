//! The case registry, the timing loop and the sample schema.
//!
//! This mirrors `../go/harness.go` field for field: both drivers write the
//! same JSON document, so the report pairs Go and Rust results by `(op,
//! workload)` without knowing anything about either language.

use std::any::Any;
use std::time::Instant;

use serde::Serialize;

use crate::env::{Env, Profile};

/// State a case's setup hands to its run function. Setup is never timed.
pub type State = Box<dyn Any>;

/// Builds the state a case reads. Never inside a measured interval.
pub type SetupFn = Box<dyn Fn(&Env) -> State>;

/// The measured call. Returns a value the driver accumulates, so the work
/// cannot be optimised away.
pub type RunFn = Box<dyn Fn(&Env, &mut State) -> u64>;

/// Releases per-repetition state. Never inside a measured interval.
pub type FreeFn = Box<dyn Fn(&Env, State)>;

/// One measured operation at one workload.
pub struct Case {
    pub group: &'static str,
    pub op: String,
    pub workload: String,
    /// The concurrency the operation was asked for: 1 for a strictly
    /// single-threaded call, n for an explicit worker count, and 0 for an
    /// operation that picks its own parallelism.
    pub threads: usize,
    /// Core operations performed by one `run` call. Short operations are
    /// batched so the measured interval stays far above the clock.
    pub ops: usize,
    /// Payload bytes moved per `run` call; 0 where no throughput figure is
    /// meaningful.
    pub bytes: i64,
    /// Rebuild the state before every repetition. Destructive operations
    /// need a fresh copy each time; making it stays outside the interval.
    pub per_rep: bool,
    pub setup: SetupFn,
    pub run: RunFn,
    pub free: Option<FreeFn>,
}

impl Case {
    pub fn new(
        group: &'static str,
        op: &str,
        workload: &str,
        threads: usize,
        ops: usize,
        setup: SetupFn,
        run: RunFn,
    ) -> Case {
        Case {
            group,
            op: op.to_string(),
            workload: workload.to_string(),
            threads,
            ops: ops.max(1),
            bytes: 0,
            per_rep: false,
            setup,
            run,
            free: None,
        }
    }

    pub fn bytes(mut self, n: i64) -> Case {
        self.bytes = n;
        self
    }

    pub fn per_rep(mut self, free: FreeFn) -> Case {
        self.per_rep = true;
        self.free = Some(free);
        self
    }
}

/// A correctness assertion. `digest`, when set, is a canonical fingerprint of
/// the operation's output; the report compares digests of the same check id
/// across the two cores, which is how format agreement is established rather
/// than assumed.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: String,
    pub group: String,
    pub op: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub digest: String,
    /// Whether the digest is supposed to equal the other core's. It is false
    /// where the two implementations are documented to be interoperable
    /// without being byte-identical — zstd-compressed record payloads come
    /// from libzstd here and klauspost/compress in Go, so records, segment
    /// bodies and wire packs containing them differ.
    pub comparable: bool,
}

/// An exported operation that exists in one core only, or a workload the core
/// cannot express. It never produces a sample: an absent operation must not
/// be readable as an infinitely fast one.
#[derive(Debug, Clone, Serialize)]
pub struct Unsupported {
    pub op: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub workload: String,
    pub reason: String,
}

/// One repetition of one case.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub group: String,
    pub op: String,
    pub workload: String,
    pub threads: usize,
    pub rep: usize,
    pub ops: usize,
    pub bytes: i64,
    pub wall_ns: i64,
    pub cpu_user_ns: i64,
    pub cpu_sys_ns: i64,
    pub max_rss_kib: i64,
    pub status: String,
}

/// A runtime-specific number with no comparable counterpart in the other
/// core. The report prints these in their own table.
#[derive(Debug, Clone, Serialize)]
pub struct Counter {
    pub op: String,
    pub workload: String,
    pub rep: usize,
    pub name: String,
    pub value: i64,
}

/// Everything a run accumulates.
#[derive(Default)]
pub struct Recorder {
    pub checks: Vec<Check>,
    pub samples: Vec<Sample>,
    pub counters: Vec<Counter>,
    pub unsupported: Vec<Unsupported>,
    pub sink: u64,
}

impl Recorder {
    /// Records a passing check whose digest must equal the other core's.
    pub fn pass(&mut self, group: &str, op: &str, id: &str, digest: impl Into<String>) {
        self.checks.push(Check {
            id: id.to_string(),
            group: group.to_string(),
            op: op.to_string(),
            passed: true,
            detail: String::new(),
            digest: digest.into(),
            comparable: true,
        });
    }

    /// Records a passing check whose digest is implementation-specific and
    /// must not be compared across cores.
    pub fn pass_local(&mut self, group: &str, op: &str, id: &str, digest: impl Into<String>) {
        self.checks.push(Check {
            id: id.to_string(),
            group: group.to_string(),
            op: op.to_string(),
            passed: true,
            detail: String::new(),
            digest: digest.into(),
            comparable: false,
        });
    }

    pub fn fail(&mut self, group: &str, op: &str, id: &str, detail: impl Into<String>) {
        self.checks.push(Check {
            id: id.to_string(),
            group: group.to_string(),
            op: op.to_string(),
            passed: false,
            detail: detail.into(),
            digest: String::new(),
            comparable: true,
        });
    }

    pub fn want(
        &mut self,
        group: &str,
        op: &str,
        id: &str,
        ok: bool,
        detail: impl Into<String>,
        digest: impl Into<String>,
    ) {
        if ok {
            self.pass(group, op, id, digest);
        } else {
            self.fail(group, op, id, detail);
        }
    }

    pub fn want_local(
        &mut self,
        group: &str,
        op: &str,
        id: &str,
        ok: bool,
        detail: impl Into<String>,
        digest: impl Into<String>,
    ) {
        if ok {
            self.pass_local(group, op, id, digest);
        } else {
            self.fail(group, op, id, detail);
        }
    }

    pub fn skip(&mut self, op: &str, workload: &str, reason: &str) {
        self.unsupported.push(Unsupported {
            op: op.to_string(),
            workload: workload.to_string(),
            reason: reason.to_string(),
        });
    }
}

fn cpu_time() -> (i64, i64) {
    // SAFETY: getrusage fills a plain POD struct we own.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
        return (0, 0);
    }
    let ns = |t: libc::timeval| t.tv_sec * 1_000_000_000 + t.tv_usec * 1_000;
    (ns(ru.ru_utime), ns(ru.ru_stime))
}

fn max_rss_kib() -> i64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
        return 0;
    }
    ru.ru_maxrss as i64
}

/// Runs one case: `warmup` unmeasured repetitions followed by `reps` measured
/// ones.
fn run_case(env: &Env, rec: &mut Recorder, c: &Case) {
    let p: &Profile = &env.profile;
    let mut shared = if c.per_rep {
        None
    } else {
        Some((c.setup)(env))
    };

    for _ in 0..p.warmup {
        match shared.as_mut() {
            Some(s) => rec.sink = rec.sink.wrapping_add((c.run)(env, s)),
            None => {
                let mut s = (c.setup)(env);
                rec.sink = rec.sink.wrapping_add((c.run)(env, &mut s));
                if let Some(f) = &c.free {
                    f(env, s);
                }
            }
        }
    }

    for rep in 0..p.reps {
        let mut owned = if c.per_rep {
            Some((c.setup)(env))
        } else {
            None
        };
        let state: &mut State = match owned.as_mut() {
            Some(s) => s,
            None => shared.as_mut().expect("shared state"),
        };
        let (u0, s0) = cpu_time();
        let t0 = Instant::now();
        let v = (c.run)(env, state);
        let wall = t0.elapsed();
        let (u1, s1) = cpu_time();
        rec.sink = rec.sink.wrapping_add(v);
        rec.samples.push(Sample {
            group: c.group.to_string(),
            op: c.op.clone(),
            workload: c.workload.clone(),
            threads: c.threads,
            rep,
            ops: c.ops,
            bytes: c.bytes,
            wall_ns: wall.as_nanos() as i64,
            cpu_user_ns: u1 - u0,
            cpu_sys_ns: s1 - s0,
            max_rss_kib: max_rss_kib(),
            status: "ok".to_string(),
        });
        if let Some(s) = owned
            && let Some(f) = &c.free
        {
            f(env, s);
        }
    }
    if let Some(s) = shared
        && let Some(f) = &c.free
    {
        f(env, s);
    }
}

/// Runs the registry in a stable order, so two runs of the same profile touch
/// the machine in the same sequence.
pub fn run_all(env: &Env, rec: &mut Recorder, mut cases: Vec<Case>) {
    cases.sort_by(|a, b| (a.group, &a.op, &a.workload).cmp(&(b.group, &b.op, &b.workload)));
    let trace = std::env::var_os("AMBER_CORE_OPS_TRACE").is_some();
    for c in &cases {
        let t0 = Instant::now();
        run_case(env, rec, c);
        if trace {
            eprintln!(
                "  {:<34} {:<28} {:8.1} ms",
                c.op,
                c.workload,
                t0.elapsed().as_secs_f64() * 1e3
            );
        }
    }
}

pub const SAMPLE_SCHEMA: &str = "amber-core-ops/samples/1";
