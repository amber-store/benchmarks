//! The generic filesystem-tree workload.
//!
//! One corpus, five stores, the same seven operations each: store it, store
//! it again unchanged, store a churned generation, enumerate it, read it all
//! back, materialise it, drop a reference, collect the garbage. Every store
//! is created for this repetition and destroyed after it, and every restored
//! tree is compared against a manifest observed from the corpus itself.

use std::path::{Path, PathBuf};

use crate::adapters::amber::AmberCli;
use crate::adapters::fscas::FsCasCli;
use crate::adapters::git::GitCli;
use crate::adapters::restic::{self, ResticCli};
use crate::adapters::semantics;
use crate::dataset::manifest::{Manifest, Strictness};
use crate::metrics::{Op, OpStatus, Phase, RunRecord, Verification};
use crate::util::fsx;
use crate::util::proc::Run;

use super::{
    Ctx, Recorder, aside, measured, measured_many, reclaimed, store_counters, store_sizes,
};

/// Scenario group name.
pub const GROUP: &str = "tree";
/// Scenario name.
pub const SCENARIO: &str = "tree/lifecycle";
/// Every backend this scenario can run.
pub const BACKENDS: [&str; 5] = ["amber-rust", "amber-go", "git", "restic", "fs-sha256"];

/// Reference name of the first generation.
const REF0: &str = "gen0";
/// Reference name of the repeated store of the first generation.
const REF0_REPEAT: &str = "gen0-again";
/// Reference name of the churned generation.
const REF1: &str = "gen1";

/// What a backend needs in order to be driven through the workload.
struct Backend {
    /// Directory whose growth is the store's cost.
    store: PathBuf,
    /// How closely a restored tree is compared.
    strict: Strictness,
    kind: Kind,
}

enum Kind {
    Amber(AmberCli),
    Git(GitCli),
    Restic {
        cli: ResticCli,
        /// Snapshot ids by reference name, filled in as backups are taken.
        snapshots: std::collections::BTreeMap<String, String>,
        /// Absolute source path each reference was taken from; restic
        /// records it in the snapshot and needs it back to restore a
        /// subtree rather than a nested absolute path.
        sources: std::collections::BTreeMap<String, String>,
    },
    FsCas(FsCasCli),
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
    for op in be.setup(ctx) {
        let failed = op.status == OpStatus::Failed;
        rec.push(op);
        if failed {
            return rec.abort(format!("{backend}: setup failed"));
        }
    }
    for note in be.strict.omissions() {
        rec.note(format!(
            "restored trees are not compared on {note}: this backend does not store it"
        ));
    }

    let gen0 = &ctx.corpus.generations[0];
    let gen1 = &ctx.corpus.generations[1];

    // --- storing -----------------------------------------------------------
    let mut op = be.ingest(
        ctx,
        &gen0.dir,
        REF0,
        "ingest_fresh",
        "store generation 0 into an empty store",
    );
    op = op
        .logical(gen0.logical_bytes)
        .store(store_sizes(&be.store))
        .counters(store_counters(&be.store));
    let failed = op.status == OpStatus::Failed;
    rec.push(op);
    if failed {
        return rec.abort(format!("{backend}: fresh ingestion failed"));
    }

    let op = be
        .ingest(
            ctx,
            &gen0.dir,
            REF0_REPEAT,
            "ingest_repeat",
            "store the identical tree a second time",
        )
        .logical(gen0.logical_bytes)
        .store(store_sizes(&be.store))
        .counters(store_counters(&be.store))
        .note(
            "the store already holds every byte; what is measured is how \
             cheaply the backend recognises that",
        );
    rec.push(op);

    let op = be
        .ingest(
            ctx,
            &gen1.dir,
            REF1,
            "ingest_changed",
            "store the churned generation 1",
        )
        .logical(gen1.logical_bytes)
        .store(store_sizes(&be.store))
        .counters(store_counters(&be.store));
    rec.push(op);

