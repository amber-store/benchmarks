//! Profiles, sizes and the knobs the command line exposes.
//!
//! Every size is written down here rather than being derived at run time, and
//! the whole structure is serialised into the report, so a number can always
//! be traced back to the configuration that produced it.

use serde::Serialize;

use crate::dataset::corpus::CorpusSpec;
use crate::dataset::gitgen::GitSpec;

/// Content-defined chunking and tree-building parameters.
///
/// These are passed *explicitly* to both Amber cores on every invocation
/// rather than left to each build's defaults, so the two are configured
/// identically even if a default ever diverges. The values are the current
/// documented defaults of both cores.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ChunkSettings {
    /// UltraCDC minimum chunk size.
    pub min: u64,
    /// UltraCDC average (normal) chunk size.
    pub avg: u64,
    /// UltraCDC maximum chunk size.
    pub max: u64,
    /// Item chunker average run = 2^bits.
    pub item_bits: u32,
    /// Extended attributes larger than this spill to a separate object.
    pub xattr_inline_max: u64,
}

impl ChunkSettings {
    /// 32 KiB / 512 KiB / 1 MiB, item-bits 7 — the documented defaults of
    /// both cores, stated explicitly.
    pub fn shared() -> ChunkSettings {
        ChunkSettings {
            min: 32 * 1024,
            avg: 512 * 1024,
            max: 1024 * 1024,
            item_bits: 7,
            xattr_inline_max: 256,
        }
    }
}

/// Sizes for the object-storage scenarios.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct BlobSettings {
    /// Simulated one-way delay injected at each request and each response, in
    /// milliseconds. Zero means the loopback path is measured as it is.
    pub latency_ms: u64,
    /// Simulated bandwidth cap per direction, in bytes per second. Zero means
    /// no cap.
    pub bandwidth_bytes_per_s: u64,
}

/// One named set of sizes.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    /// Human-readable statement of what this profile costs.
    pub description: String,
    pub corpus: CorpusSpec,
    pub git: GitSpec,
    /// Which `nix-fixtures-*` flake output the Nix group builds.
    pub nix_fixture: String,
    /// Repetitions of every (scenario, backend) pair.
    pub repeats: usize,
    /// Explicit pack segment size for both Amber cores.
    ///
    /// The cores default to 2 GiB. That is the right production value and the
    /// wrong benchmark value: a segment is the unit garbage collection reaps,
    /// so with 2 GiB segments a profile that stores less than 2 GiB can never
    /// reclaim anything and the GC measurement would be vacuous. Each profile
    /// therefore sets a segment size a few times smaller than the data it
    /// stores, identically for both cores, and the report says so.
    pub segment_size: u64,
    pub chunk: ChunkSettings,
    pub blob: BlobSettings,
    /// How many retained versions the source-history scenario restores and
    /// compares.
    ///
    /// `None` means *every* one of them, which is what the smoke and
    /// standard profiles do: a claim that all retained versions came back
    /// has to be earned by checking all of them. The large profile keeps a
    /// budget, and its check is then named `sampled_…` and reports how many
    /// versions it did not look at.
    pub verify_version_budget: Option<usize>,
    /// Rough disk footprint, for the pre-flight free-space check.
    pub approx_disk_bytes: u64,
    /// Per-command deadline in seconds.
    pub command_timeout_secs: u64,
}

/// The three profiles. `smoke` is what CI runs.
pub fn profile(name: &str) -> Option<Profile> {
    match name {
        "smoke" => Some(smoke()),
        "standard" => Some(standard()),
        "large" => Some(large()),
        _ => None,
    }
}

/// Every profile name, in increasing size.
pub const PROFILES: [&str; 3] = ["smoke", "standard", "large"];

fn smoke() -> Profile {
    let corpus = CorpusSpec {
        tiny_files: 200,
        tiny_max_bytes: 4 * 1024,
        random_files: 2,
        random_file_bytes: 4 * 1024 * 1024,
        compressible_files: 2,
        compressible_file_bytes: 4 * 1024 * 1024,
        duplicate_files: 2,
        shifted_files: 2,
        shift_insert_bytes: 4096,
        mixed_fanout: 2,
        mixed_depth: 3,
        mixed_files_per_dir: 4,
        mixed_file_bytes: 8 * 1024,
        generations: 3,
        churn_percent: 20,
        churn_added_files: 10,
        churn_deleted_files: 10,
    };
    let git = GitSpec {
        src_files: 120,
        src_file_bytes: 1024,
        docs_files: 6,
        docs_file_bytes: 4096,
        asset_files: 3,
        asset_file_bytes: 512 * 1024,
        vendor_files: 40,
        vendor_file_bytes: 8 * 1024,
        main_commits: 9,
        changed_src_files_per_commit: 4,
        branch_commits: 2,
    };
    Profile {
        name: "smoke".into(),
        description: "~35 MiB per corpus generation, 3 generations, 14 commits, \
                      the 5.7 MiB Nix fixture closure, 1 repetition by default. Minutes."
            .into(),
        verify_version_budget: None,
        approx_disk_bytes: 4 * 1024 * 1024 * 1024,
        corpus,
        git,
        nix_fixture: "nix-fixtures-smoke".into(),
        repeats: 1,
        segment_size: 8 * 1024 * 1024,
        chunk: ChunkSettings::shared(),
        blob: BlobSettings {
            latency_ms: 0,
            bandwidth_bytes_per_s: 0,
        },
        command_timeout_secs: 900,
    }
}

