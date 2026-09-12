//! amberignore and ingest.

use std::fs;

use amber_store_core::amberignore::{self, Matcher};
use amber_store_core::fstree::{self, Entry};
use amber_store_core::ingest;
use amber_store_core::key::Key;
use amber_store_core::packstore;

use crate::env::Env;
use crate::fixtures::{
    MemStore, TreeCounts, digest_strings, first_difference, fold_bool, fold_i64, fold_key,
    fold_u64, manifest_lines, new_fold,
};
use crate::fixtures_build::store_options;
use crate::harness::{Case, Dims, Recorder, State, bytes_kind};
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
                let mut acc = new_fold();
                for _ in 0..64 {
                    let m = Matcher::root(&env.fx.ignore_dir).unwrap();
                    // Probe the loaded matcher on both sides of each rule, so
                    // the whole pattern set has to have been parsed.
                    acc = fold_bool(acc, m.ignored(b"x.o", false));
                    acc = fold_bool(acc, m.ignored(b"keep.tmp", false));
                    acc = fold_bool(acc, m.ignored(b"vendor", true));
                    std::hint::black_box(&m);
                }
                acc
            }),
        )
        .dims(Dims {
            entries: 64,
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
        Case::new(
            "amberignore",
            "amberignore.descend",
            "8-subdirs",
            1,
            64 * 8,
            Box::new(|env| Box::new(Matcher::root(&env.fx.ignore_dir).unwrap()) as State),
            Box::new(|env, s| {
                let m = s.downcast_ref::<Matcher>().unwrap();
                let mut acc = new_fold();
                for _ in 0..64 {
                    for d in 0..8 {
                        let name = format!("sub{d:02}");
                        let sub = m
                            .descend(env.fx.ignore_dir.join(&name), name.as_bytes())
                            .unwrap();
                        acc = fold_bool(acc, sub.ignored(format!("gen{d:02}-a").as_bytes(), false));
                        acc = fold_bool(
                            acc,
                            sub.ignored(format!("gen{d:02}-keep").as_bytes(), false),
                        );
                        std::hint::black_box(&sub);
                    }
                }
                acc
            }),
        )
        .dims(Dims {
            entries: 64 * 8,
            depth: 1,
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
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
                let mut acc = new_fold();
                for nm in names {
                    acc = fold_bool(acc, m.ignored(nm, false));
                }
                acc
            }),
        )
        .dims(Dims {
            entries: n as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
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
                let mut acc = new_fold();
                for nm in names {
                    acc = fold_bool(acc, amberignore::ignored_opt(None, nm, false));
                }
                acc
            }),
        )
        .dims(Dims {
            entries: n as i64,
            content: "structured".into(),
            ..Default::default()
        })
        .cross(),
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
    let fx = &env.fx;
    let mut out = Vec::new();

    // Dimensions of an on-disk tree case. The counts come from the fixture's
    // scan, taken outside every measured interval.
    let tree_dims = |c: TreeCounts| Dims {
        files: c.files,
        entries: c.files,
        content: "tree".into(),
        ..Default::default()
    };

    for jobs in [p.threads_single, p.threads_multi] {
        // Scan walks directory entries and stats inodes; it never opens a
        // file body. Its byte figure is therefore the logical size of the
        // tree it covered, not bandwidth, and is labelled as such.
        out.push(
            Case::new(
                "ingest",
                "ingest.scan",
                &format!("filtered/jobs-{jobs}"),
                jobs,
                fx.v1_included.files as usize,
                Box::new(|_| Box::new(()) as State),
                Box::new(move |env, _| {
                    let (files, bytes) = ingest::scan(&env.fx.tree_v1, false, jobs).unwrap();
                    fold_u64(fold_u64(new_fold(), files), bytes)
                }),
            )
            .bytes_of(fx.v1_included.bytes, bytes_kind::LOGICAL_SCANNED)
            .dims(tree_dims(fx.v1_included))
            .cross(),
        );
        out.push(
            Case::new(
                "ingest",
                "ingest.objects",
                &format!("tree/jobs-{jobs}"),
                jobs,
                fx.v1_included.files as usize,
                Box::new(|_| Box::new(()) as State),
                Box::new(move |env, _| {
                    let (stream, root) =
                        ingest::objects(&env.fx.tree_v1, opts(jobs, false)).unwrap();
                    let mut n = 0u64;
                    let mut count = 0u64;
                    for o in stream {
                        n += o.unwrap().bytes.len() as u64;
                        count += 1;
                    }
                    // The root key is the whole tree's identity: folding it
                    // whole is 32 bytes, and it is the same in both cores.
                    fold_key(
                        fold_u64(fold_u64(new_fold(), count), n),
                        &root.get().unwrap(),
                    )
                }),
            )
            .bytes_of(fx.v1_included.bytes, bytes_kind::INCLUDED)
            .dims(tree_dims(fx.v1_included))
            .cross(),
        );
        out.push(
            Case::new(
                "ingest",
                "ingest.dir",
                &format!("tree/fresh-store/jobs-{jobs}"),
                jobs,
                fx.v1_included.files as usize,
                Box::new(|env| Box::new(fresh_store(env, "ingest-dir")) as State),
                Box::new(move |env, s| {
                    let h = s.downcast_ref::<StoreHandle>().unwrap();
                    let (stats, res) = ingest::dir(h.store(), &env.fx.tree_v1, opts(jobs, false));
                    fold_key(
                        fold_i64(
                            fold_i64(new_fold(), stats.stored as i64),
                            stats.deduped as i64,
                        ),
                        &res.unwrap(),
                    )
                }),
            )
            .bytes_of(fx.v1_included.bytes, bytes_kind::INCLUDED)
            .dims(tree_dims(fx.v1_included))
            .cross()
            .per_rep(Box::new(|_, s| {
                s.downcast::<StoreHandle>().unwrap().close();
            })),
        );
        // The successor tree covers different files and different bytes, so
        // it gets its own denominator rather than the first tree's.
        out.push(
            Case::new(
                "ingest",
                "ingest.dir",
                &format!("tree/incremental-change/jobs-{jobs}"),
                jobs,
                fx.v2_included.files as usize,
                Box::new(|env| {
                    Box::new(copied_store(env, &env.fx.ingest_template, "ingest-inc")) as State
                }),
                Box::new(move |env, s| {
                    let h = s.downcast_ref::<StoreHandle>().unwrap();
                    let (stats, res) = ingest::dir(h.store(), &env.fx.tree_v2, opts(jobs, false));
                    fold_key(
                        fold_i64(
                            fold_i64(new_fold(), stats.stored as i64),
                            stats.deduped as i64,
                        ),
                        &res.unwrap(),
                    )
                }),
            )
            .bytes_of(fx.v2_included.bytes, bytes_kind::INCLUDED)
            .dims(tree_dims(fx.v2_included))
            .cross()
            .per_rep(Box::new(|_, s| {
                s.downcast::<StoreHandle>().unwrap().close();
            })),
        );
        // Re-ingesting an unchanged tree is the common case in practice and
        // the pure dedup path: the same files are walked and chunked, and
        // nothing at all is stored.
        out.push(
            Case::new(
                "ingest",
                "ingest.dir",
                &format!("tree/unchanged-repeat/jobs-{jobs}"),
                jobs,
                fx.v1_included.files as usize,
                Box::new(|env| {
                    Box::new(copied_store(env, &env.fx.ingest_template, "ingest-same")) as State
                }),
                Box::new(move |env, s| {
                    let h = s.downcast_ref::<StoreHandle>().unwrap();
                    let (stats, res) = ingest::dir(h.store(), &env.fx.tree_v1, opts(jobs, false));
                    assert_eq!(stats.stored, 0, "unchanged repeat stored objects");
                    fold_key(
                        fold_i64(
                            fold_i64(new_fold(), stats.stored as i64),
                            stats.deduped as i64,
                        ),
                        &res.unwrap(),
                    )
                }),
            )
            .bytes_of(fx.v1_included.bytes, bytes_kind::INCLUDED)
            .dims(tree_dims(fx.v1_included))
            .cross()
            .per_rep(Box::new(|_, s| {
                s.downcast::<StoreHandle>().unwrap().close();
            })),
        );
    }
    // The same walk with the ignore rules disabled covers strictly more
    // files and more bytes, and says so in its own denominator.
    out.push(
        Case::new(
            "ingest",
            "ingest.scan",
            "unfiltered/jobs-1",
            1,
            fx.v1_unfiltered.files as usize,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let (files, bytes) = ingest::scan(&env.fx.tree_v1, true, 1).unwrap();
                fold_u64(fold_u64(new_fold(), files), bytes)
            }),
        )
        .bytes_of(fx.v1_unfiltered.bytes, bytes_kind::LOGICAL_SCANNED)
        .dims(tree_dims(fx.v1_unfiltered))
        .cross(),
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
                let (stats, res) =
                    ingest::dir(h.store(), env.fx.tree_v1.join("big.bin"), opts(1, false));
                fold_key(fold_i64(new_fold(), stats.stored as i64), &res.unwrap())
            }),
        )
        .bytes_of(env.profile.corpus_bytes / 8, bytes_kind::INCLUDED)
        .dims(Dims {
            files: 1,
            item_bytes: env.profile.corpus_bytes / 8,
            content: "text".into(),
            ..Default::default()
        })
        .cross()
        .per_rep(Box::new(|_, s| {
            s.downcast::<StoreHandle>().unwrap().close();
        })),
    );
    out
}