    // --- reading -----------------------------------------------------------
    rec.push(be.list_recursive(REF0));
    let devnull = PathBuf::from("/dev/null");
    rec.push(
        be.read_full(REF0, &devnull)
            .logical(gen0.logical_bytes)
            .describe("stream every byte of generation 0 out of the store"),
    );

    let restored = dir.join("restore-gen0");
    let prep = be.restore_prep(&restored);
    for cmd in &prep {
        rec.push(aside(
            "restore_prepare",
            Phase::Setup,
            "prepare the destination and index state",
            cmd,
        ));
    }
    let op = be
        .restore(REF0, &restored)
        .logical(gen0.logical_bytes)
        .describe("materialise generation 0 into an empty directory");
    let restore_ok = op.status == OpStatus::Ok;
    rec.push(op);

    if restore_ok {
        rec.verify(verify_restore(
            "restore_matches_manifest",
            &ctx.corpus.manifest(0),
            &be.restore_root(&restored, &gen0.dir),
            be.strict,
        ));
    } else {
        rec.verify(Verification::fail(
            "restore_matches_manifest",
            "the restore operation failed, so there was nothing to verify",
        ));
    }

    // --- integrity ---------------------------------------------------------
    let mut integrity_ok = true;
    if let Kind::Amber(cli) = &be.kind {
        for name in [REF0, REF1] {
            if let Ok(out) = cli.ref_get(name).ok() {
                rec.root(name, &out.last_stdout_line());
            }
        }
    }

    for cmd in be.integrity() {
        let op = aside(
            "integrity_check",
            Phase::Verify,
            "re-read and re-check every stored object",
            &cmd,
        );
        if op.status == OpStatus::Failed {
            integrity_ok = false;
        }
        rec.push(op);
    }
    rec.verify(if integrity_ok {
        Verification::pass(
            "integrity_check",
            "the backend's own consistency check passed over the whole store",
        )
    } else {
        Verification::fail(
            "integrity_check",
            "the backend's own consistency check failed; see the failed operation",
        )
    });

    // --- deleting and collecting ------------------------------------------
    //
    // Both references to generation 0 are dropped, not just one. Dropping
    // only `gen0` would leave `gen0-again` pointing at exactly the same
    // content, so nothing would become unreachable and the collection that
    // follows would be measured reclaiming nothing — a vacuous number that
    // would flatter every backend equally.
    let before_delete = store_sizes(&be.store);
    let op = be
        .ref_delete(&[REF0, REF0_REPEAT])
        .describe(
            "drop every reference to generation 0, so its unique content \
             really becomes unreachable",
        )
        .store(store_sizes(&be.store))
        .counters(store_counters(&be.store));
    rec.push(op);

    if let Kind::Amber(cli) = &be.kind {
        rec.push(super::age_segments(cli));
    }
    let before_gc = store_sizes(&be.store);
    let packs_before = store_counters(&be.store);
    let mut op = be
        .gc()
        .describe("collect whatever the dropped references freed");
    let after_gc = store_sizes(&be.store);
    let packs_after = store_counters(&be.store);
    op = op
        .store(after_gc)
        .reclaimed(reclaimed(&before_gc, &after_gc))
        .counters(packs_after.clone())
        .counter(
            "allocated_before_delete",
            before_delete.allocated_bytes as i64,
        )
        .counter("allocated_before_gc", before_gc.allocated_bytes as i64)
        .counter("allocated_after_gc", after_gc.allocated_bytes as i64);
    // The pack bytes on their own, because a reference database that
    // compacts itself is not storage the collection reclaimed.
    if let (Some(before), Some(after)) = (
        packs_before.get("store_packstore_allocated_bytes"),
        packs_after.get("store_packstore_allocated_bytes"),
    ) {
        op = op
            .counter("reclaimed_packstore_bytes", before - after)
            .note(
                "reclaimed_packstore_bytes is the pack-segment bytes the \
             collection released; the whole-store reclaim figure also \
             includes the reference database, which compacts itself and is \
             not comparable between the two cores",
            );
    }
    rec.push(op);

