//! The shared filesystem corpus.
//!
//! Every backend ingests the *same* bytes: one generator produces the whole
//! corpus, and the scenarios point different backends at the same directory.
//! Shapes that storage systems behave very differently on are all present and
//! separately named, so a report can say *where* a backend won:
//!
//! | subtree         | what it exercises                                    |
//! |-----------------|------------------------------------------------------|
//! | `tiny/`         | per-object overhead: thousands of 0-4 KiB files       |
//! | `random/`       | raw throughput: large incompressible files            |
//! | `compressible/` | the compressor: large, structured, text-like files    |
//! | `duplicates/`   | whole-file dedup: byte-identical copies of `random/`  |
//! | `shifted/`      | content-defined chunking: `random/` files with bytes  |
//! |                 | inserted near the front, which shifts every boundary  |
//! | `mixed/`        | tree metadata: deep directories, symlinks, exec bits, |
//! |                 | odd permissions, empty directories                    |
//!
//! Generation `g > 0` is the snapshot-churn axis: a deterministic subset of
//! files is rewritten, a deterministic prefix is deleted, and a deterministic
//! set is added. Content is a pure function of `(seed, label)`, so generation
//! `g` is reproducible on its own without materialising `g-1` first.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::manifest::Manifest;
use crate::util::fsx;
use crate::util::rng::{Rng, Stream};

/// Modification times are `BASE_MTIME + offset`, never "now", because both
/// Amber cores hash mtime into their directory objects: a corpus with wall
/// clock timestamps would have a different root key on every run.
pub const BASE_MTIME: i64 = 1_700_000_000;

/// How much of what to generate.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CorpusSpec {
    /// Files of 0..=`tiny_max_bytes` bytes under `tiny/`.
    pub tiny_files: u32,
    pub tiny_max_bytes: u64,
    /// Incompressible files under `random/`.
    pub random_files: u32,
    pub random_file_bytes: u64,
    /// Compressible, text-like files under `compressible/`.
    pub compressible_files: u32,
    pub compressible_file_bytes: u64,
    /// Byte-identical copies of `random/` files, under `duplicates/`.
    pub duplicate_files: u32,
    /// Near-copies of `random/` files with an inserted run, under `shifted/`.
    pub shifted_files: u32,
    /// Bytes inserted into each shifted file.
    pub shift_insert_bytes: u64,
    /// Directories at each level of `mixed/`, and how deep it goes.
    pub mixed_fanout: u32,
    pub mixed_depth: u32,
    pub mixed_files_per_dir: u32,
    pub mixed_file_bytes: u64,
    /// Number of generations, i.e. snapshots. Must be at least 2 for the
    /// changed-ingestion measurement to have anything to change.
    pub generations: u32,
    /// Percentage of files rewritten per generation.
    pub churn_percent: u64,
    /// Files added per generation.
    pub churn_added_files: u32,
    /// Files deleted per generation.
    pub churn_deleted_files: u32,
}

impl CorpusSpec {
    /// Upper bound on the bytes one generation occupies, for the disk-space
    /// check the runner performs before it starts writing.
    pub fn approx_generation_bytes(&self) -> u64 {
        let mixed_dirs = geometric_dirs(self.mixed_fanout, self.mixed_depth);
        self.tiny_files as u64 * (self.tiny_max_bytes / 2)
            + self.random_files as u64 * self.random_file_bytes
            + self.compressible_files as u64 * self.compressible_file_bytes
            + self.duplicate_files as u64 * self.random_file_bytes
            + self.shifted_files as u64 * (self.random_file_bytes + self.shift_insert_bytes)
            + mixed_dirs * self.mixed_files_per_dir as u64 * self.mixed_file_bytes
    }
}

fn geometric_dirs(fanout: u32, depth: u32) -> u64 {
    let mut total = 0u64;
    let mut level = 1u64;
    for _ in 0..depth {
        level = level.saturating_mul(fanout as u64);
        total += level;
    }
    total
}

