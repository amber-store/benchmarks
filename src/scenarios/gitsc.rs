//! The source-history workload: Git against the Amber cores.
//!
//! Both sides are handed the same sequence of working-tree states and asked
//! to retain every one of them. Git records them as commits on branches with
//! tags and a merge; the Amber cores record each one as a reference to a root
//! key. Then both are asked to hand every retained version back, and every
//! version that comes back is compared against a manifest observed from the
//! staging tree when it was generated.
//!
//! Where Git can do something the cores have no equivalent for — cloning over
//! a protocol, fetching incrementally — the operation is recorded as
//! unsupported with the reason, not as a zero and not as an omission. The
//! blob group measures what the cores *can* do to publish to remote storage.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::adapters::amber::AmberCli;
use crate::adapters::git::GitCli;
use crate::adapters::semantics;
use crate::dataset::gitgen;
use crate::dataset::manifest::{Manifest, Strictness};
use crate::metrics::{Op, OpStatus, Phase, RunRecord, Verification};
use crate::util::fsx;
use crate::util::proc::{Run, Usage};

use super::{
    Ctx, Recorder, aside, measured, measured_many, reclaimed, store_counters, store_sizes,
    versions_to_verify,
};

/// Scenario group name.
pub const GROUP: &str = "git";
/// Scenario name.
pub const SCENARIO: &str = "git/history";
/// Every backend this scenario can run.
pub const BACKENDS: [&str; 3] = ["git", "amber-rust", "amber-go"];

enum Kind {
    Git(GitCli),
    Amber(AmberCli),
}

struct Backend {
    kind: Kind,
    store: PathBuf,
    work: PathBuf,
    strict: Strictness,
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
    let be = match build(ctx, backend, &dir) {
        Ok(b) => b,
        Err(e) => return rec.abort(e),
    };
    if let Err(e) = std::fs::create_dir_all(&be.work) {
        return rec.abort(format!("{}: {e}", be.work.display()));
    }
    for op in be.setup() {
        let failed = op.status == OpStatus::Failed;
        rec.push(op);
        if failed {
            return rec.abort(format!("{backend}: setup failed"));
        }
    }
    for note in be.strict.omissions() {
        rec.note(format!(
            "retained versions are not compared on {note}: this backend does not store it"
        ));
    }

    let steps = ctx.history.steps.len();
    // Three quarters of the history first, so the rest is a genuine
    // incremental addition to a repository that already holds most of it.
    let split = (steps * 3 / 4).max(1);

    let (op, setup) = be.record_range(ctx, 0, split, "history_store");
    rec.push(setup);
    let failed = op.status == OpStatus::Failed;
    rec.push(
        op.logical(history_bytes(ctx, 0, split))
            .store(store_sizes(&be.store))
            .counters(store_counters(&be.store))
            .counter("commits", split as i64),
    );
    if failed {
        return rec.abort(format!("{backend}: recording the history failed"));
    }

    // Clone before the rest of the history exists, so the later fetch has
    // something to fetch.
    let clone_dir = dir.join("clone");
    let op = be.clone_full(&clone_dir);
    let cloned = op.status == OpStatus::Ok;
    rec.push(op);
    // The clone itself is verified, not just the repository it came from: a
    // checkout of the *source* proves nothing about what arrived at the
    // destination.
    if let Some(v) = be.verify_transfer(ctx, &clone_dir, split, cloned, "clone_matches_source") {
        rec.verify(v);
    }

    let (op, setup) = be.record_range(ctx, split, steps, "history_store_incremental");
    rec.push(setup);
    rec.push(
        op.logical(history_bytes(ctx, split, steps))
            .store(store_sizes(&be.store))
            .counters(store_counters(&be.store))
            .counter("commits", (steps - split) as i64),
    );

    let op = be.fetch_incremental(&clone_dir);
    let fetched = op.status == OpStatus::Ok;
    rec.push(op);
    if let Some(v) = be.verify_transfer(
        ctx,
        &clone_dir,
        steps,
        fetched,
        "incremental_fetch_matches_source",
    ) {
        rec.verify(v);
    }

