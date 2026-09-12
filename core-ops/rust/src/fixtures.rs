//! Deterministic fixture construction, shared digests and filesystem
//! helpers. Every generator here has a byte-for-byte counterpart in
//! `../go/fixtures.go`; the fixture digest checks prove the two agree before
//! any timing is compared.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use amber_store_core::fstree::{self, Entry, Object};
use amber_store_core::key::{Key, Type};
use amber_store_core::packstore;
use amber_store_core::reference::Reference;
use amber_store_core::refstore;

use crate::env::Profile;
use crate::harness::Dims;

// ---------------------------------------------------------------------------
// Deterministic pseudo-randomness
// ---------------------------------------------------------------------------

/// SplitMix64. The Go driver implements the same generator with the same
/// constants, so both cores see byte-identical fixtures from the same seed.
pub struct Rng {
    s: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng { s: seed }
    }

    pub fn next(&mut self) -> u64 {
        self.s = self.s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `[0, bound)`.
    pub fn n(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next() % bound as u64) as usize
    }
}

/// `n` bytes of incompressible noise.
pub fn random_bytes(seed: u64, n: usize) -> Vec<u8> {
    let mut r = Rng::new(seed);
    let mut b = vec![0u8; n];
    let mut i = 0;
    while i + 8 <= n {
        b[i..i + 8].copy_from_slice(&r.next().to_le_bytes());
        i += 8;
    }
    let rem = n % 8;
    if rem != 0 {
        let tail = r.next().to_le_bytes();
        b[n - rem..].copy_from_slice(&tail[..rem]);
    }
    b
}

/// The deterministic word list the compressible corpus is drawn from.
pub fn lexicon() -> Vec<String> {
    let parts = [
        "amber", "store", "chunk", "pack", "segment", "key", "tree", "leaf", "node", "index",
        "blob", "entry", "record", "footer", "filter",
    ];
    (0..256)
        .map(|i: usize| {
            format!(
                "{}-{}-{:02x}",
                parts[i % parts.len()],
                parts[(i / parts.len() + 3) % parts.len()],
                i
            )
        })
        .collect()
}

/// `n` bytes of repetitive, zstd-friendly text.
pub fn compressible_bytes(seed: u64, n: usize) -> Vec<u8> {
    let lex = lexicon();
    let mut r = Rng::new(seed);
    let mut b = String::with_capacity(n + 32);
    while b.len() < n {
        b.push_str(&lex[r.n(lex.len())]);
        if r.n(12) == 0 {
            b.push('\n');
        } else {
            b.push(' ');
        }
    }
    b.into_bytes()[..n].to_vec()
}

// ---------------------------------------------------------------------------
// Canonical digests
// ---------------------------------------------------------------------------

/// Fingerprints bytes with the core's own key construction, so both drivers
/// compute it with the same primitive and the report can compare the hex
/// strings directly.
pub fn digest(b: &[u8]) -> String {
    Key::new(Type::Blob, b.len() as u64, b).to_string()
}

/// Fingerprints a sequence of byte strings unambiguously: each item is
/// prefixed with its big-endian length, so no concatenation collides with a
/// different split.
pub fn digest_list<'a, I: IntoIterator<Item = &'a [u8]>>(items: I) -> String {
    let mut buf = Vec::new();
    for it in items {
        buf.extend_from_slice(&(it.len() as u64).to_be_bytes());
        buf.extend_from_slice(it);
    }
    digest(&buf)
}

pub fn digest_vecs(items: &[Vec<u8>]) -> String {
    digest_list(items.iter().map(|v| v.as_slice()))
}

pub fn digest_keys(keys: &[Key]) -> String {
    let owned: Vec<Vec<u8>> = keys.iter().map(|k| k.as_bytes().to_vec()).collect();
    digest_vecs(&owned)
}

pub fn digest_strings<S: AsRef<str>>(ss: &[S]) -> String {
    let owned: Vec<Vec<u8>> = ss.iter().map(|s| s.as_ref().as_bytes().to_vec()).collect();
    digest_vecs(&owned)
}

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

