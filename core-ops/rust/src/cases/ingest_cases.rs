//! amberignore and ingest.

use std::fs;

use amber_store_core::amberignore::{self, Matcher};
use amber_store_core::fstree::{self, Entry};
use amber_store_core::ingest;
use amber_store_core::key::Key;
use amber_store_core::packstore;

use crate::env::Env;
use crate::fixtures::{MemStore, digest_strings};
use crate::fixtures_build::store_options;
use crate::harness::{Case, Recorder, State};
use crate::stores::{StoreHandle, copied_store, fresh_store, work_dir};

// ---------------------------------------------------------------------------
// amberignore
// ---------------------------------------------------------------------------

/// The deterministic name batch the matcher is asked about: a mix of ignored,
/// negated and kept names, so the case is not dominated by one branch.
fn ignore_names() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for i in 0..256 {
        out.push(format!("source{i:03}.c").into_bytes());
        out.push(format!("object{i:03}.o").into_bytes());
        out.push(format!("scratch{i:03}.tmp").into_bytes());
        out.push(b"keep.tmp".to_vec());
    }
    out
}

pub fn ignore_cases(_env: &Env) -> Vec<Case> {
    let names = ignore_names();
    let n = names.len();
    vec![
        Case::new(
            "amberignore",
            "amberignore.root",
            "load-root",
            1,
            64,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut acc = 0u64;
                for _ in 0..64 {
                    let m = Matcher::root(&env.fx.ignore_dir).unwrap();
                    if m.ignored(b"x.o", false) {
                        acc += 1;
                    }
                }
                acc
            }),
        ),
        Case::new(
            "amberignore",
            "amberignore.descend",
            "8-subdirs",
            1,
            64 * 8,
            Box::new(|env| Box::new(Matcher::root(&env.fx.ignore_dir).unwrap()) as State),
            Box::new(|env, s| {
                let m = s.downcast_ref::<Matcher>().unwrap();
                let mut acc = 0u64;
                for _ in 0..64 {
                    for d in 0..8 {
                        let name = format!("sub{d:02}");
                        let sub = m
                            .descend(env.fx.ignore_dir.join(&name), name.as_bytes())
                            .unwrap();
                        if sub.ignored(format!("gen{d:02}-a").as_bytes(), false) {
                            acc += 1;
                        }
                    }
                }
                acc
            }),
        ),
        Case::new(
            "amberignore",
            "amberignore.ignored",
            "mixed-names",
            1,
            n,
            Box::new(|env| {
                Box::new((Matcher::root(&env.fx.ignore_dir).unwrap(), ignore_names())) as State
            }),
            Box::new(|_, s| {
                let (m, names) = s.downcast_ref::<(Matcher, Vec<Vec<u8>>)>().unwrap();
                let mut acc = 0u64;
                for nm in names {
                    if m.ignored(nm, false) {
                        acc += 1;
                    }
                }
                acc
            }),
        ),
        // Go's nil *Matcher is spelled `Option<&Matcher>` here, with the
        // `ignored_opt` helper; the coverage matrix pairs the two.
        Case::new(
            "amberignore",
            "amberignore.ignored",
            "nil-matcher",
            1,
            n,
            Box::new(|_| Box::new(ignore_names()) as State),
            Box::new(|_, s| {
                let names = s.downcast_ref::<Vec<Vec<u8>>>().unwrap();
                let mut acc = 0u64;
                for nm in names {
                    if amberignore::ignored_opt(None, nm, false) {
                        acc += 1;
                    }
                }
                acc
            }),
        ),
    ]
}

