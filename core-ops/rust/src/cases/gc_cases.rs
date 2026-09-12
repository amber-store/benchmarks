//! gc: the collector's reference hooks, status, cycle and wipe.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use amber_store_core::fstree;
use amber_store_core::gc;
use amber_store_core::key::Key;
use amber_store_core::packstore;
use amber_store_core::refstore;

use crate::env::{Env, Profile};
use crate::fixtures::{fold_bool, fold_i64, fold_key, fold_str, fold_u64, new_fold};
use crate::fixtures_build::store_options;
use crate::harness::{Case, Dims, Recorder, State};
use crate::stores::{copied_dir, dir_bytes};

/// A collector over its own copy of the gc fixture.
struct GcState {
    dir: PathBuf,
    objs: Option<Arc<packstore::Store>>,
    refs: Option<Arc<refstore::Store>>,
    col: Option<gc::Collector>,
}

impl GcState {
    fn close(mut self) {
        if let Some(c) = self.col.take() {
            let _ = c.close();
        }
        if let Some(o) = self.objs.take() {
            let _ = o.close();
        }
        drop(self.refs.take());
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn free_gc(_: &Env, s: State) {
    s.downcast::<GcState>().unwrap().close();
}

/// The collector configuration both cores are given. The grace period is one
/// nanosecond so the freshly written fixture segments are eligible; the
/// garbage line is passed explicitly to run, so the policy fallback (and its
/// free-space sensitivity) never enters the measurement.
fn gc_options(p: &Profile) -> gc::Options {
    gc::Options {
        grace: Duration::from_nanos(1),
        garbage: gc::DEFAULT_GARBAGE,
        jobs: p.threads_multi,
        ..Default::default()
    }
}

/// Copies the fixture and opens the two stores; the collector itself is
/// opened by the caller, so `gc.open` can be measured on its own.
fn open_gc(env: &Env, name: &str) -> GcState {
    let dir = copied_dir(env, &env.fx.gc_template, name);
    let objs =
        packstore::Store::open_with(dir.join("objects"), store_options(&env.profile)).unwrap();
    let refs = refstore::Store::open(dir.join("refs"), false).unwrap();
    GcState {
        dir,
        objs: Some(Arc::new(objs)),
        refs: Some(Arc::new(refs)),
        col: None,
    }
}

fn open_gc_full(env: &Env, name: &str) -> GcState {
    let mut s = open_gc(env, name);
    s.col = Some(
        gc::Collector::open(
            s.dir.join("closures"),
            Arc::clone(s.objs.as_ref().unwrap()),
            Arc::clone(s.refs.as_ref().unwrap()),
            gc_options(&env.profile),
        )
        .unwrap(),
    );
    s
}

pub fn cases(env: &Env) -> Vec<Case> {
    let t = env.profile.threads_multi;
    // Every collection case runs over its own copy of the same fixture: a
    // store holding one referenced tree plus enough unreferenced objects to
    // fill whole sealed segments. `objects` is what a mark has to walk,
    // `files` what the referenced tree covers.
    let objects = env.profile.store_objects as i64;
    let files = env.fx.v1_included.files;
    let gdims = move |items: i64| Dims {
        items,
        objects,
        files,
        content: "structured".into(),
        ..Default::default()
    };
    let mut out = Vec::new();

    out.push(
        Case::new(
            "gc",
            "gc.open",
            "populated",
            t,
            1,
            Box::new(|env| Box::new(open_gc(env, "gc-open")) as State),
            Box::new(|env, s| {
                let st = s.downcast_mut::<GcState>().unwrap();
                st.col = Some(
                    gc::Collector::open(
                        st.dir.join("closures"),
                        Arc::clone(st.objs.as_ref().unwrap()),
                        Arc::clone(st.refs.as_ref().unwrap()),
                        gc_options(&env.profile),
                    )
                    .unwrap(),
                );
                fold_bool(new_fold(), st.col.is_some())
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.close",
            "idle",
            t,
            1,
            Box::new(|env| Box::new(open_gc_full(env, "gc-close")) as State),
            Box::new(|_, s| {
                let st = s.downcast_mut::<GcState>().unwrap();
                st.col.take().unwrap().close().unwrap();
                fold_bool(new_fold(), true)
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.prepare_ref",
            "tree-root/commit",
            t,
            1,
            Box::new(|env| Box::new(open_gc_full(env, "gc-prepare")) as State),
            Box::new(|env, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                st.col
                    .as_ref()
                    .unwrap()
                    .prepare_ref(env.fx.gc_live_root)
                    .unwrap()
                    .commit();
                fold_key(new_fold(), &env.fx.gc_live_root)
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.prepare_ref",
            "tree-root/abort",
            t,
            1,
            Box::new(|env| Box::new(open_gc_full(env, "gc-prepare-abort")) as State),
            Box::new(|env, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                st.col
                    .as_ref()
                    .unwrap()
                    .prepare_ref(env.fx.gc_live_root)
                    .unwrap()
                    .abort();
                fold_key(new_fold(), &env.fx.gc_live_root)
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.prepare_ref",
            "missing-root",
            t,
            64,
            Box::new(|env| Box::new(open_gc_full(env, "gc-prepare-missing")) as State),
            Box::new(|env, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                let col = st.col.as_ref().unwrap();
                let mut acc = new_fold();
                for _ in 0..64 {
                    acc = fold_bool(acc, col.prepare_ref(env.fx.wide_root).is_err());
                }
                acc
            }),
        )
        .dims(gdims(64))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.release_ref",
            "batch",
            t,
            4096,
            Box::new(|env| Box::new(open_gc_full(env, "gc-release")) as State),
            Box::new(|env, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                let col = st.col.as_ref().unwrap();
                let mut acc = new_fold();
                for _ in 0..4096 {
                    acc = fold_bool(acc, col.release_ref(env.fx.gc_live_root).is_ok());
                }
                acc
            }),
        )
        .dims(gdims(4096))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.status",
            "mark+score",
            t,
            1,
            Box::new(|env| Box::new(open_gc_full(env, "gc-status")) as State),
            Box::new(|_, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                let status = st.col.as_ref().unwrap().status().unwrap();
                let mut acc = fold_i64(
                    fold_i64(new_fold(), status.marked as i64),
                    status.packs.len() as i64,
                );
                for pk in &status.packs {
                    acc = fold_u64(acc, pk.id);
                }
                acc
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.why",
            "live-root",
            t,
            1,
            Box::new(|env| Box::new(open_gc_full(env, "gc-why")) as State),
            Box::new(|env, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                let names = st.col.as_ref().unwrap().why(env.fx.gc_live_root).unwrap();
                let mut acc = fold_i64(new_fold(), names.len() as i64);
                for nm in &names {
                    acc = fold_str(acc, nm);
                }
                acc
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.run",
            "reclaimable-packs",
            t,
            1,
            Box::new(|env| Box::new(open_gc_full(env, "gc-run")) as State),
            Box::new(|_, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                let stats = st.col.as_ref().unwrap().run(gc::DEFAULT_GARBAGE).unwrap();
                let mut acc = fold_i64(
                    fold_i64(new_fold(), stats.marked as i64),
                    stats.reaped.len() as i64,
                );
                for r in &stats.reaped {
                    acc = fold_u64(acc, *r);
                }
                acc
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.run",
            "nothing-to-reclaim",
            t,
            1,
            Box::new(|env| {
                let st = open_gc_full(env, "gc-run-clean");
                st.col.as_ref().unwrap().run(gc::DEFAULT_GARBAGE).unwrap();
                Box::new(st) as State
            }),
            Box::new(|_, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                let stats = st.col.as_ref().unwrap().run(gc::DEFAULT_GARBAGE).unwrap();
                fold_i64(
                    fold_i64(
                        fold_i64(new_fold(), stats.marked as i64),
                        stats.scored as i64,
                    ),
                    stats.reaped.len() as i64,
                )
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out.push(
        Case::new(
            "gc",
            "gc.wipe",
            "store-reset",
            t,
            1,
            Box::new(|env| Box::new(open_gc_full(env, "gc-wipe")) as State),
            Box::new(|_, s| {
                let st = s.downcast_ref::<GcState>().unwrap();
                let objs = Arc::clone(st.objs.as_ref().unwrap());
                let refs = Arc::clone(st.refs.as_ref().unwrap());
                st.col
                    .as_ref()
                    .unwrap()
                    .wipe(|| -> Result<(), String> {
                        objs.wipe().map_err(|e| e.to_string())?;
                        refs.wipe().map_err(|e| e.to_string())
                    })
                    .unwrap();
                fold_bool(new_fold(), true)
            }),
        )
        .dims(gdims(1))
        .per_rep(Box::new(free_gc)),
    );
    out
}

pub fn checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    // The Go collector exports BeginWrite; the Rust collector keeps the
    // equivalent write gate internal to the packstore (Store::begin_write is
    // pub(super)). It is recorded as Go-only and never paired.
    rec.skip(
        "gc.begin_write",
        "gate-span",
        "the Rust core keeps the write gate internal: packstore::Store::begin_write is pub(super) and the collector exports no BeginWrite",
    );

    let st = open_gc_full(env, "check-gc");
    {
        let objs = st.objs.as_ref().unwrap();
        let col = st.col.as_ref().unwrap();

        let live: Vec<Key> = match fstree::reachable_keys(fx.gc_live_root, |k| objs.get(k)) {
            Ok(v) => v,
            Err(e) => {
                rec.fail("gc", "gc.run", "gc/reachable-before", e.to_string());
                st.close();
                return;
            }
        };
        let before_segs = objs.segments().unwrap_or_default().len();
        let before_bytes = dir_bytes(&st.dir.join("objects"));

        let status = col.status();
        rec.want(
            "gc",
            "gc.status",
            "gc/status-marks-live",
            matches!(&status, Ok(s) if s.refs == 1 && s.marked == live.len() && s.packs.len() == before_segs),
            format!("status: {status:?} (live={}, sealed={before_segs})", live.len()),
            status.as_ref().map(|s| format!("marked={}", s.marked)).unwrap_or_default(),
        );
        rec.want(
            "gc",
            "gc.status",
            "gc/status-sees-garbage",
            matches!(&status, Ok(s) if s.garbage_bytes > 0 && s.live_bytes > 0),
            format!("status found {status:?}"),
            status
                .as_ref()
                .map(|s| format!("garbage>0={}", s.garbage_bytes > 0))
                .unwrap_or_default(),
        );

        let names = col.why(fx.gc_live_root);
        rec.want(
            "gc",
            "gc.why",
            "gc/why-names-the-reference",
            matches!(&names, Ok(v) if v.len() == 1 && v[0] == "gc/live"),
            format!("why returned {names:?}"),
            crate::fixtures::digest_strings(names.as_deref().unwrap_or(&[])),
        );
        let none = col.why(fx.store_miss_keys[0]);
        rec.want(
            "gc",
            "gc.why",
            "gc/why-unreferenced",
            matches!(&none, Ok(v) if v.is_empty()),
            format!("an unreferenced key is explained by {none:?}"),
            "0",
        );

        let bad = col.prepare_ref(fx.wide_root);
        rec.want(
            "gc",
            "gc.prepare_ref",
            "gc/prepare-rejects-incomplete",
            bad.is_err(),
            "prepare_ref accepted a root that is not stored",
            "rejected",
        );
        drop(bad);
        let good = col.prepare_ref(fx.gc_live_root);
        let good_ok = good.is_ok();
        if let Ok(p) = good {
            p.commit();
        }
        rec.want(
            "gc",
            "gc.prepare_ref",
            "gc/prepare-accepts-complete",
            good_ok,
            "prepare_ref rejected the stored tree",
            "accepted",
        );
        rec.want(
            "gc",
            "gc.release_ref",
            "gc/release-is-a-noop",
            col.release_ref(fx.gc_live_root).is_ok(),
            "release_ref reported an error",
            "ok",
        );

        let stats = col.run(gc::DEFAULT_GARBAGE);
        let after_bytes = dir_bytes(&st.dir.join("objects"));
        // How many packs a cycle reaps depends on how the objects packed
        // into segments, which follows the compressed record sizes; the
        // count is a within-core anchor, the reclamation itself is the
        // cross-core statement.
        rec.want_local(
            "gc",
            "gc.run",
            "gc/run-reclaims",
            matches!(&stats, Ok(s) if !s.reaped.is_empty() && s.freed_bytes > 0)
                && after_bytes < before_bytes,
            format!("cycle {stats:?}, directory {before_bytes} -> {after_bytes}"),
            stats
                .as_ref()
                .map(|s| format!("reaped={}", s.reaped.len()))
                .unwrap_or_default(),
        );
        rec.want(
            "gc",
            "gc.run",
            "gc/run-marks-live",
            matches!(&stats, Ok(s) if s.marked == live.len()),
            format!("the cycle marked {stats:?} of {} live objects", live.len()),
            stats
                .as_ref()
                .map(|s| format!("marked={}", s.marked))
                .unwrap_or_default(),
        );

        let retained = live
            .iter()
            .filter(|k| objs.has(**k).unwrap_or(false))
            .count();
        rec.want(
            "gc",
            "gc.run",
            "gc/run-retains-live",
            retained == live.len(),
            format!(
                "{retained} of {} referenced objects survived the sweep",
                live.len()
            ),
            retained.to_string(),
        );
        let complete: Result<Vec<Key>, _> =
            fstree::check_complete(fx.gc_live_root, |k| objs.get(k), |k| objs.has(k), 1);
        rec.want(
            "gc",
            "gc.run",
            "gc/run-tree-still-complete",
            complete.is_ok(),
            format!("the referenced tree is incomplete after collection: {complete:?}"),
            "complete",
        );
        rec.want(
            "gc",
            "gc.run",
            "gc/store-scrubs-clean-after-run",
            objs.verify(|| false).is_ok(),
            "the store did not scrub clean after a collection cycle",
            "clean",
        );

        // Repeated cycles reach a fixed point: sealing the active segment
        // can expose one more mostly-dead pack to the next cycle, so the
        // guarantee is that the sweep terminates, not that the very next
        // cycle is idle.
        let mut cycles = 0;
        let mut quiet = false;
        while cycles < 5 && !quiet {
            match col.run(gc::DEFAULT_GARBAGE) {
                Ok(s) => {
                    cycles += 1;
                    quiet = s.reaped.is_empty();
                }
                Err(e) => {
                    rec.fail("gc", "gc.run", "gc/reaches-a-fixed-point", e.to_string());
                    break;
                }
            }
        }
        rec.want(
            "gc",
            "gc.run",
            "gc/reaches-a-fixed-point",
            quiet,
            format!("the collector still reaped packs after {cycles} extra cycles"),
            "quiet",
        );
        let retained_after = live
            .iter()
            .filter(|k| objs.has(**k).unwrap_or(false))
            .count();
        rec.want(
            "gc",
            "gc.run",
            "gc/fixed-point-retains-live",
            retained_after == live.len(),
            format!(
                "{retained_after} of {} referenced objects survived the repeated cycles",
                live.len()
            ),
            retained_after.to_string(),
        );

        let objs2 = Arc::clone(objs);
        let refs2 = Arc::clone(st.refs.as_ref().unwrap());
        let wiped = col.wipe(|| -> Result<(), String> {
            objs2.wipe().map_err(|e| e.to_string())?;
            refs2.wipe().map_err(|e| e.to_string())
        });
        let has = objs.has(live[0]).unwrap_or(true);
        let recs = st.refs.as_ref().unwrap().all().unwrap_or_default();
        rec.want(
            "gc",
            "gc.wipe",
            "gc/wipe-resets",
            wiped.is_ok() && !has && recs.is_empty(),
            format!("wipe left {has} objects / {} refs ({wiped:?})", recs.len()),
            "empty",
        );
    }
    st.close();
}
