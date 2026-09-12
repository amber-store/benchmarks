//! The Nix-store workload.
//!
//! A deterministic fixture closure is built once into the host store; every
//! repetition then imports it into a store created for that repetition alone
//! (`nix --store <dir>`), so nothing measured here touches `/nix/store`.
//!
//! What is compared is the *storage layer*: importing a closure, importing a
//! second generation that shares most of it, enumerating what is held,
//! reading it all back, materialising it somewhere else, dropping a path and
//! collecting. A Nix store additionally enforces a reference graph,
//! immutability, signatures and a build model; the Amber cores implement none
//! of that, and the operations that depend on it are recorded as unsupported
//! rather than scored. Nothing here supports a claim that a tree store can
//! replace a Nix store.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::adapters::amber::AmberCli;
use crate::adapters::nixstore::{self, ClientCache, NixCli};
use crate::adapters::semantics;
use crate::dataset::manifest::{Manifest, Strictness};
use crate::metrics::{Op, OpStatus, Phase, RunRecord, Verification};
use crate::util::proc::Run;

use super::{
    Ctx, NixFixture, Recorder, aside, measured, measured_many, reclaimed, store_counters,
    store_sizes,
};

/// Scenario group name.
pub const GROUP: &str = "nix";
/// Scenario name.
pub const SCENARIO: &str = "nix/closure";
/// Every backend this scenario can run.
pub const BACKENDS: [&str; 3] = ["nix", "amber-rust", "amber-go"];

enum Kind {
    Nix {
        cli: NixCli,
        /// A second isolated store, the destination of the materialisation
        /// and substitution measurements.
        second: NixCli,
        /// Local binary cache directory.
        cache: PathBuf,
    },
    Amber(AmberCli),
}

struct Backend {
    kind: Kind,
    store: PathBuf,
}

/// Runs the workload for one backend.
pub fn run(ctx: &Ctx, backend: &str, rep: usize, order_index: usize) -> RunRecord {
    let mut rec = Recorder::new(
        GROUP,
        SCENARIO,
        backend,
        rep,
        order_index,
        semantics(backend),
    );
    let Some(fixture) = ctx.nix.as_ref() else {
        return rec.abort("the Nix fixture closure was not built; see the setup errors");
    };
    let dir = match ctx.prepare_run_dir(GROUP, backend, rep) {
        Ok(d) => d,
        Err(e) => return rec.abort(e),
    };
    let be = match build(ctx, backend, &dir) {
        Ok(b) => b,
        Err(e) => return rec.abort(e),
    };
    rec.note(format!(
        "closure source: {}. Generation 1 has {} paths and generation 2 has \
         {}, sharing {} of them",
        fixture.origin.describe(),
        fixture.gen1_closure.len(),
        fixture.gen2_closure.len(),
        fixture
            .gen1_closure
            .iter()
            .filter(|p| fixture.gen2_closure.contains(p))
            .count()
    ));
    if fixture.origin == super::NixOrigin::Fixture {
        rec.note(
            "the fixture contains executables, relative and absolute \
             symlinks, a dangling symlink and read-only directories"
                .to_string(),
        );
    }

    let gen1_bytes = closure_bytes(&fixture.gen1_closure);
    let gen2_bytes = closure_bytes(&fixture.gen2_closure);

    let op = be
        .import(fixture, 1)
        .logical(gen1_bytes)
        .store(store_sizes(&be.store))
        .counters(store_counters(&be.store))
        .describe("import generation 1's closure into an empty store");
    let failed = op.status == OpStatus::Failed;
    rec.push(op);
    if failed {
        return rec.abort(format!("{backend}: importing the closure failed"));
    }

    rec.push(
        be.import_named(fixture, 1, "import_repeat")
            .logical(gen1_bytes)
            .store(store_sizes(&be.store))
            .counters(store_counters(&be.store))
            .describe("import the identical closure again"),
    );
    rec.push(if fixture.origin.has_churn() {
        be.import_named(fixture, 2, "import_incremental")
            .logical(gen2_bytes)
            .store(store_sizes(&be.store))
            .counters(store_counters(&be.store))
            .describe("import generation 2, which shares most of generation 1")
    } else {
        Op::unsupported("import_incremental", fixture.origin.no_churn_reason())
            .describe("import a second generation that shares most of the first")
    });

    rec.push(be.catalog_list());
    rec.push(be.closure_query(fixture));
    rec.push(be.nar_registration());
    rec.push(
        be.read_closure(fixture)
            .logical(gen2_bytes)
            .describe("read every byte of the retained closure back"),
    );
    rec.push(be.cache_export(fixture));

    let dest = dir.join("materialised");
    let _ = std::fs::create_dir_all(&dest);
    let op = be
        .materialize(fixture, &dest)
        .logical(gen2_bytes)
        .describe("materialise generation 2's closure somewhere else");
    let ok = op.status == OpStatus::Ok;
    rec.push(op);
    for v in verify_closure(ctx, &be, fixture, &dest, ok) {
        rec.verify(v);
    }
    rec.push(be.cache_substitute(fixture));

    let before = store_sizes(&be.store);
    rec.push(
        be.delete_path(fixture)
            .store(store_sizes(&be.store))
            .counters(store_counters(&be.store)),
    );
    rec.push(be.register_gc_root(fixture));
    if let Kind::Amber(cli) = &be.kind {
        rec.push(super::age_segments(cli));
    }
    let before_gc = store_sizes(&be.store);
    let packs_before = store_counters(&be.store);
    let mut gc = be.gc();
    let after = store_sizes(&be.store);
    let packs_after = store_counters(&be.store);
    gc = gc
        .store(after)
        .reclaimed(reclaimed(&before_gc, &after))
        .counters(packs_after.clone())
        .counter("allocated_before_delete", before.allocated_bytes as i64);
    if let (Some(b), Some(a)) = (
        packs_before.get("store_packstore_allocated_bytes"),
        packs_after.get("store_packstore_allocated_bytes"),
    ) {
        gc = gc.counter("reclaimed_packstore_bytes", b - a);
    }
    rec.push(gc);

    // What survived collection must still come back.
    let after_dir = dir.join("materialised-after-gc");
    let _ = std::fs::create_dir_all(&after_dir);
    let op = be.materialize(fixture, &after_dir);
    let ok = op.status == OpStatus::Ok;
    let mut op = op.describe("materialise the retained closure after collection");
    op.phase = Phase::Verify;
    op.name = "post_gc_materialize".into();
    rec.push(op);
    let mut post = verify_closure(ctx, &be, fixture, &after_dir, ok);
    for v in &mut post {
        v.name = format!("post_gc_{}", v.name);
    }
    for v in post {
        rec.verify(v);
    }

    for cmd in be.integrity(fixture) {
        rec.push(aside(
            "integrity_check",
            Phase::Verify,
            "the backend's own consistency check",
            &cmd,
        ));
    }
    rec.finish()
}