fn standard() -> Profile {
    let corpus = CorpusSpec {
        tiny_files: 5_000,
        tiny_max_bytes: 4 * 1024,
        random_files: 8,
        random_file_bytes: 64 * 1024 * 1024,
        compressible_files: 8,
        compressible_file_bytes: 64 * 1024 * 1024,
        duplicate_files: 8,
        shifted_files: 8,
        shift_insert_bytes: 64 * 1024,
        mixed_fanout: 3,
        mixed_depth: 4,
        mixed_files_per_dir: 8,
        mixed_file_bytes: 32 * 1024,
        generations: 3,
        churn_percent: 15,
        churn_added_files: 200,
        churn_deleted_files: 200,
    };
    let git = GitSpec {
        src_files: 1_500,
        src_file_bytes: 2048,
        docs_files: 40,
        docs_file_bytes: 8192,
        asset_files: 12,
        asset_file_bytes: 8 * 1024 * 1024,
        vendor_files: 400,
        vendor_file_bytes: 16 * 1024,
        main_commits: 40,
        changed_src_files_per_commit: 15,
        branch_commits: 5,
    };
    Profile {
        name: "standard".into(),
        description: "~2.1 GiB per corpus generation, 3 generations, 51 commits, \
                      the 180 MiB Nix fixture closure, 3 repetitions by default. Hours."
            .into(),
        verify_version_budget: None,
        approx_disk_bytes: 120 * 1024 * 1024 * 1024,
        corpus,
        git,
        nix_fixture: "nix-fixtures-standard".into(),
        repeats: 3,
        segment_size: 64 * 1024 * 1024,
        chunk: ChunkSettings::shared(),
        blob: BlobSettings {
            latency_ms: 0,
            bandwidth_bytes_per_s: 0,
        },
        command_timeout_secs: 3600,
    }
}

fn large() -> Profile {
    let corpus = CorpusSpec {
        tiny_files: 50_000,
        tiny_max_bytes: 4 * 1024,
        random_files: 16,
        random_file_bytes: 256 * 1024 * 1024,
        compressible_files: 16,
        compressible_file_bytes: 256 * 1024 * 1024,
        duplicate_files: 16,
        shifted_files: 16,
        shift_insert_bytes: 1024 * 1024,
        mixed_fanout: 4,
        mixed_depth: 4,
        mixed_files_per_dir: 8,
        mixed_file_bytes: 64 * 1024,
        generations: 3,
        churn_percent: 10,
        churn_added_files: 2_000,
        churn_deleted_files: 2_000,
    };
    let git = GitSpec {
        src_files: 8_000,
        src_file_bytes: 4096,
        docs_files: 200,
        docs_file_bytes: 16 * 1024,
        asset_files: 24,
        asset_file_bytes: 64 * 1024 * 1024,
        vendor_files: 2_000,
        vendor_file_bytes: 32 * 1024,
        main_commits: 80,
        changed_src_files_per_commit: 40,
        branch_commits: 10,
    };
    Profile {
        name: "large".into(),
        description: "~17 GiB per corpus generation, 3 generations, 101 commits, \
                      the 1.4 GiB Nix fixture closure, 3 repetitions by default. Many hours, \
                      and roughly 1 TiB of free space."
            .into(),
        // 101 commits restored and re-hashed three times over would dominate
        // the run; the check is named and counted as a sample instead.
        verify_version_budget: Some(12),
        approx_disk_bytes: 1024 * 1024 * 1024 * 1024,
        corpus,
        git,
        nix_fixture: "nix-fixtures-large".into(),
        repeats: 3,
        segment_size: 256 * 1024 * 1024,
        chunk: ChunkSettings::shared(),
        blob: BlobSettings {
            latency_ms: 0,
            bandwidth_bytes_per_s: 0,
        },
        command_timeout_secs: 14_400,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_profile_resolves() {
        for name in PROFILES {
            let p = profile(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(p.name, name);
        }
        assert!(profile("nope").is_none());
    }

    #[test]
    fn profiles_grow_monotonically() {
        let sizes: Vec<u64> = PROFILES
            .iter()
            .map(|n| profile(n).unwrap().corpus.approx_generation_bytes())
            .collect();
        assert!(sizes[0] < sizes[1] && sizes[1] < sizes[2], "{sizes:?}");
    }

    #[test]
    fn both_cores_get_the_same_explicit_chunk_settings() {
        for name in PROFILES {
            let c = profile(name).unwrap().chunk;
            assert_eq!(
                (c.min, c.avg, c.max, c.item_bits),
                (32768, 524288, 1048576, 7)
            );
        }
    }

    #[test]
    fn segment_size_is_small_enough_for_collection_to_reclaim_anything() {
        for name in PROFILES {
            let p = profile(name).unwrap();
            let per_gen = p.corpus.approx_generation_bytes();
            assert!(
                p.segment_size * 4 <= per_gen,
                "{name}: {} byte segments over {per_gen} bytes of data leaves \
                 nothing for gc to reap",
                p.segment_size
            );
        }
    }

    #[test]
    fn every_profile_has_enough_generations_for_a_changed_ingest() {
        for name in PROFILES {
            assert!(profile(name).unwrap().corpus.generations >= 2, "{name}");
        }
    }
}