    // --- what survived -----------------------------------------------------
    //
    // Every reference that was *not* dropped has to come back intact, and
    // every reference that was dropped has to be gone according to the
    // store's own listing. `gen1` is the only survivor here, so it is
    // restored and compared in full.
    rec.verify(retention_took_effect(&be, &[REF1], &[REF0, REF0_REPEAT]));

    let survived = dir.join("restore-gen1");
    for cmd in &be.restore_prep(&survived) {
        rec.push(aside(
            "post_gc_restore_prepare",
            Phase::Setup,
            "prepare the destination for the post-collection restore",
            cmd,
        ));
    }
    let op = be.restore(REF1, &survived);
    let ok = op.status == OpStatus::Ok;
    let mut op = op.describe("restore the retained generation after collection");
    op.phase = Phase::Verify;
    rec.push(op);
    rec.verify(if ok {
        verify_restore(
            "post_gc_restore_matches_manifest",
            &ctx.corpus.manifest(1),
            &be.restore_root(&survived, &gen1.dir),
            be.strict,
        )
    } else {
        Verification::fail(
            "post_gc_restore_matches_manifest",
            "restoring the retained generation after collection failed",
        )
    });

    for cmd in be.integrity() {
        rec.push(aside(
            "post_gc_integrity_check",
            Phase::Verify,
            "re-check the store after collection",
            &cmd,
        ));
    }

    rec.finish()
}

/// The id of the snapshot restic tagged `name`, which the harness made
/// unique per reference.
fn resolve_snapshot(cli: &ResticCli, name: &str) -> Result<String, String> {
    let out = cli.snapshots_tagged(name).ok()?;
    restic::resolve_tagged_snapshot(&out.stdout_text(), name)
}

/// Asks the store what it still holds and checks that against what retention
/// was supposed to do.
fn retention_took_effect(be: &Backend, kept: &[&str], dropped: &[&str]) -> Verification {
    super::check_retention(
        "retention_took_effect",
        super::RetentionEvidence {
            listing: be.retained_references(),
            still_readable: Vec::new(),
        },
        kept,
        dropped,
    )
}

fn verify_restore(
    name: &str,
    reference: &std::io::Result<Manifest>,
    root: &Path,
    strict: Strictness,
) -> Verification {
    match reference {
        Ok(m) => m.verify_tree(name, root, strict),
        Err(e) => Verification::fail(name, format!("the reference manifest is unreadable: {e}")),
    }
}

fn build(ctx: &Ctx, backend: &str, dir: &Path) -> Result<Backend, String> {
    let timeout = ctx.timeout;
    match backend {
        "amber-rust" | "amber-go" => {
            let bin = ctx.tools.path(backend)?;
            Ok(Backend {
                store: dir.join("store"),
                strict: Strictness::full(),
                kind: Kind::Amber(AmberCli {
                    bin,
                    store: dir.join("store"),
                    segment_size: ctx.profile.segment_size,
                    chunk: ctx.profile.chunk,
                    jobs: ctx.jobs,
                    timeout,
                }),
            })
        }
        "git" => {
            let bin = ctx.tools.path("git")?;
            Ok(Backend {
                store: dir.join("store.git"),
                strict: Strictness::git(),
                kind: Kind::Git(GitCli {
                    bin,
                    repo: dir.join("store.git"),
                    timeout,
                }),
            })
        }
        "restic" => {
            let bin = ctx.tools.path("restic")?;
            let password = dir.join("restic-password");
            fsx::write_file(&password, b"amber-cas-bench\n").map_err(|e| e.to_string())?;
            Ok(Backend {
                store: dir.join("restic-repo"),
                strict: Strictness::full(),
                kind: Kind::Restic {
                    cli: ResticCli {
                        bin,
                        repo: dir.join("restic-repo").display().to_string(),
                        password_file: password,
                        cache_dir: dir.join("restic-cache"),
                        timeout,
                        s3_credentials: None,
                    },
                    snapshots: Default::default(),
                    sources: Default::default(),
                },
            })
        }
        "fs-sha256" => Ok(Backend {
            store: dir.join("fscas"),
            strict: Strictness::full(),
            kind: Kind::FsCas(FsCasCli {
                bin: std::env::current_exe().map_err(|e| e.to_string())?,
                store: dir.join("fscas"),
                timeout,
            }),
        }),
        other => Err(format!("tree scenario: unknown backend {other}")),
    }
}