pub fn ingest_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let p = &env.profile;

    // The counts the rate denominators use were taken at fixture-build time;
    // assert here that they are what they claim to be, and record them as
    // comparable digests so a difference between the two cores' denominators
    // fails the run instead of silently skewing a rate.
    rec.pass(
        "ingest",
        "ingest.scan",
        "ingest/scan-filtered",
        format!("{}/{}", fx.v1_included.files, fx.v1_included.bytes),
    );
    rec.want(
        "ingest",
        "ingest.scan",
        "ingest/scan-unfiltered",
        fx.v1_unfiltered.files > fx.v1_included.files
            && fx.v1_unfiltered.bytes > fx.v1_included.bytes,
        format!(
            "ignoring .amberignore must see more files: {}/{} vs {}/{}",
            fx.v1_unfiltered.files,
            fx.v1_unfiltered.bytes,
            fx.v1_included.files,
            fx.v1_included.bytes
        ),
        format!("{}/{}", fx.v1_unfiltered.files, fx.v1_unfiltered.bytes),
    );
    rec.pass(
        "ingest",
        "ingest.scan",
        "ingest/scan-v2-included",
        format!("{}/{}", fx.v2_included.files, fx.v2_included.bytes),
    );
    let parallel = ingest::scan(&fx.tree_v1, false, p.threads_multi);
    rec.want(
        "ingest",
        "ingest.scan",
        "ingest/scan-jobs-invariant",
        matches!(&parallel, Ok((f, b)) if *f as i64 == fx.v1_included.files && *b as i64 == fx.v1_included.bytes),
        format!(
            "jobs changed the scan totals: {parallel:?} vs {}/{}",
            fx.v1_included.files, fx.v1_included.bytes
        ),
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
    // The whole restored tree, not a spot check. The archive is exported
    // from the ingested root, extracted into an empty directory, and the
    // complete listing of what comes out is compared against the listing the
    // harness derived from the source tree and the fixture's own ignore
    // rules. Nothing in this comparison is taken from either core.
    let restored = work_dir(env, "check-restore");
    let mut tar_buf: Vec<u8> = Vec::new();
    let exported = amber_store_core::tarexport::write(&mut tar_buf, root1, |k| mem.get(k));
    if let Err(e) = exported {
        rec.fail(
            "ingest",
            "ingest.objects",
            "ingest/restored-tree-matches-source",
            e.to_string(),
        );
    } else if let Err(e) = amber_store_core::tarextract::extract(&mut &tar_buf[..], &restored) {
        rec.fail(
            "ingest",
            "ingest.objects",
            "ingest/restored-tree-matches-source",
            e.to_string(),
        );
    } else {
        let got = manifest_lines(&restored, None);
        let want = &fx.v1_included_manifest;
        let ok = matches!(&got, Ok(g) if !want.is_empty() && g == want);
        let empty: Vec<String> = Vec::new();
        let got_lines = got.as_ref().unwrap_or(&empty);
        rec.want(
            "ingest",
            "ingest.objects",
            "ingest/restored-tree-matches-source",
            ok,
            format!(
                "the restored tree is not the included source tree: {:?}; {}",
                got.as_ref().err(),
                first_difference(want, got_lines)
            ),
            digest_strings(got_lines),
        );
        // And the expectation itself is a comparable statement, so the two
        // cores are shown to have started from the same source tree.
        rec.pass(
            "ingest",
            "ingest.objects",
            "ingest/included-source-manifest",
            digest_strings(want),
        );
    }
    let _ = fs::remove_dir_all(&restored);

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
    // This is the unchanged-repeat workload's contract: the same tree,
    // ingested again, stores nothing at all.
    let (stats2, res2) = ingest::dir(&st, &fx.tree_v1, opts(p.threads_multi, false));
    rec.want(
        "ingest",
        "ingest.dir",
        "ingest/dir-unchanged-repeat-stores-nothing",
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
        res3.is_ok()
            && stats3.stored > 0
            && stats3.stored < stats.stored
            && res3.as_ref().ok() != Some(&root1),
        format!(
            "incremental ingest stored {} of the original {} objects",
            stats3.stored, stats.stored
        ),
        format!("stored={}", stats3.stored),
    );
    rec.pass(
        "ingest",
        "ingest.dir",
        "ingest/dir-v2-root",
        res3.as_ref().map(|k| k.to_string()).unwrap_or_default(),
    );
    st.close().unwrap();
    let _ = fs::remove_dir_all(&dir);
}
