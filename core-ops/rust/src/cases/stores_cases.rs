//! refstore and inbox.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use amber_store_core::amberpack;
use amber_store_core::inbox::{Inbox, Meta};
use amber_store_core::key::Key;
use amber_store_core::refstore;

use crate::env::Env;
use crate::fixtures::{
    digest, digest_strings, fold_bool, fold_bytes, fold_i64, fold_key, fold_str, key_bytes,
    new_fold, sink_bytes,
};
use crate::harness::{Case, Dims, Recorder, State, bytes_kind};
use crate::stores::{
    RefHandle, StoreHandle, copied_dir, copied_refs, fresh_refs, fresh_store, work_dir,
};

// ---------------------------------------------------------------------------
// refstore
// ---------------------------------------------------------------------------

/// The reference name the reopen case looks up to prove the store is usable
/// after open returns.
const REF_PROBE: &str = "bench/ref/000000";

struct RefOpenState {
    dir: PathBuf,
    st: Option<refstore::Store>,
}

fn free_ref_open(_: &Env, s: State) {
    let mut st = s.downcast::<RefOpenState>().unwrap();
    drop(st.st.take());
    let _ = fs::remove_dir_all(&st.dir);
}

fn free_ref_handle(_: &Env, s: State) {
    s.downcast::<RefHandle>().unwrap().close();
}