/// What a file's bytes are.
#[derive(Debug, Clone)]
enum Payload {
    /// Incompressible: raw stream bytes under `label`.
    Stream { label: String, bytes: u64 },
    /// Compressible: generated text under `label`.
    Text { label: String, bytes: u64 },
    /// `base`'s first `at` bytes, then `insert` fresh bytes, then the rest of
    /// `base`. Every chunk boundary after `at` moves.
    Shifted {
        base: String,
        bytes: u64,
        at: u64,
        insert: u64,
    },
}

#[derive(Debug, Clone)]
struct FilePlan {
    path: String,
    payload: Payload,
    mode: u32,
    mtime: i64,
}

#[derive(Debug, Clone)]
struct LinkPlan {
    path: String,
    target: String,
    mtime: i64,
}

/// The complete description of one generation, before anything is written.
#[derive(Debug, Clone, Default)]
struct Plan {
    dirs: Vec<(String, i64)>,
    files: Vec<FilePlan>,
    links: Vec<LinkPlan>,
}

/// One materialised generation.
#[derive(Debug, Clone, Serialize)]
pub struct Generation {
    pub index: u32,
    /// Directory holding the generation's tree.
    pub dir: PathBuf,
    /// Path of the independently observed manifest.
    pub manifest_path: PathBuf,
    /// Sum of file sizes in this generation.
    pub logical_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub symlink_count: u64,
    /// Digest of the observed manifest: two runs with the same seed and spec
    /// must produce the same value.
    pub manifest_digest: String,
}

/// The generated corpus.
#[derive(Debug, Clone, Serialize)]
pub struct Corpus {
    pub root: PathBuf,
    pub seed: u64,
    pub spec: CorpusSpec,
    pub generations: Vec<Generation>,
}

impl Corpus {
    /// The generation a scenario treats as "the data".
    pub fn gen0(&self) -> &Generation {
        &self.generations[0]
    }

    /// Loads the manifest observed for generation `i`.
    pub fn manifest(&self, i: usize) -> io::Result<Manifest> {
        Manifest::load(&self.generations[i].manifest_path)
    }
}

/// Generates the corpus under `root`, which must already exist and be owned
/// by the harness. Writes one directory and one manifest per generation.
pub fn generate(spec: &CorpusSpec, seed: u64, root: &Path, jobs: usize) -> io::Result<Corpus> {
    assert!(spec.generations >= 1, "at least one generation is required");
    let mut generations = Vec::new();
    for g in 0..spec.generations {
        let dir = root.join(format!("gen{g}"));
        fs::create_dir_all(&dir)?;
        let plan = plan_generation(spec, seed, g);
        materialize(&plan, &dir, seed, jobs)?;
        let manifest = Manifest::observe(&format!("corpus/gen{g}"), &dir)?;
        let manifest_path = root.join(format!("gen{g}.manifest.json"));
        manifest.save(&manifest_path)?;
        generations.push(Generation {
            index: g,
            dir,
            manifest_path,
            logical_bytes: manifest.total_file_bytes,
            file_count: manifest.file_count,
            dir_count: manifest.dir_count,
            symlink_count: manifest.symlink_count,
            manifest_digest: manifest.digest(),
        });
    }
    Ok(Corpus {
        root: root.to_path_buf(),
        seed,
        spec: *spec,
        generations,
    })
}

/// True when file index `i` of class `class` is rewritten by generation `g`.
/// A pure function of the indices, so generation `g` never has to consult
/// generation `g-1`.
fn churned(spec: &CorpusSpec, class: &str, i: u32, g: u32) -> bool {
    if g == 0 || spec.churn_percent == 0 {
        return false;
    }
    let mut h = blake3::Hasher::new();
    h.update(b"churn");
    h.update(class.as_bytes());
    h.update(&i.to_le_bytes());
    let mut b = [0u8; 8];
    h.finalize_xof().fill(&mut b);
    u64::from_le_bytes(b) % 100 < spec.churn_percent
}

