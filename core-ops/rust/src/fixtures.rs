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
    let mut lines: Vec<String> = Vec::new();
    fn walk(root: &Path, dir: &Path, lines: &mut Vec<String>) -> io::Result<()> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().to_string();
            let ft = entry.file_type()?;
            let md = fs::symlink_metadata(&p)?;
            let mt = format!("{}.{:09}", md.mtime(), md.mtime_nsec());
            if ft.is_dir() {
                lines.push(format!(
                    "d {:04o} {} {}",
                    md.permissions().mode() & 0o7777,
                    mt,
                    rel
                ));
                walk(root, &p, lines)?;
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
    walk(root, root, &mut lines)?;
    lines.sort();
    Ok(digest(lines.join("\n").as_bytes()))
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
pub struct PayloadSet {
    pub name: String,
    pub items: Vec<Vec<u8>>,
    pub bytes: i64,
}

pub fn new_payload_set(name: &str, count: usize, size: usize, seed: u64, text: bool) -> PayloadSet {
    let mut items = Vec::with_capacity(count);
    let mut bytes = 0i64;
    for i in 0..count {
        let s = seed.wrapping_add((i as u64).wrapping_mul(0x0100_0000_01B3));
        items.push(if text {
            compressible_bytes(s, size)
        } else {
            random_bytes(s, size)
        });
        bytes += size as i64;
    }
    PayloadSet {
        name: name.to_string(),
        items,
        bytes,
    }
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
    pub wide_root: Key,
    pub wide_names: Vec<Vec<u8>>,
    pub miss_name: Vec<u8>,
    pub deep_root: Key,
    pub deep_path: String,
    pub shallow_root: Key,
    pub file_root: Key,
    pub file_bytes: i64,
    pub incomplete_root: Key,
    pub incomplete_store: MemStore,

    pub tree_v1: PathBuf,
    pub tree_v2: PathBuf,
    pub ingest_template: PathBuf,
    pub tree_bytes: i64,
    #[allow(dead_code)]
    pub tree_files: i64,
    pub ignore_dir: PathBuf,

    pub pack_objects: Vec<Object>,
    pub wire_pack: Vec<u8>,
    pub wire_records: Vec<Vec<u8>>,

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