pub fn key_bytes(k: Key) -> Vec<u8> {
    k.as_bytes().to_vec()
}

/// Duplicates a directory so a destructive operation measures on its own copy
/// and the template survives for the next repetition.
pub fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_tree(&from, &to)?;
        } else if ft.is_symlink() {
            let target = fs::read_link(&from)?;
            let _ = fs::remove_file(&to);
            std::os::unix::fs::symlink(target, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// The deterministic modification time of one fixture path. Ingest encodes
/// mtimes into every directory entry, so two trees with the same contents but
/// different timestamps produce different root keys. Both drivers derive the
/// timestamp from the relative path with the same FNV-1a hash, which makes
/// the whole tree — content and metadata — reproducible.
pub fn fixed_time(rel: &str) -> (i64, i64) {
    let mut h: u64 = 14695981039346656037;
    for b in rel.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    (
        1_700_000_000 + (h % 100_000) as i64,
        (h % 1_000_000_000) as i64,
    )
}

/// Gives every entry of the tree its deterministic timestamp. Directories are
/// stamped after their contents, because creating a child updates its
/// parent's mtime.
pub fn stamp_tree(root: &Path) -> io::Result<()> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    fn walk(root: &Path, dir: &Path, dirs: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            if entry.file_type()?.is_dir() {
                dirs.push(p.clone());
                walk(root, &p, dirs)?;
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().to_string();
                stamp_path(&p, &rel)?;
            }
        }
        Ok(())
    }
    walk(root, root, &mut dirs)?;
    // Deepest first, so a parent is stamped after its children.
    dirs.sort_by_key(|p| std::cmp::Reverse(p.as_os_str().len()));
    for d in &dirs {
        let rel = d.strip_prefix(root).unwrap().to_string_lossy().to_string();
        stamp_path(d, &rel)?;
    }
    Ok(())
}

fn stamp_path(path: &Path, rel: &str) -> io::Result<()> {
    let (sec, nsec) = fixed_time(rel);
    let ts = [
        libc::timespec {
            tv_sec: sec as libc::time_t,
            tv_nsec: nsec as _,
        },
        libc::timespec {
            tv_sec: sec as libc::time_t,
            tv_nsec: nsec as _,
        },
    ];
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    // SAFETY: the path is NUL-terminated and the timespec array is ours.
    let rc = unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            c.as_ptr(),
            ts.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Renders a directory tree as a canonical, platform-stable listing: one line
/// per path in sorted order carrying the type, the permission bits, the size,
/// the modification time and the content digest. Both drivers compute it the
/// same way, so comparing the two digests proves the fixtures really are
/// identical — including the timestamps ingest folds into every entry.
pub fn manifest(root: &Path) -> io::Result<String> {
    Ok(digest_strings(&manifest_lines(root, None)?))
}

/// `manifest`'s listing, before it is digested, optionally restricted to the
/// paths `keep` accepts. `None` accepts everything.
pub fn manifest_lines(
    root: &Path,
    keep: Option<&dyn Fn(&str, bool) -> bool>,
) -> io::Result<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    fn walk(
        root: &Path,
        dir: &Path,
        keep: Option<&dyn Fn(&str, bool) -> bool>,
        lines: &mut Vec<String>,
    ) -> io::Result<()> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().to_string();
            let ft = entry.file_type()?;
            if let Some(k) = keep
                && !k(&rel, ft.is_dir())
            {
                continue;
            }
            let md = fs::symlink_metadata(&p)?;
            let mt = format!("{}.{:09}", md.mtime(), md.mtime_nsec());
            if ft.is_dir() {
                lines.push(format!(
                    "d {:04o} {} {}",
                    md.permissions().mode() & 0o7777,
                    mt,
                    rel
                ));
                walk(root, &p, keep, lines)?;
            } else if ft.is_symlink() {
                let target = fs::read_link(&p)?;
                lines.push(format!("l {} {} -> {}", rel, mt, target.to_string_lossy()));
            } else {
                let b = fs::read(&p)?;
                lines.push(format!(
                    "f {:04o} {} {} {} {}",
                    md.permissions().mode() & 0o7777,
                    b.len(),
                    mt,
                    digest(&b),
                    rel
                ));
            }
        }
        Ok(())
    }
    walk(root, root, keep, &mut lines)?;
    lines.sort();
    Ok(lines)
}