impl Backend {
    fn setup(&self, _ctx: &Ctx) -> Vec<Op> {
        match &self.kind {
            Kind::Amber(_) | Kind::FsCas(_) => {
                // Both create their store on first use.
                Vec::new()
            }
            Kind::Git(git) => git
                .init(true)
                .iter()
                .map(|c| aside("git_init", Phase::Setup, "create a bare repository", c))
                .collect(),
            Kind::Restic { cli, .. } => vec![aside(
                "restic_init",
                Phase::Setup,
                "create the repository",
                &cli.init(),
            )],
        }
    }

    fn ingest(&mut self, ctx: &Ctx, src: &Path, name: &str, op: &str, desc: &str) -> Op {
        match &mut self.kind {
            Kind::Amber(cli) => measured(op, desc, &cli.ingest(src, Some(name))),
            Kind::FsCas(cli) => measured(op, desc, &cli.ingest(src, name)),
            Kind::Git(git) => {
                // Each generation is an independent root commit, exactly as
                // each Amber reference is an independent root: that is what
                // makes "drop a reference and collect" mean the same thing on
                // both sides. The index is emptied first so `add -A` sees the
                // whole tree rather than a diff against the previous
                // generation's stat cache.
                let work = |r: Run| r.env("GIT_WORK_TREE", src).cwd(src);
                let read_tree = work(git.git().args(["read-tree", "--empty"]));
                let add = work(git.add_all());
                let write_tree = work(git.git().arg("write-tree"));
                let mut usage = crate::util::proc::Usage::default();
                let mut commands = Vec::new();
                for cmd in [&read_tree, &add, &write_tree] {
                    commands.push(cmd.display());
                }
                for cmd in [&read_tree, &add] {
                    match cmd.run() {
                        Ok(out) if out.success() => usage = usage.add(out.usage),
                        Ok(out) => {
                            return Op::failed(op, out.require_success().unwrap_err())
                                .describe(desc)
                                .commands(commands);
                        }
                        Err(e) => return Op::failed(op, e).describe(desc).commands(commands),
                    }
                }
                let tree = match write_tree.run() {
                    Ok(out) if out.success() => {
                        usage = usage.add(out.usage);
                        out.last_stdout_line()
                    }
                    Ok(out) => {
                        return Op::failed(op, out.require_success().unwrap_err())
                            .describe(desc)
                            .commands(commands);
                    }
                    Err(e) => return Op::failed(op, e).describe(desc).commands(commands),
                };
                let commit = work(git.at(crate::dataset::corpus::BASE_MTIME).args([
                    "commit-tree",
                    &tree,
                    "-m",
                    name,
                ]));
                commands.push(commit.display());
                let sha = match commit.run() {
                    Ok(out) if out.success() => {
                        usage = usage.add(out.usage);
                        out.last_stdout_line()
                    }
                    Ok(out) => {
                        return Op::failed(op, out.require_success().unwrap_err())
                            .describe(desc)
                            .commands(commands);
                    }
                    Err(e) => return Op::failed(op, e).describe(desc).commands(commands),
                };
                let update =
                    work(
                        git.git()
                            .args(["update-ref", &format!("refs/heads/{name}"), &sha]),
                    );
                commands.push(update.display());
                match update.run() {
                    Ok(out) if out.success() => usage = usage.add(out.usage),
                    Ok(out) => {
                        return Op::failed(op, out.require_success().unwrap_err())
                            .describe(desc)
                            .commands(commands);
                    }
                    Err(e) => return Op::failed(op, e).describe(desc).commands(commands),
                }
                Op::measured(op, usage).describe(desc).commands(commands)
            }
            Kind::Restic {
                cli,
                snapshots,
                sources,
            } => {
                let out = measured(op, desc, &cli.backup(src, name, ctx.jobs));
                if out.status == OpStatus::Ok {
                    sources.insert(name.to_string(), src.display().to_string());
                    // Resolved by the snapshot's own unique tag, not by
                    // position in a listing that also contains every other
                    // snapshot this repetition took.
                    match resolve_snapshot(cli, name) {
                        Ok(id) => {
                            snapshots.insert(name.to_string(), id);
                        }
                        Err(e) => {
                            return Op::failed(
                                op,
                                format!(
                                    "the backup succeeded but its snapshot could \
                                     not be identified, so nothing later could \
                                     name it: {e}"
                                ),
                            )
                            .describe(desc);
                        }
                    }
                }
                out
            }
        }
    }