    let before_gc = store_sizes(&be.store);

    let mut gc_op = be.gc();
    let after = store_sizes(&be.store);
    gc_op = gc_op
        .store(after)
        .reclaimed(reclaimed(&before_gc, &after))
        .describe("repack and drop unreachable objects");
    rec.push(gc_op);

    let head = ctx.history.steps.len() - 1;
    rec.push(be.list_head(ctx, head));
    rec.push(
        be.read_all_versions(ctx)
            .logical(history_bytes(ctx, 0, steps))
            .describe("stream every retained version out of the store"),
    );

    let checkout = dir.join("checkout-head");
    let _ = std::fs::create_dir_all(&checkout);
    rec.push(be.checkout(ctx, head, &checkout));
    rec.verify(verify_version(
        ctx,
        head,
        &checkout,
        be.strict,
        "head_version_matches_manifest",
    ));

    // Every retained version must still be there. By default that means
    // every single one; only the large profile samples, and then the check
    // is named and counted as a sample rather than claiming more than it
    // checked.
    let budget = ctx.profile.verify_version_budget;
    let picked = match budget {
        None => (0..steps).collect::<Vec<usize>>(),
        Some(b) => versions_to_verify(steps, b),
    };
    let sampled = picked.len() < steps;
    let mut bad = Vec::new();
    for i in &picked {
        let out = dir.join(format!("verify-{i:04}"));
        let _ = std::fs::create_dir_all(&out);
        let op = be.checkout(ctx, *i, &out);
        let ok = op.status == OpStatus::Ok;
        let mut op = op.describe(format!("restore retained version {i}"));
        op.phase = Phase::Verify;
        op.name = "retained_version_restore".into();
        rec.push(op);
        let v = if ok {
            verify_version(
                ctx,
                *i,
                &out,
                be.strict,
                "retained_version_matches_manifest",
            )
        } else {
            Verification::fail(
                "retained_version_matches_manifest",
                format!("version {i} could not be restored at all"),
            )
        };
        if !v.passed {
            bad.push(format!("{i}: {}", v.detail));
        }
        let _ = crate::util::fsx::make_writable_tree(&out);
        let _ = std::fs::remove_dir_all(&out);
    }
    let check_name = if sampled {
        "sampled_retained_versions_match"
    } else {
        "all_retained_versions_match"
    };
    rec.verify(if bad.is_empty() {
        Verification::pass(
            check_name,
            if sampled {
                format!(
                    "{} of {steps} retained versions were restored and compared \
                     byte for byte; {} version(s) were NOT verified, because \
                     profile {:?} samples them. This is not a statement about \
                     the unverified versions.",
                    picked.len(),
                    steps - picked.len(),
                    ctx.profile.name,
                )
            } else {
                format!(
                    "every one of the {steps} retained versions was restored \
                     and compared byte for byte; 0 unverified"
                )
            },
        )
    } else {
        let mut v = Verification::fail(
            check_name,
            format!(
                "{} of {} verified versions did not match ({} of {steps} not \
                 verified at all)",
                bad.len(),
                picked.len(),
                steps - picked.len(),
            ),
        );
        v.corrupt = bad;
        v
    });
    rec.push(
        Op::aside(
            "retained_version_coverage",
            Phase::Verify,
            Default::default(),
        )
        .describe("how much of the retained history was checked back out")
        .counter("versions_retained", steps as i64)
        .counter("versions_verified", picked.len() as i64)
        .counter("versions_unverified", (steps - picked.len()) as i64),
    );

    rec.push(be.ref_delete(ctx));

    for cmd in be.integrity(ctx) {
        rec.push(aside(
            "integrity_check",
            Phase::Verify,
            "the backend's own consistency check",
            &cmd,
        ));
    }

    if let Kind::Amber(cli) = &be.kind {
        for i in [0usize, head] {
            if let Ok(out) = cli.ref_get(&ctx.history.steps[i].ref_name).ok() {
                rec.root(&ctx.history.steps[i].ref_name, &out.last_stdout_line());
            }
        }
    }

