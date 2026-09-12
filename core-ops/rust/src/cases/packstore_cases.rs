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
use crate::fixtures::{RecordLoc, digest_strings};
use crate::fixtures_build::{object_seq, store_objects, store_objects_of_size, store_options};
use crate::harness::{Case, Recorder, State};
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
                st.st = Some(Arc::new(store));
                1
            }),
        )
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
                let n = store.segments().unwrap().len() as u64;
                st.st = Some(Arc::new(store));
                n
            }),
        )
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
                1
            }),
        )
        .per_rep(Box::new(free_open_state)),
    );

    // --- reads --------------------------------------------------------
    macro_rules! read_case {
        ($op:expr, $wl:expr, $ops:expr, $body:expr) => {
            out.push(Case::new(
                "packstore",
                $op,
                $wl,
                1,
                $ops,
                Box::new(|_| Box::new(()) as State),
                Box::new($body),
            ))
        };
    }

    let hk = hit_keys.clone();
    read_case!(
        "packstore.get",
        "hit",
        lookups,
        move |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = 0u64;
            for k in &hk {
                acc += st.get(*k).unwrap().len() as u64;
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
            let mut acc = 0u64;
            for k in &mk {
                if matches!(st.get(*k), Err(e) if e.is_not_found()) {
                    acc += 1;
                }
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
            let mut acc = 0u64;
            for k in &hk {
                acc += st.get_record(*k).unwrap().len() as u64;
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
            let mut acc = 0u64;
            for k in &hk {
                if st.has(*k).unwrap() {
                    acc += 1;
                }
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
            let mut acc = 0u64;
            for k in &mk {
                if !st.has(*k).unwrap() {
                    acc += 1;
                }
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
            let mut acc = 0u64;
            for k in &hk {
                if let Some(n) = st.stored_size(*k).unwrap() {
                    acc += n;
                }
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
            env.fx.ro.as_ref().unwrap().missing(&mx).unwrap().len() as u64
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
            ks[0].as_bytes()[0] as u64
        }
    );
    read_case!(
        "packstore.segments",
        "list",
        64,
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = 0u64;
            for _ in 0..64 {
                acc += st.segments().unwrap().len() as u64;
            }
            acc
        }
    );
    read_case!(
        "packstore.scan_index",
        "one-segment",
        1,
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            let mut acc = 0u64;
            st.scan_index(env.fx.ro_seg_id, |k, _off, slen| {
                acc += slen as u64 + k.as_bytes()[0] as u64
            })
            .unwrap();
            acc
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
            let mut acc = 0u64;
            for i in 0..nloc {
                let l = locs[(i * 7919) % locs.len()];
                acc += st.record(l.id, l.off).unwrap().len() as u64;
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
            let mut acc = 0u64;
            for k in &hk {
                if st.has_outside(env.fx.ro_seg_id, *k).unwrap() {
                    acc += 1;
                }
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
            let mut acc = 0u64;
            for _ in 0..4096 {
                if st.oldest_inflight_write().is_some() {
                    acc += 1;
                }
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
            let mut acc = 0u64;
            for _ in 0..16 {
                acc += st.new_mark_set().marked() as u64 + 1;
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
            let mut acc = 0u64;
            for k in &env.fx.store_keys {
                let (newly, present) = ms.mark(*k);
                if newly && present {
                    acc += 1;
                }
            }
            acc + ms.marked() as u64
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
            let mut acc = 0u64;
            for k in &env.fx.store_keys {
                if ms.contains(*k) {
                    acc += 1;
                }
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
            ls.iter().map(|l| (l.live_keys + l.dead_keys) as u64).sum()
        }
    );
    read_case!(
        "packstore.verify",
        "full-scrub",
        1,
        |env: &Env, _: &mut State| {
            env.fx.ro.as_ref().unwrap().verify(|| false).unwrap();
            1
        }
    );
    read_case!(
        "packstore.barrier",
        "begin+observe+abort",
        fx.store_keys.len(),
        |env: &Env, _: &mut State| {
            let st = env.fx.ro.as_ref().unwrap();
            st.begin_barrier();
            st.observe_keys(&env.fx.store_keys);
            st.abort_barrier();
            env.fx.store_keys.len() as u64
        }
    );

    // --- writes -------------------------------------------------------
    let write_objs: Arc<Vec<Object>> = Arc::new(store_objects(p, p.store_objects.min(8000), 50000));
    let tiny: Arc<Vec<Object>> =
        Arc::new(store_objects_of_size(p, p.batch_ops.min(4000), 128, 60000));
    let large: Arc<Vec<Object>> = Arc::new(store_objects_of_size(
        p,
        (p.batch_ops / 16).clamp(16, 256),
        256 << 10,
        61000,
    ));

    let put_case = |workload: &str, objs: Arc<Vec<Object>>, sync: bool| {
        let n = objs.len();
        let b = crate::cases::pack::pack_bytes(&objs);
        let o2 = Arc::clone(&objs);
        Case::new(
            "packstore",
            "packstore.put",
            workload,
            1,
            n,
            Box::new(move |env| {
                Box::new(if sync {
                    fresh_store_sync(env, "ps-put")
                } else {
                    fresh_store(env, "ps-put")
                }) as State
            }),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let mut acc = 0u64;
                for o in o2.iter() {
                    h.store().put(o.key, &o.bytes).unwrap();
                    acc += o.key.as_bytes()[0] as u64;
                }
                acc
            }),
        )
        .bytes(b)
        .per_rep(Box::new(free_store_handle))
    };
    out.push(put_case("tiny-128B/sync-off", Arc::clone(&tiny), false));
    out.push(put_case("tiny-128B/sync-on", Arc::clone(&tiny), true));
    out.push(put_case("large-256KiB/sync-off", Arc::clone(&large), false));

    let t1 = Arc::clone(&tiny);
    let t2 = Arc::clone(&tiny);
    out.push(
        Case::new(
            "packstore",
            "packstore.put",
            "duplicate/sync-off",
            1,
            tiny.len(),
            Box::new(move |env| {
                let h = fresh_store(env, "ps-put-dup");
                for o in t1.iter() {
                    h.store().put(o.key, &o.bytes).unwrap();
                }
                Box::new(h) as State
            }),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let mut acc = 0u64;
                for o in t2.iter() {
                    h.store().put(o.key, &o.bytes).unwrap();
                    acc += 1;
                }
                acc
            }),
        )
        .bytes(crate::cases::pack::pack_bytes(&tiny))
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
                w1.len() as u64
            }),
        )
        .bytes(crate::cases::pack::pack_bytes(&write_objs))
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
                for (i, rec) in env.fx.wire_records.iter().enumerate() {
                    h.store()
                        .append_record(env.fx.pack_objects[i].key, rec)
                        .unwrap();
                }
                h.store().sync().unwrap();
                env.fx.wire_records.len() as u64
            }),
        )
        .per_rep(Box::new(free_store_handle)),
    );

    for writers in [p.threads_single, p.threads_multi] {
        for verify in [false, true] {
            let objs = Arc::clone(&write_objs);
            out.push(
                Case::new(
                    "packstore",
                    "packstore.write_parallel",
                    &format!("mixed-objects/writers-{writers}/verify-{verify}"),
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
                        stats.stored as u64
                    }),
                )
                .bytes(crate::cases::pack::pack_bytes(&write_objs))
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
            "duplicate-stream/writers-8",
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
                stats.deduped as u64
            }),
        )
        .bytes(crate::cases::pack::pack_bytes(&write_objs))
        .per_rep(Box::new(free_store_handle)),
    );

    // --- destructive maintenance -------------------------------------
    out.push(
        Case::new(
            "packstore",
            "packstore.compact",
            "90-percent-dead",
            1,
            1,
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
                stats.records_copied as u64 + stats.bytes_freed
            }),
        )
        .per_rep(Box::new(free_store_handle)),
    );
    out.push(
        Case::new(
            "packstore",
            "packstore.compact",
            "nothing-dead",
            1,
            1,
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
                stats.segments_scanned as u64
            }),
        )
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
                segs[0].id + 1
            }),
        )
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
                s.downcast_ref::<StoreHandle>()
                    .unwrap()
                    .store()
                    .wipe()
                    .unwrap();
                1
            }),
        )
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
                let mut acc = 0u64;
                for i in 0..pv_ops {
                    let o = &env.fx.pack_objects[i % env.fx.pack_objects.len()];
                    h.store().put_verified(o.key, &o.bytes).unwrap();
                    acc += 1;
                }
                acc
            }),
        )
        .per_rep(Box::new(free_store_handle)),
    );
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
        let retained = fx.garbage_live.iter().all(|k| cst.get(*k).is_ok());
        let dropped = fx
            .store_keys
            .iter()
            .enumerate()
            .filter(|(i, k)| i % 10 != 0 && !cst.has(**k).unwrap_or(true))
            .count();
        rec.want(
            "packstore",
            "packstore.compact",
            "packstore/compact-retains-live",
            stats.is_ok() && retained,
            format!("a live object was lost by compaction ({stats:?})"),
            format!("live={}", fx.garbage_live.len()),
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
}