pub fn refstore_cases(env: &Env) -> Vec<Case> {
    let fx = &env.fx;
    let n = fx.ref_batch.len();
    let hits: Vec<String> = (0..env.profile.batch_ops.min(n))
        .map(|i| fx.ref_names[(i * 7919) % n].clone())
        .collect();
    let misses: Vec<String> = (0..hits.len())
        .map(|i| format!("bench/absent/{i:06}"))
        .collect();

    let (h2len, m2len) = (hits.len(), misses.len());
    // Every refstore case is described by how many records the store holds
    // and how many calls one measured interval makes.
    let rdims = |entries: i64, items: i64| Dims {
        entries,
        items,
        content: "structured".into(),
        ..Default::default()
    };

    let mut out = Vec::new();
    out.push(
        Case::new(
            "refstore",
            "refstore.open",
            "empty",
            1,
            1,
            Box::new(|env| {
                Box::new(RefOpenState {
                    dir: work_dir(env, "rs-open-empty"),
                    st: None,
                }) as State
            }),
            Box::new(|_, s| {
                let st = s.downcast_mut::<RefOpenState>().unwrap();
                let store = refstore::Store::open(&st.dir, false).unwrap();
                let empty = store.all().unwrap().is_empty();
                st.st = Some(store);
                fold_bool(new_fold(), empty)
            }),
        )
        .dims(rdims(0, 1))
        .per_rep(Box::new(free_ref_open)),
    );
    out.push(
        Case::new(
            "refstore",
            "refstore.open",
            "populated-reopen",
            1,
            1,
            Box::new(|env| {
                Box::new(RefOpenState {
                    dir: copied_dir(env, &env.fx.refs_template, "rs-open-full"),
                    st: None,
                }) as State
            }),
            Box::new(|_, s| {
                let st = s.downcast_mut::<RefOpenState>().unwrap();
                let store = refstore::Store::open(&st.dir, false).unwrap();
                let acc = sink_bytes(new_fold(), &store.get(REF_PROBE).unwrap());
                st.st = Some(store);
                acc
            }),
        )
        .dims(rdims(n as i64, 1))
        .per_rep(Box::new(free_ref_open)),
    );
    // Go's refstore exposes Close; the redb port closes on drop and exports
    // no close method. The coverage matrix records the spelling difference.
    out.push(
        Case::new(
            "refstore",
            "refstore.close",
            "populated",
            1,
            1,
            Box::new(|env| {
                let dir = copied_dir(env, &env.fx.refs_template, "rs-close");
                let st = refstore::Store::open(&dir, false).unwrap();
                Box::new(RefOpenState { dir, st: Some(st) }) as State
            }),
            Box::new(|_, s| {
                let st = s.downcast_mut::<RefOpenState>().unwrap();
                drop(st.st.take());
                fold_bool(new_fold(), true)
            }),
        )
        .dims(rdims(n as i64, 1))
        .per_rep(Box::new(free_ref_open)),
    );

    for sync in [false, true] {
        out.push(
            Case::new(
                "refstore",
                "refstore.put",
                &format!("records-{n}/sync-{sync}"),
                1,
                n,
                Box::new(move |env| Box::new(fresh_refs(env, "rs-put", sync)) as State),
                Box::new(|env, s| {
                    let h = s.downcast_ref::<RefHandle>().unwrap();
                    let mut acc = new_fold();
                    for r in &env.fx.ref_batch {
                        h.store().put(&r.name, &r.data).unwrap();
                        acc = fold_bool(acc, true);
                    }
                    acc
                }),
            )
            .dims(rdims(n as i64, n as i64))
            .per_rep(Box::new(free_ref_handle)),
        );
    }
    out.push(
        Case::new(
            "refstore",
            "refstore.put_batch",
            &format!("records-{n}/sync-false"),
            1,
            n,
            Box::new(|env| Box::new(fresh_refs(env, "rs-batch", false)) as State),
            Box::new(|env, s| {
                let h = s.downcast_ref::<RefHandle>().unwrap();
                h.store().put_batch(&env.fx.ref_batch).unwrap();
                fold_i64(new_fold(), env.fx.ref_batch.len() as i64)
            }),
        )
        .dims(rdims(n as i64, n as i64))
        .per_rep(Box::new(free_ref_handle)),
    );
    let h2 = hits.clone();
    out.push(
        Case::new(
            "refstore",
            "refstore.get",
            "hit",
            1,
            hits.len(),
            Box::new(|env| {
                Box::new(copied_refs(env, &env.fx.refs_template, "rs-get", false)) as State
            }),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<RefHandle>().unwrap();
                let mut acc = new_fold();
                for name in &h2 {
                    acc = sink_bytes(acc, &h.store().get(name).unwrap());
                }
                acc
            }),
        )
        .dims(rdims(n as i64, h2len as i64))
        .per_rep(Box::new(free_ref_handle)),
    );
    let m2 = misses.clone();
    out.push(
        Case::new(
            "refstore",
            "refstore.get",
            "miss",
            1,
            misses.len(),
            Box::new(|env| {
                Box::new(copied_refs(
                    env,
                    &env.fx.refs_template,
                    "rs-get-miss",
                    false,
                )) as State
            }),
            Box::new(move |_, s| {
                let h = s.downcast_ref::<RefHandle>().unwrap();
                let mut acc = new_fold();
                for name in &m2 {
                    acc = fold_bool(
                        acc,
                        matches!(h.store().get(name), Err(e) if e.is_not_found()),
                    );
                }
                acc
            }),
        )
        .dims(rdims(n as i64, m2len as i64))
        .per_rep(Box::new(free_ref_handle)),
    );
    out.push(
        Case::new(
            "refstore",
            "refstore.all",
            &format!("records-{n}"),
            1,
            n,
            Box::new(|env| {
                Box::new(copied_refs(env, &env.fx.refs_template, "rs-all", false)) as State
            }),
            Box::new(|_, s| {
                let recs = s
                    .downcast_ref::<RefHandle>()
                    .unwrap()
                    .store()
                    .all()
                    .unwrap();
                let mut acc = fold_i64(new_fold(), recs.len() as i64);
                for r in &recs {
                    acc = fold_str(acc, &r.name);
                    acc = sink_bytes(acc, &r.data);
                }
                acc
            }),
        )
        .dims(rdims(n as i64, 1))
        .per_rep(Box::new(free_ref_handle)),
    );
    out.push(
        Case::new(
            "refstore",
            "refstore.delete",
            &format!("records-{n}"),
            1,
            n,
            Box::new(|env| {
                Box::new(copied_refs(env, &env.fx.refs_template, "rs-delete", false)) as State
            }),
            Box::new(|env, s| {
                let h = s.downcast_ref::<RefHandle>().unwrap();
                let mut acc = new_fold();
                for name in &env.fx.ref_names {
                    h.store().delete(name).unwrap();
                    acc = fold_bool(acc, true);
                }
                acc
            }),
        )
        .dims(rdims(n as i64, n as i64))
        .per_rep(Box::new(free_ref_handle)),
    );
    out.push(
        Case::new(
            "refstore",
            "refstore.wipe",
            &format!("records-{n}"),
            1,
            1,
            Box::new(|env| {
                Box::new(copied_refs(env, &env.fx.refs_template, "rs-wipe", false)) as State
            }),
            Box::new(|_, s| {
                let h = s.downcast_ref::<RefHandle>().unwrap();
                h.store().wipe().unwrap();
                fold_i64(new_fold(), h.store().all().unwrap().len() as i64)
            }),
        )
        .dims(rdims(n as i64, 1))
        .per_rep(Box::new(free_ref_handle)),
    );
    out
}