    rec.finish()
}

fn history_bytes(ctx: &Ctx, from: usize, to: usize) -> u64 {
    (from..to).map(|i| ctx.history.state_bytes(i)).sum()
}

fn verify_version(
    ctx: &Ctx,
    index: usize,
    root: &Path,
    strict: Strictness,
    name: &str,
) -> Verification {
    let Some(path) = ctx.history_manifests.get(index) else {
        return Verification::fail(name, format!("no reference manifest for version {index}"));
    };
    match Manifest::load(path) {
        Ok(m) => m.verify_tree(name, root, strict),
        Err(e) => Verification::fail(name, format!("reference manifest {}: {e}", path.display())),
    }
}

fn build(ctx: &Ctx, backend: &str, dir: &Path) -> Result<Backend, String> {
    match backend {
        "git" => Ok(Backend {
            kind: Kind::Git(GitCli {
                bin: ctx.tools.path("git")?,
                repo: dir.join("work"),
                timeout: ctx.timeout,
            }),
            store: dir.join("work/.git"),
            work: dir.join("work"),
            strict: Strictness::git(),
        }),
        "amber-rust" | "amber-go" => Ok(Backend {
            kind: Kind::Amber(AmberCli {
                bin: ctx.tools.path(backend)?,
                store: dir.join("store"),
                segment_size: ctx.profile.segment_size,
                chunk: ctx.profile.chunk,
                jobs: ctx.jobs,
                timeout: ctx.timeout,
            }),
            store: dir.join("store"),
            work: dir.join("work"),
            strict: Strictness::full(),
        }),
        other => Err(format!("git scenario: unknown backend {other}")),
    }
}

impl Backend {
    fn setup(&self) -> Vec<Op> {
        match &self.kind {
            Kind::Git(git) => git
                .init(false)
                .iter()
                .map(|c| aside("git_init", Phase::Setup, "create the repository", c))
                .collect(),
            Kind::Amber(_) => Vec::new(),
        }
    }