    /// The id the backend knows a snapshot by.
    ///
    /// For restic this was resolved from the snapshot's unique tag when the
    /// backup was taken. It deliberately does *not* fall back to `latest`:
    /// silently operating on whatever happens to be newest is how a
    /// benchmark ends up measuring the wrong snapshot. An unresolved name is
    /// passed through, which makes the command that uses it fail loudly.
    fn snapshot_id(&self, name: &str) -> String {
        match &self.kind {
            Kind::Restic { snapshots, .. } => snapshots
                .get(name)
                .cloned()
                .unwrap_or_else(|| format!("tag:{name}:unresolved")),
            _ => name.to_string(),
        }
    }

    fn list_recursive(&self, name: &str) -> Op {
        match &self.kind {
            Kind::Amber(cli) => match cli.list_recursive(&format!("ref:{name}")) {
                Ok(l) => Op::measured("list_recursive", l.usage)
                    .describe("enumerate the whole stored tree")
                    .counter("entries", l.entries as i64)
                    .counter("directories", l.directories as i64)
                    .counter("cli_invocations", l.invocations as i64)
                    .note(format!(
                        "neither Amber CLI has a recursive listing, so this is \
                         {} separate `amber-store ls --keys` processes, one per \
                         directory; the number therefore includes {} process \
                         startups and is not comparable with a single-process \
                         listing without allowing for them",
                        l.invocations, l.invocations
                    )),
                Err(e) => Op::failed("list_recursive", e),
            },
            Kind::Git(git) => measured(
                "list_recursive",
                "enumerate the whole stored tree",
                &git.ls_tree(name).stdout_to("/dev/null"),
            ),
            Kind::Restic { cli, .. } => measured(
                "list_recursive",
                "enumerate the whole stored tree",
                &cli.ls(&self.snapshot_id(name)).stdout_to("/dev/null"),
            ),
            Kind::FsCas(cli) => measured(
                "list_recursive",
                "enumerate the whole stored tree",
                &cli.list(name).stdout_to("/dev/null"),
            ),
        }
    }

    fn read_full(&self, name: &str, devnull: &Path) -> Op {
        match &self.kind {
            Kind::Amber(cli) => measured(
                "read_full",
                "stream the tree out",
                &cli.export_stdout(&format!("ref:{name}"), devnull),
            ),
            Kind::Git(git) => measured(
                "read_full",
                "stream the tree out",
                &git.git()
                    .args(["archive", "--format=tar", name])
                    .stdout_to(devnull),
            ),
            Kind::Restic { cli, .. } => measured(
                "read_full",
                "stream the tree out",
                &cli.restic()
                    .args(["dump", "--archive", "tar", &self.snapshot_id(name), "/"])
                    .stdout_to(devnull),
            ),
            Kind::FsCas(cli) => measured(
                "read_full",
                "stream the tree out",
                &cli.read(name).stdout_to(devnull),
            ),
        }
    }

