//! Builds the fixtures. Every step mirrors `../go/fixtures_build.go`, and the
//! resulting digests are recorded as checks so a divergence is caught before
//! any timing is compared.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use amber_store_core::amberpack;
use amber_store_core::cbor;
use amber_store_core::chunkers::{self, ItemChunker};
use amber_store_core::fstree::{self, DirBuilder, DirPair, Entry, IndexBuilder, Object};
use amber_store_core::ingest;
use amber_store_core::key::{Key, Type};
use amber_store_core::packstore::{self, WriteOpts};
use amber_store_core::reference::Reference;
use amber_store_core::refstore;

use crate::env::Profile;
use crate::fixtures::*;

pub fn store_options(p: &Profile) -> packstore::Options {
    packstore::Options::new()
        .segment_size(p.segment_bytes as u64)
        .sync(false)
}

/// The object sequence the store writers consume.
pub fn object_seq(objs: &[Object]) -> Vec<Result<packstore::Object, io::Error>> {
    objs.iter()
        .map(|o| Ok(packstore::Object::from(o.clone())))
        .collect()
}

pub fn build_fixtures(scratch: &Path, p: &Profile, xattrs: bool) -> Fixtures {
    let seed = p.seed;
    let corpus_random = random_bytes(seed + 1, p.corpus_bytes as usize);
    let corpus_text = compressible_bytes(seed + 2, p.corpus_bytes as usize);

    let batch = p.batch_ops;
    let tiny = new_payload_set("tiny-64B-random", batch, 64, seed + 10, false);
    let small = new_payload_set(
        "small-4KiB-random",
        (batch / 8).max(32),
        4 << 10,
        seed + 11,
        false,
    );
    let text = new_payload_set(
        "small-4KiB-text",
        (batch / 8).max(32),
        4 << 10,
        seed + 12,
        true,
    );
    let large = new_payload_set(
        "large-1MiB-text",
        (batch / 256).max(4),
        1 << 20,
        seed + 13,
        true,
    );
    let rand = new_payload_set(
        "large-1MiB-random",
        (batch / 256).max(4),
        1 << 20,
        seed + 14,
        false,
    );

    let (keys, key_bytes_v, bad_key_bytes) = build_key_fixtures(p);
    let (xattrs_small, xattrs_large) = build_xattr_fixtures(p);
    let xattrs_small_enc = cbor::encode_xattrs(&xattrs_small);
    let xattrs_large_enc = cbor::encode_xattrs(&xattrs_large);

    let (entries_small, entries_large, pairs_small, pairs_large, children_small, children_large) =
        build_codec_fixtures(p, &xattrs_small_enc);
    let enc_dir_leaf_small = fstree::encode_dir_leaf(&entries_small).unwrap().bytes;
    let enc_dir_leaf_large = fstree::encode_dir_leaf(&entries_large).unwrap().bytes;
    let enc_dir_node_small = fstree::encode_dir_node(&pairs_small).unwrap().bytes;
    let enc_dir_node_large = fstree::encode_dir_node(&pairs_large).unwrap().bytes;
    let enc_file_node_small = fstree::encode_file_node(&children_small).bytes;
    let enc_file_node_large = fstree::encode_file_node(&children_large).bytes;
    let item_encodings: Vec<Vec<u8>> = (0..batch)
        .map(|i| keys[i % keys.len()].as_bytes().to_vec())
        .collect();

    // In-memory trees.
    let mem = MemStore::new();
    let ic = ItemChunker::new(ingest::DEFAULT_ITEM_BITS);
    let (wide_root, wide_names) = build_dir(&mem, ic, p.synthetic_wide, seed + 70);
    let (shallow_root, _) = build_dir(&mem, ic, 16, seed + 71);

    let mut child = shallow_root;
    let mut comps: Vec<String> = Vec::new();
    for d in (0..p.tree_depth).rev() {
        let mut db = DirBuilder::new(ic);
        let name = format!("level{d:03}");
        let mut emit = |o: Object| mem.put(o);
        // Entries must be added in bytewise name order: "entry-..." sorts
        // before "level...".
        db.add_entry(&mut emit, stored_entry(&mem, d, seed + 72))
            .unwrap();
        db.add_entry(
            &mut emit,
            Entry {
                name: name.clone().into_bytes(),
                mode: MODE_DIR,
                uid: 1000,
                gid: 1000,
                mtime: 1_700_000_000_000_000_000,
                content_key: key_bytes(child),
                ..Default::default()
            },
        )
        .unwrap();
        child = db.finish(&mut emit).unwrap();
        comps.insert(0, name);
    }
    let deep_root = child;
    let deep_path = comps.join("/");

    // One file built the way ingest builds files.
    let mut fb = IndexBuilder::new_file(ic);
    {
        let mut emit = |o: Object| mem.put(o);
        chunkers::split_bytes(&corpus_text[..], None, |chunk| {
            let o = fstree::encode_blob(&chunk);
            let k = o.key;
            emit(o)?;
            fb.add_child(&mut emit, k, &[])
                .map_err(|e| MemMissing(Key::new(Type::Blob, 0, e.to_string().as_bytes())))
        })
        .unwrap();
    }
    let file_root = {
        let mut emit = |o: Object| mem.put(o);
        fb.finish(&mut emit).unwrap()
    };
    let file_bytes = corpus_text.len() as i64;

    // A copy of the shallow tree with one leaf missing.
    let incomplete_store = mem.snapshot();
    let incomplete_root = shallow_root;
    let entries = fstree::collect_entries(shallow_root, |k| incomplete_store.get(k)).unwrap();
    incomplete_store.drop_key(Key::parse(&entries[0].content_key).unwrap());

    // On-disk trees.
    let tree_v1 = scratch.join("tree-v1");
    let tree_v2 = scratch.join("tree-v2");
    let ignore_dir = scratch.join("ignore-fixture");
    let _ = fs::remove_dir_all(&tree_v1);
    let _ = fs::remove_dir_all(&tree_v2);
    let _ = fs::remove_dir_all(&ignore_dir);
    let (tree_files, tree_bytes) = write_fixture_tree(&tree_v1, p, xattrs);
    stamp_tree(&tree_v1).unwrap();
    copy_tree(&tree_v1, &tree_v2).unwrap();
    mutate_fixture_tree(&tree_v2, p);
    stamp_tree(&tree_v2).unwrap();
    write_ignore_fixture(&ignore_dir);
    stamp_tree(&ignore_dir).unwrap();

    // Pack fixtures.
    let n = (p.batch_ops / 4).max(64);
    let mut pack_objects = Vec::with_capacity(n);
    for i in 0..n {
        let s = seed + 9000 + i as u64;
        let b = match i % 4 {
            0 => random_bytes(s, 512),
            1 => compressible_bytes(s, 4096),
            2 => random_bytes(s, 64 << 10),
            _ => compressible_bytes(s, 64 << 10),
        };
        pack_objects.push(fstree::encode_blob(&b));
    }
    let mut wire = Vec::new();
    {
        let mut w = amberpack::Writer::new(&mut wire);
        for o in &pack_objects {
            w.add(o.key, &o.bytes).unwrap();
        }
        w.finish().unwrap();
    }
    let wire_records: Vec<Vec<u8>> = pack_objects
        .iter()
        .map(|o| amberpack::encode_record(o.key, &o.bytes).unwrap())
        .collect();

    // Store templates.
    let store_template = scratch.join("store-template");
    let _ = fs::remove_dir_all(&store_template);
    fs::create_dir_all(&store_template).unwrap();
    let objs = store_objects(p, p.store_objects, 10000);
    {
        let st = packstore::Store::open_with(&store_template, store_options(p)).unwrap();
        let (_, res) = st.write_parallel(
            object_seq(&objs),
            WriteOpts {
                writers: p.threads_multi,
                ..Default::default()
            },
        );
        res.unwrap();
        st.sync().unwrap();
        st.close().unwrap();
    }
    let store_keys: Vec<Key> = objs.iter().map(|o| o.key).collect();
    let store_miss_keys: Vec<Key> = store_objects(p, p.batch_ops.max(1024), 20000)
        .iter()
        .map(|o| o.key)
        .collect();

    let garbage_template = scratch.join("store-garbage");
    let _ = fs::remove_dir_all(&garbage_template);
    fs::create_dir_all(&garbage_template).unwrap();
    {
        let st = packstore::Store::open_with(&garbage_template, store_options(p)).unwrap();
        let (_, res) = st.write_parallel(
            object_seq(&objs),
            WriteOpts {
                writers: p.threads_multi,
                ..Default::default()
            },
        );
        res.unwrap();
        st.sync().unwrap();
        st.close().unwrap();
    }
    let garbage_live: Vec<Key> = objs
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 10 == 0)
        .map(|(_, o)| o.key)
        .collect();

    // Reference records.
    let ref_record = Reference {
        name: "bench/reference/main".into(),
        key: key_bytes(store_keys[0]),
        user: "bench@example.org".into(),
        created_at: 1_700_000_000_000_000_000,
        signature: random_bytes(seed + 11000, 128),
        public_key: random_bytes(seed + 11001, 96),
    };
    let ref_record_enc = ref_record.encode().unwrap();
    let mut ref_names = Vec::with_capacity(p.ref_records);
    let mut ref_batch = Vec::with_capacity(p.ref_records);
    for i in 0..p.ref_records {
        let rec = Reference {
            name: format!("bench/ref/{i:06}"),
            key: key_bytes(store_keys[i % store_keys.len()]),
            user: "bench@example.org".into(),
            created_at: 1_700_000_000_000_000_000 + i as i64,
            ..Default::default()
        };
        let enc = rec.encode().unwrap();
        ref_names.push(rec.name.clone());
        ref_batch.push(refstore::Record {
            name: rec.name,
            data: enc,
        });
    }
    let refs_template = scratch.join("refs-template");
    let _ = fs::remove_dir_all(&refs_template);
    {
        let rs = refstore::Store::open(&refs_template, false).unwrap();
        rs.put_batch(&ref_batch).unwrap();
    }

    // Inbox packs.
    let inbox_objs = store_objects(p, p.inbox_packs * 32, 30000);
    let mut inbox_packs = Vec::with_capacity(p.inbox_packs);
    let mut inbox_roots = Vec::with_capacity(p.inbox_packs);
    for i in 0..p.inbox_packs {
        let mut buf = Vec::new();
        {
            let mut w = amberpack::Writer::new(&mut buf);
            for j in 0..32 {
                let o = &inbox_objs[i * 32 + j];
                w.add(o.key, &o.bytes).unwrap();
            }
            w.finish().unwrap();
        }
        inbox_packs.push(buf);
        inbox_roots.push(inbox_objs[i * 32].key);
    }

    // The gc template: one referenced tree plus enough unreferenced objects
    // to fill whole sealed segments.
    let gc_template = scratch.join("gc-template");
    let _ = fs::remove_dir_all(&gc_template);
    let gc_objects = gc_template.join("objects");
    fs::create_dir_all(&gc_objects).unwrap();
    let gc_live_root;
    {
        let st = packstore::Store::open_with(&gc_objects, store_options(p)).unwrap();
        let (_, res) = ingest::dir(
            &st,
            &tree_v1,
            ingest::Opts {
                jobs: p.threads_multi,
                ..Default::default()
            },
        );
        gc_live_root = res.unwrap();
        let garbage = store_objects(p, p.store_objects, 40000);
        let (_, res) = st.write_parallel(
            object_seq(&garbage),
            WriteOpts {
                writers: p.threads_multi,
                ..Default::default()
            },
        );
        res.unwrap();
        st.sync().unwrap();
        st.close().unwrap();
    }
    {
        let rs = refstore::Store::open(gc_template.join("refs"), false).unwrap();
        let rec = Reference {
            name: "gc/live".into(),
            key: key_bytes(gc_live_root),
            user: "bench@example.org".into(),
            created_at: 1_700_000_000_000_000_000,
            ..Default::default()
        };
        rs.put(&rec.name, &rec.encode().unwrap()).unwrap();
    }

    // The ingested tree in memory, plus the archive exported from it.
    let tree_mem = MemStore::new();
    let (stream, root) = ingest::objects(
        &tree_v1,
        ingest::Opts {
            jobs: p.threads_multi,
            ..Default::default()
        },
    )
    .unwrap();
    for o in stream {
        tree_mem.put(o.unwrap()).unwrap();
    }
    let tree_root = root.get().unwrap();
    let mut tar_bytes = Vec::new();
    amber_store_core::tarexport::write(&mut tar_bytes, tree_root, |k| tree_mem.get(k)).unwrap();

    // A packstore that already holds the V1 tree, for the incremental case.
    let ingest_template = scratch.join("ingest-template");
    let _ = fs::remove_dir_all(&ingest_template);
    fs::create_dir_all(&ingest_template).unwrap();
    {
        let st = packstore::Store::open_with(&ingest_template, store_options(p)).unwrap();
        let (_, res) = ingest::dir(
            &st,
            &tree_v1,
            ingest::Opts {
                jobs: p.threads_multi,
                ..Default::default()
            },
        );
        res.unwrap();
        st.close().unwrap();
    }

    // One long-lived copy of the store template for the read-only cases.
    let ro_dir = scratch.join("store-readonly");
    let _ = fs::remove_dir_all(&ro_dir);
    copy_tree(&store_template, &ro_dir).unwrap();
    let ro = Arc::new(packstore::Store::open_with(&ro_dir, store_options(p)).unwrap());
    let segs = ro.segments().unwrap();
    assert!(
        !segs.is_empty(),
        "the store fixture has no sealed segment; raise store_objects or lower segment_bytes"
    );
    let ro_seg_id = segs[0].id;
    let mut ro_locs = Vec::new();
    ro.scan_index(ro_seg_id, |_k, off, slen| {
        ro_locs.push(RecordLoc {
            id: ro_seg_id,
            off,
            len: slen,
        })
    })
    .unwrap();

    Fixtures {
        corpus_random,
        corpus_text,
        tiny,
        small,
        text,
        large,
        rand,
        keys,
        key_bytes: key_bytes_v,
        bad_key_bytes,
        xattrs_small,
        xattrs_large,
        xattrs_small_enc,
        xattrs_large_enc,
        entries_small,
        entries_large,
        pairs_small,
        pairs_large,
        children_small,
        children_large,
        enc_dir_leaf_small,
        enc_dir_leaf_large,
        enc_dir_node_small,
        enc_dir_node_large,
        enc_file_node_small,
        enc_file_node_large,
        item_encodings,
        mem,
        wide_root,
        wide_names,
        miss_name: b"entry-zzzzzzzz".to_vec(),
        deep_root,
        deep_path,
        shallow_root,
        file_root,
        file_bytes,
        incomplete_root,
        incomplete_store,
        tree_v1,
        tree_v2,
        ingest_template,
        tree_bytes,
        tree_files,
        ignore_dir,
        pack_objects,
        wire_pack: wire,
        wire_records,
        store_template,
        store_keys,
        store_miss_keys,
        garbage_template,
        garbage_live,
        refs_template,
        ref_names,
        gc_template,
        gc_live_root,
        inbox_packs,
        inbox_roots,
        ref_record,
        ref_record_enc,
        ref_batch,
        tree_mem,
        tree_root,
        tar_bytes,
        ro: Some(ro),
        ro_dir,
        ro_locs,
        ro_seg_id,
    }
}

