//! fstree: encoders, decoders, the bottom-up builders and the read paths.

use std::io;

use amber_store_core::chunkers::ItemChunker;
use amber_store_core::fstree::{self, DirBuilder, Entry, IndexBuilder, Object};
use amber_store_core::ingest;
use amber_store_core::key::{Key, Type};

use crate::env::Env;
use crate::fixtures::{
    MemMissing, digest, digest_keys, digest_list, digest_vecs, entry_for, random_bytes,
};
use crate::harness::{Case, Recorder, State};

const REPS: usize = 64;

/// Counts built objects without retaining them, so a builder case measures
/// the builder rather than a map insert.
#[derive(Default)]
struct Counting {
    n: usize,
    bytes: i64,
}

/// Builds one directory of `n` synthetic entries and returns the object count
/// and the root key.
fn build_dir_once(n: usize, entries: &[Entry]) -> (usize, Key) {
    let mut c = Counting::default();
    let mut db = DirBuilder::new(ItemChunker::new(ingest::DEFAULT_ITEM_BITS));
    let root = {
        let mut emit = |o: Object| -> Result<(), io::Error> {
            c.n += 1;
            c.bytes += o.bytes.len() as i64;
            Ok(())
        };
        for e in entries.iter().take(n) {
            db.add_entry(&mut emit, e.clone()).unwrap();
        }
        db.finish(&mut emit).unwrap()
    };
    (c.n, root)
}

fn build_index_once(keys: &[Key]) -> (usize, Key) {
    let mut c = Counting::default();
    let mut ib = IndexBuilder::new_file(ItemChunker::new(ingest::DEFAULT_ITEM_BITS));
    let root = {
        let mut emit = |o: Object| -> Result<(), io::Error> {
            c.n += 1;
            c.bytes += o.bytes.len() as i64;
            Ok(())
        };
        for k in keys {
            ib.add_child(&mut emit, *k, &[]).unwrap();
        }
        ib.finish(&mut emit).unwrap()
    };
    (c.n, root)
}