    /// Steps that put the destination and any index into a known state.
    /// Deliberately outside the measured window.
    fn restore_prep(&self, dest: &Path) -> Vec<Run> {
        let _ = std::fs::create_dir_all(dest);
        match &self.kind {
            // Git checks out only what differs from its index, so an index
            // left over from the last generation would make the restore look
            // free. Emptying it first is what makes the comparison honest.
            Kind::Git(git) => vec![git.git().args(["read-tree", "--empty"])],
            _ => Vec::new(),
        }
    }

    /// Where the restored tree actually lands. restic recreates the source's
    /// absolute path inside the target unless the snapshot subtree is named,
    /// which the harness does, so every backend lands at `dest`.
    fn restore_root(&self, dest: &Path, _src: &Path) -> PathBuf {
        dest.to_path_buf()
    }

    fn restore(&self, name: &str, dest: &Path) -> Op {
        match &self.kind {
            Kind::Amber(cli) => measured(
                "restore_full",
                "materialise the tree",
                &cli.restore(&format!("ref:{name}"), dest),
            ),
            Kind::Git(git) => measured_many(
                "restore_full",
                "materialise the tree",
                &[git
                    .git()
                    .env("GIT_WORK_TREE", dest)
                    .cwd(dest)
                    .args(["checkout", "--force", name, "--", "."])],
            ),
            Kind::Restic { cli, .. } => {
                // `snapshot:absolute-path` restores that subtree's *contents*
                // into --target, so the result is directly comparable with
                // every other backend's.
                let spec = format!("{}:{}", self.snapshot_id(name), self.restic_source(name));
                measured(
                    "restore_full",
                    "materialise the tree",
                    &cli.restic()
                        .args(["restore", &spec])
                        .arg("--target")
                        .arg(dest),
                )
            }
            Kind::FsCas(cli) => measured(
                "restore_full",
                "materialise the tree",
                &cli.restore(name, dest),
            ),
        }
    }

    /// The absolute path restic recorded for a reference, filled in by the
    /// scenario before any restore happens.
    fn restic_source(&self, name: &str) -> String {
        match &self.kind {
            Kind::Restic { sources, .. } => sources
                .get(name)
                .cloned()
                .unwrap_or_else(|| "/".to_string()),
            _ => "/".to_string(),
        }
    }

    /// Drops every one of `names`, charged to a single operation: the
    /// retention step the collection measurement depends on.
    fn ref_delete(&self, names: &[&str]) -> Op {
        const OP: &str = "ref_delete";
        const DESC: &str = "drop references";
        match &self.kind {
            Kind::Amber(cli) => {
                let cmds: Vec<Run> = names.iter().map(|n| cli.ref_rm(n)).collect();
                measured_many(OP, DESC, &cmds)
            }
            Kind::Git(git) => {
                let cmds: Vec<Run> = names
                    .iter()
                    .map(|n| {
                        git.git()
                            .args(["update-ref", "-d", &format!("refs/heads/{n}")])
                    })
                    .collect();
                measured_many(OP, DESC, &cmds)
            }
            Kind::Restic { cli, .. } => {
                // restic takes several ids in one invocation, which is what a
                // retention policy would really run.
                let ids: Vec<String> = names.iter().map(|n| self.snapshot_id(n)).collect();
                let mut cmd = cli.restic().arg("forget");
                for id in &ids {
                    cmd = cmd.arg(id);
                }
                measured(OP, DESC, &cmd)
            }
            Kind::FsCas(cli) => {
                let cmds: Vec<Run> = names.iter().map(|n| cli.rm(n)).collect();
                measured_many(OP, DESC, &cmds)
            }
        }
    }