pub fn refstore_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let h = copied_refs(env, &fx.refs_template, "check-refs", false);
    {
        let st = h.store();
        let b = st.get(&fx.ref_names[0]);
        rec.want(
            "refstore",
            "refstore.get",
            "refstore/get-verbatim",
            matches!(&b, Ok(v) if *v == fx.ref_batch[0].data),
            format!("stored record bytes differ from what was put: {b:?}"),
            digest(b.as_deref().unwrap_or(&[])),
        );
        let miss = st.get("bench/absent");
        rec.want(
            "refstore",
            "refstore.get",
            "refstore/get-miss",
            matches!(&miss, Err(e) if e.is_not_found()),
            format!("an absent name must report not-found, got {miss:?}"),
            "ErrNotFound",
        );

        let all = st.all();
        let names: Vec<String> = all
            .as_ref()
            .map(|v| v.iter().map(|r| r.name.clone()).collect())
            .unwrap_or_default();
        let ordered = all.as_ref().map(|v| v.len()).unwrap_or(0) == fx.ref_batch.len()
            && names.windows(2).all(|w| w[0] < w[1]);
        rec.want(
            "refstore",
            "refstore.all",
            "refstore/all-lexicographic",
            ordered,
            format!(
                "all returned {} records, not in strict name order",
                names.len()
            ),
            digest_strings(&names),
        );

        st.put(&fx.ref_names[0], &[0x01, 0x02]).unwrap();
        let b = st.get(&fx.ref_names[0]);
        rec.want(
            "refstore",
            "refstore.put",
            "refstore/put-overwrites",
            matches!(&b, Ok(v) if v == &[0x01, 0x02]),
            format!("put did not overwrite: {b:?}"),
            digest(b.as_deref().unwrap_or(&[])),
        );
        st.delete(&fx.ref_names[0]).unwrap();
        let gone = st.get(&fx.ref_names[0]);
        rec.want(
            "refstore",
            "refstore.delete",
            "refstore/delete",
            matches!(&gone, Err(e) if e.is_not_found()),
            format!("the deleted name is still readable: {gone:?}"),
            "ErrNotFound",
        );
        let again = st.delete(&fx.ref_names[0]);
        rec.want(
            "refstore",
            "refstore.delete",
            "refstore/delete-absent",
            matches!(&again, Err(e) if e.is_not_found()),
            format!("deleting an absent name must report not-found, got {again:?}"),
            "ErrNotFound",
        );
        st.put_batch(&[
            refstore::Record {
                name: "bench/dup".into(),
                data: b"first".to_vec(),
            },
            refstore::Record {
                name: "bench/dup".into(),
                data: b"second".to_vec(),
            },
        ])
        .unwrap();
        let b = st.get("bench/dup");
        rec.want(
            "refstore",
            "refstore.put_batch",
            "refstore/batch-last-wins",
            matches!(&b, Ok(v) if v == b"second"),
            format!("repeated batch name resolved to {b:?}"),
            "second",
        );
        st.wipe().unwrap();
        let all = st.all();
        rec.want(
            "refstore",
            "refstore.wipe",
            "refstore/wipe",
            matches!(&all, Ok(v) if v.is_empty()),
            format!("wipe left records behind: {all:?}"),
            "0",
        );
    }
    h.close();
}

// ---------------------------------------------------------------------------
// inbox
// ---------------------------------------------------------------------------

/// An open inbox over its own store, plus the staged files a case prepared
/// outside the measured interval.
struct InboxState {
    dir: PathBuf,
    store: Option<StoreHandle>,
    ib: Option<Inbox>,
    tmps: Vec<PathBuf>,
    hashes: Vec<[u8; 32]>,
}