pub fn cases(env: &Env) -> Vec<Case> {
    let fx = &env.fx;
    let mut out = Vec::new();

    // --- encoders -----------------------------------------------------
    for (name, which) in [("tiny-64B", 0usize), ("large-1MiB-text", 3usize)] {
        let ps = crate::cases::payload_set(fx, which);
        out.push(
            Case::new(
                "fstree",
                "fstree.encode_blob",
                name,
                1,
                ps.items.len(),
                Box::new(move |_| Box::new(which) as State),
                Box::new(move |env, s| {
                    let ps =
                        crate::cases::payload_set(&env.fx, *s.downcast_ref::<usize>().unwrap());
                    let mut acc = 0u64;
                    for b in &ps.items {
                        acc += fstree::encode_blob(b).key.as_bytes()[0] as u64;
                    }
                    acc
                }),
            )
            .bytes(ps.bytes),
        );
    }

    macro_rules! enc_case {
        ($op:expr, $wl:expr, $bytes:expr, $body:expr) => {
            out.push(
                Case::new(
                    "fstree",
                    $op,
                    $wl,
                    1,
                    REPS,
                    Box::new(|_| Box::new(()) as State),
                    Box::new($body),
                )
                .bytes($bytes),
            )
        };
    }

    enc_case!(
        "fstree.encode_dir_leaf",
        "entries-8",
        (REPS * fx.enc_dir_leaf_small.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::encode_dir_leaf(&env.fx.entries_small)
                    .unwrap()
                    .bytes
                    .len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.encode_dir_leaf",
        "entries-128-with-xattrs",
        (REPS * fx.enc_dir_leaf_large.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::encode_dir_leaf(&env.fx.entries_large)
                    .unwrap()
                    .bytes
                    .len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.encode_dir_node",
        "pairs-8",
        (REPS * fx.enc_dir_node_small.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::encode_dir_node(&env.fx.pairs_small)
                    .unwrap()
                    .bytes
                    .len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.encode_dir_node",
        "pairs-128",
        (REPS * fx.enc_dir_node_large.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::encode_dir_node(&env.fx.pairs_large)
                    .unwrap()
                    .bytes
                    .len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.encode_file_node",
        "children-8",
        (REPS * fx.enc_file_node_small.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::encode_file_node(&env.fx.children_small).bytes.len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.encode_file_node",
        "children-1024",
        (REPS * fx.enc_file_node_large.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::encode_file_node(&env.fx.children_large).bytes.len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.encode_xattr_set",
        "xattrs-64",
        0,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::encode_xattr_set(&env.fx.xattrs_large).bytes.len() as u64;
            }
            acc
        }
    );

    // --- decoders -----------------------------------------------------
    enc_case!(
        "fstree.decode_dir_leaf",
        "entries-8",
        (REPS * fx.enc_dir_leaf_small.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::decode_dir_leaf(&env.fx.enc_dir_leaf_small)
                    .unwrap()
                    .len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.decode_dir_leaf",
        "entries-128-with-xattrs",
        (REPS * fx.enc_dir_leaf_large.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::decode_dir_leaf(&env.fx.enc_dir_leaf_large)
                    .unwrap()
                    .len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.decode_dir_node",
        "pairs-128",
        (REPS * fx.enc_dir_node_large.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::decode_dir_node(&env.fx.enc_dir_node_large)
                    .unwrap()
                    .len() as u64;
            }
            acc
        }
    );
    enc_case!(
        "fstree.decode_file_node",
        "children-1024",
        (REPS * fx.enc_file_node_large.len()) as i64,
        |env: &Env, _: &mut State| {
            let mut acc = 0u64;
            for _ in 0..REPS {
                acc += fstree::decode_file_node(&env.fx.enc_file_node_large)
                    .unwrap()
                    .len() as u64;
            }
            acc
        }
    );

    // --- child keys ---------------------------------------------------
    for (name, which) in [
        ("dir-leaf-128", 0usize),
        ("dir-node-128", 1usize),
        ("file-node-1024", 2usize),
    ] {
        out.push(Case::new(
            "fstree",
            "fstree.child_keys",
            name,
            1,
            REPS,
            Box::new(move |env| {
                let (k, b): (Key, Vec<u8>) = match which {
                    0 => (
                        fstree::encode_dir_leaf(&env.fx.entries_large).unwrap().key,
                        env.fx.enc_dir_leaf_large.clone(),
                    ),
                    1 => (
                        fstree::encode_dir_node(&env.fx.pairs_large).unwrap().key,
                        env.fx.enc_dir_node_large.clone(),
                    ),
                    _ => (
                        fstree::encode_file_node(&env.fx.children_large).key,
                        env.fx.enc_file_node_large.clone(),
                    ),
                };
                Box::new((k, b)) as State
            }),
            Box::new(|_, s| {
                let (k, b) = s.downcast_ref::<(Key, Vec<u8>)>().unwrap();
                let mut acc = 0u64;
                for _ in 0..REPS {
                    acc += fstree::child_keys(*k, b).unwrap().len() as u64;
                }
                acc
            }),
        ));
    }

    // --- builders -----------------------------------------------------
    for n in [128usize, env.profile.synthetic_wide] {
        out.push(Case::new(
            "fstree",
            "fstree.dir_builder",
            &format!("entries-{n}"),
            1,
            n,
            Box::new(move |env| {
                let seed = env.profile.seed + 70;
                let entries: Vec<Entry> = (0..n).map(|i| entry_for(i, seed, None)).collect();
                Box::new(entries) as State
            }),
            Box::new(move |_, s| {
                let entries = s.downcast_ref::<Vec<Entry>>().unwrap();
                let (count, root) = build_dir_once(n, entries);
                count as u64 + root.as_bytes()[0] as u64
            }),
        ));
    }
    for n in [128usize, 65536] {
        out.push(Case::new(
            "fstree",
            "fstree.index_builder_file",
            &format!("children-{n}"),
            1,
            n,
            Box::new(move |env| {
                let seed = env.profile.seed + 90;
                let keys: Vec<Key> = (0..n)
                    .map(|i| {
                        let h: [u8; 32] = random_bytes(seed + i as u64, 32).try_into().unwrap();
                        Key::new_from_hash(Type::Blob, 65536, h)
                    })
                    .collect();
                Box::new(keys) as State
            }),
            Box::new(|_, s| {
                let keys = s.downcast_ref::<Vec<Key>>().unwrap();
                let (count, root) = build_index_once(keys);
                count as u64 + root.as_bytes()[0] as u64
            }),
        ));
    }

    // --- read paths ---------------------------------------------------
    let lookups = env.profile.batch_ops;
    out.push(Case::new(
        "fstree",
        "fstree.lookup_entry",
        "wide/hit",
        1,
        lookups,
        Box::new(|_| Box::new(()) as State),
        Box::new(move |env, _| {
            let names = &env.fx.wide_names;
            let mut acc = 0u64;
            for i in 0..lookups {
                let e: Entry =
                    fstree::lookup_entry(env.fx.wide_root, &names[(i * 7919) % names.len()], |k| {
                        env.fx.mem.get(k)
                    })
                    .unwrap();
                acc += e.name.len() as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "fstree",
        "fstree.lookup_entry",
        "wide/miss",
        1,
        lookups,
        Box::new(|_| Box::new(()) as State),
        Box::new(move |env, _| {
            let mut acc = 0u64;
            for _ in 0..lookups {
                let r: Result<Entry, _> =
                    fstree::lookup_entry(env.fx.wide_root, &env.fx.miss_name, |k| {
                        env.fx.mem.get(k)
                    });
                if matches!(&r, Err(e) if e.is_not_found()) {
                    acc += 1;
                }
            }
            acc
        }),
    ));
    out.push(Case::new(
        "fstree",
        "fstree.lookup_entry",
        "shallow/hit",
        1,
        lookups,
        Box::new(|_| Box::new(()) as State),
        Box::new(move |env, _| {
            let mut acc = 0u64;
            for i in 0..lookups {
                let name = format!("entry-{:08}", i % 16);
                let e: Entry = fstree::lookup_entry(env.fx.shallow_root, name.as_bytes(), |k| {
                    env.fx.mem.get(k)
                })
                .unwrap();
                acc += e.name.len() as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "fstree",
        "fstree.list_entries",
        "wide/first-page-100",
        1,
        64,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for _ in 0..64 {
                let (es, more): (Vec<Entry>, bool) =
                    fstree::list_entries(env.fx.wide_root, &[], 100, |k| env.fx.mem.get(k))
                        .unwrap();
                acc += es.len() as u64 + more as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "fstree",
        "fstree.list_entries",
        "wide/full-paging-100",
        1,
        1,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            let mut after: Vec<u8> = Vec::new();
            loop {
                let (es, more): (Vec<Entry>, bool) =
                    fstree::list_entries(env.fx.wide_root, &after, 100, |k| env.fx.mem.get(k))
                        .unwrap();
                acc += es.len() as u64;
                if !more || es.is_empty() {
                    break;
                }
                after = es.last().unwrap().name.clone();
            }
            acc
        }),
    ));
    out.push(Case::new(
        "fstree",
        "fstree.collect_entries",
        "wide",
        1,
        1,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let es: Vec<Entry> =
                fstree::collect_entries(env.fx.wide_root, |k| env.fx.mem.get(k)).unwrap();
            es.len() as u64
        }),
    ));
    out.push(Case::new(
        "fstree",
        "fstree.resolve_path",
        "deep",
        1,
        256,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for _ in 0..256 {
                let k: Key = fstree::resolve_path(env.fx.deep_root, &env.fx.deep_path, |k| {
                    env.fx.mem.get(k)
                })
                .unwrap();
                acc += k.as_bytes()[0] as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "fstree",
        "fstree.resolve_entry",
        "deep",
        1,
        256,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let path = format!("{}/entry-00000000", env.fx.deep_path);
            let mut acc = 0u64;
            for _ in 0..256 {
                let e: Option<Entry> =
                    fstree::resolve_entry(env.fx.deep_root, &path, |k| env.fx.mem.get(k)).unwrap();
                acc += e.unwrap().name.len() as u64;
            }
            acc
        }),
    ));
    out.push(
        Case::new(
            "fstree",
            "fstree.write_content",
            "file-corpus",
            1,
            1,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut c = CountingWriter::default();
                fstree::write_content(&mut c, env.fx.file_root, |k| env.fx.mem.get(k)).unwrap();
                c.n as u64
            }),
        )
        .bytes(fx.file_bytes),
    );
    // reachable_keys and check_complete choose their own parallelism
    // (available_parallelism here, GOMAXPROCS in Go). Threads 0 marks that;
    // the driver runs under a fixed CPU set so both cores see the same bound.
    for (name, which) in [("wide", 0usize), ("deep", 1usize), ("file-corpus", 2usize)] {
        out.push(Case::new(
            "fstree",
            "fstree.reachable_keys",
            name,
            0,
            1,
            Box::new(move |_| Box::new(which) as State),
            Box::new(|env, s| {
                let root = match *s.downcast_ref::<usize>().unwrap() {
                    0 => env.fx.wide_root,
                    1 => env.fx.deep_root,
                    _ => env.fx.file_root,
                };
                let ks: Vec<Key> = fstree::reachable_keys(root, |k| env.fx.mem.get(k)).unwrap();
                ks.len() as u64
            }),
        ));
    }
    for jobs in [env.profile.threads_single, env.profile.threads_multi] {
        out.push(Case::new(
            "fstree",
            "fstree.check_complete",
            &format!("wide/jobs-{jobs}"),
            jobs,
            1,
            Box::new(|_| Box::new(()) as State),
            Box::new(move |env, _| {
                let ks: Vec<Key> = fstree::check_complete(
                    env.fx.wide_root,
                    |k| env.fx.mem.get(k),
                    |k| env.fx.mem.has(k),
                    jobs,
                )
                .unwrap();
                ks.len() as u64
            }),
        ));
    }
    out.push(Case::new(
        "fstree",
        "fstree.check_complete",
        "incomplete/jobs-1",
        1,
        1,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let r: Result<Vec<Key>, _> = fstree::check_complete(
                env.fx.incomplete_root,
                |k| env.fx.incomplete_store.get(k),
                |k| env.fx.incomplete_store.has(k),
                1,
            );
            match r {
                Err(e) if e.missing_object().is_some() => 1,
                other => panic!("expected a missing-object error, got {other:?}"),
            }
        }),
    ));
    out
}

#[derive(Default)]
struct CountingWriter {
    n: i64,
}

impl io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.n += buf.len() as i64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;

    for (name, b, op) in [
        (
            "dir-leaf-8",
            &fx.enc_dir_leaf_small,
            "fstree.encode_dir_leaf",
        ),
        (
            "dir-leaf-128",
            &fx.enc_dir_leaf_large,
            "fstree.encode_dir_leaf",
        ),
        (
            "dir-node-8",
            &fx.enc_dir_node_small,
            "fstree.encode_dir_node",
        ),
        (
            "dir-node-128",
            &fx.enc_dir_node_large,
            "fstree.encode_dir_node",
        ),
        (
            "file-node-8",
            &fx.enc_file_node_small,
            "fstree.encode_file_node",
        ),
        (
            "file-node-1024",
            &fx.enc_file_node_large,
            "fstree.encode_file_node",
        ),
    ] {
        rec.pass("fstree", op, &format!("fstree/encode-{name}"), digest(b));
    }
    let xo = fstree::encode_xattr_set(&fx.xattrs_large);
    rec.pass(
        "fstree",
        "fstree.encode_xattr_set",
        "fstree/encode-xattr-set-64",
        digest(&xo.bytes),
    );
    let bo = fstree::encode_blob(&fx.small.items[0]);
    rec.want(
        "fstree",
        "fstree.encode_blob",
        "fstree/encode-blob",
        bo.bytes == fx.small.items[0] && bo.key.type_() == Type::Blob,
        "a Blob object must be the raw content bytes",
        bo.key.to_string(),
    );

    let es = fstree::decode_dir_leaf(&fx.enc_dir_leaf_large);
    rec.want(
        "fstree",
        "fstree.decode_dir_leaf",
        "fstree/decode-dir-leaf",
        matches!(&es, Ok(v) if v.len() == fx.entries_large.len()
            && v[0].name == fx.entries_large[0].name
            && v[8].xattrs_in == fx.entries_large[8].xattrs_in),
        format!("dir leaf round trip failed: {es:?}"),
        digest(&fstree::encode_dir_leaf(es.as_ref().unwrap()).unwrap().bytes),
    );
    let ps = fstree::decode_dir_node(&fx.enc_dir_node_large);
    rec.want(
        "fstree",
        "fstree.decode_dir_node",
        "fstree/decode-dir-node",
        matches!(&ps, Ok(v) if v.len() == fx.pairs_large.len() && v[3].sep_name == fx.pairs_large[3].sep_name),
        format!("dir node round trip failed: {ps:?}"),
        digest(&fstree::encode_dir_node(ps.as_ref().unwrap()).unwrap().bytes),
    );
    let ks = fstree::decode_file_node(&fx.enc_file_node_large);
    rec.want(
        "fstree",
        "fstree.decode_file_node",
        "fstree/decode-file-node",
        matches!(&ks, Ok(v) if v.len() == fx.children_large.len() && v[5] == fx.children_large[5]),
        format!("file node round trip failed: {ks:?}"),
        digest_keys(ks.as_ref().unwrap()),
    );
    let half = fx.enc_dir_leaf_large.len() / 2;
    rec.want(
        "fstree",
        "fstree.decode_dir_leaf",
        "fstree/decode-rejects-truncated",
        fstree::decode_dir_leaf(&fx.enc_dir_leaf_large[..half]).is_err(),
        "a truncated DirLeaf body was accepted",
        "rejected",
    );

    let leaf_key = fstree::encode_dir_leaf(&fx.entries_large).unwrap().key;
    let ck = fstree::child_keys(leaf_key, &fx.enc_dir_leaf_large);
    rec.want(
        "fstree",
        "fstree.child_keys",
        "fstree/child-keys-dir-leaf",
        matches!(&ck, Ok(v) if !v.is_empty()),
        format!("child keys of a DirLeaf: {ck:?}"),
        digest_keys(ck.as_ref().map(|v| v.as_slice()).unwrap_or(&[])),
    );
    let blob_key = fstree::encode_blob(&fx.small.items[0]).key;
    let ck = fstree::child_keys(blob_key, &fx.small.items[0]);
    rec.want(
        "fstree",
        "fstree.child_keys",
        "fstree/child-keys-blob-is-leaf",
        matches!(&ck, Ok(v) if v.is_empty()),
        format!("a Blob must have no children: {ck:?}"),
        "0",
    );

    rec.pass(
        "fstree",
        "fstree.dir_builder",
        "fstree/dir-builder-root",
        fx.wide_root.to_string(),
    );
    rec.pass(
        "fstree",
        "fstree.index_builder_file",
        "fstree/file-index-root",
        fx.file_root.to_string(),
    );

    let mid = fx.wide_names.len() / 2;
    let ent: Result<Entry, _> =
        fstree::lookup_entry(fx.wide_root, &fx.wide_names[mid], |k| fx.mem.get(k));
    rec.want(
        "fstree",
        "fstree.lookup_entry",
        "fstree/lookup-hit",
        matches!(&ent, Ok(e) if e.name == fx.wide_names[mid]),
        format!("lookup failed: {ent:?}"),
        digest(ent.as_ref().map(|e| e.name.as_slice()).unwrap_or(&[])),
    );
    let miss: Result<Entry, _> =
        fstree::lookup_entry(fx.wide_root, &fx.miss_name, |k| fx.mem.get(k));
    rec.want(
        "fstree",
        "fstree.lookup_entry",
        "fstree/lookup-miss",
        matches!(&miss, Err(e) if e.is_not_found()),
        format!("a missing name must be reported as not-found, got {miss:?}"),
        "ErrNotFound",
    );

    let all: Result<Vec<Entry>, _> = fstree::collect_entries(fx.wide_root, |k| fx.mem.get(k));
    let all_names: Vec<Vec<u8>> = all
        .as_ref()
        .map(|v| v.iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default();
    rec.want(
        "fstree",
        "fstree.collect_entries",
        "fstree/collect-entries",
        matches!(&all, Ok(v) if v.len() == fx.wide_names.len()),
        format!("collect entries: {:?}", all.as_ref().err()),
        digest_vecs(&all_names),
    );

    let mut paged: Vec<Vec<u8>> = Vec::new();
    let mut after: Vec<u8> = Vec::new();
    let mut paging_ok = true;
    loop {
        match fstree::list_entries(fx.wide_root, &after, 100, |k| fx.mem.get(k)) {
            Ok((page, more)) => {
                for p in &page {
                    paged.push(p.name.clone());
                }
                if !more || page.is_empty() {
                    break;
                }
                after = page.last().unwrap().name.clone();
            }
            Err(e) => {
                rec.fail(
                    "fstree",
                    "fstree.list_entries",
                    "fstree/list-paging",
                    e.to_string(),
                );
                paging_ok = false;
                break;
            }
        }
    }
    if paging_ok {
        let mut sorted = fx.wide_names.clone();
        sorted.sort();
        rec.want(
            "fstree",
            "fstree.list_entries",
            "fstree/list-paging",
            paged.len() == fx.wide_names.len() && paged[0] == sorted[0],
            format!(
                "paging returned {} of {} entries",
                paged.len(),
                fx.wide_names.len()
            ),
            digest_vecs(&paged),
        );
    }

    let k: Result<Key, _> = fstree::resolve_path(fx.deep_root, &fx.deep_path, |k| fx.mem.get(k));
    rec.want(
        "fstree",
        "fstree.resolve_path",
        "fstree/resolve-path",
        matches!(&k, Ok(v) if *v == fx.shallow_root),
        format!("resolve path: {k:?}"),
        k.map(|v| v.to_string()).unwrap_or_default(),
    );
    let missing: Result<Key, _> =
        fstree::resolve_path(fx.deep_root, &format!("{}/nope", fx.deep_path), |k| {
            fx.mem.get(k)
        });
    rec.want(
        "fstree",
        "fstree.resolve_path",
        "fstree/resolve-path-missing",
        matches!(&missing, Err(e) if e.is_not_found()),
        format!("a missing component must be reported as not-found, got {missing:?}"),
        "ErrNotFound",
    );
    let dotdot: Result<Key, _> = fstree::resolve_path(fx.deep_root, "..", |k| fx.mem.get(k));
    rec.want(
        "fstree",
        "fstree.resolve_path",
        "fstree/resolve-path-rejects-dotdot",
        dotdot.is_err(),
        "'..' must be rejected",
        "rejected",
    );
    let re: Result<Option<Entry>, _> = fstree::resolve_entry(fx.deep_root, "", |k| fx.mem.get(k));
    rec.want(
        "fstree",
        "fstree.resolve_entry",
        "fstree/resolve-entry-root-is-nil",
        matches!(&re, Ok(None)),
        format!("the root is not an entry: {re:?}"),
        "nil",
    );

    let mut buf: Vec<u8> = Vec::new();
    let wc = fstree::write_content(&mut buf, fx.file_root, |k| fx.mem.get(k));
    rec.want(
        "fstree",
        "fstree.write_content",
        "fstree/write-content",
        wc.is_ok() && buf == fx.corpus_text,
        format!("reassembled content differs from the source corpus: {wc:?}"),
        digest(&buf),
    );

    let rk: Result<Vec<Key>, _> = fstree::reachable_keys(fx.wide_root, |k| fx.mem.get(k));
    let rk_len = rk.as_ref().map(|v| v.len()).unwrap_or(0);
    rec.want(
        "fstree",
        "fstree.reachable_keys",
        "fstree/reachable-root-first",
        matches!(&rk, Ok(v) if !v.is_empty() && v[0] == fx.wide_root),
        format!("reachable keys: {:?}", rk.as_ref().err()),
        format!("n={rk_len}"),
    );
    let distinct = rk
        .as_ref()
        .map(|v| {
            let set: std::collections::HashSet<&Key> = v.iter().collect();
            set.len() == v.len()
        })
        .unwrap_or(false);
    rec.want(
        "fstree",
        "fstree.reachable_keys",
        "fstree/reachable-distinct",
        distinct,
        "a key was reported twice",
        format!("n={rk_len}"),
    );

    let vis: Result<Vec<Key>, _> =
        fstree::check_complete(fx.wide_root, |k| fx.mem.get(k), |k| fx.mem.has(k), 1);
    rec.want(
        "fstree",
        "fstree.check_complete",
        "fstree/check-complete",
        matches!(&vis, Ok(v) if v.len() == rk_len && v[0] == fx.wide_root),
        format!("check complete: {:?}", vis.as_ref().err()),
        format!("n={}", vis.as_ref().map(|v| v.len()).unwrap_or(0)),
    );
    let inc: Result<Vec<Key>, _> = fstree::check_complete(
        fx.incomplete_root,
        |k| fx.incomplete_store.get(k),
        |k| fx.incomplete_store.has(k),
        1,
    );
    rec.want(
        "fstree",
        "fstree.check_complete",
        "fstree/check-complete-missing",
        matches!(&inc, Err(e) if e.missing_object().is_some()),
        format!("an absent leaf must surface as MissingObjectError, got {inc:?}"),
        "MissingObjectError",
    );
    let _: Option<MemMissing> = None;
    let _ = digest_list(std::iter::empty::<&[u8]>());
}