/// Every entry points at a Blob that really is in the store, so the
/// completeness walk has something to find.
fn stored_entry(mem: &MemStore, i: usize, seed: u64) -> Entry {
    let body = format!("amber-core-ops leaf {seed:016x}/{i:08}");
    let o = fstree::encode_blob(body.as_bytes());
    let k = o.key;
    mem.put(o).unwrap();
    let mut e = entry_for(i, seed, None);
    e.content_key = key_bytes(k);
    e
}

fn build_dir(mem: &MemStore, ic: ItemChunker, n: usize, seed: u64) -> (Key, Vec<Vec<u8>>) {
    let mut db = DirBuilder::new(ic);
    let mut names = Vec::with_capacity(n);
    let mut emit = |o: Object| mem.put(o);
    for i in 0..n {
        let e = stored_entry(mem, i, seed);
        names.push(e.name.clone());
        db.add_entry(&mut emit, e).unwrap();
    }
    let root = db.finish(&mut emit).unwrap();
    (root, names)
}

fn build_key_fixtures(p: &Profile) -> (Vec<Key>, Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut r = Rng::new(p.seed + 20);
    let types = [
        Type::Blob,
        Type::FileNode,
        Type::DirLeaf,
        Type::DirNode,
        Type::XattrSet,
    ];
    let mut keys = Vec::with_capacity(p.batch_ops);
    let mut kb = Vec::with_capacity(p.batch_ops);
    for i in 0..p.batch_ops {
        let h: [u8; 32] = random_bytes(p.seed + 20 + i as u64, 32).try_into().unwrap();
        let length = r.next() % (1 << 40);
        let k = Key::new_from_hash(types[i % types.len()], length, h);
        keys.push(k);
        kb.push(k.as_bytes().to_vec());
    }
    // Malformed keys: the reserved header bit, a reserved object type, a
    // non-canonical (zero-padded) length and a short buffer.
    let base = kb[0].clone();
    let mut reserved = base.clone();
    reserved[0] |= 0x08;
    let mut bad_type = base.clone();
    bad_type[0] = (bad_type[0] & 0x0F) | 0x50;
    let padded = Key::new_from_hash(Type::Blob, 1 << 16, [0u8; 32]);
    let mut non_canon = padded.as_bytes().to_vec();
    non_canon[0] = (non_canon[0] & 0xF0) | 0x04;
    non_canon[1] = 0x00;
    let short = base[..31].to_vec();
    (keys, kb, vec![reserved, bad_type, non_canon, short])
}