/// The content label of file `i` of `class` in generation `g`. Files that
/// have not churned keep generation 0's label, hence generation 0's bytes.
fn label(spec: &CorpusSpec, class: &str, i: u32, g: u32) -> String {
    if churned(spec, class, i, g) {
        format!("{class}/{i}@{g}")
    } else {
        format!("{class}/{i}")
    }
}

fn plan_generation(spec: &CorpusSpec, seed: u64, g: u32) -> Plan {
    let mut plan = Plan::default();
    let mut sizes = Rng::new(seed, "corpus-sizes");
    let mut modes = Rng::new(seed, "corpus-modes");

    // Deletions remove a deterministic prefix of tiny/, so a snapshot really
    // loses objects rather than only gaining them.
    let deleted = (spec.churn_deleted_files as u64 * g as u64).min(spec.tiny_files as u64) as u32;

    plan.dirs.push(("tiny".into(), BASE_MTIME + g as i64));
    for i in 0..spec.tiny_files {
        let bytes = sizes.below(spec.tiny_max_bytes + 1);
        if i < deleted {
            continue;
        }
        plan.files.push(FilePlan {
            path: format!("tiny/{i:06}.dat"),
            payload: Payload::Stream {
                label: label(spec, "tiny", i, g),
                bytes,
            },
            mode: if modes.chance(1, 10) { 0o755 } else { 0o644 },
            mtime: BASE_MTIME + (i as i64 % 5000),
        });
    }

    plan.dirs.push(("random".into(), BASE_MTIME + g as i64));
    for i in 0..spec.random_files {
        plan.files.push(FilePlan {
            path: format!("random/{i:04}.bin"),
            payload: Payload::Stream {
                label: label(spec, "random", i, g),
                bytes: spec.random_file_bytes,
            },
            mode: 0o644,
            mtime: BASE_MTIME + 10_000 + i as i64,
        });
    }

    plan.dirs
        .push(("compressible".into(), BASE_MTIME + g as i64));
    for i in 0..spec.compressible_files {
        plan.files.push(FilePlan {
            path: format!("compressible/{i:04}.txt"),
            payload: Payload::Text {
                label: label(spec, "compressible", i, g),
                bytes: spec.compressible_file_bytes,
            },
            mode: 0o644,
            mtime: BASE_MTIME + 20_000 + i as i64,
        });
    }

    // Exact duplicates carry another file's label verbatim, so their bytes
    // are identical and a deduplicating store should pay nothing for them.
    if spec.random_files > 0 {
        plan.dirs.push(("duplicates".into(), BASE_MTIME + g as i64));
        for i in 0..spec.duplicate_files {
            let src = i % spec.random_files;
            plan.files.push(FilePlan {
                path: format!("duplicates/{i:04}.bin"),
                payload: Payload::Stream {
                    label: label(spec, "random", src, g),
                    bytes: spec.random_file_bytes,
                },
                mode: 0o644,
                mtime: BASE_MTIME + 30_000 + i as i64,
            });
        }

        plan.dirs.push(("shifted".into(), BASE_MTIME + g as i64));
        for i in 0..spec.shifted_files {
            let src = i % spec.random_files;
            // Insert near the front: a fixed-block store must then rewrite
            // every following block, a content-defined chunker must not.
            let at = spec.random_file_bytes / 16 + i as u64 * 977;
            plan.files.push(FilePlan {
                path: format!("shifted/{i:04}.bin"),
                payload: Payload::Shifted {
                    base: label(spec, "random", src, g),
                    bytes: spec.random_file_bytes,
                    at: at.min(spec.random_file_bytes),
                    insert: spec.shift_insert_bytes,
                },
                mode: 0o644,
                mtime: BASE_MTIME + 40_000 + i as i64,
            });
        }
    }

    plan_mixed(spec, g, &mut plan, &mut modes);

    // Additions: files that exist only from generation g onwards.
    for a in 0..(spec.churn_added_files * g) {
        plan.dirs.push(("added".into(), BASE_MTIME + g as i64));
        plan.files.push(FilePlan {
            path: format!("added/{a:06}.dat"),
            payload: Payload::Stream {
                label: format!("added/{a}"),
                bytes: spec.tiny_max_bytes.max(1),
            },
            mode: 0o644,
            mtime: BASE_MTIME + 50_000 + a as i64,
        });
    }

    plan.dirs.sort();
    plan.dirs.dedup();
    plan
}