/// Whether one path of the fixture tree is expected to survive ingest, by
/// applying the `.amberignore` rules the harness itself wrote into that tree
/// ("*.tmp", "!keep.tmp", "/build/").
///
/// This is deliberately an independent statement of the expectation: the
/// restored-tree check compares the complete restored listing against it,
/// not against anything either core computed. Two cores agreeing with each
/// other proves only that they agree.
pub fn fixture_included(rel: &str, _is_dir: bool) -> bool {
    if rel == "build" || rel.starts_with("build/") {
        return false;
    }
    let base = rel.rsplit('/').next().unwrap_or(rel);
    if base.ends_with(".tmp") && base != "keep.tmp" {
        return false;
    }
    true
}

/// The first line on which two sorted listings disagree, so a failed
/// comparison names the path rather than two hashes.
pub fn first_difference(want: &[String], got: &[String]) -> String {
    let n = want.len().max(got.len());
    for i in 0..n {
        let w = want.get(i).map(String::as_str).unwrap_or("");
        let g = got.get(i).map(String::as_str).unwrap_or("");
        if w != g {
            return format!(
                "line {} of {}/{}: expected {:?}, restored {:?}",
                i,
                want.len(),
                got.len(),
                w,
                g
            );
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// In-memory object bag
// ---------------------------------------------------------------------------

/// The in-memory object bag the pure tree cases read through. It is
/// concurrency-safe because the walks call `get` from several threads.
pub struct MemStore {
    m: RwLock<std::collections::HashMap<Key, Vec<u8>>>,
}

/// The error a memory-store read returns for an absent key.
#[derive(Debug)]
pub struct MemMissing(pub Key);

impl std::fmt::Display for MemMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "memstore: {} not found", self.0)
    }
}

impl std::error::Error for MemMissing {}

impl Default for MemStore {
    fn default() -> MemStore {
        MemStore::new()
    }
}

