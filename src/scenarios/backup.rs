//! The backup workload: restic against the Amber cores.
//!
//! Four snapshots of a changing tree, then the operations a retention policy
//! actually performs: list, restore the newest, forget the oldest, reclaim
//! its space, and check that what was kept is still intact. This is the one
//! group where a backend that encrypts everything it stores is compared with
//! backends that store plaintext, so the semantics block matters more here
//! than anywhere else and the report prints it next to every number.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::adapters::amber::AmberCli;
use crate::adapters::restic::{self, ResticCli};
use crate::adapters::semantics;
use crate::dataset::manifest::Strictness;
use crate::metrics::{Op, OpStatus, Phase, RunRecord, Verification};
use crate::util::fsx;
use crate::util::proc::Run;

use super::{Ctx, Recorder, aside, measured, reclaimed, store_counters, store_sizes};

/// Scenario group name.
pub const GROUP: &str = "backup";
/// Scenario name.
pub const SCENARIO: &str = "backup/retention";
/// Every backend this scenario can run.
pub const BACKENDS: [&str; 3] = ["restic", "amber-rust", "amber-go"];

/// Snapshot names, in the order they are taken, with the corpus generation
/// each one holds.
const SNAPSHOTS: [(&str, usize); 4] =
    [("snap0", 0), ("snap0-again", 0), ("snap1", 1), ("snap2", 2)];

/// What retention drops: every snapshot of generation 0.
const DROPPED: [&str; 2] = ["snap0", "snap0-again"];

/// What retention keeps, and therefore what has to be verified afterwards —
/// all of it, not only the newest.
const KEPT: [&str; 2] = ["snap1", "snap2"];