fn closure_bytes(paths: &[String]) -> u64 {
    paths
        .iter()
        .map(|p| {
            crate::util::fsx::measure_dir(Path::new(p))
                .map(|s| s.apparent_bytes)
                .unwrap_or(0)
        })
        .sum()
}

/// Everything a materialised closure has to satisfy.
fn verify_closure(
    ctx: &Ctx,
    be: &Backend,
    fixture: &NixFixture,
    dest: &Path,
    materialized: bool,
) -> Vec<Verification> {
    let mut out = Vec::new();
    if !materialized {
        out.push(Verification::fail(
            "closure_matches_source",
            "materialising the closure failed, so there was nothing to verify",
        ));
        return out;
    }
    // 1. Every path of the closure must be present, with the same bytes,
    //    modes and symlink targets as the source store path — and nothing
    //    else may be present either. The expected set and the observed set
    //    are compared in both directions, so a path that never arrived
    //    cannot pass by simply not being iterated over.
    let mut missing = Vec::new();
    let mut corrupt = Vec::new();
    let expected: std::collections::BTreeSet<String> =
        fixture.gen2_closure.iter().map(|p| base_name(p)).collect();
    let observed = be.observed_names(dest);
    let extra: Vec<String> = match &observed {
        Ok(names) => names.difference(&expected).cloned().collect(),
        Err(e) => {
            corrupt.push(format!("cannot enumerate the destination: {e}"));
            Vec::new()
        }
    };
    if let Ok(names) = &observed {
        for want in &expected {
            if !names.contains(want) {
                missing.push(want.clone());
            }
        }
    }
    for path in &fixture.gen2_closure {
        let name = Path::new(path).file_name().unwrap_or_default();
        let here = be.materialized_path(dest, path);
        if !here.exists() {
            if !missing.contains(&name.to_string_lossy().into_owned()) {
                missing.push(name.to_string_lossy().into_owned());
            }
            continue;
        }
        let want = match Manifest::observe("source", Path::new(path)) {
            Ok(m) => m,
            Err(e) => {
                corrupt.push(format!("{path}: source unreadable: {e}"));
                continue;
            }
        };
        let v = want.verify_tree("path", &here, Strictness::full());
        if !v.passed {
            corrupt.push(format!("{path}: {}", v.detail));
            corrupt.extend(v.corrupt.into_iter().take(3));
            corrupt.extend(
                v.missing
                    .into_iter()
                    .take(3)
                    .map(|m| format!("missing {m}")),
            );
            corrupt.extend(v.extra.into_iter().take(3).map(|m| format!("extra {m}")));
        }
    }
    let mut v = if missing.is_empty() && corrupt.is_empty() && extra.is_empty() {
        Verification::pass(
            "closure_matches_source",
            format!(
                "all {} store paths of generation 2 came back with identical \
                 bytes, permissions, symlink targets and timestamps, and the \
                 destination holds nothing else",
                fixture.gen2_closure.len()
            ),
        )
    } else {
        Verification::fail(
            "closure_matches_source",
            format!(
                "{} of {} expected store paths missing, {} differing, {} \
                 unexpected paths in the destination",
                missing.len(),
                fixture.gen2_closure.len(),
                corrupt.len(),
                extra.len(),
            ),
        )
    };
    v.missing = missing;
    v.corrupt = corrupt;
    v.extra = extra;
    out.push(v);

    // 2. The closure must be *complete*: every path referenced by a retained
    //    path must itself be retained. Nix enforces this; the Amber cores do
    //    not model references at all, so the harness checks the recorded
    //    graph on their behalf and says so.
    let present: std::collections::BTreeSet<&String> = fixture.gen2_closure.iter().collect();
    let mut dangling = Vec::new();
    for path in &fixture.gen2_closure {
        if let Some((_, refs)) = fixture.details.get(path) {
            for r in refs {
                if !present.contains(r) {
                    dangling.push(format!("{path} -> {r}"));
                }
            }
        }
    }
    let enforced = matches!(be.kind, Kind::Nix { .. });
    let mut v = if dangling.is_empty() {
        Verification::pass(
            "closure_is_complete",
            if enforced {
                "every reference of every retained path is itself retained; \
                 Nix enforces this itself"
                    .to_string()
            } else {
                "every reference of every retained path is itself retained. \
                 These cores store trees and have no notion of references \
                 between roots, so this was checked by the harness against the \
                 graph recorded from the source store, not by the store under \
                 test."
                    .to_string()
            },
        )
    } else {
        Verification::fail(
            "closure_is_complete",
            format!(
                "{} references point outside the retained set",
                dangling.len()
            ),
        )
    };
    v.missing = dangling;
    out.push(v);

    // 3. NAR hashes.
    //
    //    Nix registers them, so for Nix this reads the destination store's
    //    own registration and compares the complete expected path set
    //    against it. Neither Amber core registers anything of the kind — but
    //    that is no reason to skip the check, because a NAR hash is a
    //    function of the bytes, the executable bits and the symlink targets
    //    alone. So for a tree store the harness recomputes it from the
    //    restored path with `nix hash path`, which needs no Nix support in
    //    the store under test, and compares that with the hash the *source*
    //    store recorded. The missing registration is reported separately, as
    //    an unsupported capability, not folded into a passing hash check.
    match &be.kind {
        Kind::Nix { cli, .. } => {
            // Ask the store the closure was just materialised into, not
            // the one it came from.
            let materialised = NixCli {
                bin: cli.bin.clone(),
                store: Some(dest.join("store")),
                timeout: cli.timeout,
                cache: ClientCache::Inherited,
            };
            let mut bad = Vec::new();
            match materialised.path_info_closure(&fixture.gen2).run() {
                Ok(output) if output.success() => {
                    match nixstore::parse_closure_details(&output.stdout_text()) {
                        Ok(got) => {
                            // Every expected path, not only every observed
                            // one: iterating the destination alone would let
                            // an absent path pass.
                            for path in &fixture.gen2_closure {
                                let Some((want_hash, want_refs)) = fixture.details.get(path) else {
                                    bad.push(format!("{path}: not recorded in the source closure"));
                                    continue;
                                };
                                match got.get(path) {
                                    Some((hash, refs)) => {
                                        if want_hash != hash {
                                            bad.push(format!(
                                                "{path}: NAR hash {hash} != {want_hash}"
                                            ));
                                        }
                                        if want_refs != refs {
                                            bad.push(format!("{path}: references differ"));
                                        }
                                    }
                                    None => bad.push(format!(
                                        "{path}: not registered in the destination store"
                                    )),
                                }
                            }
                            for path in got.keys() {
                                if !fixture.details.contains_key(path) {
                                    bad.push(format!("{path}: not in the source closure"));
                                }
                            }
                        }
                        Err(e) => bad.push(e),
                    }
                }
                Ok(output) => bad.push(output.require_success().unwrap_err()),
                Err(e) => bad.push(e),
            }
            let mut v = if bad.is_empty() {
                Verification::pass(
                    "nar_hashes_and_references_match",
                    format!(
                        "all {} paths of the closure are registered in the \
                         destination store with the source store's NAR hash \
                         and reference list",
                        fixture.gen2_closure.len()
                    ),
                )
            } else {
                Verification::fail(
                    "nar_hashes_and_references_match",
                    format!("{} paths disagreed with the source store", bad.len()),
                )
            };
            v.corrupt = bad;
            out.push(v);
        }
        Kind::Amber(_) => {
            out.push(recomputed_nar_hashes(ctx, be, fixture, dest));
        }
    }
    out
}