pub fn ignore_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let m = match Matcher::root(&fx.ignore_dir) {
        Ok(m) => m,
        Err(e) => {
            rec.fail(
                "amberignore",
                "amberignore.root",
                "amberignore/root",
                e.to_string(),
            );
            return;
        }
    };
    let probes: [(&str, bool, bool); 7] = [
        ("main.c", false, false),
        ("main.o", false, true),
        ("scratch.tmp", false, true),
        ("keep.tmp", false, false),
        ("vendor", true, true),
        ("target", true, true),
        (amberignore::FILE_NAME, false, false),
    ];
    let mut ok = true;
    let mut got = Vec::new();
    for (name, is_dir, want) in probes {
        let g = m.ignored(name.as_bytes(), is_dir);
        got.push(format!("{name}={g}"));
        if g != want {
            ok = false;
        }
    }
    rec.want(
        "amberignore",
        "amberignore.ignored",
        "amberignore/root-patterns",
        ok,
        format!("root pattern results: {got:?}"),
        digest_strings(&got),
    );

    let sub = match m.descend(fx.ignore_dir.join("sub00"), b"sub00") {
        Ok(s) => s,
        Err(e) => {
            rec.fail(
                "amberignore",
                "amberignore.descend",
                "amberignore/descend",
                e.to_string(),
            );
            return;
        }
    };
    let sub_ok = sub.ignored(b"gen00-a", false)
        && !sub.ignored(b"gen00-keep", false)
        && sub.ignored(b"x.o", false);
    let sub_got: Vec<String> = ["gen00-a", "gen00-keep", "x.o", "x.c"]
        .iter()
        .map(|n| format!("{n}={}", sub.ignored(n.as_bytes(), false)))
        .collect();
    rec.want(
        "amberignore",
        "amberignore.descend",
        "amberignore/descend-composes",
        sub_ok,
        format!("descended pattern results: {sub_got:?}"),
        digest_strings(&sub_got),
    );
    rec.want(
        "amberignore",
        "amberignore.ignored",
        "amberignore/nil-matcher-ignores-nothing",
        !amberignore::ignored_opt(None, b"anything.o", false),
        "a nil matcher must ignore nothing",
        "false",
    );
}

// ---------------------------------------------------------------------------
// ingest
// ---------------------------------------------------------------------------

fn opts(jobs: usize, no_ignore: bool) -> ingest::Opts {
    ingest::Opts {
        jobs,
        no_ignore,
        ..Default::default()
    }
}

pub fn ingest_cases(env: &Env) -> Vec<Case> {
    let p = &env.profile;
    let mut out = Vec::new();
    for jobs in [p.threads_single, p.threads_multi] {
        out.push(
            Case::new(
                "ingest",
                "ingest.scan",
                &format!("tree/jobs-{jobs}"),
                jobs,
                1,
                Box::new(|_| Box::new(()) as State),
                Box::new(move |env, _| {
                    let (files, bytes) = ingest::scan(&env.fx.tree_v1, false, jobs).unwrap();
                    files + bytes
                }),
            )
            .bytes(env.fx.tree_bytes),
        );
        out.push(
            Case::new(
                "ingest",
                "ingest.objects",
                &format!("tree/jobs-{jobs}"),
                jobs,
                1,
                Box::new(|_| Box::new(()) as State),
                Box::new(move |env, _| {
                    let (stream, root) =
                        ingest::objects(&env.fx.tree_v1, opts(jobs, false)).unwrap();
                    let mut n = 0u64;
                    for o in stream {
                        n += o.unwrap().bytes.len() as u64;
                    }
                    n + root.get().unwrap().as_bytes()[0] as u64
                }),
            )
            .bytes(env.fx.tree_bytes),
        );
        out.push(
            Case::new(
                "ingest",
                "ingest.dir",
                &format!("tree/fresh-store/jobs-{jobs}"),
                jobs,
                1,
                Box::new(|env| Box::new(fresh_store(env, "ingest-dir")) as State),
                Box::new(move |env, s| {
                    let h = s.downcast_ref::<StoreHandle>().unwrap();
                    let (stats, res) = ingest::dir(h.store(), &env.fx.tree_v1, opts(jobs, false));
                    stats.stored as u64 + res.unwrap().as_bytes()[0] as u64
                }),
            )
            .bytes(env.fx.tree_bytes)
            .per_rep(Box::new(|_, s| {
                s.downcast::<StoreHandle>().unwrap().close();
            })),
        );
        out.push(
            Case::new(
                "ingest",
                "ingest.dir",
                &format!("tree/incremental-change/jobs-{jobs}"),
                jobs,
                1,
                Box::new(|env| {
                    Box::new(copied_store(env, &env.fx.ingest_template, "ingest-inc")) as State
                }),
                Box::new(move |env, s| {
                    let h = s.downcast_ref::<StoreHandle>().unwrap();
                    let (stats, res) = ingest::dir(h.store(), &env.fx.tree_v2, opts(jobs, false));
                    stats.stored as u64 + stats.deduped as u64 + res.unwrap().as_bytes()[0] as u64
                }),
            )
            .bytes(env.fx.tree_bytes)
            .per_rep(Box::new(|_, s| {
                s.downcast::<StoreHandle>().unwrap().close();
            })),
        );
    }
    out.push(
        Case::new(
            "ingest",
            "ingest.scan",
            "tree/no-ignore/jobs-1",
            1,
            1,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let (files, bytes) = ingest::scan(&env.fx.tree_v1, true, 1).unwrap();
                files + bytes
            }),
        )
        .bytes(env.fx.tree_bytes),
    );
    out.push(
        Case::new(
            "ingest",
            "ingest.dir",
            "single-file/jobs-1",
            1,
            1,
            Box::new(|env| Box::new(fresh_store(env, "ingest-file")) as State),
            Box::new(|env, s| {
                let h = s.downcast_ref::<StoreHandle>().unwrap();
                let (_, res) =
                    ingest::dir(h.store(), env.fx.tree_v1.join("big.bin"), opts(1, false));
                res.unwrap().as_bytes()[0] as u64
            }),
        )
        .bytes(env.profile.corpus_bytes / 8)
        .per_rep(Box::new(|_, s| {
            s.downcast::<StoreHandle>().unwrap().close();
        })),
    );
    out
}