impl InboxState {
    fn close(mut self) {
        if let Some(ib) = self.ib.take() {
            ib.close();
        }
        if let Some(st) = self.store.take() {
            st.close();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }

    fn stage_all(&mut self, env: &Env) {
        let ib = self.ib.as_ref().unwrap();
        for (i, body) in env.fx.inbox_packs.iter().enumerate() {
            let meta = Meta {
                ref_: format!("bench/inbox/{i:03}"),
                root: key_bytes(env.fx.inbox_roots[i]),
                received_at: 1_700_000_000_000_000_000 + i as i64,
            };
            let (tmp, hash, _) = ib.stage(&meta, &body[..]).unwrap();
            self.tmps.push(tmp);
            self.hashes.push(hash);
        }
    }
}

fn new_inbox(env: &Env, name: &str, workers: usize) -> InboxState {
    let store = fresh_store(env, &format!("{name}-store"));
    let dir = work_dir(env, name);
    let arc = Arc::clone(store.st.as_ref().unwrap());
    let ib = Inbox::open(&dir, arc, workers, None).unwrap();
    InboxState {
        dir,
        store: Some(store),
        ib: Some(ib),
        tmps: Vec::new(),
        hashes: Vec::new(),
    }
}

fn free_inbox(_: &Env, s: State) {
    s.downcast::<InboxState>().unwrap().close();
}

/// The payload the staged packs carry. The packs' own encoded sizes differ
/// between the cores, so the shared denominator is the logical content.
fn inbox_bytes(env: &Env) -> i64 {
    env.fx.inbox_logical_bytes
}

pub fn inbox_cases(env: &Env) -> Vec<Case> {
    let p = &env.profile;
    let workers = p.threads_multi;
    let packs = p.inbox_packs;
    let bytes = inbox_bytes(env);
    // Every inbox case handles the same staged pack set: `items` is how many
    // packs, `objects` how many objects they carry between them.
    let idims = Dims {
        items: packs as i64,
        objects: (packs * 32) as i64,
        content: "structured".into(),
        ..Default::default()
    };
    let mut out = Vec::new();

    out.push(
        Case::new(
            "inbox",
            "inbox.open",
            "empty",
            workers,
            1,
            Box::new(|env| {
                let store = fresh_store(env, "inbox-open-store");
                Box::new(InboxState {
                    dir: work_dir(env, "inbox-open"),
                    store: Some(store),
                    ib: None,
                    tmps: Vec::new(),
                    hashes: Vec::new(),
                }) as State
            }),
            Box::new(move |_, s| {
                let is = s.downcast_mut::<InboxState>().unwrap();
                let arc = Arc::clone(is.store.as_ref().unwrap().st.as_ref().unwrap());
                is.ib = Some(Inbox::open(&is.dir, arc, workers, None).unwrap());
                fold_bool(new_fold(), is.ib.is_some())
            }),
        )
        .dims(Dims {
            items: 1,
            content: "structured".into(),
            ..Default::default()
        })
        .per_rep(Box::new(free_inbox)),
    );
    out.push(
        Case::new(
            "inbox",
            "inbox.open",
            "sweeps-staged-tmp-files",
            workers,
            packs,
            Box::new(move |env| {
                // Stage without committing, then close: the tmp files are
                // exactly what the next open has to sweep.
                let mut is = new_inbox(env, "inbox-recover", workers);
                is.stage_all(env);
                is.ib.take().unwrap().close();
                Box::new(is) as State
            }),
            Box::new(move |_, s| {
                let is = s.downcast_mut::<InboxState>().unwrap();
                let arc = Arc::clone(is.store.as_ref().unwrap().st.as_ref().unwrap());
                is.ib = Some(Inbox::open(&is.dir, arc, workers, None).unwrap());
                fold_bool(new_fold(), is.ib.is_some())
            }),
        )
        .dims(idims.clone())
        .per_rep(Box::new(free_inbox)),
    );
    out.push(
        Case::new(
            "inbox",
            "inbox.stage",
            &format!("packs-{packs}"),
            workers,
            packs,
            Box::new(move |env| Box::new(new_inbox(env, "inbox-stage", workers)) as State),
            Box::new(|env, s| {
                let is = s.downcast_mut::<InboxState>().unwrap();
                is.stage_all(env);
                let mut acc = fold_i64(new_fold(), is.tmps.len() as i64);
                for h in &is.hashes {
                    acc = fold_bytes(acc, h);
                }
                acc
            }),
        )
        .bytes(bytes)
        .dims(idims.clone())
        .per_rep(Box::new(free_inbox)),
    );
    out.push(
        Case::new(
            "inbox",
            "inbox.discard",
            &format!("packs-{packs}"),
            workers,
            packs,
            Box::new(move |env| {
                let mut is = new_inbox(env, "inbox-discard", workers);
                is.stage_all(env);
                Box::new(is) as State
            }),
            Box::new(|_, s| {
                let is = s.downcast_ref::<InboxState>().unwrap();
                let ib = is.ib.as_ref().unwrap();
                let mut acc = new_fold();
                for tmp in &is.tmps {
                    ib.discard(tmp);
                    acc = fold_str(acc, &tmp.to_string_lossy());
                }
                acc
            }),
        )
        .dims(idims.clone())
        .per_rep(Box::new(free_inbox)),
    );
    out.push(
        Case::new(
            "inbox",
            "inbox.drain",
            &format!("packs-{packs}/new"),
            workers,
            packs,
            Box::new(move |env| {
                let mut is = new_inbox(env, "inbox-drain", workers);
                is.stage_all(env);
                Box::new(is) as State
            }),
            Box::new(|env, s| {
                let is = s.downcast_ref::<InboxState>().unwrap();
                let ib = is.ib.as_ref().unwrap();
                let mut acc = new_fold();
                for (i, tmp) in is.tmps.iter().enumerate() {
                    let added = ib
                        .commit(tmp, &is.hashes[i], env.fx.inbox_roots[i])
                        .unwrap();
                    acc = fold_bool(acc, added);
                }
                for root in &env.fx.inbox_roots {
                    ib.wait_for(*root);
                    acc = fold_key(acc, root);
                }
                acc
            }),
        )
        .bytes(bytes)
        .dims(idims.clone())
        .per_rep(Box::new(free_inbox)),
    );
    out.push(
        Case::new(
            "inbox",
            "inbox.commit",
            &format!("packs-{packs}/duplicate"),
            workers,
            packs,
            Box::new(move |env| {
                // The idempotent path needs the first entry to still be in
                // the directory. Closing the inbox first retires the workers,
                // so the committed entries stay unprocessed and the duplicate
                // commit is deterministic in both cores (stage and commit do
                // not check the closed flag).
                let mut is = new_inbox(env, "inbox-commit-dup", workers);
                is.ib.as_ref().unwrap().close();
                is.stage_all(env);
                {
                    let ib = is.ib.as_ref().unwrap();
                    for (i, tmp) in is.tmps.iter().enumerate() {
                        ib.commit(tmp, &is.hashes[i], env.fx.inbox_roots[i])
                            .unwrap();
                    }
                }
                is.tmps.clear();
                is.hashes.clear();
                is.stage_all(env);
                Box::new(is) as State
            }),
            Box::new(|env, s| {
                let is = s.downcast_ref::<InboxState>().unwrap();
                let ib = is.ib.as_ref().unwrap();
                let mut acc = new_fold();
                for (i, tmp) in is.tmps.iter().enumerate() {
                    let added = ib
                        .commit(tmp, &is.hashes[i], env.fx.inbox_roots[i])
                        .unwrap();
                    acc = fold_bool(acc, added);
                }
                acc
            }),
        )
        .dims(idims.clone())
        .per_rep(Box::new(free_inbox)),
    );
    out.push(
        Case::new(
            "inbox",
            "inbox.close",
            "drained",
            workers,
            1,
            Box::new(move |env| {
                let mut is = new_inbox(env, "inbox-close", workers);
                is.stage_all(env);
                {
                    let ib = is.ib.as_ref().unwrap();
                    for (i, tmp) in is.tmps.iter().enumerate() {
                        ib.commit(tmp, &is.hashes[i], env.fx.inbox_roots[i])
                            .unwrap();
                    }
                }
                Box::new(is) as State
            }),
            Box::new(|_, s| {
                let is = s.downcast_mut::<InboxState>().unwrap();
                is.ib.take().unwrap().close();
                fold_bool(new_fold(), true)
            }),
        )
        .dims(idims.clone())
        .per_rep(Box::new(free_inbox)),
    );
    out
}

/// Lists the keys a wire pack carries.
fn read_pack_keys(pack: &[u8]) -> Vec<Key> {
    amberpack::Reader::new(pack)
        .records()
        .map(|r| r.unwrap().record.key)
        .collect()
}

pub fn inbox_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let is = new_inbox(env, "check-inbox", env.profile.threads_multi);
    {
        let ib = is.ib.as_ref().unwrap();
        let meta = Meta {
            ref_: "bench/inbox/check".into(),
            root: key_bytes(fx.inbox_roots[0]),
            received_at: 1_700_000_000_000_000_000,
        };
        let staged = ib.stage(&meta, &fx.inbox_packs[0][..]);
        rec.want(
            "inbox",
            "inbox.stage",
            "inbox/stage-hashes-body",
            matches!(&staged, Ok((_, h, n)) if *n == fx.inbox_packs[0].len() as u64 && h.len() == 32),
            format!("stage returned {staged:?}"),
            "ok",
        );
        let (tmp, hash, _) = staged.unwrap();
        rec.pass_local(
            "inbox",
            "inbox.stage",
            "inbox/stage-body-hash",
            crate::fixtures::digest_list(std::iter::once(&hash[..])),
        );
        let added = ib.commit(&tmp, &hash, fx.inbox_roots[0]);
        rec.want(
            "inbox",
            "inbox.commit",
            "inbox/commit-adds",
            matches!(&added, Ok(true)),
            format!("commit reported {added:?}"),
            "true",
        );
        ib.wait_for(fx.inbox_roots[0]);

        let keys = read_pack_keys(&fx.inbox_packs[0]);
        let store = is.store.as_ref().unwrap().store();
        let stored = keys
            .iter()
            .filter(|k| store.has(**k).unwrap_or(false))
            .count();
        rec.want(
            "inbox",
            "inbox.drain",
            "inbox/drain-stores-objects",
            stored == keys.len() && stored > 0,
            format!("{stored} of {} objects reached the store", keys.len()),
            stored.to_string(),
        );

        // Discard removes the staged file.
        let (tmp3, _, _) = ib.stage(&meta, &fx.inbox_packs[1][..]).unwrap();
        ib.discard(&tmp3);
        rec.want(
            "inbox",
            "inbox.discard",
            "inbox/discard-removes-tmp",
            !tmp3.exists(),
            "the discarded tmp file is still there",
            "removed",
        );

        // wait_for on an unknown root returns immediately.
        ib.wait_for(fx.store_miss_keys[0]);
        rec.pass(
            "inbox",
            "inbox.wait_for",
            "inbox/wait-for-empty-group",
            "returns",
        );
    }

