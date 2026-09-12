//! The case registry, the timing loop and the sample schema.
//!
//! This mirrors `../go/harness.go` field for field: both drivers write the
//! same JSON document, so the report pairs Go and Rust results by `(op,
//! workload)` without knowing anything about either language.

use std::any::Any;
use std::hint::black_box;
use std::time::Instant;

use serde::Serialize;

use crate::env::{Env, Profile};

/// State a case's setup hands to its run function. Setup is never timed.
pub type State = Box<dyn Any>;

/// Builds the state a case reads. Never inside a measured interval.
pub type SetupFn = Box<dyn Fn(&Env) -> State>;

/// The measured call. Performs the case's operations and returns an
/// accumulator built by consuming their outputs.
///
/// Consumption is deliberately cheap and constant-time per call: a
/// fixed-size output (a key, a header) is folded whole, and a byte stream
/// goes through [`crate::fixtures::sink_bytes`], which is `#[inline(never)]`
/// and reads only its length and its two ends. No O(payload) pass is ever
/// added inside a measured interval -- that would turn an encode measurement
/// into encode plus a second hash. The full output digests that prove the
/// two cores agree are computed in the checks, before any timing starts.
pub type RunFn = Box<dyn Fn(&Env, &mut State) -> u64>;

/// Releases per-repetition state. Never inside a measured interval.
pub type FreeFn = Box<dyn Fn(&Env, State)>;

/// What a case's `bytes` field counts. A rate computed from it is labelled
/// with this, because "bytes per second" over a metadata scan and over a
/// payload hash are not the same statement. The strings match `../go/harness.go`.
pub mod bytes_kind {
    /// Bytes of object payload the operation actually moved through memory.
    pub const PAYLOAD: &str = "payload";
    /// The logical size of the files a metadata walk covered. The walk reads
    /// directory entries and inode metadata, not the file bodies, so this
    /// measures the tree it traversed and not the bandwidth it achieved.
    pub const LOGICAL_SCANNED: &str = "logical-scanned";
    /// Payload bytes of the subset an operation actually included -- the
    /// filtered tree, the changed files -- counted outside the interval.
    pub const INCLUDED: &str = "included";
    /// Bytes of encoded, possibly compressed wire output. The two cores'
    /// encoders legitimately produce different sizes, so encoded rates are
    /// reported per core and never divided by one another.
    pub const ENCODED: &str = "encoded";
}