/// Recomputes the NAR hash of every restored store path with `nix hash path`
/// and compares it with the hash the source store recorded.
///
/// This is deliberately independent of the store under test: `nix hash path`
/// reads the bytes on disk and needs no store at all, so a tree store that
/// knows nothing about Nix is still held to Nix's own integrity evidence.
fn recomputed_nar_hashes(
    ctx: &Ctx,
    be: &Backend,
    fixture: &NixFixture,
    dest: &Path,
) -> Verification {
    const NAME: &str = "nar_hashes_match_recomputed";
    let bin = match ctx.tools.path("nix") {
        Ok(b) => b,
        Err(e) => {
            return Verification::fail(
                NAME,
                format!("nix is needed to recompute NAR hashes and is unavailable: {e}"),
            );
        }
    };
    let hasher = NixCli {
        bin,
        store: None,
        timeout: ctx.timeout,
        cache: ClientCache::Inherited,
    };
    let mut bad = Vec::new();
    let mut checked = 0usize;
    for path in &fixture.gen2_closure {
        let Some((want_hash, _)) = fixture.details.get(path) else {
            bad.push(format!("{path}: not recorded in the source closure"));
            continue;
        };
        let here = be.materialized_path(dest, path);
        let got = match hasher.hash_path(&here).run() {
            Ok(o) if o.success() => nixstore::parse_hash_path(&o.stdout_text()),
            Ok(o) => Err(o.require_success().unwrap_err()),
            Err(e) => Err(e),
        };
        match got {
            Ok(hash) if &hash == want_hash => checked += 1,
            Ok(hash) => bad.push(format!(
                "{path}: recomputed NAR hash {hash} != source {want_hash}"
            )),
            Err(e) => bad.push(format!("{path}: {e}")),
        }
    }
    let mut v = if bad.is_empty() && checked == fixture.gen2_closure.len() {
        Verification::pass(
            NAME,
            format!(
                "the NAR hash recomputed with `nix hash path` over every one \
                 of the {checked} restored store paths equals the hash the \
                 source Nix store recorded. These cores register no NAR \
                 hashes themselves — see the unsupported \
                 nar_and_reference_registration operation — so this was \
                 recomputed from the restored bytes."
            ),
        )
    } else {
        Verification::fail(
            NAME,
            format!(
                "{} of {} restored store paths did not reproduce the source \
                 store's NAR hash",
                bad.len(),
                fixture.gen2_closure.len()
            ),
        )
    };
    v.corrupt = bad;
    v
}

