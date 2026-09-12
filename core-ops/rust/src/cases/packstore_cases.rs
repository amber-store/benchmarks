//! packstore: opens, reads, writes, indexes and destructive maintenance.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use amber_store_core::amberpack;
use amber_store_core::fstree::Object;
use amber_store_core::key::{Key, Type};
use amber_store_core::packstore::{self, CompactOpts, WriteOpts};

use crate::env::Env;
use crate::fixtures::{
    PayloadSet, RecordLoc, digest_strings, fold_bool, fold_i64, fold_key, fold_u64, new_fold,
    payload_named, sink_bytes, writers_label,
};
use crate::fixtures_build::{object_seq, store_objects, store_options};
use crate::harness::{Case, Dims, Recorder, State};
use crate::stores::{
    StoreHandle, copied_dir, copied_store, dir_bytes, fresh_store, fresh_store_sync, open_store,
    work_dir,
};

/// A store a case opens inside the measured interval, so the close can happen
/// outside it.
struct OpenState {
    dir: PathBuf,
    st: Option<Arc<packstore::Store>>,
}

fn free_open_state(_: &Env, s: State) {
    let mut st = s.downcast::<OpenState>().unwrap();
    if let Some(s) = st.st.take() {
        let _ = s.close();
    }
    let _ = fs::remove_dir_all(&st.dir);
}

fn free_store_handle(_: &Env, s: State) {
    s.downcast::<StoreHandle>().unwrap().close();
}

/// Encodes a payload set as store objects, outside every measured interval.
fn blobs_of(ps: &PayloadSet) -> Vec<Object> {
    ps.items
        .iter()
        .map(|b| amber_store_core::fstree::encode_blob(b))
        .collect()
}

fn key_set(ks: &[Key]) -> HashSet<Key> {
    ks.iter().copied().collect()
}

