//! fstree: encoders, decoders, the bottom-up builders and the read paths.

use std::io;

use amber_store_core::chunkers::ItemChunker;
use amber_store_core::fstree::{self, DirBuilder, Entry, IndexBuilder, Object};
use amber_store_core::ingest;
use amber_store_core::key::{Key, Type};

use crate::env::Env;
use crate::fixtures::{
    MemMissing, PayloadSet, digest, digest_keys, digest_list, digest_vecs, entry_for, fold_bool,
    fold_i64, fold_key, fold_u64, new_fold, random_bytes, sink_bytes,
};
use crate::harness::{Case, Dims, Recorder, State, bytes_kind};

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
fn build_dir_once(n: usize, entries: &[Entry]) -> (usize, i64, Key) {
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
    (c.n, c.bytes, root)
}

fn build_index_once(keys: &[Key]) -> (usize, i64, Key) {
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
    (c.n, c.bytes, root)
}

pub fn cases(env: &Env) -> Vec<Case> {
    let fx = &env.fx;
    let mut out = Vec::new();

    // --- encoders: the payload-size sweep -------------------------------
    for i in 0..fx.payloads.len() {
        let lim = fx.payloads[i].limit(4 << 20);
        out.push(
            Case::new(
                "fstree",
                "fstree.encode_blob",
                &lim.name,
                1,
                lim.items.len(),
                Box::new(move |env| Box::new(env.fx.payloads[i].limit(4 << 20)) as State),
                Box::new(|_, s| {
                    let ps = s.downcast_ref::<PayloadSet>().unwrap();
                    let mut acc = new_fold();
                    for b in &ps.items {
                        let o = fstree::encode_blob(b);
                        // The whole content key, so the hash is observed, and
                        // the encoded object's ends, without a second pass.
                        acc = fold_key(acc, &o.key);
                        acc = sink_bytes(acc, &o.bytes);
                    }
                    acc
                }),
            )
            .bytes(lim.bytes)
            .dims(lim.dims())
            .cross(),
        );
    }

    // --- node codecs: the entry-count sweep -----------------------------
    // Every point differs from its neighbours in one number only, so the
    // per-entry cost of an encode or a decode can be read off the row.
    for (i, es) in fx.entry_sets.iter().enumerate() {
        let dims = Dims {
            entries: es.entries.len() as i64,
            items: REPS as i64,
            item_bytes: es.enc.len() as i64,
            content: es.content.clone(),
            shape: "leaf".into(),
            ..Default::default()
        };
        out.push(
            Case::new(
                "fstree",
                "fstree.encode_dir_leaf",
                &es.name,
                1,
                REPS * es.entries.len(),
                Box::new(move |_| Box::new(i) as State),
                Box::new(|env, s| {
                    let es = &env.fx.entry_sets[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..REPS {
                        let o = fstree::encode_dir_leaf(&es.entries).unwrap();
                        acc = fold_key(acc, &o.key);
                        acc = sink_bytes(acc, &o.bytes);
                    }
                    acc
                }),
            )
            .bytes_of((REPS * es.enc.len()) as i64, bytes_kind::ENCODED)
            .dims(dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.decode_dir_leaf",
                &es.name,
                1,
                REPS * es.entries.len(),
                Box::new(move |_| Box::new(i) as State),
                Box::new(|env, s| {
                    let es = &env.fx.entry_sets[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..REPS {
                        let ents = fstree::decode_dir_leaf(&es.enc).unwrap();
                        acc = fold_u64(acc, ents.len() as u64);
                        for e in &ents {
                            // Each decoded entry's fields: names and keys are
                            // short, so this stays proportional to the entry
                            // count rather than to the payload.
                            acc = sink_bytes(acc, &e.name);
                            acc = sink_bytes(acc, &e.content_key);
                            acc = fold_u64(acc, e.mode);
                            acc = fold_i64(acc, e.mtime);
                        }
                    }
                    acc
                }),
            )
            .bytes_of((REPS * es.enc.len()) as i64, bytes_kind::ENCODED)
            .dims(dims)
            .cross(),
        );
    }
    for (i, ps) in fx.pair_sets.iter().enumerate() {
        let dims = Dims {
            entries: ps.pairs.len() as i64,
            items: REPS as i64,
            item_bytes: ps.enc.len() as i64,
            content: "structured".into(),
            shape: "index".into(),
            ..Default::default()
        };
        out.push(
            Case::new(
                "fstree",
                "fstree.encode_dir_node",
                &ps.name,
                1,
                REPS * ps.pairs.len(),
                Box::new(move |_| Box::new(i) as State),
                Box::new(|env, s| {
                    let ps = &env.fx.pair_sets[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..REPS {
                        let o = fstree::encode_dir_node(&ps.pairs).unwrap();
                        acc = fold_key(acc, &o.key);
                        acc = sink_bytes(acc, &o.bytes);
                    }
                    acc
                }),
            )
            .bytes_of((REPS * ps.enc.len()) as i64, bytes_kind::ENCODED)
            .dims(dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.decode_dir_node",
                &ps.name,
                1,
                REPS * ps.pairs.len(),
                Box::new(move |_| Box::new(i) as State),
                Box::new(|env, s| {
                    let ps = &env.fx.pair_sets[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..REPS {
                        let prs = fstree::decode_dir_node(&ps.enc).unwrap();
                        acc = fold_u64(acc, prs.len() as u64);
                        for pr in &prs {
                            acc = sink_bytes(acc, &pr.sep_name);
                            acc = sink_bytes(acc, &pr.child_key);
                        }
                    }
                    acc
                }),
            )
            .bytes_of((REPS * ps.enc.len()) as i64, bytes_kind::ENCODED)
            .dims(dims)
            .cross(),
        );
    }
    for (i, cs) in fx.child_sets.iter().enumerate() {
        let dims = Dims {
            entries: cs.keys.len() as i64,
            items: REPS as i64,
            item_bytes: cs.enc.len() as i64,
            content: "structured".into(),
            shape: "file-index".into(),
            ..Default::default()
        };
        out.push(
            Case::new(
                "fstree",
                "fstree.encode_file_node",
                &cs.name,
                1,
                REPS * cs.keys.len(),
                Box::new(move |_| Box::new(i) as State),
                Box::new(|env, s| {
                    let cs = &env.fx.child_sets[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..REPS {
                        let o = fstree::encode_file_node(&cs.keys);
                        acc = fold_key(acc, &o.key);
                        acc = sink_bytes(acc, &o.bytes);
                    }
                    acc
                }),
            )
            .bytes_of((REPS * cs.enc.len()) as i64, bytes_kind::ENCODED)
            .dims(dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.decode_file_node",
                &cs.name,
                1,
                REPS * cs.keys.len(),
                Box::new(move |_| Box::new(i) as State),
                Box::new(|env, s| {
                    let cs = &env.fx.child_sets[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..REPS {
                        let ks = fstree::decode_file_node(&cs.enc).unwrap();
                        acc = fold_u64(acc, ks.len() as u64);
                        for k in &ks {
                            acc = fold_key(acc, k);
                        }
                    }
                    acc
                }),
            )
            .bytes_of((REPS * cs.enc.len()) as i64, bytes_kind::ENCODED)
            .dims(dims)
            .cross(),
        );
    }
    out.push(
        Case::new(
            "fstree",
            "fstree.encode_xattr_set",
            "xattrs-64",
            1,
            REPS * fx.xattrs_large.len(),
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut acc = new_fold();
                for _ in 0..REPS {
                    let o = fstree::encode_xattr_set(&env.fx.xattrs_large);
                    acc = fold_key(acc, &o.key);
                    acc = sink_bytes(acc, &o.bytes);
                }
                acc
            }),
        )
        .dims(Dims {
            entries: fx.xattrs_large.len() as i64,
            items: REPS as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
    );

    // --- child keys ---------------------------------------------------
    for (name, entries, which) in [
        ("dir-leaf-128", 128i64, 0usize),
        ("dir-node-128", 128, 1usize),
        ("file-node-1024", 1024, 2usize),
    ] {
        out.push(
            Case::new(
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
                    let mut acc = new_fold();
                    for _ in 0..REPS {
                        let ks = fstree::child_keys(*k, b).unwrap();
                        acc = fold_u64(acc, ks.len() as u64);
                        for kk in &ks {
                            acc = fold_key(acc, kk);
                        }
                    }
                    acc
                }),
            )
            .dims(Dims {
                entries,
                items: REPS as i64,
                content: "structured".into(),
                ..Default::default()
            })
            .cross(),
        );
    }

    // --- builders: the entry-count sweep --------------------------------
    for n in env.profile.tree_widths.iter().copied() {
        out.push(
            Case::new(
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
                    let (count, bytes, root) = build_dir_once(n, entries);
                    fold_key(fold_i64(fold_i64(new_fold(), count as i64), bytes), &root)
                }),
            )
            .dims(Dims {
                entries: n as i64,
                width: n as i64,
                depth: 1,
                shape: "wide".into(),
                content: "structured".into(),
                ..Default::default()
            })
            .cross(),
        );
    }
    for n in env.profile.fan_outs.iter().copied() {
        out.push(
            Case::new(
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
                    let (count, bytes, root) = build_index_once(keys);
                    fold_key(fold_i64(fold_i64(new_fold(), count as i64), bytes), &root)
                }),
            )
            .dims(Dims {
                entries: n as i64,
                width: n as i64,
                shape: "file-index".into(),
                content: "structured".into(),
                ..Default::default()
            })
            .cross(),
        );
    }

    // --- read paths: the directory-width sweep --------------------------
    let lookups = env.profile.batch_ops;
    for (di, d) in fx.dirs.iter().enumerate() {
        let wdims = Dims {
            entries: d.entries as i64,
            width: d.entries as i64,
            depth: 1,
            objects: d.objects,
            shape: "wide".into(),
            content: "structured".into(),
            ..Default::default()
        };
        out.push(
            Case::new(
                "fstree",
                "fstree.lookup_entry",
                &format!("width-{}/hit", d.entries),
                1,
                lookups,
                Box::new(move |_| Box::new(di) as State),
                Box::new(move |env, s| {
                    let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for i in 0..lookups {
                        let e: Entry = fstree::lookup_entry(
                            d.root,
                            &d.names[(i * 7919) % d.names.len()],
                            |k| env.fx.mem.get(k),
                        )
                        .unwrap();
                        acc = sink_bytes(acc, &e.name);
                        acc = sink_bytes(acc, &e.content_key);
                    }
                    acc
                }),
            )
            .dims(wdims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.lookup_entry",
                &format!("width-{}/miss", d.entries),
                1,
                lookups,
                Box::new(move |_| Box::new(di) as State),
                Box::new(move |env, s| {
                    let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..lookups {
                        let r: Result<Entry, _> =
                            fstree::lookup_entry(d.root, &d.miss_name, |k| env.fx.mem.get(k));
                        acc = fold_bool(acc, matches!(&r, Err(e) if e.is_not_found()));
                    }
                    acc
                }),
            )
            .dims(wdims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.collect_entries",
                &format!("width-{}", d.entries),
                1,
                d.entries,
                Box::new(move |_| Box::new(di) as State),
                Box::new(|env, s| {
                    let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                    let es: Vec<Entry> =
                        fstree::collect_entries(d.root, |k| env.fx.mem.get(k)).unwrap();
                    let mut acc = fold_u64(new_fold(), es.len() as u64);
                    for e in &es {
                        acc = sink_bytes(acc, &e.name);
                        acc = sink_bytes(acc, &e.content_key);
                    }
                    acc
                }),
            )
            .dims(wdims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.list_entries",
                &format!("width-{}/full-paging-100", d.entries),
                1,
                d.entries,
                Box::new(move |_| Box::new(di) as State),
                Box::new(|env, s| {
                    let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    let mut after: Vec<u8> = Vec::new();
                    loop {
                        let (es, more): (Vec<Entry>, bool) =
                            fstree::list_entries(d.root, &after, 100, |k| env.fx.mem.get(k))
                                .unwrap();
                        acc = fold_u64(acc, es.len() as u64);
                        acc = fold_bool(acc, more);
                        if !more || es.is_empty() {
                            break;
                        }
                        after = es.last().unwrap().name.clone();
                    }
                    acc
                }),
            )
            .dims(wdims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.reachable_keys",
                &format!("width-{}", d.entries),
                0,
                d.objects as usize,
                Box::new(move |_| Box::new(di) as State),
                Box::new(|env, s| {
                    let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                    let ks: Vec<Key> =
                        fstree::reachable_keys(d.root, |k| env.fx.mem.get(k)).unwrap();
                    fold_u64(new_fold(), ks.len() as u64)
                }),
            )
            .dims(wdims.clone())
            .cross(),
        );
    }
    // The first page alone, at the widest directory.
    let widest_i = fx.dirs.len() - 1;
    let widest_entries = fx.dirs[widest_i].entries;
    out.push(
        Case::new(
            "fstree",
            "fstree.list_entries",
            "widest/first-page-100",
            1,
            64 * 100,
            Box::new(move |_| Box::new(widest_i) as State),
            Box::new(|env, s| {
                let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                let mut acc = new_fold();
                for _ in 0..64 {
                    let (es, more): (Vec<Entry>, bool) =
                        fstree::list_entries(d.root, &[], 100, |k| env.fx.mem.get(k)).unwrap();
                    acc = fold_u64(acc, es.len() as u64);
                    acc = fold_bool(acc, more);
                }
                acc
            }),
        )
        .dims(Dims {
            entries: widest_entries as i64,
            width: widest_entries as i64,
            depth: 1,
            items: 64,
            shape: "wide".into(),
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
    );

    // --- read paths: the depth sweep ------------------------------------
    for (ci, ch) in fx.chains.iter().enumerate() {
        let ddims = Dims {
            depth: ch.depth as i64,
            width: fx.dirs[0].entries as i64,
            items: 256,
            shape: "deep".into(),
            content: "structured".into(),
            ..Default::default()
        };
        out.push(
            Case::new(
                "fstree",
                "fstree.resolve_path",
                &format!("depth-{}", ch.depth),
                1,
                256,
                Box::new(move |_| Box::new(ci) as State),
                Box::new(|env, s| {
                    let ch = &env.fx.chains[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for _ in 0..256 {
                        let k: Key =
                            fstree::resolve_path(ch.root, &ch.path, |k| env.fx.mem.get(k)).unwrap();
                        acc = fold_key(acc, &k);
                    }
                    acc
                }),
            )
            .dims(ddims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "fstree",
                "fstree.resolve_entry",
                &format!("depth-{}", ch.depth),
                1,
                256,
                Box::new(move |_| Box::new(ci) as State),
                Box::new(|env, s| {
                    let ch = &env.fx.chains[*s.downcast_ref::<usize>().unwrap()];
                    let path = format!("{}/entry-00000000", ch.path);
                    let mut acc = new_fold();
                    for _ in 0..256 {
                        let e: Option<Entry> =
                            fstree::resolve_entry(ch.root, &path, |k| env.fx.mem.get(k)).unwrap();
                        let e = e.unwrap();
                        acc = sink_bytes(acc, &e.name);
                        acc = sink_bytes(acc, &e.content_key);
                    }
                    acc
                }),
            )
            .dims(ddims)
            .cross(),
        );
    }

    out.push(
        Case::new(
            "fstree",
            "fstree.write_content",
            "file-corpus",
            1,
            fx.file_chunks as usize,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut c = CountingWriter::default();
                fstree::write_content(&mut c, env.fx.file_root, |k| env.fx.mem.get(k)).unwrap();
                fold_i64(new_fold(), c.n)
            }),
        )
        .bytes(fx.file_bytes)
        .dims(Dims {
            item_bytes: fx.file_bytes,
            entries: fx.file_chunks,
            shape: "file-index".into(),
            content: "text".into(),
            ..Default::default()
        })
        .cross(),
    );
    // reachable_keys and check_complete choose their own parallelism
    // (available_parallelism here, GOMAXPROCS in Go). Threads 0 marks that;
    // the driver runs under a fixed CPU set so both cores see the same bound,
    // and the effective width is recorded in the document's environment block.
    out.push(
        Case::new(
            "fstree",
            "fstree.reachable_keys",
            "deep",
            0,
            fx.deep_depth,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let ks: Vec<Key> =
                    fstree::reachable_keys(env.fx.deep_root, |k| env.fx.mem.get(k)).unwrap();
                fold_u64(new_fold(), ks.len() as u64)
            }),
        )
        .dims(Dims {
            depth: fx.deep_depth as i64,
            shape: "deep".into(),
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
    );
    out.push(
        Case::new(
            "fstree",
            "fstree.reachable_keys",
            "file-corpus",
            0,
            fx.file_chunks as usize,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let ks: Vec<Key> =
                    fstree::reachable_keys(env.fx.file_root, |k| env.fx.mem.get(k)).unwrap();
                fold_u64(new_fold(), ks.len() as u64)
            }),
        )
        .dims(Dims {
            entries: fx.file_chunks,
            shape: "file-index".into(),
            content: "text".into(),
            ..Default::default()
        })
        .cross(),
    );

    // Worker scaling, at the widest directory: the same walk asked for one
    // worker and for the profile's concurrent count, so the row pair is a
    // scaling measurement rather than two unrelated numbers.
    for jobs in [env.profile.threads_single, env.profile.threads_multi] {
        out.push(
            Case::new(
                "fstree",
                "fstree.check_complete",
                &format!("widest/jobs-{jobs}"),
                jobs,
                fx.dirs[widest_i].objects as usize,
                Box::new(move |_| Box::new(widest_i) as State),
                Box::new(move |env, s| {
                    let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                    let ks: Vec<Key> = fstree::check_complete(
                        d.root,
                        |k| env.fx.mem.get(k),
                        |k| env.fx.mem.has(k),
                        jobs,
                    )
                    .unwrap();
                    fold_u64(new_fold(), ks.len() as u64)
                }),
            )
            .dims(Dims {
                entries: widest_entries as i64,
                width: widest_entries as i64,
                objects: fx.dirs[widest_i].objects,
                shape: "wide".into(),
                content: "structured".into(),
                ..Default::default()
            })
            .cross(),
        );
    }
    // The same walk across the width sweep, at one worker.
    for (di, d) in fx.dirs.iter().enumerate() {
        out.push(
            Case::new(
                "fstree",
                "fstree.check_complete",
                &format!("width-{}/jobs-1", d.entries),
                1,
                d.objects as usize,
                Box::new(move |_| Box::new(di) as State),
                Box::new(|env, s| {
                    let d = &env.fx.dirs[*s.downcast_ref::<usize>().unwrap()];
                    let ks: Vec<Key> = fstree::check_complete(
                        d.root,
                        |k| env.fx.mem.get(k),
                        |k| env.fx.mem.has(k),
                        1,
                    )
                    .unwrap();
                    fold_u64(new_fold(), ks.len() as u64)
                }),
            )
            .dims(Dims {
                entries: d.entries as i64,
                width: d.entries as i64,
                objects: d.objects,
                shape: "wide".into(),
                content: "structured".into(),
                ..Default::default()
            })
            .cross(),
        );
    }
    out.push(
        Case::new(
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
                    // The key the walk found missing is the output; folding
                    // it whole is what makes this a measurement of the walk.
                    Err(e) if e.missing_object().is_some() => {
                        fold_key(new_fold(), &e.missing_object().unwrap().key)
                    }
                    other => panic!("expected a missing-object error, got {other:?}"),
                }
            }),
        )
        .dims(Dims {
            entries: fx.dirs[0].entries as i64,
            shape: "wide".into(),
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
    );
    out
}

#[derive(Default)]
pub struct CountingWriter {
    pub n: i64,
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