/// The final component of a store path, which is how a materialised path is
/// named in every destination.
fn base_name(store_path: &str) -> String {
    Path::new(store_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn build(ctx: &Ctx, backend: &str, dir: &Path) -> Result<Backend, String> {
    match backend {
        "nix" => {
            let bin = ctx.tools.path("nix")?;
            Ok(Backend {
                store: dir.join("store"),
                kind: Kind::Nix {
                    cli: NixCli {
                        bin: bin.clone(),
                        store: Some(dir.join("store")),
                        timeout: ctx.timeout,
                        cache: ClientCache::Inherited,
                    },
                    // The substituting client is cold in both senses: an
                    // empty store and an empty narinfo cache of its own.
                    // Exporting to the cache fills the environment's narinfo
                    // cache for that URI, and substituting with that still
                    // warm would be measuring a client that had already
                    // asked.
                    second: NixCli {
                        bin,
                        store: Some(dir.join("store2")),
                        timeout: ctx.timeout,
                        cache: ClientCache::Private(dir.join("nix-cache-substituter")),
                    },
                    cache: dir.join("binary-cache"),
                },
            })
        }
        "amber-rust" | "amber-go" => Ok(Backend {
            store: dir.join("store"),
            kind: Kind::Amber(AmberCli {
                bin: ctx.tools.path(backend)?,
                store: dir.join("store"),
                segment_size: ctx.profile.segment_size,
                chunk: ctx.profile.chunk,
                jobs: ctx.jobs,
                timeout: ctx.timeout,
            }),
        }),
        other => Err(format!("nix scenario: unknown backend {other}")),
    }
}

/// The reference name an Amber core stores one store path under.
fn ref_for(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("nixpath-{name}")
}

impl Backend {
    fn closure<'a>(
        &self,
        fixture: &'a NixFixture,
        generation: u8,
    ) -> (&'a Vec<String>, &'a String) {
        if generation == 1 {
            (&fixture.gen1_closure, &fixture.gen1)
        } else {
            (&fixture.gen2_closure, &fixture.gen2)
        }
    }

    fn import(&self, fixture: &NixFixture, generation: u8) -> Op {
        self.import_named(fixture, generation, "import_initial")
    }

    fn import_named(&self, fixture: &NixFixture, generation: u8, name: &str) -> Op {
        let (paths, top) = self.closure(fixture, generation);
        match &self.kind {
            Kind::Nix { cli, .. } => measured(
                name,
                "copy a closure into the isolated store",
                &cli.copy(
                    "daemon",
                    &nixstore::local_store_uri(self.store.as_path()),
                    std::slice::from_ref(top),
                ),
            ),
            Kind::Amber(amber) => {
                let cmds: Vec<Run> = paths
                    .iter()
                    .map(|p| amber.ingest(Path::new(p), Some(&ref_for(p))))
                    .collect();
                measured_many(
                    name,
                    "store every path of the closure as its own reference",
                    &cmds,
                )
                .note(
                    "one reference per store path: these cores have no notion \
                     of a closure, so the set is the harness's bookkeeping",
                )
            }
        }
    }

    fn catalog_list(&self) -> Op {
        match &self.kind {
            Kind::Nix { cli, .. } => measured(
                "catalog_list",
                "list everything the store holds",
                &cli.nix()
                    .args(["path-info", "--all"])
                    .stdout_to("/dev/null"),
            ),
            Kind::Amber(amber) => measured(
                "catalog_list",
                "list everything the store holds",
                &amber.ref_list().stdout_to("/dev/null"),
            ),
        }
    }

    fn closure_query(&self, fixture: &NixFixture) -> Op {
        match &self.kind {
            Kind::Nix { cli, .. } => measured(
                "closure_query",
                "ask the store for a path's transitive references",
                &cli.path_info_closure(&fixture.gen2).stdout_to("/dev/null"),
            ),
            Kind::Amber(_) => Op::unsupported(
                "closure_query",
                "these cores store trees keyed by content and record no \
                 references between roots, so there is no closure to query. \
                 The harness keeps the reference graph itself and verifies \
                 completeness separately; see the closure_is_complete check.",
            )
            .describe("ask the store for a path's transitive references"),
        }
    }

    fn read_closure(&self, fixture: &NixFixture) -> Op {
        let devnull = Path::new("/dev/null");
        let (paths, _) = self.closure(fixture, 2);
        match &self.kind {
            Kind::Nix { cli, .. } => {
                let cmds: Vec<Run> = paths.iter().map(|p| cli.dump_path(p, devnull)).collect();
                measured_many("read_closure", "stream every path back out", &cmds)
            }
            Kind::Amber(amber) => {
                let cmds: Vec<Run> = paths
                    .iter()
                    .map(|p| amber.export_stdout(&format!("ref:{}", ref_for(p)), devnull))
                    .collect();
                measured_many("read_closure", "stream every path back out", &cmds)
            }
        }
    }

    fn cache_export(&self, fixture: &NixFixture) -> Op {
        match &self.kind {
            Kind::Nix { cli, cache, .. } => {
                let _ = std::fs::create_dir_all(cache);
                measured(
                    "cache_export",
                    "publish the closure to a binary cache",
                    &cli.copy(
                        &nixstore::local_store_uri(self.store.as_path()),
                        &format!("file://{}", cache.display()),
                        std::slice::from_ref(&fixture.gen2),
                    ),
                )
            }
            Kind::Amber(_) => Op::unsupported(
                "cache_export",
                "these cores have no binary-cache publication format; the blob \
                 scenario group measures what they can do, which is to publish \
                 their on-disk pack segments to object storage",
            )
            .describe("publish the closure to a binary cache"),
        }
    }

    fn cache_substitute(&self, fixture: &NixFixture) -> Op {
        match &self.kind {
            Kind::Nix { second, cache, .. } => measured(
                "cache_substitute",
                "fetch the closure from a binary cache into a fresh store",
                &second.copy(
                    &format!("file://{}", cache.display()),
                    &nixstore::local_store_uri(
                        second.store.as_deref().unwrap_or(Path::new("/nonexistent")),
                    ),
                    std::slice::from_ref(&fixture.gen2),
                ),
            ),
            Kind::Amber(_) => Op::unsupported(
                "cache_substitute",
                "these cores have no binary-cache client; see the blob \
                 scenario group",
            )
            .describe("fetch the closure from a binary cache"),
        }
    }

    /// Every store-path name actually present in a materialisation
    /// destination, so the observed set can be compared with the expected
    /// one in both directions.
    fn observed_names(&self, dest: &Path) -> Result<std::collections::BTreeSet<String>, String> {
        let dir = match &self.kind {
            Kind::Nix { .. } => dest.join("store/nix/store"),
            Kind::Amber(_) => dest.to_path_buf(),
        };
        let mut out = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // Nix keeps its own bookkeeping beside the store paths.
            if name == ".links" || name.ends_with(".lock") {
                continue;
            }
            out.insert(name);
        }
        Ok(out)
    }

    /// Registering a NAR hash and a reference list for a stored path: a Nix
    /// store capability neither core has. Recorded as an unsupported
    /// operation so it appears in the report as a missing capability rather
    /// than being quietly absent.
    fn nar_registration(&self) -> Op {
        match &self.kind {
            Kind::Nix { .. } => Op::aside(
                "nar_and_reference_registration",
                Phase::Setup,
                Default::default(),
            )
            .describe("register each stored path's NAR hash and references")
            .note(
                "Nix registers a NAR hash and a reference list for every \
                     store path; `nix store verify` re-derives them, and the \
                     harness compares them with the source store's",
            ),
            Kind::Amber(_) => Op::unsupported(
                "nar_and_reference_registration",
                "these cores record no NAR hash and no reference list for a \
                 stored tree: they address content by their own key and model \
                 no edges between roots. The harness therefore recomputes the \
                 NAR hash of every restored path with `nix hash path` and \
                 compares it with the source store's — see the \
                 nar_hashes_match_recomputed check — but nothing in the store \
                 under test would have noticed a mismatch.",
            )
            .describe("register each stored path's NAR hash and references"),
        }
    }

    /// Where a materialised store path ends up.
    fn materialized_path(&self, dest: &Path, store_path: &str) -> PathBuf {
        match &self.kind {
            Kind::Nix { .. } => dest
                .join("store/nix/store")
                .join(Path::new(store_path).file_name().unwrap_or_default()),
            Kind::Amber(_) => dest.join(Path::new(store_path).file_name().unwrap_or_default()),
        }
    }

    fn materialize(&self, fixture: &NixFixture, dest: &Path) -> Op {
        let (paths, top) = self.closure(fixture, 2);
        match &self.kind {
            Kind::Nix { cli, .. } => {
                // A store of its own per materialisation, so a second
                // materialisation cannot be satisfied by what the first
                // one already left behind.
                let target = dest.join("store");
                measured(
                    "materialize_closure",
                    "copy the closure into a fresh isolated store",
                    &cli.copy(
                        &nixstore::local_store_uri(self.store.as_path()),
                        &nixstore::local_store_uri(&target),
                        std::slice::from_ref(top),
                    ),
                )
            }
            Kind::Amber(amber) => {
                let cmds: Vec<Run> = paths
                    .iter()
                    .map(|p| {
                        amber.restore(
                            &format!("ref:{}", ref_for(p)),
                            &self.materialized_path(dest, p),
                        )
                    })
                    .collect();
                measured_many(
                    "materialize_closure",
                    "restore every path of the closure into a directory",
                    &cmds,
                )
            }
        }
    }

    fn delete_path(&self, fixture: &NixFixture) -> Op {
        // The top of generation 1's closure: nothing references it, so it
        // is the path a retention policy would actually drop, and
        // dropping it is what lets collection reclaim everything only
        // generation 1 needed.
        let doomed = if fixture.gen2_closure.contains(&fixture.gen1) {
            None
        } else {
            Some(&fixture.gen1)
        };
        match (&self.kind, doomed) {
            (Kind::Nix { cli, .. }, Some(p)) => measured(
                "delete_path",
                "drop a path only the older generation needed",
                &cli.delete(p),
            ),
            (Kind::Amber(amber), Some(p)) => measured(
                "delete_path",
                "drop the reference to a path only the older generation needed",
                &amber.ref_rm(&ref_for(p)),
            ),
            // With a single supplied closure there is no older generation at
            // all, which is a stated limitation of that input rather than
            // something that went wrong.
            _ if !fixture.origin.has_churn() => {
                Op::unsupported("delete_path", fixture.origin.no_churn_reason())
                    .describe("drop a path only the older generation needed")
            }
            _ => Op::failed(
                "delete_path",
                "the two fixture generations share their whole closure, so \
                 there is nothing a deletion could free",
            ),
        }
    }

    /// Registers a garbage-collection root for generation 2 in the
    /// isolated store.
    ///
    /// An isolated Nix store starts with no roots at all, so an
    /// unqualified collection would delete everything in it and the
    /// measurement would be of emptying a store rather than of
    /// collecting one. The Amber cores need no equivalent: a reference
    /// *is* a root there.
    fn register_gc_root(&self, fixture: &NixFixture) -> Op {
        match &self.kind {
            Kind::Nix { .. } => {
                let roots = self.store.join("nix/var/nix/gcroots");
                let link = roots.join("amber-bench-gen2");
                let result = std::fs::create_dir_all(&roots).and_then(|()| {
                    let _ = std::fs::remove_file(&link);
                    std::os::unix::fs::symlink(&fixture.gen2, &link)
                });
                match result {
                    Ok(()) => Op::aside("register_gc_root", Phase::Setup, Default::default())
                        .describe("keep generation 2 alive across collection"),
                    Err(e) => Op::failed("register_gc_root", e.to_string()),
                }
            }
            Kind::Amber(_) => Op::aside("register_gc_root", Phase::Setup, Default::default())
                .describe(
                    "not needed: in these cores a reference is a collection \
                 root, so the references that survived the deletion are \
                 what collection preserves",
                ),
        }
    }

    fn gc(&self) -> Op {
        match &self.kind {
            Kind::Nix { cli, .. } => measured("gc", "collect unreachable paths", &cli.gc()),
            Kind::Amber(amber) => measured("gc", "collect unreachable storage", &amber.gc_run()),
        }
    }

    fn integrity(&self, fixture: &NixFixture) -> Vec<Run> {
        match &self.kind {
            Kind::Nix { cli, .. } => vec![cli.verify_all()],
            Kind::Amber(amber) => {
                let (paths, _) = self.closure(fixture, 2);
                paths
                    .iter()
                    .map(|p| {
                        amber.export_stdout(&format!("ref:{}", ref_for(p)), Path::new("/dev/null"))
                    })
                    .collect()
            }
        }
    }
}

