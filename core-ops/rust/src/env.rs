//! Profile, environment and source identity.

use std::path::PathBuf;

use serde::Serialize;

use crate::fixtures::Fixtures;

/// Fixes every dimension of a run: how many repetitions are measured and how
/// big the fixtures are. Both cores are handed the same profile, so a paired
/// comparison always compares the same amount of work. The field names and
/// values mirror `../go/main.go`.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    pub seed: u64,
    pub reps: usize,
    pub warmup: usize,
    pub threads_single: usize,
    pub threads_multi: usize,
    pub corpus_bytes: i64,
    pub tree_files: usize,
    pub tree_wide: usize,
    pub tree_depth: usize,
    pub synthetic_wide: usize,
    pub store_objects: usize,
    pub segment_bytes: i64,
    pub ref_records: usize,
    pub inbox_packs: usize,
    pub batch_ops: usize,
    /// Roughly how many bytes each point of the (size, content) payload grid
    /// holds, so every size class costs about the same.
    pub payload_total: i64,
    /// The swept dimensions of the in-memory tree cases: how many entries a
    /// directory holds, how many levels a resolved path descends, and how
    /// many children a file index covers.
    pub tree_widths: Vec<usize>,
    pub tree_depths: Vec<usize>,
    pub fan_outs: Vec<usize>,
}

pub fn quick_profile(seed: u64) -> Profile {
    Profile {
        name: "quick".into(),
        seed,
        reps: 1,
        warmup: 0,
        threads_single: 1,
        threads_multi: 4,
        corpus_bytes: 4 << 20,
        tree_files: 120,
        tree_wide: 200,
        tree_depth: 8,
        synthetic_wide: 2000,
        store_objects: 1500,
        segment_bytes: 2 << 20,
        ref_records: 200,
        inbox_packs: 8,
        batch_ops: 200,
        payload_total: 1 << 20,
        tree_widths: vec![16, 256, 2000],
        tree_depths: vec![1, 4, 8],
        fan_outs: vec![8, 128, 1024],
    }
}

pub fn standard_profile(seed: u64) -> Profile {
    Profile {
        name: "standard".into(),
        seed,
        reps: 7,
        warmup: 2,
        threads_single: 1,
        threads_multi: 8,
        corpus_bytes: 64 << 20,
        tree_files: 1200,
        tree_wide: 4000,
        tree_depth: 24,
        synthetic_wide: 40000,
        store_objects: 30000,
        segment_bytes: 16 << 20,
        ref_records: 5000,
        inbox_packs: 48,
        batch_ops: 2000,
        payload_total: 8 << 20,
        tree_widths: vec![16, 256, 4096, 40000],
        tree_depths: vec![1, 4, 12, 24],
        fan_outs: vec![8, 128, 1024, 65536],
    }
}

/// What the report needs to trust a paired result: which core source the
/// driver was linked against, and the hash of the executable that produced
/// the samples. The harness revision is recorded separately, so a harness
/// edit is never mistaken for a core change.
#[derive(Debug, Clone, Serialize)]
pub struct Identity {
    pub core: String,
    pub core_repo: String,
    pub core_revision: String,
    pub core_dirty: bool,
    pub core_module: String,
    pub core_version: String,
    pub driver_path: String,
    pub driver_sha256: String,
    pub toolchain: String,
    pub harness_revision: String,
    pub harness_dirty: bool,
    pub build_settings: std::collections::BTreeMap<String, String>,
}

/// The machine the samples were taken on.
#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    pub host: String,
    pub os: String,
    pub arch: String,
    pub num_cpu: usize,
    /// The worker count an operation that picks its own parallelism actually
    /// got, under the CPU set the driver was pinned to. The Go driver
    /// records GOMAXPROCS in the same field; the report requires the two to
    /// be equal, because an `auto` case measured at different widths is not
    /// a comparison.
    pub auto_parallelism: usize,
    /// Kept under its native name as well, so the document says which
    /// primitive produced the number above.
    pub available_parallelism: usize,
    /// Whether the driver forces a garbage collection before every measured
    /// repetition. The Go driver does; this one has no collector to run, so
    /// it is false here and the report states the asymmetry rather than
    /// hiding it.
    pub forced_gc_before_rep: bool,
    pub scratch: String,
    pub scratch_fs: String,
    pub xattrs: bool,
}

/// Everything the cases read: the profile, the scratch directory and the
/// fixtures built once at startup.
pub struct Env {
    pub profile: Profile,
    pub scratch: PathBuf,
    /// The directory holding one wire pack per producing core, written by
    /// the `--emit-wire` pass before either driver measures anything.
    #[allow(dead_code)]
    pub wire_dir: PathBuf,
    /// The slice of the profile's repetitions this invocation measures.
    /// run.sh makes two passes in opposite core order and merges them, so
    /// neither core is systematically measured on a colder machine; see
    /// `../../../run.sh`.
    pub rep_base: usize,
    pub rep_count: usize,
    /// Whether the scratch filesystem accepted the fixture's extended
    /// attributes; recorded in the report so a run without them is not read
    /// as one with them.
    #[allow(dead_code)]
    pub xattrs: bool,
    pub fx: Fixtures,
}

impl Env {
    /// The worker count an operation that picks its own parallelism will
    /// use. Both drivers run under the same CPU set, so Go's GOMAXPROCS and
    /// this value agree.
    pub fn auto_jobs() -> usize {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
}