/// The metadata-heavy subtree: `mixed/d0/d1/...` with files, executables,
/// relative and dangling symlinks, an empty directory at every level, and one
/// directory whose permissions are unusual.
fn plan_mixed(spec: &CorpusSpec, g: u32, plan: &mut Plan, modes: &mut Rng) {
    let mut dirs = vec![String::from("mixed")];
    plan.dirs.push(("mixed".into(), BASE_MTIME + g as i64));
    for depth in 0..spec.mixed_depth {
        let mut next = Vec::new();
        for parent in &dirs {
            for b in 0..spec.mixed_fanout {
                let d = format!("{parent}/d{depth}_{b}");
                plan.dirs
                    .push((d.clone(), BASE_MTIME + 60_000 + depth as i64));
                next.push(d);
            }
            plan.dirs.push((
                format!("{parent}/empty"),
                BASE_MTIME + 60_000 + depth as i64,
            ));
        }
        dirs = next;
    }
    let mut index = 0u32;
    for d in &dirs {
        for f in 0..spec.mixed_files_per_dir {
            let exec = modes.chance(1, 4);
            plan.files.push(FilePlan {
                path: format!("{d}/f{f}.dat"),
                payload: Payload::Stream {
                    label: label(spec, "mixed", index, g),
                    bytes: spec.mixed_file_bytes,
                },
                mode: if exec { 0o750 } else { 0o640 },
                mtime: BASE_MTIME + 70_000 + index as i64,
            });
            index += 1;
        }
        if spec.mixed_files_per_dir > 0 {
            plan.links.push(LinkPlan {
                path: format!("{d}/rel.link"),
                target: "f0.dat".into(),
                mtime: BASE_MTIME + 80_000,
            });
        }
        plan.links.push(LinkPlan {
            path: format!("{d}/dangling.link"),
            target: "../nowhere/at/all".into(),
            mtime: BASE_MTIME + 80_001,
        });
    }
}

/// Writes a plan to disk. Files are written by `jobs` worker threads; content
/// depends only on the label, so the result does not depend on the schedule.
fn materialize(plan: &Plan, root: &Path, seed: u64, jobs: usize) -> io::Result<()> {
    for (d, _) in &plan.dirs {
        fs::create_dir_all(root.join(d))?;
    }
    for f in &plan.files {
        if let Some(parent) = Path::new(&f.path).parent() {
            fs::create_dir_all(root.join(parent))?;
        }
    }

    let jobs = jobs.max(1);
    let chunks: Vec<&[FilePlan]> = plan
        .files
        .chunks(plan.files.len().div_ceil(jobs).max(1))
        .collect();
    let mut failures: Vec<io::Error> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .iter()
            .map(|chunk| {
                scope.spawn(move || -> io::Result<()> {
                    for f in chunk.iter() {
                        write_payload(&root.join(&f.path), &f.payload, seed)?;
                        fs::set_permissions(
                            root.join(&f.path),
                            fs::Permissions::from_mode(f.mode),
                        )?;
                    }
                    Ok(())
                })
            })
            .collect();
        for h in handles {
            match h.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => failures.push(e),
                Err(_) => failures.push(io::Error::other("corpus writer panicked")),
            }
        }
    });
    if let Some(e) = failures.into_iter().next() {
        return Err(e);
    }

    for l in &plan.links {
        let p = root.join(&l.path);
        let _ = fs::remove_file(&p);
        std::os::unix::fs::symlink(&l.target, &p)?;
    }

    // Timestamps last, and directories after their contents: writing a child
    // updates its parent's mtime.
    for f in &plan.files {
        fsx::set_mtime(&root.join(&f.path), f.mtime, 0)?;
    }
    for l in &plan.links {
        fsx::set_mtime(&root.join(&l.path), l.mtime, 0)?;
    }
    let mut dirs = plan.dirs.clone();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.0.len()));
    for (d, mtime) in &dirs {
        fsx::set_mtime(&root.join(d), *mtime, 0)?;
    }
    fsx::set_mtime(root, BASE_MTIME, 0)?;
    Ok(())
}