enum Kind {
    Restic {
        cli: ResticCli,
        ids: BTreeMap<String, String>,
        sources: BTreeMap<String, String>,
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
    let dir = match ctx.prepare_run_dir(GROUP, backend, rep) {
        Ok(d) => d,
        Err(e) => return rec.abort(e),
    };
    let mut be = match build(ctx, backend, &dir) {
        Ok(b) => b,
        Err(e) => return rec.abort(e),
    };
    for op in be.setup() {
        let failed = op.status == OpStatus::Failed;
        rec.push(op);
        if failed {
            return rec.abort(format!("{backend}: setup failed"));
        }
    }

    let generations = ctx.corpus.generations.len();
    for (name, generation) in SNAPSHOTS {
        if generation >= generations {
            rec.push(
                Op::unsupported(
                    &format!("backup_{name}"),
                    format!(
                        "the corpus was generated with {generations} generations; \
                         this profile has no generation {generation}"
                    ),
                )
                .describe("take a snapshot"),
            );
            continue;
        }
        let g = &ctx.corpus.generations[generation];
        let op_name = match name {
            "snap0" => "backup_initial",
            "snap0-again" => "backup_unchanged",
            "snap1" => "backup_changed",
            _ => "backup_changed_again",
        };
        let op = be
            .backup(ctx, &g.dir, name, op_name)
            .logical(g.logical_bytes)
            .store(store_sizes(&be.store))
            .counters(store_counters(&be.store))
            .describe(format!("snapshot corpus generation {generation}"));
        let failed = op.status == OpStatus::Failed;
        rec.push(op);
        if failed {
            return rec.abort(format!("{backend}: {op_name} failed"));
        }
    }

    let latest = SNAPSHOTS[SNAPSHOTS.len() - 1];
    rec.push(be.list(latest.0));
    let restored = dir.join("restore-latest");
    let _ = std::fs::create_dir_all(&restored);
    let g = &ctx.corpus.generations[latest.1];
    let op = be
        .restore(latest.0, &restored)
        .logical(g.logical_bytes)
        .describe("restore the newest snapshot");
    let ok = op.status == OpStatus::Ok;
    rec.push(op);
    rec.verify(verify(
        ctx,
        latest.1,
        &restored,
        ok,
        "restore_matches_manifest",
    ));

    // Retention drops *every* snapshot of generation 0 — both `snap0` and
    // the unchanged `snap0-again` that followed it. Forgetting only the
    // first would leave the second pointing at exactly the same content, so
    // generation 0 would stay pinned and the prune that follows would be
    // measured reclaiming nothing.
    let before_forget = store_sizes(&be.store);
    rec.push(
        be.forget(&DROPPED)
            .describe(
                "apply retention: forget every snapshot of the oldest \
                 generation, so its unique content really becomes unreachable",
            )
            .store(store_sizes(&be.store))
            .counters(store_counters(&be.store)),
    );
    if let Kind::Amber(cli) = &be.kind {
        rec.push(super::age_segments(cli));
    }
    let before_prune = store_sizes(&be.store);
    let packs_before = store_counters(&be.store);
    let mut prune = be.prune();
    let after = store_sizes(&be.store);
    let packs_after = store_counters(&be.store);
    prune = prune
        .describe("reclaim the space the forgotten snapshots held")
        .store(after)
        .reclaimed(reclaimed(&before_prune, &after))
        .counters(packs_after.clone())
        .counter(
            "allocated_before_forget",
            before_forget.allocated_bytes as i64,
        )
        .counter("allocated_after_prune", after.allocated_bytes as i64);
    if let (Some(before), Some(after)) = (
        packs_before.get("store_packstore_allocated_bytes"),
        packs_after.get("store_packstore_allocated_bytes"),
    ) {
        prune = prune
            .counter("reclaimed_packstore_bytes", before - after)
            .note(
                "reclaimed_packstore_bytes is the pack-segment bytes the \
                 prune released; the whole-store figure also includes the \
                 reference database, which compacts itself",
            );
    }
    rec.push(prune);

    for cmd in be.check() {
        rec.push(aside(
            "integrity_check",
            Phase::Verify,
            "the backend's own consistency check over the whole repository",
            &cmd,
        ));
    }

    // Retention must not have damaged *anything* it kept, so every retained
    // snapshot is restored and compared, not only the newest one.
    let mut damaged = Vec::new();
    for name in KEPT {
        let Some((_, generation)) = SNAPSHOTS.iter().find(|(n, _)| *n == name) else {
            continue;
        };
        if *generation >= generations {
            continue;
        }
        let dest = dir.join(format!("restore-after-retention-{name}"));
        let _ = std::fs::create_dir_all(&dest);
        let op = be.restore(name, &dest);
        let ok = op.status == OpStatus::Ok;
        let mut op = op.describe(format!(
            "restore retained snapshot {name} after retention ran"
        ));
        op.phase = Phase::Verify;
        op.name = "post_retention_restore".into();
        rec.push(op);
        let v = verify(
            ctx,
            *generation,
            &dest,
            ok,
            "post_retention_restore_matches_manifest",
        );
        if !v.passed {
            damaged.push(format!("{name}: {}", v.detail));
            damaged.extend(v.corrupt.iter().take(3).cloned());
        }
        let _ = fsx::make_writable_tree(&dest);
        let _ = std::fs::remove_dir_all(&dest);
    }
    let mut v = if damaged.is_empty() {
        Verification::pass(
            "every_retained_snapshot_matches_manifest",
            format!(
                "all {} retained snapshots ({}) were restored after retention \
                 and matched their manifests byte for byte",
                KEPT.len(),
                KEPT.join(", ")
            ),
        )
    } else {
        Verification::fail(
            "every_retained_snapshot_matches_manifest",
            format!("{} retained snapshot(s) did not verify", damaged.len()),
        )
    };
    v.corrupt = damaged;
    rec.verify(v);

    // And the forgotten snapshots must really be gone.
    //
    // A restore that *fails* is no evidence of that: it fails just as
    // readily when the executable is missing, when a command times out or
    // when an unrelated error occurs. So absence is read from the backend's
    // own snapshot listing, which has to have succeeded, and the restore
    // attempt is kept only for the opposite conclusion — a dropped snapshot
    // that can still be read back is a failure however the listing looked.
    let mut still_readable = Vec::new();
    for name in DROPPED {
        let dest = dir.join(format!("restore-forgotten-{name}"));
        let _ = std::fs::create_dir_all(&dest);
        let op = be.restore(name, &dest);
        let readable = op.status == OpStatus::Ok;
        let mut probe = Op::aside(
            "forgotten_snapshot_probe",
            Phase::Verify,
            Default::default(),
        )
        .describe(format!("attempt to restore the forgotten snapshot {name}"))
        .note(
            "recorded for transparency only: a failure here is not what \
                 proves the snapshot is gone",
        );
        if readable {
            probe = probe.note(format!("{name} could still be restored in full"));
            still_readable.push(name.to_string());
        } else if let Some(reason) = op.reason {
            probe = probe.note(format!("the attempt failed with: {}", first_line(&reason)));
        }
        rec.push(probe);
        let _ = fsx::make_writable_tree(&dest);
        let _ = std::fs::remove_dir_all(&dest);
    }
    rec.verify(super::check_retention(
        "forgotten_snapshots_are_absent",
        super::RetentionEvidence {
            listing: be.retained_names(),
            still_readable,
        },
        &KEPT,
        &DROPPED,
    ));

    rec.finish()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

fn verify(ctx: &Ctx, generation: usize, root: &Path, restored: bool, name: &str) -> Verification {
    if !restored {
        return Verification::fail(name, "the restore failed, so there was nothing to verify");
    }
    match ctx.corpus.manifest(generation) {
        Ok(m) => m.verify_tree(name, root, Strictness::full()),
        Err(e) => Verification::fail(name, format!("reference manifest unreadable: {e}")),
    }
}

fn build(ctx: &Ctx, backend: &str, dir: &Path) -> Result<Backend, String> {
    match backend {
        "restic" => {
            let password = dir.join("restic-password");
            fsx::write_file(&password, b"amber-cas-bench\n").map_err(|e| e.to_string())?;
            Ok(Backend {
                store: dir.join("restic-repo"),
                kind: Kind::Restic {
                    cli: ResticCli {
                        bin: ctx.tools.path("restic")?,
                        repo: dir.join("restic-repo").display().to_string(),
                        password_file: password,
                        cache_dir: dir.join("restic-cache"),
                        timeout: ctx.timeout,
                        s3_credentials: None,
                    },
                    ids: BTreeMap::new(),
                    sources: BTreeMap::new(),
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
        other => Err(format!("backup scenario: unknown backend {other}")),
    }
}

impl Backend {
    fn setup(&self) -> Vec<Op> {
        match &self.kind {
            Kind::Restic { cli, .. } => vec![aside(
                "restic_init",
                Phase::Setup,
                "create the repository",
                &cli.init(),
            )],
            Kind::Amber(_) => Vec::new(),
        }
    }

    fn backup(&mut self, ctx: &Ctx, src: &Path, name: &str, op_name: &str) -> Op {
        match &mut self.kind {
            Kind::Amber(cli) => measured(op_name, "store a snapshot", &cli.ingest(src, Some(name))),
            Kind::Restic { cli, ids, sources } => {
                let op = measured(
                    op_name,
                    "store a snapshot",
                    &cli.backup(src, name, ctx.jobs),
                );
                if op.status == OpStatus::Ok {
                    sources.insert(name.to_string(), src.display().to_string());
                    // Resolved from this snapshot's own unique tag rather
                    // than from the position of an unrelated listing.
                    let resolved = cli
                        .snapshots_tagged(name)
                        .ok()
                        .and_then(|out| restic::resolve_tagged_snapshot(&out.stdout_text(), name));
                    match resolved {
                        Ok(id) => {
                            ids.insert(name.to_string(), id);
                        }
                        Err(e) => {
                            return Op::failed(
                                op_name,
                                format!(
                                    "the backup succeeded but its snapshot could \
                                     not be identified, so nothing later could \
                                     name it: {e}"
                                ),
                            )
                            .describe("store a snapshot");
                        }
                    }
                }
                op
            }
        }
    }

    /// The id the backend knows a snapshot by. An unresolved name is passed
    /// through unchanged rather than being replaced by `latest`, so a
    /// command that needs it fails instead of quietly operating on some
    /// other snapshot.
    fn id(&self, name: &str) -> String {
        match &self.kind {
            Kind::Restic { ids, .. } => ids
                .get(name)
                .cloned()
                .unwrap_or_else(|| format!("tag:{name}:unresolved")),
            Kind::Amber(_) => name.to_string(),
        }
    }

    fn source(&self, name: &str) -> String {
        match &self.kind {
            Kind::Restic { sources, .. } => {
                sources.get(name).cloned().unwrap_or_else(|| "/".into())
            }
            Kind::Amber(_) => "/".into(),
        }
    }

    fn list(&self, name: &str) -> Op {
        match &self.kind {
            Kind::Restic { cli, .. } => measured(
                "list_snapshot",
                "enumerate a snapshot's contents",
                &cli.ls(&self.id(name)).stdout_to("/dev/null"),
            ),
            Kind::Amber(cli) => match cli.list_recursive(&format!("ref:{name}")) {
                Ok(l) => Op::measured("list_snapshot", l.usage)
                    .describe("enumerate a snapshot's contents")
                    .counter("entries", l.entries as i64)
                    .counter("cli_invocations", l.invocations as i64)
                    .note(format!(
                        "{} separate `amber-store ls` processes, one per \
                         directory: neither core's CLI has a recursive listing",
                        l.invocations
                    )),
                Err(e) => Op::failed("list_snapshot", e),
            },
        }
    }

    fn restore(&self, name: &str, dest: &Path) -> Op {
        match &self.kind {
            Kind::Restic { cli, .. } => {
                let spec = format!("{}:{}", self.id(name), self.source(name));
                measured(
                    "restore_latest",
                    "restore a snapshot",
                    &cli.restic()
                        .args(["restore", &spec])
                        .arg("--target")
                        .arg(dest),
                )
            }
            Kind::Amber(cli) => measured(
                "restore_latest",
                "restore a snapshot",
                &cli.restore(&format!("ref:{name}"), dest),
            ),
        }
    }

    /// Drops every one of `names` as one retention step.
    fn forget(&self, names: &[&str]) -> Op {
        const OP: &str = "forget_oldest";
        const DESC: &str = "drop the snapshots retention no longer keeps";
        match &self.kind {
            Kind::Restic { cli, .. } => {
                // restic takes every id in one invocation, which is what a
                // retention policy really runs.
                let mut cmd = cli.restic().arg("forget");
                for name in names {
                    cmd = cmd.arg(self.id(name));
                }
                measured(OP, DESC, &cmd)
            }
            Kind::Amber(cli) => {
                let cmds: Vec<Run> = names.iter().map(|n| cli.ref_rm(n)).collect();
                super::measured_many(OP, DESC, &cmds)
            }
        }
    }

    /// What the repository itself says it still holds, by the names this
    /// scenario knows them by.
    ///
    /// The command must succeed; an error is returned as an error, because a
    /// listing that failed says nothing about what is there.
    fn retained_names(&self) -> Result<Vec<String>, String> {
        match &self.kind {
            Kind::Restic { cli, .. } => {
                let out = cli.snapshots().ok()?;
                // Every snapshot is tagged with the name it was taken under.
                restic::parse_snapshot_tags(&out.stdout_text())
            }
            Kind::Amber(cli) => {
                let out = cli.ref_list().ok()?;
                Ok(crate::adapters::amber::parse_ref_list(&out.stdout_text()))
            }
        }
    }

    fn prune(&self) -> Op {
        match &self.kind {
            Kind::Restic { cli, .. } => measured("prune", "reclaim space", &cli.prune()),
            Kind::Amber(cli) => measured("prune", "reclaim space", &cli.gc_run()),
        }
    }

    fn check(&self) -> Vec<Run> {
        match &self.kind {
            Kind::Restic { cli, .. } => vec![cli.check(true)],
            Kind::Amber(cli) => {
                vec![cli.export_stdout(&format!("ref:{}", SNAPSHOTS[3].0), Path::new("/dev/null"))]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_releases_a_generation_rather_than_leaving_it_pinned() {
        // Every snapshot that holds generation 0 must be dropped, or the
        // prune measurement would have nothing to reclaim.
        let dropped_generations: Vec<usize> = SNAPSHOTS
            .iter()
            .filter(|(n, _)| DROPPED.contains(n))
            .map(|(_, g)| *g)
            .collect();
        let kept_generations: Vec<usize> = SNAPSHOTS
            .iter()
            .filter(|(n, _)| KEPT.contains(n))
            .map(|(_, g)| *g)
            .collect();
        assert!(!dropped_generations.is_empty());
        for g in &dropped_generations {
            assert!(
                !kept_generations.contains(g),
                "generation {g} is dropped by one snapshot and pinned by another, \
                 so nothing could be reclaimed"
            );
        }
        // Together they account for every snapshot taken.
        assert_eq!(DROPPED.len() + KEPT.len(), SNAPSHOTS.len());
        for (name, _) in SNAPSHOTS {
            assert!(
                DROPPED.contains(&name) || KEPT.contains(&name),
                "{name} is neither kept nor dropped"
            );
        }
    }

    #[test]
    fn every_retained_snapshot_is_verified_not_only_the_newest() {
        assert!(KEPT.len() > 1, "there is more than one survivor to check");
        assert_eq!(*KEPT.last().unwrap(), SNAPSHOTS.last().unwrap().0);
    }

    #[test]
    fn absence_is_read_from_the_listing_and_not_from_a_failed_restore() {
        // A dropped snapshot that is absent from a successful listing passes.
        let v = super::super::check_retention(
            "forgotten_snapshots_are_absent",
            super::super::RetentionEvidence {
                listing: Ok(vec!["snap1".into(), "snap2".into()]),
                still_readable: vec![],
            },
            &KEPT,
            &DROPPED,
        );
        assert!(v.passed, "{v:?}");

        // The same conclusion is refused when the listing itself failed,
        // however plausibly a restore may have gone wrong.
        let v = super::super::check_retention(
            "forgotten_snapshots_are_absent",
            super::super::RetentionEvidence {
                listing: Err("restic snapshots: timed out".into()),
                still_readable: vec![],
            },
            &KEPT,
            &DROPPED,
        );
        assert!(!v.passed, "a timeout is not proof of absence");
        assert!(v.detail.contains("timed out"), "{}", v.detail);

        // And a dropped snapshot that can still be read back fails.
        let v = super::super::check_retention(
            "forgotten_snapshots_are_absent",
            super::super::RetentionEvidence {
                listing: Ok(vec!["snap1".into(), "snap2".into()]),
                still_readable: vec!["snap0".into()],
            },
            &KEPT,
            &DROPPED,
        );
        assert!(!v.passed);
        assert!(
            v.corrupt[0].contains("still be read back"),
            "{:?}",
            v.corrupt
        );
    }

    #[test]
    fn the_snapshot_plan_covers_initial_unchanged_and_changed() {
        assert_eq!(SNAPSHOTS[0].1, 0);
        assert_eq!(SNAPSHOTS[1].1, 0, "the second snapshot must be unchanged");
        assert_eq!(SNAPSHOTS[2].1, 1);
        assert_eq!(SNAPSHOTS[3].1, 2);
        let names: Vec<&str> = SNAPSHOTS.iter().map(|s| s.0).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "snapshot names must be distinct");
    }
}
