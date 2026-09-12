//! One module per storage system under comparison.
//!
//! Every adapter drives its backend through the command line that backend
//! actually ships, as a child process. That is what makes the CPU-time and
//! peak-RSS columns mean the same thing in every row, and it is what keeps
//! the harness from accidentally measuring a privileged in-process path that
//! no user of the software has.
//!
//! The one exception is [`fscas`], the uncompressed SHA-256 baseline, which
//! has no upstream: it is implemented here and invoked as a subcommand of the
//! harness binary, still as a separate child process.

pub mod amber;
pub mod fscas;
pub mod git;
pub mod nixstore;
pub mod restic;

use crate::metrics::Semantics;

/// The guarantees of each backend, recorded beside its numbers.
pub fn semantics(backend: &str) -> Semantics {
    match backend {
        "amber-rust" | "amber-go" => Semantics {
            compression: "per-record zstd; content keys hash the uncompressed \
                          bytes, so compression never affects addressing"
                .into(),
            encryption: "none: these cores store plaintext".into(),
            durability: "pack segments are appended and fsynced on seal; the \
                         reference store is a transactional key/value database \
                         (redb in Rust, Pebble in Go)"
                .into(),
            concurrency: "tree building is parallel across --jobs workers; \
                          pack writes are batched and deduplicated"
                .into(),
            transport: None,
            notes: vec![
                "The two cores are byte-compatible at the content-addressing \
                 layer: identical trees produce identical root keys, which the \
                 harness checks."
                    .into(),
                "The two stores are not byte-for-byte comparable in size: the \
                 reference database is redb in the Rust core and Pebble in the \
                 Go core, and redb preallocates several megabytes. The \
                 store_packstore_* and store_refs_* counters separate the pack \
                 bytes, which is what the storage comparison is about, from the \
                 database file."
                    .into(),
            ],
        },
        "restic" => Semantics {
            compression: "zstd (repository format 2), enabled by default".into(),
            encryption: "mandatory AES-256-CTR with Poly1305-AES authentication; \
                         every byte stored is encrypted and authenticated. This \
                         is real work the other backends do not do, and it is \
                         the main reason restic's CPU column is not comparable \
                         to theirs."
                .into(),
            durability: "pack files and index files are written then fsynced; \
                         snapshots are committed last"
                .into(),
            concurrency: "parallel file readers and uploaders".into(),
            transport: None,
            notes: vec![
                "restic deduplicates with content-defined chunking, like the \
                 Amber cores, but with its own parameters (512 KiB average). \
                 The harness does not try to equalise them: they are not \
                 exposed on restic's command line."
                    .into(),
            ],
        },
        "git" => Semantics {
            compression: "zlib per object, plus delta compression inside packs".into(),
            encryption: "none".into(),
            durability: "loose objects and packs are written then renamed; \
                         `core.fsyncObjectFiles` is left at its default"
                .into(),
            concurrency: "single-threaded for commits; packing uses --threads \
                          (left at its default)"
                .into(),
            transport: None,
            notes: vec![
                "Git stores whole file versions, deltified against other \
                 versions at pack time. It records no modification times, so \
                 restored trees are compared without them."
                    .into(),
            ],
        },
        "nix" => Semantics {
            compression: "none in a local store; xz for NARs published to a \
                          binary cache"
                .into(),
            encryption: "none; integrity comes from NAR hashes and optional \
                         signatures"
                .into(),
            durability: "store paths are built or unpacked into a temporary \
                         name, made read-only, then registered in the SQLite \
                         database"
                .into(),
            concurrency: "parallel substitution and copying".into(),
            transport: None,
            notes: vec![
                "A Nix store is not a general filesystem-tree store: it holds \
                 whole store paths with recorded references, and deduplicates \
                 only by whole path (plus optional hard-linking of identical \
                 files). Comparisons here are storage-layer comparisons."
                    .into(),
            ],
        },
        "fs-sha256" => Semantics {
            compression: "none, by construction".into(),
            encryption: "none".into(),
            durability: "objects are written to a temporary name, fsynced and \
                         renamed; the reference file is fsynced last"
                .into(),
            concurrency: "single-threaded, by construction".into(),
            transport: None,
            notes: vec![
                "This is the control, not a product: whole-file SHA-256 \
                 addressing with no chunking and no compression. It shows what \
                 the workload costs with every clever technique removed."
                    .into(),
            ],
        },
        other => Semantics {
            compression: "unrecorded".into(),
            encryption: "unrecorded".into(),
            durability: "unrecorded".into(),
            concurrency: "unrecorded".into(),
            transport: None,
            notes: vec![format!("no semantics recorded for backend {other}")],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_backend_states_its_encryption_and_durability() {
        for b in [
            "amber-rust",
            "amber-go",
            "restic",
            "git",
            "nix",
            "fs-sha256",
        ] {
            let s = semantics(b);
            assert!(!s.encryption.is_empty(), "{b}");
            assert!(!s.durability.is_empty(), "{b}");
            assert!(!s.compression.is_empty(), "{b}");
            assert!(!s.concurrency.is_empty(), "{b}");
            assert_ne!(s.encryption, "unrecorded", "{b}");
        }
    }

    #[test]
    fn restic_is_never_presented_as_doing_the_same_work_as_the_others() {
        assert!(semantics("restic").encryption.contains("mandatory"));
        assert!(semantics("amber-rust").encryption.contains("none"));
    }

    #[test]
    fn an_unknown_backend_says_so_rather_than_inventing_guarantees() {
        let s = semantics("something-else");
        assert_eq!(s.encryption, "unrecorded");
        assert!(s.notes[0].contains("no semantics recorded"));
    }
}