pub fn cases(env: &Env) -> Vec<Case> {
    let p = &env.profile;
    let fx = &env.fx;
    let mut out = Vec::new();

    let lookups = p.batch_ops;
    let hit_keys: Vec<Key> = (0..lookups)
        .map(|i| fx.store_keys[(i * 7919) % fx.store_keys.len()])
        .collect();
    let miss_keys: Vec<Key> = (0..lookups)
        .map(|i| fx.store_miss_keys[i % fx.store_miss_keys.len()])
        .collect();
    let mixed: Vec<Key> = (0..lookups)
        .map(|i| {
            if i % 2 == 0 {
                hit_keys[i]
            } else {
                miss_keys[i]
            }
        })
        .collect();

    // --- open ---------------------------------------------------------
    out.push(
        Case::new(
            "packstore",
            "packstore.open",
            "empty",
            1,
            1,
            Box::new(|env| {
                Box::new(OpenState {
                    dir: work_dir(env, "ps-open-empty"),
                    st: None,
                }) as State
            }),
            Box::new(|env, s| {
                let st = s.downcast_mut::<OpenState>().unwrap();
                let store =
                    packstore::Store::open_with(&st.dir, store_options(&env.profile)).unwrap();
                let n = store.segments().unwrap().len() as i64;
                st.st = Some(Arc::new(store));
                fold_i64(new_fold(), n)
            }),
        )
        .dims(Dims {
            objects: 0,
            items: 1,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_open_state)),
    );
    out.push(
        Case::new(
            "packstore",
            "packstore.open",
            "populated-reopen",
            1,
            1,
            Box::new(|env| {
                Box::new(OpenState {
                    dir: copied_dir(env, &env.fx.store_template, "ps-open-full"),
                    st: None,
                }) as State
            }),
            Box::new(|env, s| {
                let st = s.downcast_mut::<OpenState>().unwrap();
                let store =
                    packstore::Store::open_with(&st.dir, store_options(&env.profile)).unwrap();
                let segs = store.segments().unwrap();
                let mut acc = fold_i64(new_fold(), segs.len() as i64);
                for sg in &segs {
                    acc = fold_u64(acc, sg.id);
                }
                st.st = Some(Arc::new(store));
                acc
            }),
        )
        .dims(Dims {
            objects: fx.store_keys.len() as i64,
            items: 1,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_open_state)),
    );
    out.push(
        Case::new(
            "packstore",
            "packstore.close",
            "populated",
            1,
            1,
            Box::new(|env| {
                let dir = copied_dir(env, &env.fx.store_template, "ps-close");
                let store = packstore::Store::open_with(&dir, store_options(&env.profile)).unwrap();
                Box::new(OpenState {
                    dir,
                    st: Some(Arc::new(store)),
                }) as State
            }),
            Box::new(|_, s| {
                let st = s.downcast_mut::<OpenState>().unwrap();
                st.st.take().unwrap().close().unwrap();
                fold_bool(new_fold(), true)
            }),
        )
        .dims(Dims {
            objects: fx.store_keys.len() as i64,
            items: 1,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_open_state)),
    );

    // --- reads --------------------------------------------------------
    // Every read case runs against the same long-lived open copy of the
    // store fixture, so the measured interval is the lookup and not an mmap.
    // `objects` is the store's object count; `items` is how many calls one
    // measured interval makes.
    let store_objects_n = fx.store_keys.len() as i64;
    macro_rules! read_case {
        ($op:expr, $wl:expr, $ops:expr, $body:expr) => {
            out.push(
                Case::new(
                    "packstore",
                    $op,
                    $wl,
                    1,
                    $ops,
                    Box::new(|_| Box::new(()) as State),
                    Box::new($body),
                )
                .dims(Dims {
                    items: $ops as i64,
                    objects: store_objects_n,
                    content: "structured".into(),
                    ..Default::default()
                }),
            )
        };
    }

    let hk = hit_keys.clone();
    read_case!(
        "packstore.get",
        "hit",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for k in &hk {
                // The returned buffer is consumed at its ends: a get that
                // handed back an empty or uninitialised slice could not
                // reproduce this number.
                acc = sink_bytes(acc, &st.get(*k).unwrap());
            }
            acc
        }
    );
    let mk = miss_keys.clone();
    read_case!(
        "packstore.get",
        "miss",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for k in &mk {
                acc = fold_bool(acc, matches!(st.get(*k), Err(e) if e.is_not_found()));
            }
            acc
        }
    );
    let hk = hit_keys.clone();
    read_case!(
        "packstore.get_record",
        "hit",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for k in &hk {
                acc = sink_bytes(acc, &st.get_record(*k).unwrap());
            }
            acc
        }
    );
    let hk = hit_keys.clone();
    read_case!(
        "packstore.has",
        "hit",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for k in &hk {
                acc = fold_bool(acc, st.has(*k).unwrap());
            }
            acc
        }
    );
    let mk = miss_keys.clone();
    read_case!(
        "packstore.has",
        "miss",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for k in &mk {
                acc = fold_bool(acc, st.has(*k).unwrap());
            }
            acc
        }
    );
    let hk = hit_keys.clone();
    read_case!(
        "packstore.stored_size",
        "hit",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for k in &hk {
                let n = st.stored_size(*k).unwrap();
                acc = fold_bool(acc, n.is_some());
                acc = fold_u64(acc, n.unwrap_or(0));
            }
            acc
        }
    );
    let mx = mixed.clone();
    read_case!(
        "packstore.missing",
        "half-present",
        mixed.len(),
        move |env: &Env, _: &mut State| {
            let out = env.fx.ro.as_ref().unwrap().missing(&mx).unwrap();
            let mut acc = fold_u64(new_fold(), out.len() as u64);
            // The whole returned set, in order: the answer is which keys are
            // missing, not how many.
            for k in &out {
                acc = fold_key(acc, k);
            }
            acc
        }
    );
    let mx = mixed.clone();
    read_case!(
        "packstore.sort_by_location",
        "scattered",
        mixed.len(),
        move |env: &Env, _: &mut State| {
            let mut ks = mx.clone();
            env.fx.ro.as_ref().unwrap().sort_by_location(&mut ks);
            // The permutation is the output, so the whole reordered slice is
            // folded rather than its first byte.
            let mut acc = new_fold();
            for k in &ks {
                acc = fold_key(acc, k);
            }
            acc
        }
    );
    read_case!(
        "packstore.segments",
        "list",
        64,
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for _ in 0..64 {
                let segs = st.segments().unwrap();
                acc = fold_u64(acc, segs.len() as u64);
                for sg in &segs {
                    acc = fold_u64(acc, sg.id);
                }
            }
            acc
        }
    );
    // Every sealed segment's index, not just the first: which segment a
    // given object lands in follows the compressed record sizes, which the
    // two cores' encoders do not produce identically, so a single-segment
    // walk would cover a different number of records on each side. Over the
    // whole store the count is the object count, in both cores.
    read_case!(
        "packstore.scan_index",
        "all-segments",
        fx.store_keys.len(),
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let segs = st.segments().unwrap();
            let mut acc = new_fold();
            let mut n = 0u64;
            for sg in &segs {
                st.scan_index(sg.id, |k, _off, _slen| {
                    // Offsets and segment ids are per-core facts (the records
                    // are packed differently), so the fold covers the key set
                    // and the record count, which are not.
                    acc = fold_key(acc, &k);
                    n += 1;
                })
                .unwrap();
            }
            fold_u64(acc, n)
        }
    );
    let nloc = lookups.min(4096);
    read_case!(
        "packstore.record",
        "by-location",
        nloc,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let locs: &Vec<RecordLoc> = &env.fx.ro_locs;
            let mut acc = new_fold();
            for i in 0..nloc {
                let l = locs[(i * 7919) % locs.len()];
                acc = sink_bytes(acc, &st.record(l.id, l.off).unwrap());
            }
            acc
        }
    );
    let hk = hit_keys.clone();
    read_case!(
        "packstore.has_outside",
        "sealed-segment",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for k in &hk {
                acc = fold_bool(acc, st.has_outside(env.fx.ro_seg_id, *k).unwrap());
            }
            acc
        }
    );
    read_case!(
        "packstore.oldest_inflight_write",
        "idle",
        4096,
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for _ in 0..4096 {
                acc = fold_bool(acc, st.oldest_inflight_write().is_some());
            }
            acc
        }
    );
    read_case!(
        "packstore.new_mark_set",
        "snapshot",
        16,
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = new_fold();
            for _ in 0..16 {
                let ms = st.new_mark_set();
                acc = fold_u64(acc, ms.marked() as u64);
                std::hint::black_box(&ms);
            }
            acc
        }
    );
    read_case!(
        "packstore.mark_set_mark",
        "all-keys",
        fx.store_keys.len(),
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut ms = st.new_mark_set();
            let mut acc = new_fold();
            for k in &env.fx.store_keys {
                let (newly, present) = ms.mark(*k);
                acc = fold_bool(acc, newly);
                acc = fold_bool(acc, present);
            }
            fold_u64(acc, ms.marked() as u64)
        }
    );
    read_case!(
        "packstore.mark_set_contains",
        "all-keys",
        fx.store_keys.len(),
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut ms = st.new_mark_set();
            for k in &env.fx.store_keys {
                ms.mark(*k);
            }
            let mut acc = new_fold();
            for k in &env.fx.store_keys {
                acc = fold_bool(acc, ms.contains(*k));
            }
            acc
        }
    );
    read_case!(
        "packstore.liveness",
        "tenth-live",
        1,
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let live = key_set(&env.fx.garbage_live);
            let ls = st.liveness(|k| live.contains(&k)).unwrap();
            let mut acc = fold_u64(new_fold(), ls.len() as u64);
            for l in &ls {
                acc = fold_i64(acc, l.live_keys as i64);
                acc = fold_i64(acc, l.dead_keys as i64);
            }
            acc
        }
    );
    read_case!(
        "packstore.verify",
        "full-scrub",
        fx.store_keys.len(),
        |env: &Env, _: &mut State| {
            let r = env.fx.ro.as_ref().unwrap().verify(|| false);
            fold_bool(new_fold(), r.is_ok())
        }
    );
    // Begin, observe the whole key set, abort. The observable result of the
    // capture is what a following compact retains, which the
    // packstore/barrier-* checks assert; here the cost of capturing is what
    // is measured, and the answer folded is whether the store really was
    // capturing at the time.
    read_case!(
        "packstore.barrier",
        "begin+observe+abort",
        fx.store_keys.len(),
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            st.begin_barrier();
            st.observe_keys(&env.fx.store_keys);
            let inflight = st.oldest_inflight_write().is_some();
            st.abort_barrier();
            fold_bool(
                fold_i64(new_fold(), env.fx.store_keys.len() as i64),
                inflight,
            )
        }
    );

    // --- writes -------------------------------------------------------
    let write_objs: Arc<Vec<Object>> = Arc::new(store_objects(p, p.store_objects.min(8000), 50000));

    // packstore::put is swept over the object-size and content grid: the
    // size decides how many segments a batch fills, and the content decides
    // whether the record compresses and whether the store sees the object at
    // all. Each point is capped to a bounded number of stored bytes, so the
    // sweep costs about the same at every size.
    let put_cap = p.payload_total / 8;
    for ps in fx.payloads.iter() {
        let lim = ps.limit(put_cap);
        let objs: Arc<Vec<Object>> = Arc::new(blobs_of(&lim));
        let n = objs.len();
        let b = crate::cases::pack::pack_bytes(&objs);
        let o2 = Arc::clone(&objs);
        out.push(
            Case::new(
                "packstore",
                "packstore.put",
                &format!("{}/sync-off", lim.name),
                1,
                n,
                Box::new(move |env| Box::new(fresh_store(env, "ps-put")) as State),
                Box::new(move |_, s| {
                    let h = s.downcast_ref::<StoreHandle>().unwrap();
                    let mut acc = new_fold();
                    for o in o2.iter() {
                        h.store().put(o.key, &o.bytes).unwrap();
                        // put's only result is whether it succeeded; the
                        // object it stored is asserted by the packstore/put-*
                        // checks, outside every measured interval.
                        acc = fold_bool(acc, true);
                    }
                    acc
                }),
            )
            .bytes(b)
            .dims(lim.dims())
            .per_rep(Box::new(free_store_handle)),
        );
    }
    // The same objects written a second time into a store that already holds
    // them: the pure dedup path, at one representative size.
    let dup_set = payload_named(&fx.payloads, "4KiB-random").limit(put_cap);
    let dup_objs: Arc<Vec<Object>> = Arc::new(blobs_of(&dup_set));
    let dup_bytes = crate::cases::pack::pack_bytes(&dup_objs);
    let d1 = Arc::clone(&dup_objs);
    let d2 = Arc::clone(&dup_objs);
    out.push(
        Case::new(
            "packstore",
            "packstore.put",
            "4KiB-random/already-present",
            1,
            dup_objs.len(),
            Box::new(move |env| {
                let h = fresh_store(env, "ps-put-dup");
                for o in d1.iter() {
                    h.store().put(o.key, &o.bytes).unwrap();
                }
                Box::new(h) as State
            }),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let mut acc = new_fold();
                for o in d2.iter() {
                    h.store().put(o.key, &o.bytes).unwrap();
                    acc = fold_bool(acc, true);
                }
                acc
            }),
        )
        .bytes(dup_bytes)
        .dims(dup_set.dims())
        .per_rep(Box::new(free_store_handle)),
    );
    // The durability dimension, at the same representative size: every
    // append is fsynced rather than batched.
    let d3 = Arc::clone(&dup_objs);
    out.push(
        Case::new(
            "packstore",
            "packstore.put",
            "4KiB-random/sync-on",
            1,
            dup_objs.len(),
            Box::new(move |env| Box::new(fresh_store_sync(env, "ps-put-sync")) as State),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let mut acc = new_fold();
                for o in d3.iter() {
                    h.store().put(o.key, &o.bytes).unwrap();
                    acc = fold_bool(acc, true);
                }
                acc
            }),
        )
        .bytes(dup_bytes)
        .dims(dup_set.dims())
        .per_rep(Box::new(free_store_handle)),
    );

    let w1 = Arc::clone(&write_objs);
    out.push(
        Case::new(
            "packstore",
            "packstore.write_batch",
            "mixed-objects",
            1,
            write_objs.len(),
            Box::new(|env| Box::new(fresh_store(env, "ps-batch")) as State),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                h.store().write_batch(object_seq(&w1)).unwrap();
                let segs = h.store().segments().unwrap();
                fold_i64(new_fold(), segs.len() as i64)
            }),
        )
        .bytes(crate::cases::pack::pack_bytes(&write_objs))
        .dims(Dims {
            items: write_objs.len() as i64,
            objects: write_objs.len() as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_store_handle)),
    );

    out.push(
        Case::new(
            "packstore",
            "packstore.append_record",
            "pre-encoded+sync",
            1,
            fx.wire_records.len(),
            Box::new(|env| Box::new(fresh_store(env, "ps-append")) as State),
            Box::new(|env, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let mut acc = new_fold();
                for (i, rec) in env.fx.wire_records.iter().enumerate() {
                    h.store()
                        .append_record(env.fx.pack_objects[i].key, rec)
                        .unwrap();
                    acc = fold_bool(acc, true);
                }
                h.store().sync().unwrap();
                fold_bool(acc, true)
            }),
        )
        .bytes(crate::cases::pack::pack_bytes(&fx.pack_objects))
        .dims(Dims {
            items: fx.wire_records.len() as i64,
            objects: fx.pack_objects.len() as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_store_handle)),
    );

    for writers in [p.threads_single, p.threads_multi] {
        for verify in [false, true] {
            let objs = Arc::clone(&write_objs);
            out.push(
                Case::new(
                    "packstore",
                    "packstore.write_parallel",
                    &format!("mixed-objects/{}/verify-{verify}", writers_label(writers)),
                    writers,
                    write_objs.len(),
                    Box::new(|env| Box::new(fresh_store(env, "ps-parallel")) as State),
                    Box::new(move |_, s| {
                        let h = s.downcast_ref::<StoreHandle>().unwrap();
                        let (stats, res) = h.store().write_parallel(
                            object_seq(&objs),
                            WriteOpts {
                                writers,
                                verify,
                                ..Default::default()
                            },
                        );
                        res.unwrap();
                        fold_i64(
                            fold_i64(new_fold(), stats.stored as i64),
                            stats.deduped as i64,
                        )
                    }),
                )
                .bytes(crate::cases::pack::pack_bytes(&write_objs))
                .dims(Dims {
                    items: write_objs.len() as i64,
                    objects: write_objs.len() as i64,
                    content: "structured".into(),
                    ..Default::default()
                })
                .cross()
                .per_rep(Box::new(free_store_handle)),
            );
        }
    }
    let d1 = Arc::clone(&write_objs);
    let d2 = Arc::clone(&write_objs);
    let multi = p.threads_multi;
    out.push(
        Case::new(
            "packstore",
            "packstore.write_parallel",
            "duplicate-stream/writers-N",
            p.threads_multi,
            write_objs.len(),
            Box::new(move |env| {
                let h = fresh_store(env, "ps-parallel-dup");
                let (_, res) = h.store().write_parallel(
                    object_seq(&d1),
                    WriteOpts {
                        writers: multi,
                        ..Default::default()
                    },
                );
                res.unwrap();
                Box::new(h) as State
            }),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let (stats, res) = h.store().write_parallel(
                    object_seq(&d2),
                    WriteOpts {
                        writers: multi,
                        ..Default::default()
                    },
                );
                res.unwrap();
                fold_i64(
                    fold_i64(new_fold(), stats.stored as i64),
                    stats.deduped as i64,
                )
            }),
        )
        .bytes(crate::cases::pack::pack_bytes(&write_objs))
        .dims(Dims {
            items: write_objs.len() as i64,
            objects: write_objs.len() as i64,
            content: "duplicate".into(),
            ..Default::default()
        })
        .cross()
        .per_rep(Box::new(free_store_handle)),
    );

    // --- destructive maintenance -------------------------------------
    out.push(
        Case::new(
            "packstore",
            "packstore.compact",
            "90-percent-dead",
            1,
            fx.store_keys.len(),
            Box::new(|env| {
                Box::new(copied_store(env, &env.fx.garbage_template, "ps-compact")) as State
            }),
            Box::new(|env, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let live = key_set(&env.fx.garbage_live);
                let stats = h
                    .store()
                    .compact(
                        |k| live.contains(&k),
                        CompactOpts {
                            min_dead_ratio: 0.5,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                fold_u64(
                    fold_i64(
                        fold_i64(new_fold(), stats.records_copied as i64),
                        stats.segments_scanned as i64,
                    ),
                    stats.bytes_freed,
                )
            }),
        )
        .dims(Dims {
            objects: fx.store_keys.len() as i64,
            items: fx.garbage_live.len() as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_store_handle)),
    );
    out.push(
        Case::new(
            "packstore",
            "packstore.compact",
            "nothing-dead",
            1,
            fx.store_keys.len(),
            Box::new(|env| {
                Box::new(copied_store(
                    env,
                    &env.fx.garbage_template,
                    "ps-compact-live",
                )) as State
            }),
            Box::new(|_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let stats = h
                    .store()
                    .compact(
                        |_| true,
                        CompactOpts {
                            min_dead_ratio: 0.5,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                fold_u64(
                    fold_i64(
                        fold_i64(new_fold(), stats.records_copied as i64),
                        stats.segments_scanned as i64,
                    ),
                    stats.bytes_freed,
                )
            }),
        )
        .dims(Dims {
            objects: fx.store_keys.len() as i64,
            items: fx.store_keys.len() as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_store_handle)),
    );
    out.push(
        Case::new(
            "packstore",
            "packstore.remove",
            "one-sealed-segment",
            1,
            1,
            Box::new(|env| {
                Box::new(copied_store(env, &env.fx.store_template, "ps-remove")) as State
            }),
            Box::new(|_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let segs = h.store().segments().unwrap();
                h.store().remove(segs[0].id).unwrap();
                let after = h.store().segments().unwrap();
                fold_i64(fold_u64(new_fold(), segs[0].id), after.len() as i64)
            }),
        )
        .dims(Dims {
            objects: fx.store_keys.len() as i64,
            items: 1,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_store_handle)),
    );
    out.push(
        Case::new(
            "packstore",
            "packstore.wipe",
            "populated",
            1,
            1,
            Box::new(|env| Box::new(copied_store(env, &env.fx.store_template, "ps-wipe")) as State),
            Box::new(|_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                h.store().wipe().unwrap();
                let segs = h.store().segments().unwrap();
                fold_i64(new_fold(), segs.len() as i64)
            }),
        )
        .dims(Dims {
            objects: fx.store_keys.len() as i64,
            items: 1,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_store_handle)),
    );
    let pv_ops = (p.batch_ops / 8).min(256);
    out.push(
        Case::new(
            "packstore",
            "packstore.put_verified",
            "already-intact",
            1,
            pv_ops,
            Box::new(|env| {
                Box::new(copied_store(env, &env.fx.store_template, "ps-putverified")) as State
            }),
            Box::new(move |env, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let mut acc = new_fold();
                for i in 0..pv_ops {
                    let o = &env.fx.pack_objects[i % env.fx.pack_objects.len()];
                    h.store().put_verified(o.key, &o.bytes).unwrap();
                    acc = fold_bool(acc, true);
                }
                acc
            }),
        )
        .dims(Dims {
            items: pv_ops as i64,
            objects: fx.store_keys.len() as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_store_handle)),
    );
    // Physical segment layout can differ between independently built fixtures.
    // Semantic correctness is checked separately for each pass.
    for case in &mut out {
        if matches!(
            case.op.as_str(),
            "packstore.has_outside"
                | "packstore.liveness"
                | "packstore.record"
                | "packstore.scan_index"
                | "packstore.sort_by_location"
        ) {
            case.unstable = true;
        }
    }
    out
}