type XattrMap = BTreeMap<Vec<u8>, Vec<u8>>;

fn build_xattr_fixtures(p: &Profile) -> (XattrMap, XattrMap) {
    let mut small = BTreeMap::new();
    small.insert(b"user.amber.a".to_vec(), b"1".to_vec());
    small.insert(b"user.amber.b".to_vec(), b"value".to_vec());
    small.insert(b"user.mime".to_vec(), b"application/octet-stream".to_vec());
    let mut large = BTreeMap::new();
    for i in 0..64u64 {
        large.insert(
            format!("user.amber.attr{i:03}").into_bytes(),
            random_bytes(p.seed + 30 + i, 48),
        );
    }
    (small, large)
}

#[allow(clippy::type_complexity)]
fn build_codec_fixtures(
    p: &Profile,
    xattrs_small_enc: &[u8],
) -> (
    Vec<Entry>,
    Vec<Entry>,
    Vec<DirPair>,
    Vec<DirPair>,
    Vec<Key>,
    Vec<Key>,
) {
    let entries_small: Vec<Entry> = (0..8).map(|i| entry_for(i, p.seed + 40, None)).collect();
    let entries_large: Vec<Entry> = (0..128)
        .map(|i| {
            entry_for(
                i,
                p.seed + 40,
                if i % 8 == 0 {
                    Some(xattrs_small_enc)
                } else {
                    None
                },
            )
        })
        .collect();
    let mk_pairs = |n: usize| -> Vec<DirPair> {
        (0..n)
            .map(|i| {
                let h: [u8; 32] = random_bytes(p.seed + 50 + i as u64, 32).try_into().unwrap();
                let k = Key::new_from_hash(Type::DirLeaf, 4096 + i as u64, h);
                DirPair {
                    sep_name: format!("entry-{:08}", i * 7).into_bytes(),
                    child_key: key_bytes(k),
                }
            })
            .collect()
    };
    let mk_children = |n: usize| -> Vec<Key> {
        (0..n)
            .map(|i| {
                let h: [u8; 32] = random_bytes(p.seed + 60 + i as u64, 32).try_into().unwrap();
                Key::new_from_hash(Type::Blob, 65536, h)
            })
            .collect()
    };
    (
        entries_small,
        entries_large,
        mk_pairs(8),
        mk_pairs(128),
        mk_children(8),
        mk_children(1024),
    )
}