impl MemStore {
    pub fn new() -> MemStore {
        MemStore {
            m: RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn put(&self, o: Object) -> Result<(), MemMissing> {
        self.m.write().unwrap().entry(o.key).or_insert(o.bytes);
        Ok(())
    }

    pub fn get(&self, k: Key) -> Result<Vec<u8>, MemMissing> {
        match self.m.read().unwrap().get(&k) {
            Some(v) => Ok(v.clone()),
            None => Err(MemMissing(k)),
        }
    }

    pub fn has(&self, k: Key) -> Result<bool, MemMissing> {
        Ok(self.m.read().unwrap().contains_key(&k))
    }

    /// How many distinct objects the bag holds. Only ever called at fixture
    /// time, to record an exact object count for a rate denominator.
    pub fn len(&self) -> usize {
        self.m.read().unwrap().len()
    }

    pub fn drop_key(&self, k: Key) {
        self.m.write().unwrap().remove(&k);
    }

    pub fn snapshot(&self) -> MemStore {
        MemStore {
            m: RwLock::new(self.m.read().unwrap().clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Fixture data
// ---------------------------------------------------------------------------

/// A batch of byte strings a short operation is run over. Short operations
/// are always measured over a whole set: one call is far below the clock's
/// resolution, a set of a few thousand is not.
///
/// A set is one point in the (size, content) grid: its dimensions are what
/// the report sweeps, and its name is only a label for that point.
#[derive(Clone)]
pub struct PayloadSet {
    pub name: String,
    pub items: Vec<Vec<u8>>,
    pub bytes: i64,
    pub item_bytes: i64,
    pub content: String,
}

impl PayloadSet {
    /// The set as workload dimensions.
    pub fn dims(&self) -> Dims {
        Dims {
            item_bytes: self.item_bytes,
            items: self.items.len() as i64,
            content: self.content.clone(),
            ..Default::default()
        }
    }

    /// The leading part of the set whose total is at most `max` bytes, for
    /// operations that write every item to disk and would otherwise dominate
    /// the run. At least one item is always kept.
    pub fn limit(&self, max: i64) -> PayloadSet {
        if self.bytes <= max || self.item_bytes == 0 {
            return self.clone();
        }
        let n = ((max / self.item_bytes) as usize).clamp(1, self.items.len());
        PayloadSet {
            name: self.name.clone(),
            items: self.items[..n].to_vec(),
            bytes: n as i64 * self.item_bytes,
            item_bytes: self.item_bytes,
            content: self.content.clone(),
        }
    }
}

/// The object-size classes every size-sensitive operation is swept over: an
/// empty object, a tiny one, a kilobyte, a megabyte and a large
/// multi-megabyte one. The classes are absolute sizes, not fractions of the
/// profile, so the same workload means the same thing in both profiles.
/// Mirrors `payloadSizes` in `../go/fixtures_build.go`.
pub const PAYLOAD_SIZES: &[(&str, usize)] = &[
    ("empty", 0),
    ("tiny-64B", 64),
    ("4KiB", 4 << 10),
    ("1MiB", 1 << 20),
    ("large-8MiB", 8 << 20),
];

/// The content kinds crossed with those sizes:
///
/// * `random` — incompressible, every item distinct;
/// * `text` — compressible, every item distinct;
/// * `duplicate` — every item byte-identical, so a content-addressed store
///   sees one object and the rest are dedup hits.
///
/// The empty size is not crossed with these: a zero-length object has no
/// content to be random, compressible or duplicated.
pub const PAYLOAD_CONTENTS: &[&str] = &["random", "text", "duplicate"];

pub fn new_payload_set(
    name: &str,
    content: &str,
    count: usize,
    size: usize,
    seed: u64,
) -> PayloadSet {
    let mut items: Vec<Vec<u8>> = Vec::with_capacity(count);
    let mut bytes = 0i64;
    for i in 0..count {
        let s = seed.wrapping_add((i as u64).wrapping_mul(0x0100_0000_01B3));
        let item = match content {
            "text" => compressible_bytes(s, size),
            // Every item is the same bytes, so the set has one distinct
            // object in it however many items it holds.
            "duplicate" => {
                if i == 0 {
                    random_bytes(seed, size)
                } else {
                    items[0].clone()
                }
            }
            _ => random_bytes(s, size),
        };
        items.push(item);
        bytes += size as i64;
    }
    PayloadSet {
        name: name.to_string(),
        items,
        bytes,
        item_bytes: size as i64,
        content: content.to_string(),
    }
}

/// Builds the whole (size, content) grid once. Every set holds about
/// `payload_total` bytes, so the grid costs the same at every size and a
/// per-byte rate is comparable along a row.
pub fn build_payload_matrix(p: &Profile) -> Vec<PayloadSet> {
    let mut out = Vec::new();
    for (i, (name, size)) in PAYLOAD_SIZES.iter().enumerate() {
        if *size == 0 {
            out.push(new_payload_set(
                "empty",
                "empty",
                p.batch_ops * 4,
                0,
                p.seed + 10,
            ));
            continue;
        }
        let items = ((p.payload_total / *size as i64) as usize).clamp(1, 16384);
        for (j, content) in PAYLOAD_CONTENTS.iter().enumerate() {
            let seed = p.seed + 10 + (i as u64) * 97 + (j as u64) * 7919;
            out.push(new_payload_set(
                &format!("{name}-{content}"),
                content,
                items,
                *size,
                seed,
            ));
        }
    }
    out
}

/// One point of the grid, by name.
pub fn payload_named<'a>(sets: &'a [PayloadSet], name: &str) -> &'a PayloadSet {
    sets.iter()
        .find(|ps| ps.name == name)
        .unwrap_or_else(|| panic!("no payload set named {name}"))
}

/// The exact extent of one on-disk tree as an ingest walk sees it: how many
/// files it covers and how many logical bytes those files hold. Both numbers
/// are taken outside every measured interval.
#[derive(Debug, Clone, Copy, Default)]
pub struct TreeCounts {
    pub files: i64,
    pub bytes: i64,
}

/// One producing core's encoding of the shared object population, as read
/// from the wire directory. Its producer, content hash and encoded size are
/// recorded in the report: two cores' packs carry the same objects but not
/// the same compressed bytes, so a decode measurement has to say which bytes
/// it decoded.
pub struct WirePack {
    pub producer: String,
    pub sha256: String,
    pub bytes: i64,
    pub objects: usize,
    pub data: Vec<u8>,
    pub records: Vec<Vec<u8>>,
}

/// One point of the directory-width sweep: a real prolly tree of `entries`
/// entries, with every name it holds and one that it does not.
pub struct MemDir {
    pub entries: usize,
    pub root: Key,
    pub names: Vec<Vec<u8>>,
    pub miss_name: Vec<u8>,
    /// How many stored objects the directory's own encoding came to,
    /// counted outside every measured interval.
    pub objects: i64,
}

/// One point of the path-depth sweep.
pub struct MemChain {
    pub depth: usize,
    pub root: Key,
    pub path: String,
}

/// One point of the file-index fan-out sweep.
pub struct MemFileIndex {
    pub children: usize,
    pub root: Key,
    #[allow(dead_code)]
    pub keys: Vec<Key>,
}

/// One point of a node-codec sweep.
pub struct EntrySet {
    pub name: String,
    pub content: String,
    pub entries: Vec<Entry>,
    pub enc: Vec<u8>,
}

pub struct PairSet {
    pub name: String,
    pub pairs: Vec<fstree::DirPair>,
    pub enc: Vec<u8>,
}

pub struct ChildSet {
    pub name: String,
    pub keys: Vec<Key>,
    pub enc: Vec<u8>,
}

/// One record's position: the sealed segment it lives in and its offset
/// inside that segment.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecordLoc {
    pub id: u64,
    pub off: u64,
    pub len: u32,
}

pub const MODE_REG: u64 = 0o100644;
pub const MODE_DIR: u64 = 0o040755;

/// Builds one deterministic directory entry. The content key is fabricated
/// (no Blob is stored for it); the tree fixtures replace it with a real one.
pub fn entry_for(i: usize, seed: u64, with_xattrs: Option<&[u8]>) -> Entry {
    let h: [u8; 32] = random_bytes(seed.wrapping_add(i as u64), 32)
        .try_into()
        .unwrap();
    let ck = Key::new_from_hash(Type::Blob, 1024 + i as u64, h);
    Entry {
        name: format!("entry-{i:08}").into_bytes(),
        mode: MODE_REG,
        uid: 1000,
        gid: 1000,
        mtime: 1_700_000_000_000_000_000 + i as i64 * 1_000_000,
        content_key: key_bytes(ck),
        xattrs_in: with_xattrs.map(|b| b.to_vec()).unwrap_or_default(),
        ..Default::default()
    }
}

/// Everything the cases read. Built once, before any measurement, and never
/// mutated by a measured operation.
pub struct Fixtures {
    pub corpus_random: Vec<u8>,
    pub corpus_text: Vec<u8>,

    /// The whole (size, content) grid: every size class crossed with every
    /// content kind. Size-sensitive operations are swept over it.
    pub payloads: Vec<PayloadSet>,
    /// Named points of that grid, for the cases and checks that need one
    /// particular shape rather than the sweep.
    pub tiny: PayloadSet,
    pub small: PayloadSet,
    pub text: PayloadSet,
    pub large: PayloadSet,
    pub rand: PayloadSet,

    pub keys: Vec<Key>,
    pub key_bytes: Vec<Vec<u8>>,
    pub bad_key_bytes: Vec<Vec<u8>>,

    pub xattrs_small: BTreeMap<Vec<u8>, Vec<u8>>,
    pub xattrs_large: BTreeMap<Vec<u8>, Vec<u8>>,
    pub xattrs_small_enc: Vec<u8>,
    pub xattrs_large_enc: Vec<u8>,

    /// The node-codec sweeps: one encoded node per entry count, plus the
    /// with-xattrs and partial-change structured variants.
    pub entry_sets: Vec<EntrySet>,
    pub pair_sets: Vec<PairSet>,
    pub child_sets: Vec<ChildSet>,
    pub entries_small: Vec<Entry>,
    pub entries_large: Vec<Entry>,
    pub pairs_small: Vec<fstree::DirPair>,
    pub pairs_large: Vec<fstree::DirPair>,
    pub children_small: Vec<Key>,
    pub children_large: Vec<Key>,
    pub enc_dir_leaf_small: Vec<u8>,
    pub enc_dir_leaf_large: Vec<u8>,
    pub enc_dir_node_small: Vec<u8>,
    pub enc_dir_node_large: Vec<u8>,
    pub enc_file_node_small: Vec<u8>,
    pub enc_file_node_large: Vec<u8>,
    pub item_encodings: Vec<Vec<u8>>,

    pub mem: MemStore,
    /// The directory-width sweep, the path-depth sweep and the file-index
    /// fan-out sweep: one fixture per point, all in `mem`.
    pub dirs: Vec<MemDir>,
    pub chains: Vec<MemChain>,
    #[allow(dead_code)]
    pub file_indexes: Vec<MemFileIndex>,
    pub wide_root: Key,
    pub wide_names: Vec<Vec<u8>>,
    pub miss_name: Vec<u8>,
    pub deep_root: Key,
    pub deep_path: String,
    pub deep_depth: usize,
    pub shallow_root: Key,
    pub file_root: Key,
    pub file_bytes: i64,
    /// How many content chunks the corpus file really split into, counted
    /// outside every measured interval.
    pub file_chunks: i64,
    pub incomplete_root: Key,
    pub incomplete_store: MemStore,

    pub tree_v1: PathBuf,
    pub tree_v2: PathBuf,
    pub ingest_template: PathBuf,
    /// What the fixture writer laid down, ignored files included. It sizes
    /// the fixture; it is *not* a rate denominator, because ingest does not
    /// include all of it.
    pub tree_bytes: i64,
    #[allow(dead_code)]
    pub tree_files: i64,
    /// Exact per-tree counts, measured outside every timed interval with the
    /// core's own scan, so each rate has the denominator that belongs to it.
    pub v1_included: TreeCounts,
    pub v1_unfiltered: TreeCounts,
    pub v2_included: TreeCounts,
    /// The listing a restored tree must reproduce: the source tree
    /// restricted to the paths the fixture's own ignore rules keep,
    /// computed by the harness rather than by either core.
    pub v1_included_manifest: Vec<String>,
    pub ignore_dir: PathBuf,

    /// The object population both cores encode, and *this* core's own
    /// encoding of it.
    pub pack_objects: Vec<Object>,
    pub wire_pack: Vec<u8>,
    pub wire_records: Vec<Vec<u8>>,
    /// Every producer's pack, read from the shared wire directory. Both
    /// drivers load the same files, so the reader and decoder cases are
    /// measured on byte-identical input.
    pub wire: Vec<WirePack>,

    pub store_template: PathBuf,
    pub store_keys: Vec<Key>,
    pub store_miss_keys: Vec<Key>,
    pub garbage_template: PathBuf,
    pub garbage_live: Vec<Key>,
    pub refs_template: PathBuf,
    pub ref_names: Vec<String>,
    pub gc_template: PathBuf,
    pub gc_live_root: Key,
    pub inbox_packs: Vec<Vec<u8>>,
    pub inbox_roots: Vec<Key>,
    /// The payload the staged packs carry, which is the same in both cores;
    /// the packs' encoded sizes are not, and are reported per core instead
    /// of used as a shared denominator.
    pub inbox_logical_bytes: i64,

    pub ref_record: Reference,
    pub ref_record_enc: Vec<u8>,
    pub ref_batch: Vec<refstore::Record>,

    pub tree_mem: MemStore,
    pub tree_root: Key,
    pub tar_bytes: Vec<u8>,

    pub ro: Option<std::sync::Arc<packstore::Store>>,
    #[allow(dead_code)]
    pub ro_dir: PathBuf,
    pub ro_locs: Vec<RecordLoc>,
    pub ro_seg_id: u64,
}

// ---------------------------------------------------------------------------
// Output consumption
// ---------------------------------------------------------------------------

// Every measured case returns an accumulator built by consuming what its
// calls produced, and the driver records that accumulator in each sample.
// Two properties follow:
//
//   * nothing the case computed can be dead code, because the output reaches
//     a value the driver writes to its report through an opaque call; and
//   * the two cores' accumulators are directly comparable wherever they are
//     specified to produce the same bytes.
//
// Consumption is constant-time per call. A fixed-size output (a 32-byte key)
// is folded whole, because that is cheap; a byte stream goes through
// `sink_bytes`, which is never inlined and touches only its length and its
// two ends. Hashing whole payloads inside a measured interval would make an
// encode measurement into encode plus a second hash, so the full-output
// digests are computed by the correctness checks instead, before any timing
// starts. `../go/fixtures.go` implements the identical FNV-1a over the
// identical byte sequences, which is what makes the cross-core comparison
// hold.

const FNV_OFFSET: u64 = 14695981039346656037;
const FNV_PRIME: u64 = 1099511628211;

/// Starts an accumulator.
pub fn new_fold() -> u64 {
    FNV_OFFSET
}

/// Folds every byte of `b`. Only ever called on fixed-size, short outputs
/// and outside measured intervals on longer ones.
pub fn fold_bytes(mut acc: u64, b: &[u8]) -> u64 {
    for x in b {
        acc ^= *x as u64;
        acc = acc.wrapping_mul(FNV_PRIME);
    }
    acc
}

/// Folds a number as its eight little-endian bytes.
pub fn fold_u64(mut acc: u64, mut v: u64) -> u64 {
    for _ in 0..8 {
        acc ^= v & 0xFF;
        acc = acc.wrapping_mul(FNV_PRIME);
        v >>= 8;
    }
    acc
}

pub fn fold_i64(acc: u64, v: i64) -> u64 {
    fold_u64(acc, v as u64)
}

pub fn fold_str(acc: u64, s: &str) -> u64 {
    fold_bytes(acc, s.as_bytes())
}

pub fn fold_bool(acc: u64, b: bool) -> u64 {
    fold_u64(acc, u64::from(b))
}

/// Folds all 32 bytes of a key: the type and length header and the whole
/// digest, so a case that constructs keys cannot be reduced to one that only
/// assembles headers.
pub fn fold_key(acc: u64, k: &Key) -> u64 {
    fold_bytes(acc, k.as_bytes())
}

/// Consumes a byte-stream output in constant time. It is never inlined, so
/// the caller has to produce a real slice pointing at real memory, and it
/// touches the length and both ends of that memory -- enough that the buffer
/// cannot be optimised away, and cheap enough that an encode or decode
/// measurement stays a measurement of encode or decode.
#[inline(never)]
pub fn sink_bytes(acc: u64, b: &[u8]) -> u64 {
    let acc = fold_u64(acc, b.len() as u64);
    if b.is_empty() {
        std::hint::black_box(acc)
    } else {
        let acc = fold_u64(acc, b[0] as u64);
        std::hint::black_box(fold_u64(acc, b[b.len() - 1] as u64))
    }
}

/// Names a worker count by kind rather than by number. The number is a
/// profile setting and is recorded in the sample's dimensions; keeping it out
/// of the workload label is what lets one coverage manifest describe every
/// profile. Mirrors `jobsLabel` in `../go/fixtures.go`.
pub fn jobs_label(n: usize) -> &'static str {
    if n == 1 { "jobs-1" } else { "jobs-N" }
}

pub fn writers_label(n: usize) -> &'static str {
    if n == 1 { "writers-1" } else { "writers-N" }
}