/// The independent workload dimensions of a case, as numbers. The workload
/// string names the point; this is what a scaling plot reads. Mirrors
/// `Dims` in `../go/harness.go` field for field, including the JSON names.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Dims {
    /// Size of one payload item, and how many of them one measured call
    /// processes. `item_bytes * items` is the payload the case moves.
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub item_bytes: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub items: i64,
    /// How the payload bytes were generated: "random" (incompressible),
    /// "text" (compressible), "duplicate" (every item identical), "empty",
    /// "structured" (an encoded node rather than a blob) or "partial-change".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// How many entries a tree holds, how many directory levels deep the
    /// measured path goes, and how wide the widest directory is.
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub entries: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub depth: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub width: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub shape: String,
    /// On-disk files and stored objects the operation covered, counted
    /// outside the measured interval.
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub files: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub objects: i64,
    /// The worker count the operation was asked for; 0 means it chose its own.
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub workers: i64,
    /// The dimension this case varies, and the family it varies within. A
    /// scaling curve is exactly the set of cases of one operation with the
    /// same sweep and series, and that is declared here rather than
    /// inferred: guessing which dimensions co-vary from the numbers alone
    /// produces curves that are secretly mixtures.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sweep: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub series: String,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

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
    /// What `bytes` counts; one of `bytes_kind`.
    pub bytes_kind: &'static str,
    /// The independent workload dimensions of this case.
    pub dims: Dims,
    /// Rebuild the state before every repetition. Destructive operations
    /// need a fresh copy each time; making it stays outside the interval.
    pub per_rep: bool,
    /// A case whose accumulator legitimately differs from one repetition to
    /// the next. Every other case must produce the same checksum in every
    /// repetition; the report enforces that, which catches a case that
    /// silently stopped doing its work.
    pub unstable: bool,
    /// A case whose accumulator must additionally equal the other core's.
    /// Set wherever the two cores are specified to produce the same bytes;
    /// not set where they legitimately differ (zstd encodings, storage-engine
    /// internals).
    pub cross_checksum: bool,
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
            bytes_kind: "",
            dims: Dims::default(),
            per_rep: false,
            unstable: false,
            cross_checksum: false,
            setup,
            run,
            free: None,
        }
    }

    /// Payload bytes per call, with the denominator's meaning attached.
    pub fn bytes_of(mut self, n: i64, kind: &'static str) -> Case {
        self.bytes = n;
        self.bytes_kind = kind;
        self
    }

    /// Payload bytes per call.
    pub fn bytes(self, n: i64) -> Case {
        self.bytes_of(n, bytes_kind::PAYLOAD)
    }

    pub fn dims(mut self, d: Dims) -> Case {
        self.dims = d;
        self
    }

    /// The accumulator must equal the other core's.
    pub fn cross(mut self) -> Case {
        self.cross_checksum = true;
        self
    }

    /// The accumulator legitimately varies between repetitions. No case
    /// needs it today -- every measured operation turned out to have a
    /// reproducible output -- but the report's rule is stated in terms of
    /// it, so the way to declare an exception stays in the API.
    #[allow(dead_code)]
    pub fn unstable(mut self) -> Case {
        self.unstable = true;
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

/// How large one core's encoder made a given input. The two cores'
/// compressors legitimately produce different sizes, so these are reported
/// side by side and never divided by one another: a rate over encoded bytes
/// would be a rate over two different denominators.
#[derive(Debug, Clone, Serialize)]
pub struct Encoding {
    pub op: String,
    pub workload: String,
    pub logical_bytes: i64,
    pub encoded_bytes: i64,
    pub items: i64,
}

/// One producing core's pack as this driver read it: the producer, the
/// content hash of the file and its encoded size. Both drivers read the same
/// files, so equal hashes here are the proof that a decode comparison was
/// made on identical bytes.
#[derive(Debug, Clone, Serialize)]
pub struct WireInput {
    pub producer: String,
    pub sha256: String,
    pub bytes: i64,
    pub objects: i64,
}

/// One repetition of one case.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub group: String,
    pub op: String,
    pub workload: String,
    pub threads: usize,
    pub dims: Dims,
    pub rep: usize,
    pub ops: usize,
    pub bytes: i64,
    pub bytes_kind: String,
    pub wall_ns: i64,
    pub cpu_user_ns: i64,
    pub cpu_sys_ns: i64,
    /// The whole process's high-water resident set at the end of this
    /// repetition, from getrusage. It only ever grows over the lifetime of
    /// the process, so it bounds the case rather than measuring it; it is
    /// not a per-operation peak and the report says so.
    pub max_rss_kib: i64,
    /// The accumulator the measured calls produced, as described on
    /// [`RunFn`]. Identical in every repetition of a stable case, and
    /// identical to the other core's for a case marked `cross_checksum`.
    ///
    /// It is evidence of output agreement and of the work not having been
    /// elided -- not, on its own, proof that the intended operation ran.
    /// The proof of that is the correctness suite, which digests complete
    /// outputs outside every measured interval.
    pub checksum: u64,
    pub stable: bool,
    pub cross_checksum: bool,
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
    pub encodings: Vec<Encoding>,
    pub wire_inputs: Vec<WireInput>,
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

    /// Records one encoder's output size for an input whose logical size is
    /// known. Always called outside a measured interval.
    pub fn encoded(&mut self, op: &str, workload: &str, logical: i64, encoded: i64, items: i64) {
        self.encodings.push(Encoding {
            op: op.to_string(),
            workload: workload.to_string(),
            logical_bytes: logical,
            encoded_bytes: encoded,
            items,
        });
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

    let mut dims = c.dims.clone();
    dims.workers = c.threads as i64;

    for _ in 0..p.warmup {
        match shared.as_mut() {
            Some(s) => rec.sink = rec.sink.wrapping_add(black_box((c.run)(env, s))),
            None => {
                let mut s = (c.setup)(env);
                rec.sink = rec.sink.wrapping_add(black_box((c.run)(env, &mut s)));
                if let Some(f) = &c.free {
                    f(env, s);
                }
            }
        }
    }

    for rep in env.rep_base..env.rep_base + env.rep_count {
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
        let v = black_box(v);
        rec.sink = rec.sink.wrapping_add(v);
        rec.samples.push(Sample {
            group: c.group.to_string(),
            op: c.op.clone(),
            workload: c.workload.clone(),
            threads: c.threads,
            dims: dims.clone(),
            rep,
            ops: c.ops,
            bytes: c.bytes,
            bytes_kind: c.bytes_kind.to_string(),
            wall_ns: wall.as_nanos() as i64,
            cpu_user_ns: u1 - u0,
            cpu_sys_ns: s1 - s0,
            max_rss_kib: max_rss_kib(),
            checksum: v,
            stable: !c.unstable,
            cross_checksum: c.cross_checksum,
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