/// Collects the fixture facts from the store the fixture was built into.
pub fn inspect_fixture(nix: &NixCli, root: &Path) -> Result<NixFixture, String> {
    let read = |name: &str| -> Result<String, String> {
        let link = root.join(name);
        std::fs::read_link(&link)
            .map(|p| p.display().to_string())
            .map_err(|e| format!("{}: {e}", link.display()))
    };
    let gen1 = read("gen1")?;
    let gen2 = read("gen2")?;
    // The closure evidence every later verification rests on is read from
    // the store the fixture was built into, with substitution switched off so
    // it describes what is really there.
    let closure = |p: &str| -> Result<Vec<String>, String> {
        let out = nix
            .host_query()
            .args(["path-info", "-r", "--json", "--json-format", "1", p])
            .ok()?;
        nixstore::parse_closure(&out.stdout_text())
    };
    let details = |p: &str| -> Result<BTreeMap<String, (String, Vec<String>)>, String> {
        let out = nix
            .host_query()
            .args(["path-info", "-r", "--json", "--json-format", "1", p])
            .ok()?;
        nixstore::parse_closure_details(&out.stdout_text())
    };
    let gen1_closure = closure(&gen1)?;
    let gen2_closure = closure(&gen2)?;
    let mut all = details(&gen1)?;
    all.extend(details(&gen2)?);
    Ok(NixFixture {
        root: root.to_path_buf(),
        gen1,
        gen2,
        gen1_closure,
        gen2_closure,
        details: all,
        origin: super::NixOrigin::Fixture,
    })
}