    // Re-committing a body whose entry is still in the directory is
    // idempotent. A drained entry is deleted, so the check runs on a second
    // inbox whose workers have been retired.
    let idem = new_inbox(env, "check-inbox-idem", env.profile.threads_multi);
    {
        let ib = idem.ib.as_ref().unwrap();
        ib.close();
        let meta = Meta {
            ref_: "bench/inbox/check".into(),
            root: key_bytes(fx.inbox_roots[0]),
            received_at: 1_700_000_000_000_000_000,
        };
        let (tmp_a, hash_a, _) = ib.stage(&meta, &fx.inbox_packs[0][..]).unwrap();
        let added_a = ib.commit(&tmp_a, &hash_a, fx.inbox_roots[0]);
        let (tmp_b, hash_b, _) = ib.stage(&meta, &fx.inbox_packs[0][..]).unwrap();
        let added_b = ib.commit(&tmp_b, &hash_b, fx.inbox_roots[0]);
        rec.want(
            "inbox",
            "inbox.commit",
            "inbox/commit-idempotent",
            matches!(&added_a, Ok(true)) && matches!(&added_b, Ok(false)) && !tmp_b.exists(),
            format!(
                "first={added_a:?} second={added_b:?}, staged file still present: {}",
                tmp_b.exists()
            ),
            "true/false",
        );
    }
    idem.close();
    is.close();
}