    /// Records steps `from..to`. Materialising each working-tree state is
    /// setup and is returned separately; only the backend's own commands are
    /// charged to the measured operation.
    /// The backend's commands for recording one already-materialised state.
    /// Records steps `from..to`.
    ///
    /// Each step is: run the commands that move the repository onto the right
    /// branch (Git only), write the working-tree state, then run the commands
    /// that record it. Materialising the files is setup and is returned as a
    /// separate operation; only the backend's own commands are charged to the
    /// measured one.
    ///
    /// The order matters. A branch switch rewrites the working tree, so it has
    /// to happen *before* the state is written, and the state then has to be
    /// written as a delta against whatever the switch left behind — which is
    /// exactly how a developer's checkout behaves, and what makes `git add -A`
    /// see a handful of changed paths instead of a fresh import.
    fn record_range(&self, ctx: &Ctx, from: usize, to: usize, op_name: &str) -> (Op, Op) {
        let mut usage = Usage::default();
        let mut setup_wall = 0u64;
        let mut commands: Vec<String> = Vec::new();
        let run_all =
            |cmds: Vec<Run>, usage: &mut Usage, commands: &mut Vec<String>| -> Result<(), String> {
                for cmd in cmds {
                    commands.push(cmd.display());
                    match cmd.run() {
                        Ok(out) if out.success() => *usage = usage.add(out.usage),
                        Ok(out) => return Err(out.require_success().unwrap_err()),
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            };

        for i in from..to {
            let (pre, post) = self.step_commands(ctx, i);
            if let Err(e) = run_all(pre, &mut usage, &mut commands) {
                return (
                    Op::failed(op_name, e).commands(commands),
                    setup_op(setup_wall),
                );
            }
            let t = Instant::now();
            let base = self.base_state(ctx, i);
            if let Err(e) = ctx.history.apply(i, base, &self.work) {
                return (
                    Op::failed(op_name, format!("materialising state {i}: {e}")).commands(commands),
                    setup_op(setup_wall),
                );
            }
            if matches!(self.kind, Kind::Amber(_))
                && let Err(e) = gitgen::normalize_mtimes(&self.work)
            {
                return (
                    Op::failed(op_name, format!("normalising state {i}: {e}")).commands(commands),
                    setup_op(setup_wall),
                );
            }
            setup_wall += t.elapsed().as_nanos() as u64;
            if let Err(e) = run_all(post, &mut usage, &mut commands) {
                return (
                    Op::failed(op_name, e).commands(commands),
                    setup_op(setup_wall),
                );
            }
        }
        let summary: Vec<String> = commands.iter().take(6).cloned().collect();
        let total = commands.len();
        (
            Op::measured(op_name, usage)
                .describe(format!("record working-tree states {from}..{to}"))
                .commands(summary)
                .counter("backend_commands", total as i64)
                .note(format!(
                    "{total} commands in total; the first few are listed"
                )),
            setup_op(setup_wall),
        )
    }

    /// Which state's files the working tree holds just before step `i` is
    /// written. Normally the previous step's; after a branch switch, the tip
    /// of the branch that was switched to.
    fn base_state(&self, ctx: &Ctx, i: usize) -> Option<usize> {
        if i == 0 {
            return None;
        }
        if matches!(self.kind, Kind::Amber(_)) {
            return Some(i - 1);
        }
        let step = &ctx.history.steps[i];
        if step.fork_from.is_some() || ctx.history.steps[i - 1].branch == step.branch {
            // Creating a branch does not touch the working tree.
            return Some(i - 1);
        }
        // A switch landed on the tip of step.branch.
        ctx.history.steps[..i]
            .iter()
            .rposition(|s| s.branch == step.branch)
            .or(Some(i - 1))
    }

    /// The commands to run before materialising step `i`, and after.
    fn step_commands(&self, ctx: &Ctx, i: usize) -> (Vec<Run>, Vec<Run>) {
        let step = &ctx.history.steps[i];
        match &self.kind {
            Kind::Amber(cli) => (
                Vec::new(),
                vec![cli.ingest(&self.work, Some(&step.ref_name))],
            ),
            Kind::Git(git) => {
                let mut pre = Vec::new();
                if i > 0 {
                    if step.fork_from.is_some() {
                        pre.push(git.branch_create(&step.branch));
                    } else if ctx.history.steps[i - 1].branch != step.branch {
                        pre.push(git.switch(&step.branch));
                    }
                    if let Some(merged) = &step.merge_of {
                        pre.push(git.merge_ours(merged, step.timestamp));
                    }
                }
                let mut post = vec![git.add_all(), git.commit(&step.subject, step.timestamp)];
                if let Some((tag, annotated)) = &step.tag {
                    post.push(git.tag(tag, *annotated, step.timestamp));
                }
                (pre, post)
            }
        }
    }

    fn clone_full(&self, dest: &Path) -> Op {
        match &self.kind {
            Kind::Git(git) => measured(
                "clone_full",
                "clone the whole repository over the local transport",
                &git.clone_to(&format!("file://{}", self.work.display()), dest, false),
            ),
            Kind::Amber(_) => Op::unsupported(
                "clone_full",
                "these cores ship no repository-to-repository transfer command; \
                 what they can do to publish a store is measured in the blob \
                 scenario group as a file-level object-storage transport",
            )
            .describe("clone the whole repository"),
        }
    }

    fn fetch_incremental(&self, clone: &Path) -> Op {
        match &self.kind {
            Kind::Git(git) => {
                let cli = GitCli {
                    bin: git.bin.clone(),
                    repo: clone.to_path_buf(),
                    timeout: git.timeout,
                };
                measured(
                    "fetch_incremental",
                    "fetch only the commits added since the clone",
                    &cli.fetch("origin"),
                )
            }
            Kind::Amber(_) => Op::unsupported(
                "fetch_incremental",
                "these cores ship no incremental fetch command; see the blob \
                 scenario group for incremental object-storage publication",
            )
            .describe("fetch only what is new"),
        }
    }

    /// Verifies a transfer at its *destination*.
    ///
    /// Returns `None` for a backend that has no transfer to verify — the
    /// unsupported operation already says so, and inventing a check for it
    /// would be worse than having none.
    ///
    /// For Git this checks three separate things about the clone itself:
    /// that its object database is intact, that every branch and tag of the
    /// source arrived with the same commit id, and that a working tree
    /// checked out *from the clone* matches the manifest observed when that
    /// state was generated.
    fn verify_transfer(
        &self,
        ctx: &Ctx,
        clone: &Path,
        upto: usize,
        performed: bool,
        name: &str,
    ) -> Option<Verification> {
        let Kind::Git(git) = &self.kind else {
            return None;
        };
        if !performed {
            return Some(Verification::fail(
                name,
                "the transfer failed, so there was nothing at the destination \
                 to verify",
            ));
        }
        let dest = GitCli {
            bin: git.bin.clone(),
            repo: clone.to_path_buf(),
            timeout: git.timeout,
        };
        let mut problems = Vec::new();

        // 1. The destination's own object database.
        if let Err(e) = dest.fsck().ok() {
            problems.push(format!("git fsck failed in the clone: {}", first_line(&e)));
        }

        // 2. Every ref of the source, by commit id, as the clone holds it.
        match (branch_ids(git), destination_branch_ids(&dest)) {
            (Ok(source), Ok(mirror)) => {
                for (branch, id) in &source {
                    match mirror.get(branch) {
                        Some(got) if got == id => {}
                        Some(got) => problems.push(format!(
                            "branch {branch}: clone is at {got}, source at {id}"
                        )),
                        None => problems.push(format!("branch {branch} never arrived")),
                    }
                }
                for branch in mirror.keys() {
                    if !source.contains_key(branch) {
                        problems.push(format!("branch {branch} exists only in the clone"));
                    }
                }
            }
            (Err(e), _) | (_, Err(e)) => problems.push(format!("listing refs: {}", first_line(&e))),
        }
        match (tag_ids(git), tag_ids(&dest)) {
            (Ok(source), Ok(mirror)) => {
                for (tag, id) in &source {
                    match mirror.get(tag) {
                        Some(got) if got == id => {}
                        Some(got) => {
                            problems.push(format!("tag {tag}: clone has {got}, source {id}"))
                        }
                        None => problems.push(format!("tag {tag} never arrived")),
                    }
                }
            }
            (Err(e), _) | (_, Err(e)) => problems.push(format!("listing tags: {}", first_line(&e))),
        }

        // 3. A working tree materialised out of the clone.
        let index = clone.join("verify.index");
        let work = clone.join("verify-worktree");
        let _ = fsx::make_writable_tree(&work);
        let _ = std::fs::remove_dir_all(&work);
        let mut checked_out = None;
        if let Some(tip) = main_tip(ctx, upto) {
            if std::fs::create_dir_all(&work).is_err() {
                problems.push(format!("could not create {}", work.display()));
            } else if let Err(e) = dest
                .git()
                .env("GIT_WORK_TREE", &work)
                .env("GIT_INDEX_FILE", &index)
                .args(["checkout", "--force", "origin/main", "--", "."])
                .ok()
            {
                problems.push(format!("checking the clone out failed: {}", first_line(&e)));
            } else {
                let v = verify_version(ctx, tip, &work, self.strict, name);
                if !v.passed {
                    problems.push(format!("the clone's main tip differs: {}", v.detail));
                    problems.extend(v.corrupt.into_iter().take(3));
                    problems.extend(
                        v.missing
                            .into_iter()
                            .take(3)
                            .map(|m| format!("missing {m}")),
                    );
                    problems.extend(v.extra.into_iter().take(3).map(|m| format!("extra {m}")));
                }
                checked_out = Some(tip);
            }
        }
        let _ = fsx::make_writable_tree(&work);
        let _ = std::fs::remove_dir_all(&work);
        let _ = std::fs::remove_file(&index);

        let mut v = if problems.is_empty() {
            Verification::pass(
                name,
                format!(
                    "the destination repository passed `git fsck`, holds every \
                     branch and tag of the source at the same commit id, and a \
                     working tree checked out of it matches the manifest of \
                     history state {}",
                    checked_out
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "—".into())
                ),
            )
        } else {
            Verification::fail(
                name,
                format!("{} problem(s) at the transfer destination", problems.len()),
            )
        };
        v.corrupt = problems;
        Some(v)
    }

    fn gc(&self) -> Op {
        match &self.kind {
            Kind::Git(git) => measured("gc", "repack and prune", &git.gc()),
            Kind::Amber(cli) => measured("gc", "repack and prune", &cli.gc_run()),
        }
    }

    fn list_head(&self, ctx: &Ctx, head: usize) -> Op {
        match &self.kind {
            Kind::Git(git) => measured(
                "list_head",
                "enumerate the head version's tree",
                &git.ls_tree("HEAD").stdout_to("/dev/null"),
            ),
            Kind::Amber(cli) => {
                let spec = format!("ref:{}", ctx.history.steps[head].ref_name);
                match cli.list_recursive(&spec) {
                    Ok(l) => Op::measured("list_head", l.usage)
                        .describe("enumerate the head version's tree")
                        .counter("entries", l.entries as i64)
                        .counter("cli_invocations", l.invocations as i64)
                        .note(format!(
                            "{} separate `amber-store ls` processes, one per \
                             directory: neither core's CLI has a recursive \
                             listing",
                            l.invocations
                        )),
                    Err(e) => Op::failed("list_head", e),
                }
            }
        }
    }

    fn read_all_versions(&self, ctx: &Ctx) -> Op {
        let devnull = Path::new("/dev/null");
        match &self.kind {
            Kind::Git(git) => {
                let cmds: Vec<Run> = (0..ctx.history.steps.len())
                    .map(|i| {
                        git.git()
                            .args(["archive", "--format=tar", &commit_ref(ctx, i)])
                            .stdout_to(devnull)
                    })
                    .collect();
                measured_many("read_all_versions", "stream every version out", &cmds)
            }
            Kind::Amber(cli) => {
                let cmds: Vec<Run> = ctx
                    .history
                    .steps
                    .iter()
                    .map(|s| cli.export_stdout(&format!("ref:{}", s.ref_name), devnull))
                    .collect();
                measured_many("read_all_versions", "stream every version out", &cmds)
            }
        }
    }

    fn checkout(&self, ctx: &Ctx, index: usize, dest: &Path) -> Op {
        match &self.kind {
            Kind::Git(git) => measured(
                "checkout_version",
                "materialise one retained version",
                &git.git()
                    .env("GIT_WORK_TREE", dest)
                    .env("GIT_INDEX_FILE", dest.with_extension("index"))
                    .cwd(dest)
                    .args(["checkout", "--force", &commit_ref(ctx, index), "--", "."]),
            ),
            Kind::Amber(cli) => measured(
                "checkout_version",
                "materialise one retained version",
                &cli.restore(&format!("ref:{}", ctx.history.steps[index].ref_name), dest),
            ),
        }
    }

    fn ref_delete(&self, ctx: &Ctx) -> Op {
        // The unmerged side branch: dropping it is what makes its objects
        // collectable in both models.
        let doomed = ctx
            .history
            .steps
            .iter()
            .rev()
            .find(|s| s.branch == "feature/b");
        match (&self.kind, doomed) {
            (Kind::Git(git), Some(_)) => measured(
                "ref_delete",
                "drop the unmerged side branch",
                &git.git().args(["update-ref", "-d", "refs/heads/feature/b"]),
            ),
            (Kind::Amber(cli), Some(step)) => measured(
                "ref_delete",
                "drop the unmerged side branch's reference",
                &cli.ref_rm(&step.ref_name),
            ),
            _ => Op::failed(
                "ref_delete",
                "the generated history has no feature/b branch",
            ),
        }
    }

    fn integrity(&self, ctx: &Ctx) -> Vec<Run> {
        match &self.kind {
            Kind::Git(git) => vec![git.fsck()],
            Kind::Amber(cli) => {
                let head = ctx.history.steps.len() - 1;
                vec![cli.export_stdout(
                    &format!("ref:{}", ctx.history.steps[head].ref_name),
                    Path::new("/dev/null"),
                )]
            }
        }
    }
}

fn setup_op(wall_ns: u64) -> Op {
    Op::aside(
        "materialise_states",
        Phase::Setup,
        Usage {
            wall_ns,
            ..Usage::default()
        },
    )
    .describe(
        "write the working-tree states the backend then records; excluded \
         from every comparison",
    )
}

/// Commit id of every local branch.
pub fn branch_ids(git: &GitCli) -> Result<BTreeMap<String, String>, String> {
    refs_of(git, "refs/heads")
}

/// Commit id of every tag.
pub fn tag_ids(git: &GitCli) -> Result<BTreeMap<String, String>, String> {
    refs_of(git, "refs/tags")
}

/// What branches a *destination* repository holds, whether it put them in
/// `refs/heads` or in `refs/remotes/origin`.
///
/// A clone stores the branches it received as remote-tracking refs and
/// creates a local branch only for `HEAD`, while a clone from a bundle can
/// do either; a comparison with the source has to accept both or it would
/// report an arrival as a loss.
pub fn destination_branch_ids(git: &GitCli) -> Result<BTreeMap<String, String>, String> {
    let mut out = refs_of(git, "refs/heads")?;
    for (name, id) in remote_refs_of(git)? {
        out.insert(name, id);
    }
    Ok(out)
}

/// Commit id of every ref under `namespace`, keyed by its short name.
fn refs_of(git: &GitCli, namespace: &str) -> Result<BTreeMap<String, String>, String> {
    let out = git
        .git()
        .args([
            "for-each-ref",
            "--format=%(objectname) %(refname:strip=2)",
            namespace,
        ])
        .ok()?;
    Ok(parse_ref_lines(&out.stdout_text()))
}

/// The same, for the remote-tracking refs a clone keeps its branches in.
fn remote_refs_of(git: &GitCli) -> Result<BTreeMap<String, String>, String> {
    let out = git
        .git()
        .args([
            "for-each-ref",
            "--format=%(objectname) %(refname:strip=3)",
            "refs/remotes/origin",
        ])
        .ok()?;
    let mut refs = parse_ref_lines(&out.stdout_text());
    // `origin/HEAD` is a symbolic alias, not a branch of its own.
    refs.remove("HEAD");
    Ok(refs)
}

/// Parses `objectname name` lines into a map.
pub fn parse_ref_lines(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(id), Some(name)) = (parts.next(), parts.next()) {
            out.insert(name.to_string(), id.to_string());
        }
    }
    out
}