/// The deterministic object population of the packstore fixtures.
pub fn store_objects(p: &Profile, n: usize, salt: u64) -> Vec<Object> {
    (0..n)
        .map(|i| {
            let s = p
                .seed
                .wrapping_add(salt)
                .wrapping_add((i as u64).wrapping_mul(0x0100_0000_01B3));
            let b = match i % 5 {
                0 => random_bytes(s, 128),
                1 => compressible_bytes(s, 1024),
                2 => random_bytes(s, 8 << 10),
                3 => compressible_bytes(s, 32 << 10),
                _ => random_bytes(s, 2 << 10),
            };
            fstree::encode_blob(&b)
        })
        .collect()
}

pub fn store_objects_of_size(p: &Profile, n: usize, size: usize, salt: u64) -> Vec<Object> {
    (0..n)
        .map(|i| {
            let s = p
                .seed
                .wrapping_add(salt)
                .wrapping_add((i as u64).wrapping_mul(0x0100_0000_01B3));
            fstree::encode_blob(&random_bytes(s, size))
        })
        .collect()
}

/// Lays out the deterministic source tree.
fn write_fixture_tree(root: &Path, p: &Profile, xattrs: bool) -> (i64, i64) {
    fs::create_dir_all(root).unwrap();
    let mut files = 0i64;
    let mut total = 0i64;
    let write = |rel: String, b: &[u8], files: &mut i64, total: &mut i64| {
        let full = root.join(&rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, b).unwrap();
        *files += 1;
        *total += b.len() as i64;
    };
    fs::write(root.join(".amberignore"), b"*.tmp\n!keep.tmp\n/build/\n").unwrap();
    files += 1;

    let mut r = Rng::new(p.seed + 80);
    let dirs = (p.tree_files / 10).max(4);
    for d in 0..dirs {
        for f in 0..10 {
            let idx = d * 10 + f;
            let mut size = 1usize << (6 + r.n(9));
            if idx % 97 == 0 {
                size = 3 << 20;
            }
            let s = p.seed + 1000 + idx as u64;
            let b = if idx % 3 == 0 {
                random_bytes(s, size)
            } else {
                compressible_bytes(s, size)
            };
            write(
                format!("data/d{d:03}/f{f:03}.bin"),
                &b,
                &mut files,
                &mut total,
            );
        }
        write(
            format!("data/d{d:03}/scratch.tmp"),
            b"ignored\n",
            &mut files,
            &mut total,
        );
        write(
            format!("data/d{d:03}/keep.tmp"),
            b"kept by negation\n",
            &mut files,
            &mut total,
        );
    }
    for i in 0..8u64 {
        let b = compressible_bytes(p.seed + 2000 + i, 4096);
        write(format!("build/gen{i}.bin"), &b, &mut files, &mut total);
    }
    for i in 0..p.tree_wide {
        let b = random_bytes(p.seed + 3000 + i as u64, 64 + i % 448);
        write(format!("wide/w{i:06}.bin"), &b, &mut files, &mut total);
    }
    let mut deep = PathBuf::from("deep");
    for d in 0..p.tree_depth {
        deep.push(format!("l{d:02}"));
    }
    let b = compressible_bytes(p.seed + 4000, 8192);
    write(
        deep.join("leaf.bin").to_string_lossy().to_string(),
        &b,
        &mut files,
        &mut total,
    );
    let b = compressible_bytes(p.seed + 5000, (p.corpus_bytes / 8) as usize);
    write("big.bin".to_string(), &b, &mut files, &mut total);

    fs::create_dir_all(root.join("links")).unwrap();
    for i in 0..8 {
        std::os::unix::fs::symlink(
            format!("../data/d000/f{i:03}.bin"),
            root.join("links").join(format!("ln{i:03}")),
        )
        .unwrap();
    }
    if xattrs {
        set_xattr(&root.join("data/d000/f000.bin"), "user.amber.small", b"v1").unwrap();
        set_xattr(
            &root.join("data/d000/f001.bin"),
            "user.amber.large",
            &random_bytes(p.seed + 6000, 512),
        )
        .unwrap();
    }
    (files, total)
}