    /// What the store itself says it still retains.
    ///
    /// The command has to succeed: a listing that failed proves nothing about
    /// what is or is not there, and must never be read as "the snapshot is
    /// gone".
    fn retained_references(&self) -> Result<Vec<String>, String> {
        match &self.kind {
            Kind::Amber(cli) => {
                let out = cli.ref_list().ok()?;
                Ok(crate::adapters::amber::parse_ref_list(&out.stdout_text()))
            }
            Kind::Git(git) => {
                let out = git
                    .git()
                    .args(["for-each-ref", "--format=%(refname:strip=2)", "refs/heads"])
                    .ok()?;
                Ok(out
                    .stdout_text()
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect())
            }
            Kind::Restic { cli, .. } => {
                let out = cli.snapshots().ok()?;
                restic::parse_snapshot_tags(&out.stdout_text())
            }
            Kind::FsCas(cli) => {
                let out = cli.refs().ok()?;
                Ok(out
                    .stdout_text()
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect())
            }
        }
    }

    fn gc(&self) -> Op {
        match &self.kind {
            Kind::Amber(cli) => measured("gc", "collect unreferenced storage", &cli.gc_run()),
            Kind::Git(git) => measured("gc", "collect unreferenced storage", &git.gc()),
            Kind::Restic { cli, .. } => {
                measured("gc", "collect unreferenced storage", &cli.prune())
            }
            Kind::FsCas(cli) => measured("gc", "collect unreferenced storage", &cli.gc()),
        }
    }

    fn integrity(&self) -> Vec<Run> {
        match &self.kind {
            // Exporting the whole tree re-reads and CRC-checks every record
            // the tree touches, which is the check these CLIs offer.
            Kind::Amber(cli) => {
                vec![cli.export_stdout(&format!("ref:{REF1}"), Path::new("/dev/null"))]
            }
            Kind::Git(git) => vec![git.fsck()],
            Kind::Restic { cli, .. } => vec![cli.check(true)],
            Kind::FsCas(cli) => vec![cli.check()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenarios::{RetentionEvidence, check_retention};

    fn evidence(listing: Result<Vec<String>, String>) -> RetentionEvidence {
        RetentionEvidence {
            listing,
            still_readable: Vec::new(),
        }
    }

    #[test]
    fn generation_zero_is_stored_under_two_distinct_references() {
        // Both are dropped before the collection measurement: leaving either
        // in place would keep generation 0 reachable, and then collection
        // could not reclaim anything it held.
        assert_ne!(REF0, REF0_REPEAT);
        assert_ne!(REF0, REF1);
        assert_ne!(REF0_REPEAT, REF1);
    }

    #[test]
    fn retention_is_judged_from_the_stores_own_listing() {
        let v = check_retention(
            "retention_took_effect",
            evidence(Ok(vec!["gen1".into()])),
            &[REF1],
            &[REF0, REF0_REPEAT],
        );
        assert!(v.passed, "{v:?}");
        assert!(v.detail.contains("gen1"), "{}", v.detail);
    }

    #[test]
    fn a_reference_that_survived_retention_fails_the_check() {
        let v = check_retention(
            "retention_took_effect",
            evidence(Ok(vec!["gen0".into(), "gen0-again".into(), "gen1".into()])),
            &[REF1],
            &[REF0, REF0_REPEAT],
        );
        assert!(!v.passed);
        assert_eq!(v.corrupt.len(), 2, "{:?}", v.corrupt);
        assert!(v.corrupt[0].contains("still lists it"), "{:?}", v.corrupt);
    }

    #[test]
    fn a_retained_reference_that_vanished_also_fails_the_check() {
        let v = check_retention(
            "retention_took_effect",
            evidence(Ok(vec![])),
            &[REF1],
            &[REF0],
        );
        assert!(!v.passed);
        assert!(
            v.corrupt[0].contains("should have been retained"),
            "{:?}",
            v.corrupt
        );
    }

    #[test]
    fn a_failed_listing_is_never_read_as_proof_of_absence() {
        // "the command failed" and "the snapshot is gone" are different
        // findings, and only one of them is a pass.
        let v = check_retention(
            "retention_took_effect",
            evidence(Err("restic: connection refused".into())),
            &[REF1],
            &[REF0],
        );
        assert!(!v.passed);
        assert!(
            v.detail.contains("nothing can be concluded"),
            "{}",
            v.detail
        );
        assert!(v.detail.contains("connection refused"), "{}", v.detail);
    }
}