/// Locates one key's record inside a sealed segment.
fn find_record_offset(st: &packstore::Store, want: Key) -> RecordLoc {
    let mut found = RecordLoc::default();
    for s in st.segments().unwrap_or_default() {
        let _ = st.scan_index(s.id, |k, off, slen| {
            if k == want && found.len == 0 {
                found = RecordLoc {
                    id: s.id,
                    off,
                    len: slen,
                };
            }
        });
        if found.len != 0 {
            return found;
        }
    }
    found
}

/// Flips one payload byte of a record inside a sealed segment file. It is
/// only ever used on an isolated copy.
fn corrupt_segment_byte(dir: &std::path::Path, loc: RecordLoc) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let path = dir.join(format!("{:016x}.seg", loc.id));
    let mut f = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let at = loc.off + amberpack::REC_HEADER_SIZE as u64;
    f.seek(SeekFrom::Start(at))?;
    let mut b = [0u8; 1];
    f.read_exact(&mut b)?;
    b[0] ^= 0xFF;
    f.seek(SeekFrom::Start(at))?;
    f.write_all(&b)?;
    Ok(())
}

pub fn checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let p = &env.profile;
    let st = fx.ro.as_ref().unwrap();

    let mut ok = true;
    let mut digests: Vec<String> = Vec::new();
    for i in 0..256.min(fx.store_keys.len()) {
        let k = fx.store_keys[i * 7 % fx.store_keys.len()];
        match st.get(k) {
            Ok(b) => {
                if Key::new(Type::Blob, b.len() as u64, &b) != k {
                    ok = false;
                    break;
                }
                digests.push(k.to_string());
            }
            Err(_) => {
                ok = false;
                break;
            }
        }
    }
    rec.want(
        "packstore",
        "packstore.get",
        "packstore/get-content-addressed",
        ok,
        "a stored object did not re-hash to its key",
        digest_strings(&digests),
    );
    let miss = st.get(fx.store_miss_keys[0]);
    rec.want(
        "packstore",
        "packstore.get",
        "packstore/get-miss",
        matches!(&miss, Err(e) if e.is_not_found()),
        format!("an absent key must report not-found, got {miss:?}"),
        "ErrNotFound",
    );
    let has = st.has(fx.store_keys[0]);
    let has_not = st.has(fx.store_miss_keys[0]);
    rec.want(
        "packstore",
        "packstore.has",
        "packstore/has",
        matches!((&has, &has_not), (Ok(true), Ok(false))),
        format!("has disagrees with the store contents: {has:?}/{has_not:?}"),
        "true/false",
    );

    let recb = st.get_record(fx.store_keys[0]);
    match recb {
        Err(e) => rec.fail(
            "packstore",
            "packstore.get_record",
            "packstore/get-record",
            e.to_string(),
        ),
        Ok(raw) => {
            let h = amberpack::parse_record(&raw);
            let want = st.get(fx.store_keys[0]).unwrap();
            let body = h.as_ref().ok().and_then(|h| {
                amberpack::decode_payload(
                    h.flags,
                    h.ulen,
                    &raw[amberpack::REC_HEADER_SIZE..amberpack::REC_HEADER_SIZE + h.slen as usize],
                )
                .ok()
            });
            rec.want(
                "packstore",
                "packstore.get_record",
                "packstore/get-record",
                matches!(&h, Ok(h) if h.key == fx.store_keys[0])
                    && body.as_deref() == Some(&want[..]),
                format!("record round trip: {h:?}"),
                fx.store_keys[0].to_string(),
            );
        }
    }

    let size = st.stored_size(fx.store_keys[0]);
    let raw_len = st.get(fx.store_keys[0]).unwrap().len() as u64;
    rec.want(
        "packstore",
        "packstore.stored_size",
        "packstore/stored-size",
        matches!(&size, Ok(Some(n)) if *n > 0 && *n <= raw_len + 64),
        format!("stored size {size:?}"),
        "ok",
    );

    let probe = [
        fx.store_keys[0],
        fx.store_miss_keys[0],
        fx.store_keys[1],
        fx.store_miss_keys[0],
    ];
    let missing = st.missing(&probe);
    rec.want(
        "packstore",
        "packstore.missing",
        "packstore/missing",
        matches!(&missing, Ok(v) if v.len() == 2 && v[0] == fx.store_miss_keys[0] && v[1] == fx.store_miss_keys[0]),
        format!("missing returned {missing:?}"),
        missing.as_ref().map(|v| v.len().to_string()).unwrap_or_default(),
    );

    let mut ks: Vec<Key> = fx.store_keys[..1024.min(fx.store_keys.len())].to_vec();
    let before = key_set(&ks);
    st.sort_by_location(&mut ks);
    let perm = key_set(&ks) == before && ks.len() == before.len();
    rec.want(
        "packstore",
        "packstore.sort_by_location",
        "packstore/sort-is-permutation",
        perm,
        "sort_by_location changed the key multiset",
        format!("n={}", ks.len()),
    );

    let segs = st.segments().unwrap_or_default();
    if segs.is_empty() {
        rec.fail(
            "packstore",
            "packstore.segments",
            "packstore/segments",
            "no sealed segments",
        );
    } else {
        let mut count = 0u64;
        st.scan_index(segs[0].id, |_k, _off, _slen| count += 1)
            .unwrap();
        // The record population of a given sealed segment depends on the
        // compressed record sizes, which libzstd and klauspost/compress do
        // not produce identically; the count is a within-core anchor.
        rec.want_local(
            "packstore",
            "packstore.scan_index",
            "packstore/scan-index-matches-footer",
            count == segs[0].keys,
            format!(
                "the index walk yielded {count} entries, the footer claims {}",
                segs[0].keys
            ),
            count.to_string(),
        );
        let rec_ok = fx.ro_locs[..64.min(fx.ro_locs.len())].iter().all(|l| {
            st.record(l.id, l.off)
                .ok()
                .and_then(|raw| amberpack::parse_record(&raw).ok())
                .is_some()
        });
        rec.want_local(
            "packstore",
            "packstore.record",
            "packstore/record-by-location",
            rec_ok,
            "a record did not parse at its indexed offset",
            format!("n={}", fx.ro_locs.len()),
        );
        let outside = st.has_outside(segs[0].id, fx.store_miss_keys[0]);
        rec.want(
            "packstore",
            "packstore.has_outside",
            "packstore/has-outside-miss",
            matches!(&outside, Ok(false)),
            format!("has_outside on an absent key: {outside:?}"),
            "false",
        );
    }

    rec.want(
        "packstore",
        "packstore.verify",
        "packstore/verify-clean",
        st.verify(|| false).is_ok(),
        "the scrub reported corruption in an untouched fixture",
        "clean",
    );

    let mut ms = st.new_mark_set();
    let (newly, present) = ms.mark(fx.store_keys[0]);
    let (_, absent) = ms.mark(fx.store_miss_keys[0]);
    rec.want(
        "packstore",
        "packstore.mark_set_mark",
        "packstore/markset",
        newly && present && !absent && ms.contains(fx.store_keys[0]) && ms.marked() == 1,
        format!(
            "mark set: newly={newly} present={present} absentPresent={absent} marked={}",
            ms.marked()
        ),
        "ok",
    );

    let live = key_set(&fx.garbage_live);
    let ls = st.liveness(|k| live.contains(&k));
    let total: usize = ls
        .as_ref()
        .map(|v| v.iter().map(|l| l.live_keys + l.dead_keys).sum())
        .unwrap_or(0);
    rec.want(
        "packstore",
        "packstore.liveness",
        "packstore/liveness-accounts-all",
        total == fx.store_keys.len(),
        format!(
            "liveness counted {total} records, the store holds {}",
            fx.store_keys.len()
        ),
        total.to_string(),
    );

    // Compaction on an isolated copy.
    let dir = copied_dir(env, &fx.garbage_template, "check-compact");
    {
        let cst = packstore::Store::open_with(&dir, store_options(p)).unwrap();
        let before_bytes = dir_bytes(&dir);
        let stats = cst.compact(
            |k| live.contains(&k),
            CompactOpts {
                min_dead_ratio: 0.5,
                ..Default::default()
            },
        );
        let after_bytes = dir_bytes(&dir);
        // Retention is a statement about content, not about presence: every
        // object that was supposed to survive is read back and re-hashed, and
        // the store is content-addressed, so a key that matches its own
        // re-hash is a complete verification of the retained bytes.
        let mut retained = true;
        let mut retained_detail = String::new();
        for k in &fx.garbage_live {
            match cst.get(*k) {
                Ok(got) => {
                    let rehash = Key::new(Type::Blob, got.len() as u64, &got);
                    if rehash != *k {
                        retained = false;
                        retained_detail = format!("{k} re-hashed to {rehash}");
                        break;
                    }
                }
                Err(e) => {
                    retained = false;
                    retained_detail = format!("{k}: {e}");
                    break;
                }
            }
        }
        let dropped = fx
            .store_keys
            .iter()
            .enumerate()
            .filter(|(i, k)| i % 10 != 0 && !cst.has(**k).unwrap_or(true))
            .count();
        rec.want(
            "packstore",
            "packstore.compact",
            "packstore/compact-retains-live-content",
            stats.is_ok() && retained,
            format!(
                "a live object did not survive compaction intact ({stats:?}): {retained_detail}"
            ),
            format!("live={}", fx.garbage_live.len()),
        );
        // And everything that was eligible really was reclaimed: no object
        // the predicate called dead may still be readable from a compacted
        // segment.
        let survivors = fx
            .store_keys
            .iter()
            .enumerate()
            .filter(|(i, k)| i % 10 != 0 && cst.has(**k).unwrap_or(false))
            .count();
        rec.want_local(
            "packstore",
            "packstore.compact",
            "packstore/compact-drops-dead",
            dropped > 0 && dropped + survivors == fx.store_keys.len() - fx.garbage_live.len(),
            format!(
                "{dropped} dead objects dropped, {survivors} still present, {} were dead",
                fx.store_keys.len() - fx.garbage_live.len()
            ),
            format!("dropped={dropped} survivors={survivors}"),
        );
        let compacted = stats.as_ref().map(|s| s.segments_compacted).unwrap_or(0);
        rec.want_local(
            "packstore",
            "packstore.compact",
            "packstore/compact-reclaims",
            compacted > 0 && after_bytes < before_bytes && dropped > 0,
            format!(
                "compaction freed nothing: {compacted} segments, {before_bytes} -> {after_bytes} bytes, {dropped} objects dropped"
            ),
            format!("compacted={compacted} dropped={dropped}"),
        );
        rec.want(
            "packstore",
            "packstore.verify",
            "packstore/verify-after-compact",
            cst.verify(|| false).is_ok(),
            "the store did not scrub clean after compaction",
            "clean",
        );
        cst.close().unwrap();
    }
    let _ = fs::remove_dir_all(&dir);

    // put_verified repairs a deliberately corrupted copy.
    let repair_dir = copied_dir(env, &fx.store_template, "check-repair");
    let victim = fx.store_keys[0];
    let (want, loc) = {
        let rst = packstore::Store::open_with(&repair_dir, store_options(p)).unwrap();
        let want = rst.get(victim).unwrap();
        let loc = find_record_offset(&rst, victim);
        rst.close().unwrap();
        (want, loc)
    };
    if loc.len == 0 {
        rec.fail(
            "packstore",
            "packstore.put_verified",
            "packstore/repairs-corruption",
            "could not locate the victim record to corrupt",
        );
    } else {
        corrupt_segment_byte(&repair_dir, loc).unwrap();
        let rst = packstore::Store::open_with(&repair_dir, store_options(p)).unwrap();
        let scrub = rst.verify(|| false);
        let put = rst.put_verified(victim, &want);
        let after = rst.get(victim);
        rec.want(
            "packstore",
            "packstore.put_verified",
            "packstore/repairs-corruption",
            scrub.is_err() && put.is_ok() && matches!(&after, Ok(b) if *b == want),
            format!("repair did not restore the object: scrub={scrub:?} put={put:?} get={after:?}"),
            victim.to_string(),
        );
        rec.want(
            "packstore",
            "packstore.verify",
            "packstore/verify-detects-corruption",
            matches!(&scrub, Err(e) if e.is_corrupt()),
            format!("the scrub did not report the injected corruption: {scrub:?}"),
            "ErrCorrupt",
        );
        rst.close().unwrap();
    }
    let _ = fs::remove_dir_all(&repair_dir);

    // Wipe empties the store and leaves it usable.
    let wipe_dir = copied_dir(env, &fx.store_template, "check-wipe");
    {
        let wst = packstore::Store::open_with(&wipe_dir, store_options(p)).unwrap();
        wst.wipe().unwrap();
        let gone = wst.has(fx.store_keys[0]).unwrap();
        wst.put(fx.pack_objects[0].key, &fx.pack_objects[0].bytes)
            .unwrap();
        let back = wst.get(fx.pack_objects[0].key);
        rec.want(
            "packstore",
            "packstore.wipe",
            "packstore/wipe",
            !gone && matches!(&back, Ok(b) if *b == fx.pack_objects[0].bytes),
            format!("wipe left data behind ({gone}) or broke the store ({back:?})"),
            "empty+usable",
        );
        wst.close().unwrap();
    }
    let _ = fs::remove_dir_all(&wipe_dir);

    // Closed stores refuse work.
    let closed_dir = work_dir(env, "check-closed");
    {
        let h = open_store(env, closed_dir.clone(), false);
        let st2 = Arc::clone(h.st.as_ref().unwrap());
        st2.close().unwrap();
        let err = st2.get(fx.store_keys[0]);
        rec.want(
            "packstore",
            "packstore.close",
            "packstore/closed-store-errors",
            matches!(&err, Err(e) if e.is_closed()),
            format!("a closed store must report closed, got {err:?}"),
            "ErrClosed",
        );
    }
    let _ = fs::remove_dir_all(&closed_dir);

    // write_parallel with verification rejects a mislabelled object.
    let vdir = work_dir(env, "check-verify-write");
    {
        let vst = packstore::Store::open_with(&vdir, store_options(p)).unwrap();
        let wrong = vec![Object {
            key: fx.store_keys[0],
            bytes: b"not the object under this key".to_vec(),
        }];
        let (_, res) = vst.write_parallel(
            object_seq(&wrong),
            WriteOpts {
                writers: 1,
                verify: true,
                ..Default::default()
            },
        );
        rec.want(
            "packstore",
            "packstore.write_parallel",
            "packstore/verify-rejects-mismatch",
            matches!(&res, Err(e) if e.is_verify()),
            format!("a key/payload mismatch must report verify failure, got {res:?}"),
            "ErrVerify",
        );
        let dup = vec![
            fx.pack_objects[0].clone(),
            fx.pack_objects[0].clone(),
            fx.pack_objects[1].clone(),
        ];
        let (ws, res) = vst.write_parallel(
            object_seq(&dup),
            WriteOpts {
                writers: 1,
                ..Default::default()
            },
        );
        rec.want(
            "packstore",
            "packstore.write_parallel",
            "packstore/dedups-within-batch",
            res.is_ok() && ws.stored == 2 && ws.deduped == 1,
            format!("stored={} deduped={} ({res:?})", ws.stored, ws.deduped),
            format!("stored={} deduped={}", ws.stored, ws.deduped),
        );
        vst.close().unwrap();
    }
    let _ = fs::remove_dir_all(&vdir);

    // --- the operations whose timings had no evidence ----------------
    // A timing says a function ran. These say it did what it is named after,
    // which is what makes the timing a measurement of that operation rather
    // than of an unknown one.

    // write_batch: every object in the batch is in the store afterwards, and
    // reads back as itself.
    let bdir = work_dir(env, "check-write-batch");
    {
        let bst = packstore::Store::open_with(&bdir, store_options(p)).unwrap();
        let batch = store_objects(p, p.batch_ops.min(512), 70000);
        let berr = bst.write_batch(object_seq(&batch));
        let mut batch_ok = berr.is_ok();
        let mut batch_detail = format!("{berr:?}");
        for o in &batch {
            if !batch_ok {
                break;
            }
            match bst.get(o.key) {
                Ok(got) if got == o.bytes => {}
                other => {
                    batch_ok = false;
                    batch_detail = format!("{}: {other:?}", o.key);
                }
            }
        }
        rec.want(
            "packstore",
            "packstore.write_batch",
            "packstore/write-batch-stores-all",
            batch_ok,
            format!("an object in the batch is not readable afterwards: {batch_detail}"),
            crate::cases::pack::pack_content_digest(&batch),
        );

        // segments: the sealed segments describe the store. Which objects
        // land in which segment follows the compressed sizes, so the counts
        // are a within-core anchor; the shape of the list is not.
        let bsegs = bst.segments().unwrap();
        let shape_ok = bsegs
            .iter()
            .enumerate()
            .all(|(i, sg)| sg.keys > 0 && (i == 0 || sg.id > bsegs[i - 1].id));
        rec.want(
            "packstore",
            "packstore.segments",
            "packstore/segments-are-ordered-and-nonempty",
            shape_ok,
            "the sealed segment list is not strictly ordered or holds an empty segment",
            "ordered",
        );
        bst.close().unwrap();
    }
    let _ = fs::remove_dir_all(&bdir);

    let tsegs = st.segments().unwrap();
    let sealed_keys: u64 = tsegs.iter().map(|sg| sg.keys).sum();
    rec.want_local(
        "packstore",
        "packstore.segments",
        "packstore/segments-account-for-the-store",
        !tsegs.is_empty() && sealed_keys > 0 && sealed_keys <= fx.store_keys.len() as u64,
        format!(
            "{} sealed segments hold {sealed_keys} of the store's {} objects",
            tsegs.len(),
            fx.store_keys.len()
        ),
        format!("segments={} keys={sealed_keys}", tsegs.len()),
    );

    // oldest_inflight_write: an idle store has no write in flight.
    rec.want(
        "packstore",
        "packstore.oldest_inflight_write",
        "packstore/oldest-inflight-idle-is-none",
        st.oldest_inflight_write().is_none(),
        "an idle store reported a write in flight",
        "none",
    );

    // remove: exactly the removed segment's objects go, the rest stay, and
    // the store still scrubs clean.
    let rmdir = copied_dir(env, &fx.store_template, "check-remove");
    {
        let rmst = packstore::Store::open_with(&rmdir, store_options(p)).unwrap();
        let rmsegs = rmst.segments().unwrap();
        if rmsegs.len() < 2 {
            rec.fail(
                "packstore",
                "packstore.remove",
                "packstore/remove-drops-only-that-segment",
                format!(
                    "the fixture has {} sealed segments; the check needs two",
                    rmsegs.len()
                ),
            );
        } else {
            let mut victim: Vec<Key> = Vec::new();
            let mut survivor: Vec<Key> = Vec::new();
            rmst.scan_index(rmsegs[0].id, |k, _, _| victim.push(k))
                .unwrap();
            rmst.scan_index(rmsegs[1].id, |k, _, _| survivor.push(k))
                .unwrap();
            rmst.remove(rmsegs[0].id).unwrap();
            let gone = victim
                .iter()
                .filter(|k| !rmst.has(**k).unwrap_or(true))
                .count();
            let kept = survivor
                .iter()
                .filter(|k| rmst.has(**k).unwrap_or(false))
                .count();
            rec.want_local(
                "packstore",
                "packstore.remove",
                "packstore/remove-drops-only-that-segment",
                gone == victim.len() && kept == survivor.len() && rmst.verify(|| false).is_ok(),
                format!(
                    "{gone} of {} removed objects are gone, {kept} of {} others kept",
                    victim.len(),
                    survivor.len()
                ),
                format!("gone={gone} kept={kept}"),
            );
        }
        rmst.close().unwrap();
    }
    let _ = fs::remove_dir_all(&rmdir);

    // --- the write barrier -------------------------------------------
    // begin_barrier/observe_keys/abort_barrier had no correctness evidence at
    // all; a timing of a call that does nothing observable is not a
    // measurement of anything. The observable contract is that keys seen
    // during a capture are treated as live by the next compact even when the
    // liveness predicate calls them dead -- which is how an ingest may run
    // concurrently with a mark -- and that an aborted capture protects
    // nothing.
    for (id, abort, want_protected) in [
        (
            "packstore/barrier-observed-keys-survive-compact",
            false,
            true,
        ),
        ("packstore/barrier-abort-protects-nothing", true, false),
    ] {
        let bdir = copied_dir(env, &fx.garbage_template, "check-barrier");
        {
            let bst = packstore::Store::open_with(&bdir, store_options(p)).unwrap();
            // Pick objects the predicate below calls dead, and observe them.
            let observed: Vec<Key> = fx
                .store_keys
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 10 != 0)
                .map(|(_, k)| *k)
                .take(32)
                .collect();
            bst.begin_barrier();
            bst.observe_keys(&observed);
            if abort {
                bst.abort_barrier();
            }
            let live_only = key_set(&fx.garbage_live);
            let cres = bst.compact(
                |k| live_only.contains(&k),
                CompactOpts {
                    min_dead_ratio: 0.5,
                    ..Default::default()
                },
            );
            let protected = observed
                .iter()
                .filter(|k| bst.has(**k).unwrap_or(false))
                .count();
            let got = protected == observed.len();
            rec.want(
                "packstore",
                "packstore.barrier",
                id,
                cres.is_ok() && got == want_protected,
                format!(
                    "{protected} of {} observed keys survived the compaction (wanted all={want_protected}): {cres:?}",
                    observed.len()
                ),
                format!("observed={} protected={got}", observed.len()),
            );
            bst.close().unwrap();
        }
        let _ = fs::remove_dir_all(&bdir);
    }

    // --- append_record + sync ----------------------------------------
    // The other operation with no check: re-appending already-encoded
    // records. The contract is that the store afterwards holds exactly those
    // objects, byte for byte, and that they are still there after the sync
    // the batch ends with and a reopen.
    let adir = work_dir(env, "check-append");
    let mut aerr: Option<String> = None;
    {
        let ast = packstore::Store::open_with(&adir, store_options(p)).unwrap();
        for (i, r) in fx.wire_records.iter().enumerate() {
            if let Err(e) = ast.append_record(fx.pack_objects[i].key, r) {
                aerr = Some(e.to_string());
                break;
            }
        }
        if aerr.is_none()
            && let Err(e) = ast.sync()
        {
            aerr = Some(e.to_string());
        }
        ast.close().unwrap();
    }
    {
        // Reopened from disk, so the check covers the sync and not just the
        // in-memory state the appends left behind.
        let ast = packstore::Store::open_with(&adir, store_options(p)).unwrap();
        let mut append_ok = aerr.is_none();
        let mut append_detail = format!("{aerr:?}");
        for o in &fx.pack_objects {
            if !append_ok {
                break;
            }
            match ast.get(o.key) {
                Ok(got) if got == o.bytes => {}
                other => {
                    append_ok = false;
                    append_detail = format!("{}: {other:?}", o.key);
                }
            }
        }
        rec.want(
            "packstore",
            "packstore.append_record",
            "packstore/append-record-roundtrip",
            append_ok,
            format!(
                "an appended record did not read back as the object it encodes: {append_detail}"
            ),
            crate::cases::pack::pack_content_digest(&fx.pack_objects),
        );
        // A nil record is rejected rather than stored as an empty object.
        // Rust spells Go's nil slice as an empty one.
        let nil_err = ast.append_record(fx.pack_objects[0].key, &[]);
        rec.want(
            "packstore",
            "packstore.append_record",
            "packstore/append-record-rejects-nil",
            matches!(&nil_err, Err(e) if e.is_corrupt()),
            format!("an empty record must be reported as corrupt, got {nil_err:?}"),
            "ErrCorrupt",
        );
        // And the scrub passes over what the appends wrote.
        rec.want(
            "packstore",
            "packstore.append_record",
            "packstore/append-record-scrubs-clean",
            ast.verify(|| false).is_ok(),
            "the store did not scrub clean after a batch of appended records",
            "clean",
        );
        ast.close().unwrap();
    }
    let _ = fs::remove_dir_all(&adir);
}