fn set_xattr(path: &Path, name: &str, value: &[u8]) -> io::Result<()> {
    let cpath = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let cname = std::ffi::CString::new(name).unwrap();
    // SAFETY: both strings are NUL-terminated and the value slice is valid.
    let rc = unsafe {
        libc::setxattr(
            cpath.as_ptr(),
            cname.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The deterministic incremental change: a few files grow, a few appear and a
/// few vanish.
fn mutate_fixture_tree(root: &Path, p: &Profile) {
    let dirs = (p.tree_files / 10).max(4);
    let mut d = 0;
    while d < dirs {
        let path = root.join(format!("data/d{d:03}/f000.bin"));
        let mut b = fs::read(&path).unwrap();
        b.extend_from_slice(&compressible_bytes(p.seed + 7000 + d as u64, 4096));
        fs::write(&path, &b).unwrap();
        d += 7;
    }
    for i in 0..16u64 {
        fs::write(
            root.join(format!("data/new{i:03}.bin")),
            compressible_bytes(p.seed + 8000 + i, 2048),
        )
        .unwrap();
    }
    let mut d = 1;
    while d < dirs && d < 9 {
        fs::remove_file(root.join(format!("data/d{d:03}/f009.bin"))).unwrap();
        d += 1;
    }
}

/// Builds a directory whose .amberignore files stack.
fn write_ignore_fixture(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join(".amberignore"),
        b"*.o\n*.tmp\n!keep.tmp\n/vendor/\n**/target/\n",
    )
    .unwrap();
    for d in 0..8 {
        let sub = root.join(format!("sub{d:02}"));
        fs::create_dir_all(&sub).unwrap();
        if d % 2 == 0 {
            fs::write(
                sub.join(".amberignore"),
                format!("gen{d:02}-*\n!gen{d:02}-keep\n"),
            )
            .unwrap();
        }
    }
}