/// The last main-line step among the first `upto` steps: the commit a clone
/// or fetch of that range leaves `origin/main` pointing at.
pub fn main_tip(ctx: &Ctx, upto: usize) -> Option<usize> {
    ctx.history.steps[..upto.min(ctx.history.steps.len())]
        .iter()
        .rposition(|s| s.branch == "main")
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// A revision that names step `i` of the generated history in a Git
/// repository. The harness tags nothing extra, so the branch tip is used for
/// the last step of each branch and `@{n}` walks back from it otherwise.
fn commit_ref(ctx: &Ctx, i: usize) -> String {
    let branch = &ctx.history.steps[i].branch;
    let mut behind = 0;
    for later in ctx.history.steps.iter().skip(i + 1) {
        if &later.branch == branch {
            behind += 1;
        }
    }
    if behind == 0 {
        branch.clone()
    } else {
        format!("{branch}~{behind}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backend_with_no_clone_says_so_instead_of_reporting_zero() {
        let be = Backend {
            kind: Kind::Amber(AmberCli {
                bin: PathBuf::from("/bin/amber-store"),
                store: PathBuf::from("/s"),
                segment_size: 1,
                chunk: crate::config::ChunkSettings::shared(),
                jobs: 1,
                timeout: std::time::Duration::from_secs(1),
            }),
            store: PathBuf::from("/s"),
            work: PathBuf::from("/w"),
            strict: Strictness::full(),
        };
        let op = be.clone_full(Path::new("/dest"));
        assert_eq!(op.status, OpStatus::Unsupported);
        assert!(op.wall_ns.is_none());
        assert!(op.reason.as_ref().unwrap().contains("blob"));
        let op = be.fetch_incremental(Path::new("/dest"));
        assert_eq!(op.status, OpStatus::Unsupported);
        // And there is no transfer destination to verify, rather than a
        // vacuous check that passes.
        assert!(
            be.verify_transfer(&ctx(), Path::new("/dest"), 1, true, "clone_matches_source")
                .is_none()
        );
    }

    fn ctx() -> Ctx {
        let profile = crate::config::profile("smoke").unwrap();
        let history = gitgen::plan(&profile.git, 7);
        Ctx {
            seed: 7,
            jobs: 1,
            scratch: PathBuf::from("/scratch"),
            out: PathBuf::from("/out"),
            tools: Default::default(),
            corpus: crate::dataset::corpus::Corpus {
                root: PathBuf::from("/scratch/corpus"),
                seed: 7,
                spec: profile.corpus,
                generations: vec![],
            },
            history,
            history_manifests: vec![],
            nix: None,
            timeout: std::time::Duration::from_secs(1),
            dropped_caches: false,
            profile,
        }
    }

    #[test]
    fn the_acceptance_profiles_verify_every_retained_version() {
        for name in ["smoke", "standard"] {
            assert_eq!(
                crate::config::profile(name).unwrap().verify_version_budget,
                None,
                "{name} must not claim more than it checked"
            );
        }
        // The large profile may sample, and then says so in the check's name.
        let large = crate::config::profile("large").unwrap();
        let budget = large.verify_version_budget.expect("a budget");
        assert!(budget >= 8, "a sample still has to span the history");
    }

    #[test]
    fn the_check_is_named_for_what_it_actually_verified() {
        let ctx = ctx();
        let steps = ctx.history.steps.len();
        // With no budget every version is verified, so the name may say so.
        let all: Vec<usize> = (0..steps).collect();
        assert_eq!(all.len(), steps);
        // With a budget smaller than the history, it is a sample.
        let sample = versions_to_verify(steps, 4);
        assert!(sample.len() < steps, "{sample:?}");
        assert_eq!(*sample.first().unwrap(), 0);
        assert_eq!(*sample.last().unwrap(), steps - 1);
    }

    #[test]
    fn the_main_tip_of_a_transferred_range_is_the_last_main_line_commit() {
        let ctx = ctx();
        let steps = ctx.history.steps.len();
        let tip = main_tip(&ctx, steps).expect("a main tip");
        assert_eq!(ctx.history.steps[tip].branch, "main");
        assert!(
            ctx.history.steps[tip + 1..]
                .iter()
                .all(|s| s.branch != "main"),
            "nothing on main may follow the tip"
        );
        // A partial range picks the tip within that range, not the final one.
        let split = (steps * 3 / 4).max(1);
        let early = main_tip(&ctx, split).expect("a main tip");
        assert!(early < split);
        assert_eq!(ctx.history.steps[early].branch, "main");
        assert!(early <= tip);
        assert!(main_tip(&ctx, 0).is_none());
    }

    #[test]
    fn ref_listings_are_parsed_into_commit_ids() {
        let text = "aaaa1111 main\nbbbb2222 feature/a\ncccc3333 v1.0\n";
        let refs = parse_ref_lines(text);
        assert_eq!(refs.get("main").map(String::as_str), Some("aaaa1111"));
        assert_eq!(refs.get("feature/a").map(String::as_str), Some("bbbb2222"));
        assert_eq!(refs.len(), 3);
        assert!(parse_ref_lines("").is_empty());
    }
}