/// Writes one generated file: `text` picks the compressible generator, so a
/// caller outside this module can build trees from the same byte sources the
/// corpus uses.
pub fn write_blob(path: &Path, seed: u64, label: &str, bytes: u64, text: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let f = fs::File::create(path)?;
    let mut w = io::BufWriter::with_capacity(CHUNK, f);
    if text {
        text_into(&mut w, seed, label, bytes)?;
    } else {
        stream_into(&mut w, seed, label, bytes)?;
    }
    w.flush()
}
const CHUNK: usize = 1 << 20;

fn write_payload(path: &Path, payload: &Payload, seed: u64) -> io::Result<()> {
    let f = fs::File::create(path)?;
    let mut w = io::BufWriter::with_capacity(CHUNK, f);
    match payload {
        Payload::Stream { label, bytes } => stream_into(&mut w, seed, label, *bytes)?,
        Payload::Text { label, bytes } => text_into(&mut w, seed, label, *bytes)?,
        Payload::Shifted {
            base,
            bytes,
            at,
            insert,
        } => {
            let mut s = Stream::new(seed, base);
            copy_stream(&mut w, &mut s, *at)?;
            let mut ins = Stream::new(seed, &format!("{base}#insert"));
            copy_stream(&mut w, &mut ins, *insert)?;
            copy_stream(&mut w, &mut s, bytes - at)?;
        }
    }
    w.flush()?;
    Ok(())
}

pub(crate) fn stream_into(
    w: &mut impl Write,
    seed: u64,
    label: &str,
    bytes: u64,
) -> io::Result<()> {
    let mut s = Stream::new(seed, label);
    copy_stream(w, &mut s, bytes)
}

fn copy_stream(w: &mut impl Write, s: &mut Stream, bytes: u64) -> io::Result<()> {
    let mut buf = vec![0u8; CHUNK];
    let mut left = bytes;
    while left > 0 {
        let n = CHUNK.min(left as usize);
        s.fill(&mut buf[..n]);
        w.write_all(&buf[..n])?;
        left -= n as u64;
    }
    Ok(())
}

/// A 512-word dictionary generated from the seedless stream, so the same
/// vocabulary is used regardless of `--seed` and only the word *order* varies.
fn dictionary() -> Vec<String> {
    let mut rng = Rng::new(0, "corpus-dictionary");
    (0..512)
        .map(|_| {
            let len = rng.range(3, 11) as usize;
            (0..len)
                .map(|_| (b'a' + rng.below(26) as u8) as char)
                .collect()
        })
        .collect()
}