pub fn ingest_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let p = &env.profile;

    let filtered = ingest::scan(&fx.tree_v1, false, 1);
    rec.want(
        "ingest",
        "ingest.scan",
        "ingest/scan-filtered",
        matches!(&filtered, Ok((f, b)) if *f > 0 && *b > 0),
        format!("scan failed: {filtered:?}"),
        filtered
            .as_ref()
            .map(|(f, b)| format!("{f}/{b}"))
            .unwrap_or_default(),
    );
    let (files, bytes_total) = filtered.unwrap();
    let unfiltered = ingest::scan(&fx.tree_v1, true, 1);
    rec.want(
        "ingest",
        "ingest.scan",
        "ingest/scan-unfiltered",
        matches!(&unfiltered, Ok((f, b)) if *f > files && *b > bytes_total),
        format!(
            "ignoring .amberignore must see more files: {unfiltered:?} vs {files}/{bytes_total}"
        ),
        unfiltered
            .as_ref()
            .map(|(f, b)| format!("{f}/{b}"))
            .unwrap_or_default(),
    );
    let parallel = ingest::scan(&fx.tree_v1, false, p.threads_multi);
    rec.want(
        "ingest",
        "ingest.scan",
        "ingest/scan-jobs-invariant",
        matches!(&parallel, Ok((f, b)) if *f == files && *b == bytes_total),
        format!("jobs changed the scan totals: {parallel:?} vs {files}/{bytes_total}"),
        parallel
            .as_ref()
            .map(|(f, b)| format!("{f}/{b}"))
            .unwrap_or_default(),
    );

    let mem = MemStore::new();
    let (stream, root) = match ingest::objects(&fx.tree_v1, opts(1, false)) {
        Ok(v) => v,
        Err(e) => {
            rec.fail("ingest", "ingest.objects", "ingest/objects", e.to_string());
            return;
        }
    };
    let mut streamed = 0usize;
    for o in stream {
        match o {
            Ok(o) => {
                streamed += 1;
                mem.put(o).unwrap();
            }
            Err(e) => {
                rec.fail("ingest", "ingest.objects", "ingest/objects", e.to_string());
                return;
            }
        }
    }
    let root1 = root.get().unwrap();
    rec.pass(
        "ingest",
        "ingest.objects",
        "ingest/objects-root",
        root1.to_string(),
    );

    let (stream2, root2) = ingest::objects(&fx.tree_v1, opts(p.threads_multi, false)).unwrap();
    let n2 = stream2.inspect(|o| assert!(o.is_ok())).count();
    rec.want(
        "ingest",
        "ingest.objects",
        "ingest/objects-jobs-invariant",
        root2.get() == Some(root1) && n2 == streamed,
        format!(
            "worker count changed the build: root {:?} vs {root1}, {n2} vs {streamed} objects",
            root2.get()
        ),
        root2.get().map(|k| k.to_string()).unwrap_or_default(),
    );

    let vis: Result<Vec<Key>, _> = fstree::check_complete(root1, |k| mem.get(k), |k| mem.has(k), 1);
    rec.want(
        "ingest",
        "ingest.objects",
        "ingest/objects-complete",
        vis.is_ok(),
        format!(
            "the streamed object set is not self-contained: {:?}",
            vis.as_ref().err()
        ),
        format!("n={}", vis.as_ref().map(|v| v.len()).unwrap_or(0)),
    );
    let ent: Result<Option<Entry>, _> =
        fstree::resolve_entry(root1, "data/d000/f000.bin", |k| mem.get(k));
    rec.want(
        "ingest",
        "ingest.objects",
        "ingest/objects-resolves-file",
        matches!(&ent, Ok(Some(e)) if e.mode & 0o170000 == 0o100000),
        format!("resolving an ingested file: {ent:?}"),
        "ok",
    );
    let build: Result<Option<Entry>, _> = fstree::resolve_entry(root1, "build", |k| mem.get(k));
    rec.want(
        "ingest",
        "ingest.objects",
        "ingest/objects-honours-ignore",
        build.is_err(),
        "the ignored build/ directory was ingested",
        "absent",
    );
    let keep: Result<Option<Entry>, _> =
        fstree::resolve_entry(root1, "data/d000/keep.tmp", |k| mem.get(k));
    rec.want(
        "ingest",
        "ingest.objects",
        "ingest/objects-honours-negation",
        matches!(&keep, Ok(Some(_))),
        format!("the negated keep.tmp was excluded: {keep:?}"),
        "present",
    );

    let dir = work_dir(env, "check-ingest-dir");
    let st = packstore::Store::open_with(&dir, store_options(p)).unwrap();
    let (stats, res) = ingest::dir(&st, &fx.tree_v1, opts(p.threads_multi, false));
    rec.want(
        "ingest",
        "ingest.dir",
        "ingest/dir-root",
        matches!(&res, Ok(k) if *k == root1),
        format!("ingest::dir root {res:?} differs from the stream root {root1}"),
        res.as_ref().map(|k| k.to_string()).unwrap_or_default(),
    );
    let (stats2, res2) = ingest::dir(&st, &fx.tree_v1, opts(p.threads_multi, false));
    rec.want(
        "ingest",
        "ingest.dir",
        "ingest/dir-dedups",
        res2.is_ok() && stats2.stored == 0 && stats2.deduped == stats.stored + stats.deduped,
        format!(
            "a repeat ingest stored {} objects (first run stored {})",
            stats2.stored, stats.stored
        ),
        format!("stored={} deduped={}", stats2.stored, stats2.deduped),
    );
    let (stats3, res3) = ingest::dir(&st, &fx.tree_v2, opts(p.threads_multi, false));
    rec.want(
        "ingest",
        "ingest.dir",
        "ingest/dir-incremental",
        res3.is_ok() && stats3.stored > 0 && stats3.stored < stats.stored,
        format!(
            "incremental ingest stored {} of the original {} objects",
            stats3.stored, stats.stored
        ),
        format!("stored={}", stats3.stored),
    );
    st.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}