/// The same, for closures rooted at store paths the user supplied.
///
/// Three things happen here and nothing else — in particular nothing is
/// written to a supplied path, which belongs to whoever supplied it:
///
/// 1. each root is confirmed to be a path the store really holds, with
///    substitution off, so a typo or a garbage-collected path is an error
///    rather than something quietly fetched from a cache;
/// 2. the closure of each root is queried, together with the NAR hash and
///    reference list the store recorded for every member — the same evidence
///    every later verification is checked against;
/// 3. every one of those NAR hashes is *recomputed* from the bytes on disk
///    with `nix hash path` and compared with what the store recorded, so a
///    benchmark is never run against a source that is already corrupt.
///
/// The first root is generation 1. A second root, if there is one, is
/// generation 2; with only one root there is no churn, and the caller marks
/// the operations that need it unsupported.
pub fn inspect_supplied(nix: &NixCli, roots: &[String]) -> Result<NixFixture, String> {
    let first = roots
        .first()
        .ok_or_else(|| "no closure root was supplied".to_string())?;
    let second = roots.get(1).unwrap_or(first);
    if roots.len() > 2 {
        return Err(format!(
            "{} closure roots were supplied; the workload has exactly two \
             generations, so it takes at most two roots (the first is \
             generation 1, the second is generation 2)",
            roots.len()
        ));
    }
    let query = |p: &str| -> Result<String, String> {
        nix.host_query()
            .args(["path-info", "-r", "--json", "--json-format", "1", p])
            .ok()
            .map(|o| o.stdout_text())
            .map_err(|e| {
                format!(
                    "the supplied root {p} could not be read from the store it \
                     is supposed to be in: {e}"
                )
            })
    };
    let json1 = query(first)?;
    let json2 = if second == first {
        json1.clone()
    } else {
        query(second)?
    };
    let gen1_closure = nixstore::parse_closure(&json1)?;
    let gen2_closure = nixstore::parse_closure(&json2)?;
    let mut details = nixstore::parse_closure_details(&json1)?;
    details.extend(nixstore::parse_closure_details(&json2)?);

    // Recompute every NAR hash from the bytes on disk. This is setup, not a
    // measurement, and it is the only way to know that what a restore is
    // later compared against describes the source as it actually is.
    let hasher = NixCli {
        bin: nix.bin.clone(),
        store: None,
        timeout: nix.timeout,
        cache: ClientCache::Inherited,
    };
    let mut bad = Vec::new();
    for (path, (want, _)) in &details {
        match hasher.hash_path(Path::new(path)).run() {
            Ok(o) if o.success() => match nixstore::parse_hash_path(&o.stdout_text()) {
                Ok(got) if &got == want => {}
                Ok(got) => bad.push(format!("{path}: recorded {want}, on disk {got}")),
                Err(e) => bad.push(format!("{path}: {e}")),
            },
            Ok(o) => {
                let err = o.require_success().unwrap_err();
                bad.push(format!("{path}: {}", err.lines().next().unwrap_or("")));
            }
            Err(e) => bad.push(format!("{path}: {e}")),
        }
    }
    if !bad.is_empty() {
        return Err(format!(
            "{} supplied store path(s) do not hash to the NAR hash their store \
             recorded, so the source cannot be used as a reference: {}",
            bad.len(),
            bad.join("; ")
        ));
    }

    Ok(NixFixture {
        root: PathBuf::from(first),
        gen1: first.clone(),
        gen2: second.clone(),
        gen1_closure,
        gen2_closure,
        details,
        origin: super::NixOrigin::Supplied {
            roots: roots.to_vec(),
            second_generation: roots.len() > 1,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amber_backend() -> Backend {
        Backend {
            store: PathBuf::from("/s"),
            kind: Kind::Amber(AmberCli {
                bin: PathBuf::from("/bin/amber-store"),
                store: PathBuf::from("/s"),
                segment_size: 1,
                chunk: crate::config::ChunkSettings::shared(),
                jobs: 1,
                timeout: std::time::Duration::from_secs(1),
            }),
        }
    }

    #[test]
    fn reference_names_are_derived_from_the_store_path() {
        assert_eq!(
            ref_for("/nix/store/abc123-amber-bench-lib-core"),
            "nixpath-abc123-amber-bench-lib-core"
        );
    }

    #[test]
    fn a_tree_store_reports_closure_queries_as_unsupported_with_the_reason() {
        let fixture = NixFixture {
            root: PathBuf::from("/nix/store/f"),
            gen1: "/nix/store/a".into(),
            gen2: "/nix/store/b".into(),
            gen1_closure: vec!["/nix/store/a".into()],
            gen2_closure: vec!["/nix/store/b".into()],
            origin: crate::scenarios::NixOrigin::Fixture,
            details: BTreeMap::new(),
        };
        let be = amber_backend();
        for op in [
            be.closure_query(&fixture),
            be.cache_export(&fixture),
            be.cache_substitute(&fixture),
        ] {
            assert_eq!(op.status, OpStatus::Unsupported, "{}", op.name);
            assert!(op.wall_ns.is_none());
            assert!(!op.reason.as_ref().unwrap().is_empty());
        }
    }

    #[test]
    fn a_tree_store_materialises_paths_side_by_side() {
        let be = amber_backend();
        assert_eq!(
            be.materialized_path(Path::new("/dest"), "/nix/store/abc-x"),
            PathBuf::from("/dest/abc-x")
        );
    }
}