/// Compressible, text-like content: numbered lines of dictionary words. It
/// compresses several-fold, but the running line number keeps it from being
/// trivially deduplicated block for block.
pub(crate) fn text_into(w: &mut impl Write, seed: u64, label: &str, bytes: u64) -> io::Result<()> {
    let dict = dictionary();
    let mut rng = Rng::new(seed, label);
    let mut written = 0u64;
    let mut line = 0u64;
    let mut buf: Vec<u8> = Vec::with_capacity(CHUNK + 128);
    while written < bytes {
        buf.clear();
        while buf.len() < CHUNK && (written + buf.len() as u64) < bytes {
            buf.extend_from_slice(format!("{line:08} ").as_bytes());
            let words = rng.range(6, 12);
            for _ in 0..words {
                buf.extend_from_slice(dict[rng.below(dict.len() as u64) as usize].as_bytes());
                buf.push(b' ');
            }
            buf.push(b'\n');
            line += 1;
        }
        let take = ((bytes - written) as usize).min(buf.len());
        w.write_all(&buf[..take])?;
        written += take as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> CorpusSpec {
        CorpusSpec {
            tiny_files: 24,
            tiny_max_bytes: 512,
            random_files: 3,
            random_file_bytes: 64 * 1024,
            compressible_files: 2,
            compressible_file_bytes: 64 * 1024,
            duplicate_files: 3,
            shifted_files: 2,
            shift_insert_bytes: 1024,
            mixed_fanout: 2,
            mixed_depth: 2,
            mixed_files_per_dir: 2,
            mixed_file_bytes: 256,
            generations: 3,
            churn_percent: 25,
            churn_added_files: 2,
            churn_deleted_files: 3,
        }
    }

    fn build(seed: u64) -> (tempfile::TempDir, Corpus) {
        let d = tempfile::tempdir().unwrap();
        let c = generate(&spec(), seed, d.path(), 4).unwrap();
        (d, c)
    }

    #[test]
    fn the_same_seed_reproduces_the_same_corpus() {
        let (_a, ca) = build(1234);
        let (_b, cb) = build(1234);
        for (x, y) in ca.generations.iter().zip(cb.generations.iter()) {
            assert_eq!(
                x.manifest_digest, y.manifest_digest,
                "generation {} differs between two runs of the same seed",
                x.index
            );
            assert_eq!(x.logical_bytes, y.logical_bytes);
            assert_eq!(x.file_count, y.file_count);
        }
    }

    #[test]
    fn a_different_seed_produces_a_different_corpus() {
        let (_a, ca) = build(1);
        let (_b, cb) = build(2);
        assert_ne!(
            ca.gen0().manifest_digest,
            cb.gen0().manifest_digest,
            "seed must change the bytes"
        );
    }

    #[test]
    fn the_parallel_writer_does_not_change_the_result() {
        let one = tempfile::tempdir().unwrap();
        let many = tempfile::tempdir().unwrap();
        let a = generate(&spec(), 99, one.path(), 1).unwrap();
        let b = generate(&spec(), 99, many.path(), 8).unwrap();
        assert_eq!(a.gen0().manifest_digest, b.gen0().manifest_digest);
    }

    #[test]
    fn every_documented_shape_is_present() {
        let (d, c) = build(7);
        let root = &c.gen0().dir;
        for sub in [
            "tiny",
            "random",
            "compressible",
            "duplicates",
            "shifted",
            "mixed",
        ] {
            assert!(root.join(sub).is_dir(), "{sub} missing");
        }
        // The mixed subtree must carry symlinks, an empty directory and at
        // least one executable file.
        let m = c.manifest(0).unwrap();
        assert!(m.symlink_count >= 2, "no symlinks in the corpus");
        assert!(
            m.entries
                .iter()
                .any(|e| e.path.ends_with("/empty") && e.kind == super::super::manifest::Kind::Dir),
            "no empty directory"
        );
        assert!(
            m.entries
                .iter()
                .any(|e| e.kind == super::super::manifest::Kind::File && e.mode & 0o111 != 0),
            "no executable file"
        );
        assert!(
            m.entries
                .iter()
                .any(|e| e.link_target.as_deref() == Some("../nowhere/at/all")),
            "no dangling symlink"
        );
        drop(d);
    }

    #[test]
    fn duplicates_are_byte_identical_and_shifted_files_are_not() {
        let (_d, c) = build(11);
        let root = &c.gen0().dir;
        let src = fs::read(root.join("random/0000.bin")).unwrap();
        let dup = fs::read(root.join("duplicates/0000.bin")).unwrap();
        assert_eq!(src, dup, "duplicates/ must be exact copies");

        let shifted = fs::read(root.join("shifted/0000.bin")).unwrap();
        assert_eq!(shifted.len(), src.len() + 1024);
        let at = (64 * 1024) / 16;
        assert_eq!(&shifted[..at], &src[..at], "prefix must be shared");
        assert_ne!(
            &shifted[at..at + 64],
            &src[at..at + 64],
            "insertion must actually shift the tail"
        );
        assert_eq!(
            &shifted[at + 1024..],
            &src[at..],
            "the tail must be the original bytes, only moved"
        );
    }

    #[test]
    fn random_data_is_high_entropy_and_text_data_is_not() {
        let (_d, c) = build(5);
        let root = &c.gen0().dir;
        let distinct = |p: &str| {
            let b = fs::read(root.join(p)).unwrap();
            let mut seen = [false; 256];
            for x in b {
                seen[x as usize] = true;
            }
            seen.iter().filter(|s| **s).count()
        };
        assert!(
            distinct("random/0000.bin") > 240,
            "random/ looks structured"
        );
        let text = distinct("compressible/0000.txt");
        assert!(text < 64, "compressible/ uses {text} distinct bytes");
    }

    #[test]
    fn churn_rewrites_some_files_adds_some_and_deletes_some() {
        let (_d, c) = build(21);
        let g0 = c.manifest(0).unwrap();
        let g1 = c.manifest(1).unwrap();
        let diff = g0.compare(&g1, super::super::manifest::Strictness::full());
        assert!(!diff.missing.is_empty(), "no file was deleted");
        assert!(!diff.extra.is_empty(), "no file was added");
        assert!(!diff.corrupt.is_empty(), "no file was rewritten");
        assert!(
            diff.extra.iter().any(|p| p.starts_with("added/")),
            "additions must land in added/: {:?}",
            diff.extra
        );
        assert!(
            diff.missing.iter().all(|p| p.starts_with("tiny/")),
            "deletions must come from tiny/: {:?}",
            diff.missing
        );
        // Most files must be untouched, or "changed ingestion" would just be
        // a second fresh ingestion.
        let changed = diff.corrupt.len() + diff.missing.len() + diff.extra.len();
        assert!(
            changed * 2 < g0.entries.len(),
            "{changed} of {} entries changed; churn is too aggressive",
            g0.entries.len()
        );
    }

    #[test]
    fn an_unchurned_file_keeps_its_bytes_across_generations() {
        let (_d, c) = build(33);
        let g0 = c.manifest(0).unwrap();
        let g1 = c.manifest(1).unwrap();
        let by_path = |m: &super::super::manifest::Manifest, p: &str| {
            m.entries.iter().find(|e| e.path == p).cloned()
        };
        let mut same = 0;
        for e in &g0.entries {
            if e.kind != super::super::manifest::Kind::File {
                continue;
            }
            if let Some(other) = by_path(&g1, &e.path)
                && other.sha256 == e.sha256
            {
                same += 1;
            }
        }
        assert!(same > 0, "no file survived unchanged");
    }

    #[test]
    fn timestamps_come_from_the_spec_not_the_clock() {
        let (_d, c) = build(3);
        let m = c.manifest(0).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        for e in &m.entries {
            assert!(
                e.mtime_unix_secs >= BASE_MTIME && e.mtime_unix_secs < now - 1_000_000,
                "{} has a wall-clock mtime {}",
                e.path,
                e.mtime_unix_secs
            );
        }
    }

    #[test]
    fn the_manifest_totals_agree_with_the_files_on_disk() {
        let (_d, c) = build(44);
        let g = c.gen0();
        let sizes = fsx::measure_dir(&g.dir).unwrap();
        assert_eq!(sizes.apparent_bytes, g.logical_bytes);
        assert_eq!(sizes.files, g.file_count);
        assert_eq!(sizes.symlinks, g.symlink_count);
    }
}
