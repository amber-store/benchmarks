//! The object-storage workloads.
//!
//! Every byte in this group crosses a real HTTP connection to a real
//! S3-compatible service (Garage, started by the harness), through a gateway
//! that counts requests by method and payload bytes in each direction. There
//! is no local copy anywhere in it that is described as a network transfer.
//!
//! Each of the three scenario groups publishes its *own* data — a backup
//! repository, a source history, a Nix closure — because a push of a Git
//! bundle and a push of a restic repository are not the same operation and
//! collapsing them into one table would be the wrong answer to the right
//! question. Within a scenario the backends are directly comparable.
//!
//! # The delivered data set
//!
//! Every backend in a scenario publishes the same two payload generations
//! and runs the same six measured operations, in the same order:
//!
//! | operation           | what it does                                      |
//! |---------------------|---------------------------------------------------|
//! | `push_initial`      | publish generation 0                              |
//! | `push_noop`         | publish it again, unchanged                       |
//! | `pull_fresh`        | deliver everything published so far to a client that has never seen the store |
//! | `push_incremental`  | publish generation 1                              |
//! | `pull_incremental`  | bring that same client up to date                 |
//! | `retention_cleanup` | drop generation 0 and propagate the deletion      |
//!
//! The point of the alignment is that a pull's cost and its *delivered
//! outcome* are stated together: each pull records how many published
//! references it delivered and how many payload bytes those are, and every
//! delivered reference is then verified at the destination. Without that,
//! one backend downloading a whole repository and another restoring a single
//! snapshot would appear in the same row as if they had done the same work.
//!
//! In `blob/source-history` that alignment is what decides what the Amber
//! cores ingest. A Git bundle carries every commit, branch and tag in the
//! range it covers, so the cores are handed *every* retained version in the
//! same range — the first three quarters of the history initially and the
//! rest incrementally — rather than a sample of it, and each pull's
//! `delivered_references` counter makes that coverage reviewable from the
//! report alone. What Git records about a version and what an Amber
//! reference records about it are still not the same thing: Git keeps commit
//! identity, authorship, parentage and branch topology, and stores no mtimes,
//! no permission bits beyond the executable one and no empty directories,
//! while an Amber reference is a name for a tree root and keeps all of those.
//!
//! `retention_cleanup` is the one operation here where the backends are not
//! asked for the same thing: consolidating Git bundles drops no version,
//! while forgetting a restic snapshot, expiring a narinfo or dropping an
//! Amber reference does. Each backend's row therefore states what it
//! retained, every member of that set is verified afterwards, and the
//! timings are not offered as a ranking.
//!
//! Where a backend has no native protocol for arbitrary object storage, the
//! harness's own transport is used and is labelled as such in every row. Git
//! has no S3 push protocol; neither Amber core has any network protocol at
//! all.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::adapters::amber::AmberCli;
use crate::adapters::git::GitCli;
use crate::adapters::nixstore::{self, ClientCache, NixCli};
use crate::adapters::restic::{self, ResticCli};
use crate::adapters::semantics;
use crate::blob::gateway::Gateway;
use crate::blob::s3::{self, MC_ALIAS, ObjectEntry, Target};
use crate::dataset::manifest::{Manifest, Strictness};
use crate::metrics::{Op, OpStatus, Phase, RunRecord, Verification};
use crate::util::fsx;
use crate::util::proc::Run;

use super::{Ctx, Recorder, aside, measured_many};

/// Scenario group name.
pub const GROUP: &str = "blob";

/// The backup repository published to object storage.
pub const SCENARIO_BACKUP: &str = "blob/backup-corpus";
/// The source history published to object storage.
pub const SCENARIO_HISTORY: &str = "blob/source-history";
/// The Nix closure published to object storage.
pub const SCENARIO_NIX: &str = "blob/nix-closure";

/// Backends of each scenario.
pub fn backends(scenario: &str) -> Vec<&'static str> {
    match scenario {
        SCENARIO_BACKUP => vec!["restic-s3", "amber-rust-s3", "amber-go-s3"],
        SCENARIO_HISTORY => vec!["git-bundle-s3", "amber-rust-s3", "amber-go-s3"],
        SCENARIO_NIX => vec!["nix-binary-cache", "amber-rust-s3", "amber-go-s3"],
        _ => Vec::new(),
    }
}

/// Every blob scenario, in run order.
pub const SCENARIOS: [&str; 3] = [SCENARIO_BACKUP, SCENARIO_HISTORY, SCENARIO_NIX];

/// The object store and the gateway in front of it.
pub struct BlobCtx {
    /// The measuring gateway backends are pointed at.
    pub gateway: Gateway,
    /// Endpoint, bucket, prefix and credentials.
    pub target: Target,
    /// The MinIO client binary.
    pub mc: PathBuf,
    /// Its configuration directory.
    pub mc_config: PathBuf,
    /// Description of the service, for the report.
    pub service: String,
}

impl BlobCtx {
    /// An `mc` command pointed at the gateway, so its traffic is counted.
    fn mc_measured(&self) -> Run {
        s3::mc(&self.mc, &self.mc_config, &self.target, false)
    }

    /// An `mc` command that bypasses the gateway, for the harness's own
    /// bookkeeping. Listing and fault injection must not be charged to the
    /// operation under measurement.
    fn mc_admin(&self) -> Run {
        s3::mc(&self.mc, &self.mc_config, &self.target, true)
    }

    /// Bytes and object count stored under `suffix`, or the reason the
    /// question could not be answered. Never `(0, 0)` for a failed listing:
    /// an empty store and an unanswered question are different facts.
    fn usage(&self, suffix: &str) -> Result<(u64, u64), String> {
        s3::usage(&self.mc, &self.mc_config, &self.target, suffix)
    }

    /// Every key in the bucket, for the namespace check.
    fn bucket_keys(&self) -> Result<Vec<ObjectEntry>, String> {
        s3::list_bucket(&self.mc, &self.mc_config, &self.target)
    }
}

/// What an operation actually did to the set of published references,
/// recorded next to its cost.
///
/// Two numbers are never enough on their own: a pull that cost ten seconds
/// and a pull that cost one have not told a reader anything until they also
/// say what each of them handed over. The same is true of retention, where
/// the backends genuinely are doing different things.
struct Delivery {
    /// Counter name prefix: `delivered` or `retained`.
    role: &'static str,
    /// Human-readable statement of the set.
    what: String,
    /// Number of published references (snapshots, store paths, versions,
    /// bundles) the statement covers.
    references: usize,
    /// Payload bytes those references represent, independent of how many
    /// bytes crossed the wire.
    logical_bytes: u64,
}

impl Delivery {
    /// What a transfer handed to its destination.
    fn delivered(what: impl Into<String>, references: usize, logical_bytes: u64) -> Delivery {
        Delivery {
            role: "delivered",
            what: what.into(),
            references,
            logical_bytes,
        }
    }

    /// What a retention step left behind.
    fn retained(what: impl Into<String>, references: usize, logical_bytes: u64) -> Delivery {
        Delivery {
            role: "retained",
            what: what.into(),
            references,
            logical_bytes,
        }
    }

    /// How the note that carries it reads.
    fn sentence(&self) -> String {
        match self.role {
            "retained" => format!(
                "retained by this retention step: {}. Retention is the one \
                 operation in this scenario where the backends are not asked \
                 for the same thing — consolidating Git bundles still \
                 publishes the whole history, while dropping an Amber \
                 reference, forgetting a restic snapshot or expiring a narinfo \
                 removes a version — so these timings are not a like-for-like \
                 ranking. What each one kept is stated here, and every member \
                 of it is verified afterwards.",
                self.what
            ),
            _ => format!("delivered to the destination: {}", self.what),
        }
    }
}

/// Runs one blob scenario for one backend.
/// The local backend whose *storage* semantics a blob backend inherits.
///
/// A blob backend differs from its local counterpart in how bytes reach the
/// object store, not in how they are compressed, encrypted or fsynced:
/// `git-bundle-s3` publishes a Git repository and `nix-binary-cache`
/// publishes a Nix binary cache. Naming the counterpart here is what keeps
/// those rows from reporting "unrecorded" for facts the suite knows. The
/// transport itself is recorded separately, by each backend, and labelled
/// when it is the harness's rather than the product's.
fn storage_base(backend: &str) -> &str {
    match backend {
        "git-bundle-s3" => "git",
        "nix-binary-cache" => "nix",
        other => other.trim_end_matches("-s3"),
    }
}

pub fn run(
    ctx: &Ctx,
    blob: &BlobCtx,
    scenario: &str,
    backend: &str,
    rep: usize,
    order_index: usize,
) -> RunRecord {
    let base = backend.trim_end_matches("-s3");
    let mut rec = Recorder::new(
        GROUP,
        scenario,
        backend,
        rep,
        order_index,
        semantics(storage_base(backend)),
    );
    rec.note(format!(
        "object store: {}; gateway shaping: {}",
        blob.service,
        blob.gateway.shaping().describe()
    ));
    rec.note(
        "the two pulls state what they delivered: pull_fresh brings a client \
         that has never seen the store up to everything published at that \
         point, and pull_incremental brings the same client up to date after \
         the incremental push. Both record delivered_references and \
         delivered_logical_bytes, and every delivered reference is verified \
         at the destination."
            .to_string(),
    );
    let dir = match ctx.prepare_run_dir(&format!("{GROUP}-{}", short(scenario)), backend, rep) {
        Ok(d) => d,
        Err(e) => return rec.abort(e),
    };
    // Each repetition publishes under its own suffix, so nothing it deletes
    // can belong to another repetition, let alone another run.
    let suffix = format!("{}/{backend}/rep{rep}", short(scenario));
    if let Err(e) = s3::remove_tree(&blob.mc, &blob.mc_config, &blob.target, &suffix) {
        return rec.abort(format!("clearing {suffix}: {e}"));
    }

    match backend {
        "restic-s3" => run_restic(ctx, blob, &mut rec, &dir, &suffix),
        "nix-binary-cache" => run_nix(ctx, blob, &mut rec, &dir, &suffix),
        "git-bundle-s3" => run_git(ctx, blob, &mut rec, &dir, &suffix),
        "amber-rust-s3" | "amber-go-s3" => {
            run_amber(ctx, blob, &mut rec, &dir, &suffix, base, scenario)
        }
        other => {
            return rec.abort(format!("blob scenario: unknown backend {other}"));
        }
    }
    rec.finish()
}

fn short(scenario: &str) -> &str {
    scenario.rsplit('/').next().unwrap_or(scenario)
}

/// Runs the commands of one measured transfer and attaches everything the
/// gateway and the object store observed about it.
fn transfer(
    blob: &BlobCtx,
    rec: &mut Recorder,
    name: &str,
    desc: &str,
    suffix: &str,
    cmds: &[Run],
    delivery: Option<Delivery>,
) -> OpStatus {
    blob.gateway.reset();
    let mut op = measured_many(name, desc, cmds);
    let stats = blob.gateway.snapshot();
    op = op
        .counters(stats.counters("s3_"))
        .logical(stats.bytes_client_to_service + stats.bytes_service_to_client)
        .note(
            "for a transfer, the logical byte count is the bytes that \
             actually crossed the wire in both directions, so the \
             throughput column is wire throughput rather than payload \
             throughput",
        );
    match blob.usage(suffix) {
        Ok((bytes, objects)) => {
            op = op
                .counter("stored_bytes", bytes as i64)
                .counter("stored_objects", objects as i64);
        }
        Err(e) => {
            // A failed listing is not an empty bucket. The counters are
            // omitted and the reason is recorded rather than reporting zero
            // bytes of storage.
            op = op.counter("stored_bytes_unavailable", 1).note(format!(
                "the stored-bytes and stored-objects counters are absent for \
                 this operation: listing the published objects failed ({}). \
                 They are not zero; they are unknown.",
                first_line(&e)
            ));
        }
    }
    if let Some(d) = delivery {
        op = op
            .counter(&format!("{}_references", d.role), d.references as i64)
            .counter(&format!("{}_logical_bytes", d.role), d.logical_bytes as i64)
            .note(d.sentence());
    }
    if stats.parse_desync {
        op = op.note(
            "the gateway's HTTP parser lost the framing on at least one \
             connection during this run: the byte totals are still exact \
             (they are counted at the socket) but the request and body counts \
             are not trustworthy",
        );
    }
    let status = op.status.clone();
    rec.push(op);
    status
}

fn setup_cmds(rec: &mut Recorder, name: &str, desc: &str, cmds: &[Run]) -> bool {
    let mut ok = true;
    for cmd in cmds {
        let op = aside(name, Phase::Setup, desc, cmd);
        if op.status == OpStatus::Failed {
            ok = false;
        }
        rec.push(op);
    }
    ok
}

/// Apparent bytes of a directory tree, used to state a delivered payload
/// size that does not depend on how anything was stored.
fn tree_bytes(path: &Path) -> u64 {
    fsx::measure_dir(path)
        .map(|s| s.apparent_bytes)
        .unwrap_or(0)
}

fn stage_bytes(stage: &Stage) -> u64 {
    stage.iter().map(|i| i.logical_bytes).sum()
}

// ---------------------------------------------------------------------------
// restic over its own S3 backend
// ---------------------------------------------------------------------------

fn run_restic(ctx: &Ctx, blob: &BlobCtx, rec: &mut Recorder, dir: &Path, suffix: &str) {
    rec.transport(
        "restic's own S3 backend (minio-go), speaking S3 directly to the \
         object store. This is a native, supported transport.",
    );
    let password = dir.join("restic-password");
    if fsx::write_file(&password, b"amber-cas-bench\n").is_err() {
        rec.push(Op::failed(
            "setup",
            "could not write the restic password file",
        ));
        return;
    }
    let repo = blob.target.restic_repo(&format!("{suffix}/repo"));
    let cli = |cache: &str| ResticCli {
        bin: ctx.tools.path("restic").unwrap_or_default(),
        repo: repo.clone(),
        password_file: password.clone(),
        cache_dir: dir.join(cache),
        timeout: ctx.timeout,
        s3_credentials: Some((
            blob.target.access_key.clone(),
            blob.target.secret_key.clone(),
        )),
    };
    let writer = cli("cache-writer");
    if !setup_cmds(rec, "repo_init", "create the repository", &[writer.init()]) {
        return;
    }
    let gens = &ctx.corpus.generations;
    if gens.len() < 2 {
        rec.push(Op::failed(
            "setup",
            "the corpus has fewer than two generations, so there is no \
             incremental publication to measure",
        ));
        return;
    }
    let jobs = ctx.jobs;
    // The published generations, by the tag each snapshot carries.
    let p0 = "g0";
    let p0_again = "g0-again";
    let p1 = "g1";

    transfer(
        blob,
        rec,
        "push_initial",
        "back up the corpus to object storage for the first time",
        suffix,
        &[writer.backup(&gens[0].dir, p0, jobs)],
        None,
    );
    transfer(
        blob,
        rec,
        "push_noop",
        "back up the identical tree again",
        suffix,
        &[writer.backup(&gens[0].dir, p0_again, jobs)],
        None,
    );

    // Snapshot ids come from each snapshot's own unique tag.
    let resolve = |tag: &str| -> Result<String, String> {
        let out = writer.snapshots_tagged(tag).ok()?;
        restic::resolve_tagged_snapshot(&out.stdout_text(), tag)
    };
    let (id0, id0_again) = match (resolve(p0), resolve(p0_again)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            rec.push(Op::failed(
                "resolve_snapshots",
                format!("the published snapshots could not be identified: {e}"),
            ));
            return;
        }
    };

    // A genuinely fresh client: a cache directory that has never seen this
    // repository. It is brought up to everything published so far, which at
    // this point is generation 0 under two snapshots of identical content.
    let reader = cli("cache-reader");
    let pull0 = dir.join("pull-initial");
    let _ = std::fs::create_dir_all(&pull0);
    let st = transfer(
        blob,
        rec,
        "pull_fresh",
        "restore everything published so far to a client with a cold cache",
        suffix,
        &[reader.restore(&format!("{id0}:{}", gens[0].dir.display()), &pull0)],
        Some(Delivery::delivered(
            format!(
                "snapshot {p0} (corpus generation 0). {p0_again} holds the \
                 identical tree, so restoring it as well would deliver the \
                 same bytes twice"
            ),
            1,
            gens[0].logical_bytes,
        )),
    );
    rec.verify(verify_corpus(
        ctx,
        0,
        &pull0,
        st == OpStatus::Ok,
        "fresh_pull_matches_manifest",
    ));

    transfer(
        blob,
        rec,
        "push_incremental",
        "back up the churned generation",
        suffix,
        &[writer.backup(&gens[1].dir, p1, jobs)],
        None,
    );
    let id1 = match resolve(p1) {
        Ok(id) => id,
        Err(e) => {
            rec.push(Op::failed(
                "resolve_snapshots",
                format!("the incrementally published snapshot could not be identified: {e}"),
            ));
            return;
        }
    };

    let pull1 = dir.join("pull-incremental");
    let _ = std::fs::create_dir_all(&pull1);
    let st = transfer(
        blob,
        rec,
        "pull_incremental",
        "bring the same client up to date with the newly published generation",
        suffix,
        &[reader.restore(&format!("{id1}:{}", gens[1].dir.display()), &pull1)],
        Some(Delivery::delivered(
            format!(
                "snapshot {p1} (corpus generation 1), into a client that already held generation 0"
            ),
            1,
            gens[1].logical_bytes,
        )),
    );
    rec.verify(verify_corpus(
        ctx,
        1,
        &pull1,
        st == OpStatus::Ok,
        "incremental_pull_matches_manifest",
    ));
    // The earlier delivery must still be intact at the destination.
    rec.verify(verify_corpus(
        ctx,
        0,
        &pull0,
        true,
        "earlier_delivery_still_matches_manifest",
    ));

    // Retention drops every snapshot of generation 0 — both of them, or
    // generation 0's content would stay pinned and the prune would have
    // nothing to reclaim.
    transfer(
        blob,
        rec,
        "retention_cleanup",
        "forget both snapshots of generation 0 and reclaim their objects",
        suffix,
        &[
            writer.restic().arg("forget").arg(&id0).arg(&id0_again),
            writer.prune(),
        ],
        Some(Delivery::retained(
            format!(
                "snapshot {p1} (corpus generation 1); both snapshots of generation 0 were forgotten and pruned"
            ),
            1,
            gens[1].logical_bytes,
        )),
    );

    // What retention kept has to come back, to a client that has never seen
    // the repository, and what it dropped has to be gone from the
    // repository's own snapshot listing.
    let after = dir.join("pull-after-cleanup");
    let _ = std::fs::create_dir_all(&after);
    let post = cli("cache-post");
    let ok = post
        .restore(&format!("{id1}:{}", gens[1].dir.display()), &after)
        .run()
        .map(|o| o.success())
        .unwrap_or(false);
    rec.verify(verify_corpus(
        ctx,
        1,
        &after,
        ok,
        "retained_content_valid_after_cleanup",
    ));
    let listing = post
        .snapshots()
        .ok()
        .and_then(|o| restic::parse_snapshot_tags(&o.stdout_text()));
    rec.verify(super::check_retention(
        "dropped_snapshots_are_absent",
        super::RetentionEvidence {
            listing,
            still_readable: Vec::new(),
        },
        &[p1],
        &[p0, p0_again],
    ));

    let v = fault_injection(blob, rec, suffix, "repo/data/", &[post.check(true)]);
    rec.verify(v);
}

// ---------------------------------------------------------------------------
// Nix's own S3 binary cache
// ---------------------------------------------------------------------------

fn run_nix(ctx: &Ctx, blob: &BlobCtx, rec: &mut Recorder, dir: &Path, suffix: &str) {
    rec.transport(
        "Nix's own S3 binary-cache store (`nix copy --to s3://...`), speaking \
         S3 directly to the object store. This is a native, supported \
         transport.",
    );
    let Some(fixture) = ctx.nix.as_ref() else {
        rec.push(Op::failed("setup", "the Nix fixture closure was not built"));
        return;
    };
    let bin = match ctx.tools.path("nix") {
        Ok(b) => b,
        Err(e) => {
            rec.push(Op::failed("setup", e));
            return;
        }
    };
    let host = NixCli {
        bin: bin.clone(),
        store: None,
        timeout: ctx.timeout,
        cache: ClientCache::Inherited,
    };
    let cache_suffix = format!("{suffix}/cache");
    let cache = blob.target.nix_store_uri(&cache_suffix);
    rec.note(format!(
        "the binary cache is published under the key prefix \
         s3://{} — Nix carries a cache's key prefix in the path after the \
         bucket, and the harness checks the keys that actually appeared \
         rather than assuming the URI was honoured",
        blob.target.namespace(&cache_suffix)
    ));
    let creds = [
        ("AWS_ACCESS_KEY_ID", blob.target.access_key.as_str()),
        ("AWS_SECRET_ACCESS_KEY", blob.target.secret_key.as_str()),
    ];
    let with_creds = |r: Run| {
        creds
            .iter()
            .fold(r, |acc, (k, v)| acc.env(k, v))
            .env("AWS_EC2_METADATA_DISABLED", "true")
    };

    let gen1_bytes = closure_bytes(&fixture.gen1_closure);
    let gen2_bytes = closure_bytes(&fixture.gen2_closure);

    // Every key in the bucket before this backend publishes anything, so the
    // namespace check can tell what this run created from what was already
    // there.
    let keys_before = blob.bucket_keys();

    transfer(
        blob,
        rec,
        "push_initial",
        "publish generation 1's closure to the binary cache",
        suffix,
        &[with_creds(host.copy(
            "daemon",
            &cache,
            std::slice::from_ref(&fixture.gen1),
        ))],
        None,
    );
    rec.verify(namespace_isolation(blob, &keys_before, &cache_suffix));
    transfer(
        blob,
        rec,
        "push_noop",
        "publish the identical closure again",
        suffix,
        &[with_creds(host.copy(
            "daemon",
            &cache,
            std::slice::from_ref(&fixture.gen1),
        ))],
        None,
    );

    // The pulling client: an empty store *and* an empty narinfo cache, so
    // `pull_fresh` is a cold client in the same sense as restic's cold cache
    // directory and Git's empty download. `pull_incremental` then reuses
    // both, because it is the same client coming back for an update.
    let fresh = dir.join("store-fresh");
    let fresh_cli = NixCli {
        bin: bin.clone(),
        store: Some(fresh.clone()),
        timeout: ctx.timeout,
        cache: ClientCache::Inherited,
    };
    let client = host.cold_client(dir.join("nix-cache-client"));
    let st = transfer(
        blob,
        rec,
        "pull_fresh",
        "substitute everything published so far into an empty store",
        suffix,
        &[with_creds(client.copy(
            &cache,
            &nixstore::local_store_uri(&fresh),
            std::slice::from_ref(&fixture.gen1),
        ))],
        Some(Delivery::delivered(
            format!(
                "generation 1's whole closure: {} store paths",
                fixture.gen1_closure.len()
            ),
            fixture.gen1_closure.len(),
            gen1_bytes,
        )),
    );
    rec.verify(verify_nix_closure(
        ctx,
        &fresh_cli,
        fixture,
        &fixture.gen1,
        &fixture.gen1_closure,
        st == OpStatus::Ok,
        "fresh_pull_matches_source",
    ));

    // A single supplied closure has no second generation, so the operations
    // that need one are recorded as unsupported with the reason. The
    // alternative would be to publish the same closure twice and call the
    // second one an incremental publication, which would be a fabricated
    // number, or to edit somebody's store paths, which is not the harness's
    // to do.
    if !fixture.origin.has_churn() {
        for name in ["push_incremental", "pull_incremental", "retention_cleanup"] {
            rec.push(Op::unsupported(name, fixture.origin.no_churn_reason()));
        }
        let probe = dir.join("store-probe");
        let probe_cmd = with_creds(host.copy(
            &cache,
            &nixstore::local_store_uri(&probe),
            std::slice::from_ref(&fixture.gen1),
        ));
        let v = fault_injection(blob, rec, suffix, "cache/nar/", &[probe_cmd]);
        rec.verify(v);
        return;
    }

    transfer(
        blob,
        rec,
        "push_incremental",
        "publish generation 2, which shares most of generation 1",
        suffix,
        &[with_creds(host.copy(
            "daemon",
            &cache,
            std::slice::from_ref(&fixture.gen2),
        ))],
        None,
    );

    let st = transfer(
        blob,
        rec,
        "pull_incremental",
        "bring the same store up to date with the newly published closure",
        suffix,
        &[with_creds(host.copy(
            &cache,
            &nixstore::local_store_uri(&fresh),
            std::slice::from_ref(&fixture.gen2),
        ))],
        Some(Delivery::delivered(
            format!(
                "generation 2's whole closure: {} store paths, {} of which the \
                 store already held",
                fixture.gen2_closure.len(),
                fixture
                    .gen2_closure
                    .iter()
                    .filter(|p| fixture.gen1_closure.contains(p))
                    .count()
            ),
            fixture.gen2_closure.len(),
            gen2_bytes,
        )),
    );
    rec.verify(verify_nix_closure(
        ctx,
        &fresh_cli,
        fixture,
        &fixture.gen2,
        &fixture.gen2_closure,
        st == OpStatus::Ok,
        "incremental_pull_matches_source",
    ));
    // Generation 1's closure must still be complete in that store too.
    rec.verify(verify_nix_closure(
        ctx,
        &fresh_cli,
        fixture,
        &fixture.gen1,
        &fixture.gen1_closure,
        true,
        "earlier_delivery_still_matches_source",
    ));

    // Retention: expire the narinfos of the paths only the older generation
    // needed. The NAR blobs they pointed at are left behind; a real cache
    // would sweep them separately, and the report says so rather than
    // pretending the space came back.
    let doomed: Vec<String> = fixture
        .gen1_closure
        .iter()
        .filter(|p| !fixture.gen2_closure.contains(p))
        .filter_map(|p| hash_part(p))
        .map(|h| format!("{h}.narinfo"))
        .collect();
    let cmds: Vec<Run> = doomed
        .iter()
        .filter_map(|o| {
            blob.target
                .mc_path(MC_ALIAS, &format!("{cache_suffix}/{o}"))
                .ok()
                .map(|path| blob.mc_measured().args(["rm", "--force"]).arg(path))
        })
        .collect();
    let mut st = OpStatus::Ok;
    if cmds.is_empty() {
        rec.push(Op::failed(
            "retention_cleanup",
            "the two fixture generations share their whole closure, so there \
             is nothing retention could expire",
        ));
    } else {
        st = transfer(
            blob,
            rec,
            "retention_cleanup",
            "expire the narinfos of superseded store paths",
            suffix,
            &cmds,
            Some(Delivery::retained(
                format!(
                    "generation 2's whole closure: {} store paths. {} narinfo \
                     object(s) of paths only generation 1 needed were expired",
                    fixture.gen2_closure.len(),
                    doomed.len()
                ),
                fixture.gen2_closure.len(),
                gen2_bytes,
            )),
        );
        rec.push(
            Op::aside("retention_cleanup_note", Phase::Verify, Default::default()).note(
                "only the narinfo objects were expired; the NAR blobs they \
                 referenced remain in the bucket, so the stored-bytes figure \
                 after cleanup is not a reclaim figure",
            ),
        );
    }

    // A client that has never seen this cache at all: its own store *and*
    // its own narinfo cache. Without the second half of that, a "fresh"
    // client in this environment answers availability questions out of the
    // narinfo cache the earlier pulls filled, which is not what a fresh
    // client would experience.
    let after = dir.join("store-after-cleanup");
    let after_cli = NixCli {
        bin,
        store: Some(after.clone()),
        timeout: ctx.timeout,
        cache: ClientCache::Inherited,
    };
    let post_client = host.cold_client(dir.join("nix-cache-after-cleanup"));
    let ok = st == OpStatus::Ok
        && with_creds(post_client.copy(
            &cache,
            &nixstore::local_store_uri(&after),
            std::slice::from_ref(&fixture.gen2),
        ))
        .run()
        .map(|o| o.success())
        .unwrap_or(false);
    rec.verify(verify_nix_closure(
        ctx,
        &after_cli,
        fixture,
        &fixture.gen2,
        &fixture.gen2_closure,
        ok,
        "retained_content_valid_after_cleanup",
    ));
    rec.verify(expired_narinfos_are_absent(
        blob,
        &host,
        &with_creds,
        fixture,
        &cache,
        &cache_suffix,
        &doomed,
        dir,
    ));

    let probe = dir.join("store-probe");
    let probe_cmd = with_creds(host.copy(
        &cache,
        &nixstore::local_store_uri(&probe),
        std::slice::from_ref(&fixture.gen2),
    ));
    let v = fault_injection(blob, rec, suffix, "cache/nar/", &[probe_cmd]);
    rec.verify(v);
}

/// Checks that retention really removed the superseded narinfos, on two
/// independent pieces of evidence.
///
/// Either alone would be misleading:
///
/// * **The object listing.** The expired keys must be gone from the bucket,
///   read from a listing that *succeeded*. A listing that failed says
///   nothing about what is there, so it fails the check rather than passing
///   it by default.
/// * **A genuinely fresh client.** A client with a new store but the
///   environment's usual narinfo cache is not fresh: Nix answers "is this
///   path available?" from that cache while the entry is inside its TTL, so
///   it will happily fetch a NAR whose narinfo was deleted minutes ago —
///   which is exactly what a first run of this check observed. The probe
///   therefore gets a cache directory of its own with both narinfo TTLs at
///   zero, and what is checked is the *outcome*: `nix copy` can exit zero
///   having transferred nothing, so the question is whether the superseded
///   paths arrived.
#[allow(clippy::too_many_arguments)]
fn expired_narinfos_are_absent(
    blob: &BlobCtx,
    host: &NixCli,
    with_creds: &impl Fn(Run) -> Run,
    fixture: &super::NixFixture,
    cache_uri: &str,
    cache_suffix: &str,
    doomed: &[String],
    dir: &Path,
) -> Verification {
    const NAME: &str = "expired_narinfos_are_absent";
    if doomed.is_empty() {
        return Verification::fail(NAME, "nothing was expired, so there was nothing to check");
    }
    let superseded: Vec<&String> = fixture
        .gen1_closure
        .iter()
        .filter(|p| !fixture.gen2_closure.contains(p))
        .collect();

    let mut problems = Vec::new();
    let mut listed = 0usize;
    match s3::list(&blob.mc, &blob.mc_config, &blob.target, cache_suffix) {
        Ok(objects) => {
            listed = objects.len();
            let names: std::collections::BTreeSet<&str> = objects
                .iter()
                .map(|o| o.key.rsplit('/').next().unwrap_or(o.key.as_str()))
                .collect();
            for key in doomed {
                if names.contains(key.as_str()) {
                    problems.push(format!("{key} is still an object in the cache"));
                }
            }
        }
        Err(e) => problems.push(format!(
            "the published objects could not be listed, so nothing can be \
             concluded about what the cache still holds: {}",
            first_line(&e)
        )),
    }

    let expired = dir.join("store-expired-probe");
    let probe_client = host.uncached_client(dir.join("nix-cache-expired-probe"));
    let _ = with_creds(probe_client.copy(
        cache_uri,
        &nixstore::local_store_uri(&expired),
        std::slice::from_ref(&fixture.gen1),
    ))
    .run();
    let expired_store = expired.join("nix/store");
    let obtained: Vec<String> = superseded
        .iter()
        .filter(|p| {
            expired_store
                .join(Path::new(p.as_str()).file_name().unwrap_or_default())
                .exists()
        })
        .map(|p| (*p).clone())
        .collect();
    for path in &obtained {
        problems.push(format!(
            "{path} was still obtained from the cache by a client with no \
             narinfo cache of its own"
        ));
    }

    let mut v = if problems.is_empty() {
        Verification::pass(
            NAME,
            format!(
                "all {} expired narinfo object(s) are absent from a successful \
                 listing of the {listed} object(s) the cache still holds, and a \
                 client with its own empty narinfo cache obtains none of the {} \
                 store paths only the superseded generation needed",
                doomed.len(),
                superseded.len(),
            ),
        )
    } else {
        Verification::fail(
            NAME,
            format!(
                "{} problem(s) after expiring {} narinfo object(s)",
                problems.len(),
                doomed.len()
            ),
        )
    };
    v.extra = problems;
    v
}

fn closure_bytes(paths: &[String]) -> u64 {
    paths.iter().map(|p| tree_bytes(Path::new(p))).sum()
}

fn hash_part(store_path: &str) -> Option<String> {
    Path::new(store_path)
        .file_name()?
        .to_string_lossy()
        .split_once('-')
        .map(|(h, _)| h.to_string())
}

/// Proves that the keys a native client actually wrote all live inside this
/// repetition's own namespace.
///
/// A store URI is a request, not a guarantee: an unrecognised parameter only
/// produces a warning, and a path prefix is honoured by some releases and not
/// others. So after the first publication the harness lists the bucket and
/// compares it with the listing taken before, and any key that appeared
/// outside `prefix/suffix` fails the run.
fn namespace_isolation(
    blob: &BlobCtx,
    before: &Result<Vec<ObjectEntry>, String>,
    suffix: &str,
) -> Verification {
    const NAME: &str = "published_keys_stay_inside_the_namespace";
    let before = match before {
        Ok(b) => b,
        Err(e) => {
            return Verification::fail(
                NAME,
                format!("the bucket could not be listed before publishing: {e}"),
            );
        }
    };
    let after = match blob.bucket_keys() {
        Ok(a) => a,
        Err(e) => {
            return Verification::fail(
                NAME,
                format!("the bucket could not be listed after publishing: {e}"),
            );
        }
    };
    let escaped = s3::keys_written_outside(before, &after, &blob.target.prefix, suffix);
    let created = after.len().saturating_sub(before.len());
    let mut v = if escaped.is_empty() && created > 0 {
        Verification::pass(
            NAME,
            format!(
                "{created} object(s) appeared and every one of them is under \
                 {}, which is what the store URI asked for",
                blob.target.namespace(suffix)
            ),
        )
    } else if created == 0 {
        Verification::fail(
            NAME,
            "the publication reported success but no object appeared in the \
             bucket at all",
        )
    } else {
        Verification::fail(
            NAME,
            format!(
                "{} object(s) were written outside {}",
                escaped.len(),
                blob.target.namespace(suffix)
            ),
        )
    };
    v.extra = escaped;
    v
}

// ---------------------------------------------------------------------------
// Git bundle publication
// ---------------------------------------------------------------------------

fn run_git(ctx: &Ctx, blob: &BlobCtx, rec: &mut Recorder, dir: &Path, suffix: &str) {
    rec.transport(
        "Git bundle publication: `git bundle create` writes the repository \
         (or an increment of it) to a file, which is then uploaded with the \
         MinIO client. Git has no native protocol for pushing to arbitrary S3 \
         object storage, so the object transfer is the harness's, and the \
         numbers below include the cost of producing the bundle as well as of \
         uploading it.",
    );
    let git_bin = match ctx.tools.path("git") {
        Ok(b) => b,
        Err(e) => {
            rec.push(Op::failed("setup", e));
            return;
        }
    };
    let work = dir.join("repo");
    let git = GitCli {
        bin: git_bin.clone(),
        repo: work.clone(),
        timeout: ctx.timeout,
    };
    if std::fs::create_dir_all(&work).is_err()
        || !setup_cmds(rec, "git_init", "create the repository", &git.init(false))
    {
        return;
    }
    let steps = ctx.history.steps.len();
    let split = (steps * 3 / 4).max(1);
    if !replay(ctx, rec, &git, &work, 0, split) {
        return;
    }

    let bundles = dir.join("bundles");
    let _ = std::fs::create_dir_all(&bundles);
    let full = bundles.join("full.bundle");
    let remote = match blob.target.mc_path(MC_ALIAS, &format!("{suffix}/bundles")) {
        Ok(p) => p,
        Err(e) => {
            rec.push(Op::failed("setup", e));
            return;
        }
    };
    // Mark what the first publication contained, so the increment can be
    // expressed against it.
    setup_cmds(
        rec,
        "mark_basis",
        "tag the commit the first publication ends at",
        &[git.tag("published", false, 0)],
    );

    transfer(
        blob,
        rec,
        "push_initial",
        "bundle the whole repository and upload it",
        suffix,
        &[
            git.bundle_create(&full, &["--all"]),
            blob.mc_measured()
                .args(["mirror", "--overwrite"])
                .arg(&bundles)
                .arg(&remote),
        ],
        None,
    );
    transfer(
        blob,
        rec,
        "push_noop",
        "publish again with nothing changed",
        suffix,
        &[blob
            .mc_measured()
            .args(["mirror", "--overwrite"])
            .arg(&bundles)
            .arg(&remote)],
        None,
    );

    // A fresh client: download what has been published and clone from it.
    let down = dir.join("download");
    let _ = std::fs::create_dir_all(&down);
    let clone = dir.join("clone");
    let st = transfer(
        blob,
        rec,
        "pull_fresh",
        "download everything published so far and clone from it",
        suffix,
        &[
            blob.mc_measured()
                .args(["mirror", "--overwrite"])
                .arg(&remote)
                .arg(&down),
            git.clone_to(
                &down.join("full.bundle").display().to_string(),
                &clone,
                false,
            ),
        ],
        Some(Delivery::delivered(
            format!("the whole published history: {split} commits, every branch and tag of them"),
            split,
            history_bytes(ctx, 0, split),
        )),
    );
    rec.verify(verify_git_destination(
        &git_bin,
        &work,
        &clone,
        ctx,
        // `replay` commits every state linearly onto main, so the tip of the
        // published range is its last state.
        split - 1,
        st == OpStatus::Ok,
        "fresh_pull_matches_source",
    ));

    if !replay(ctx, rec, &git, &work, split, steps) {
        return;
    }
    let incr = bundles.join("incremental.bundle");
    transfer(
        blob,
        rec,
        "push_incremental",
        "bundle only the new commits and upload them",
        suffix,
        &[
            git.bundle_create_since(&incr, "published", &["main"]),
            blob.mc_measured()
                .args(["mirror", "--overwrite"])
                .arg(&bundles)
                .arg(&remote),
        ],
        None,
    );

    let clone_cli = GitCli {
        bin: git_bin.clone(),
        repo: clone.clone(),
        timeout: ctx.timeout,
    };
    let st = transfer(
        blob,
        rec,
        "pull_incremental",
        "download the increment and fetch it into the existing clone",
        suffix,
        &[
            blob.mc_measured()
                .args(["mirror", "--overwrite"])
                .arg(&remote)
                .arg(&down),
            clone_cli
                .git()
                .args(["fetch", "--quiet"])
                .arg(down.join("incremental.bundle"))
                .arg("main:refs/remotes/origin/main"),
        ],
        Some(Delivery::delivered(
            format!(
                "the {} commits added to main since the first publication, \
                 into a clone that already held the rest",
                steps - split
            ),
            steps - split,
            history_bytes(ctx, split, steps),
        )),
    );
    // The incremental bundle carries only main, so only main's tip is
    // compared: the other branches were delivered by the full bundle and are
    // checked by the clone_matches_source check above.
    rec.verify(verify_git_main_tip(
        &git_bin,
        &clone,
        ctx,
        steps - 1,
        st == OpStatus::Ok,
        "incremental_pull_matches_source",
    ));

    // Retention: replace the pair of bundles with a single current one and
    // drop the superseded objects.
    let consolidated = bundles.join("current.bundle");
    let mut cleanup = vec![git.bundle_create(&consolidated, &["--all"])];
    for old in ["full.bundle", "incremental.bundle"] {
        let _ = std::fs::remove_file(bundles.join(old));
        if let Ok(p) = blob
            .target
            .mc_path(MC_ALIAS, &format!("{suffix}/bundles/{old}"))
        {
            cleanup.push(blob.mc_measured().args(["rm", "--force"]).arg(p));
        }
    }
    cleanup.push(
        blob.mc_measured()
            .args(["mirror", "--overwrite"])
            .arg(&bundles)
            .arg(&remote),
    );
    let st = transfer(
        blob,
        rec,
        "retention_cleanup",
        "consolidate the published bundles and drop the superseded objects",
        suffix,
        &cleanup,
        Some(Delivery::retained(
            format!(
                "the whole history: {steps} commits with every branch and tag, \
                 in one consolidated bundle. Consolidating bundles drops no \
                 version — what was superseded is the two bundle objects, not \
                 any commit"
            ),
            steps,
            history_bytes(ctx, 0, steps),
        )),
    );

    // What retention kept must still reconstruct the whole history in a
    // clone that has never seen it.
    let after_dir = dir.join("download-after");
    let after_clone = dir.join("clone-after");
    let _ = std::fs::create_dir_all(&after_dir);
    let ok = st == OpStatus::Ok
        && blob
            .mc_admin()
            .args(["mirror", "--overwrite", "--remove"])
            .arg(&remote)
            .arg(&after_dir)
            .run()
            .map(|o| o.success())
            .unwrap_or(false)
        && git
            .clone_to(
                &after_dir.join("current.bundle").display().to_string(),
                &after_clone,
                false,
            )
            .run()
            .map(|o| o.success())
            .unwrap_or(false);
    rec.verify(verify_git_destination(
        &git_bin,
        &work,
        &after_clone,
        ctx,
        steps - 1,
        ok,
        "retained_content_valid_after_cleanup",
    ));
    // And the superseded objects are really gone from the bucket.
    let remaining = s3::list(
        &blob.mc,
        &blob.mc_config,
        &blob.target,
        &format!("{suffix}/bundles"),
    );
    rec.verify(match remaining {
        Ok(objects) => {
            let names: Vec<String> = objects
                .iter()
                .map(|o| o.key.rsplit('/').next().unwrap_or("").to_string())
                .collect();
            let stale: Vec<String> = names
                .iter()
                .filter(|n| *n == "full.bundle" || *n == "incremental.bundle")
                .cloned()
                .collect();
            if stale.is_empty() && names.iter().any(|n| n == "current.bundle") {
                Verification::pass(
                    "superseded_objects_are_absent",
                    format!(
                        "the bucket holds the consolidated bundle and none of \
                         the superseded ones ({} object(s) remain)",
                        objects.len()
                    ),
                )
            } else {
                let mut v = Verification::fail(
                    "superseded_objects_are_absent",
                    format!(
                        "{} superseded object(s) are still published, or the \
                         consolidated bundle is missing",
                        stale.len()
                    ),
                );
                v.extra = stale;
                v
            }
        }
        Err(e) => Verification::fail(
            "superseded_objects_are_absent",
            format!("the published objects could not be listed: {e}"),
        ),
    });

    let probe_dir = dir.join("download-probe");
    let _ = std::fs::create_dir_all(&probe_dir);
    let v = fault_injection(
        blob,
        rec,
        suffix,
        "bundles/",
        &[
            blob.mc_admin()
                .args(["mirror", "--overwrite"])
                .arg(&remote)
                .arg(&probe_dir),
            git.clone_to(
                &probe_dir.join("current.bundle").display().to_string(),
                &dir.join("clone-probe"),
                false,
            ),
        ],
    );
    rec.verify(v);
}

fn history_bytes(ctx: &Ctx, from: usize, to: usize) -> u64 {
    (from..to).map(|i| ctx.history.state_bytes(i)).sum()
}

/// Replays history steps into a working tree and commits them. Setup: this is
/// the data being published, not the publication.
fn replay(
    ctx: &Ctx,
    rec: &mut Recorder,
    git: &GitCli,
    work: &Path,
    from: usize,
    to: usize,
) -> bool {
    let mut previous = from.checked_sub(1);
    for i in from..to {
        let step = &ctx.history.steps[i];
        if ctx.history.apply(i, previous, work).is_err() {
            rec.push(Op::failed("replay_history", format!("state {i}")));
            return false;
        }
        previous = Some(i);
        if !setup_cmds(
            rec,
            "replay_history",
            "build the history that is then published",
            &[git.add_all(), git.commit(&step.subject, step.timestamp)],
        ) {
            return false;
        }
    }
    true
}

/// Verifies a Git transfer at its destination: the clone's own object
/// database, the refs it received, and a working tree materialised out of it.
fn verify_git_destination(
    git_bin: &Path,
    source: &Path,
    clone: &Path,
    ctx: &Ctx,
    // The history state the destination is expected to hold.
    tip: usize,
    delivered: bool,
    name: &str,
) -> Verification {
    if !delivered {
        return Verification::fail(name, "the transfer failed, so there was nothing to verify");
    }
    let timeout = std::time::Duration::from_secs(1800);
    let src = GitCli {
        bin: git_bin.to_path_buf(),
        repo: source.to_path_buf(),
        timeout,
    };
    let dest = GitCli {
        bin: git_bin.to_path_buf(),
        repo: clone.to_path_buf(),
        timeout,
    };
    let mut problems = Vec::new();
    if let Err(e) = dest.fsck().ok() {
        problems.push(format!("git fsck failed in the clone: {}", first_line(&e)));
    }
    // A bundle clone lands the source's branches as local branches, so both
    // sides are compared under refs/heads.
    match (
        crate::scenarios::gitsc::branch_ids(&src),
        crate::scenarios::gitsc::destination_branch_ids(&dest),
    ) {
        (Ok(want), Ok(got)) => {
            for (branch, id) in &want {
                match got.get(branch) {
                    Some(seen) if seen == id => {}
                    Some(seen) => problems.push(format!(
                        "branch {branch}: destination at {seen}, source at {id}"
                    )),
                    None => problems.push(format!("branch {branch} never arrived")),
                }
            }
        }
        (Err(e), _) | (_, Err(e)) => problems.push(format!("listing branches: {}", first_line(&e))),
    }
    match (
        crate::scenarios::gitsc::tag_ids(&src),
        crate::scenarios::gitsc::tag_ids(&dest),
    ) {
        (Ok(want), Ok(got)) => {
            for (tag, id) in &want {
                // `published` is the harness's own basis marker.
                if tag == "published" {
                    continue;
                }
                match got.get(tag) {
                    Some(seen) if seen == id => {}
                    Some(seen) => {
                        problems.push(format!("tag {tag}: destination has {seen}, source {id}"))
                    }
                    None => problems.push(format!("tag {tag} never arrived")),
                }
            }
        }
        (Err(e), _) | (_, Err(e)) => problems.push(format!("listing tags: {}", first_line(&e))),
    }
    match checkout_and_compare(&dest, ctx, tip, "main", name) {
        Ok(()) => {}
        Err(e) => problems.push(e),
    }
    let mut v = if problems.is_empty() {
        Verification::pass(
            name,
            format!(
                "the destination repository passed `git fsck`, holds every \
                 branch and tag of the source at the same commit id, and a \
                 working tree checked out of it matches the manifest of \
                 the delivered history state {tip}"
            ),
        )
    } else {
        Verification::fail(
            name,
            format!("{} problem(s) at the transfer destination", problems.len()),
        )
    };
    v.corrupt = problems;
    v
}

/// The lighter check used after an incremental bundle fetch, which carries
/// only `main`: the destination's main tip has to materialise into the
/// expected tree.
fn verify_git_main_tip(
    git_bin: &Path,
    clone: &Path,
    ctx: &Ctx,
    // The history state the destination is expected to hold.
    tip: usize,
    delivered: bool,
    name: &str,
) -> Verification {
    if !delivered {
        return Verification::fail(name, "the transfer failed, so there was nothing to verify");
    }
    let dest = GitCli {
        bin: git_bin.to_path_buf(),
        repo: clone.to_path_buf(),
        timeout: std::time::Duration::from_secs(1800),
    };
    let mut problems = Vec::new();
    if let Err(e) = dest.fsck().ok() {
        problems.push(format!("git fsck failed in the clone: {}", first_line(&e)));
    }
    if let Err(e) = checkout_and_compare(&dest, ctx, tip, "origin/main", name) {
        problems.push(e);
    }
    let mut v = if problems.is_empty() {
        Verification::pass(
            name,
            format!(
                "the increment applied to the existing clone, which passed \
                 `git fsck`, and a working tree checked out of its main tip \
                 matches the manifest of history state {tip}"
            ),
        )
    } else {
        Verification::fail(
            name,
            format!("{} problem(s) after the fetch", problems.len()),
        )
    };
    v.corrupt = problems;
    v
}

/// Checks `rev` out of `repo` into a scratch worktree and compares it with
/// the manifest of history state `tip`.
fn checkout_and_compare(
    repo: &GitCli,
    ctx: &Ctx,
    tip: usize,
    rev: &str,
    name: &str,
) -> Result<(), String> {
    let Some(manifest_path) = ctx.history_manifests.get(tip) else {
        return Err(format!("no reference manifest for history state {tip}"));
    };
    let work = repo.repo.join("verify-worktree");
    let index = repo.repo.join("verify.index");
    let _ = fsx::make_writable_tree(&work);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let outcome = repo
        .git()
        .env("GIT_WORK_TREE", &work)
        .env("GIT_INDEX_FILE", &index)
        .args(["checkout", "--force", rev, "--", "."])
        .ok()
        .map_err(|e| {
            format!(
                "checking {rev} out of the destination failed: {}",
                first_line(&e)
            )
        })
        .and_then(|_| {
            let m =
                Manifest::load(manifest_path).map_err(|e| format!("reference manifest: {e}"))?;
            let v = m.verify_tree(name, &work, Strictness::git());
            if v.passed {
                Ok(())
            } else {
                let mut detail = format!("{rev} differs from history state {tip}: {}", v.detail);
                for extra in v.corrupt.iter().chain(&v.missing).chain(&v.extra).take(3) {
                    detail.push_str("; ");
                    detail.push_str(extra);
                }
                Err(detail)
            }
        });
    let _ = fsx::make_writable_tree(&work);
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&index);
    outcome
}

// ---------------------------------------------------------------------------
// The Amber cores: file-level publication of the on-disk store
// ---------------------------------------------------------------------------

/// One published item: the reference it is stored under, how big the payload
/// it represents is, and what a restore of it is compared against.
struct Item {
    /// Reference name in the Amber store.
    reference: String,
    /// Payload bytes, independent of how they were stored or transferred.
    logical_bytes: u64,
    kind: ItemKind,
}

/// Where an item's bytes come from, and what a restore of it is checked
/// against.
enum ItemKind {
    /// A directory that exists for the whole run — a corpus generation, a
    /// Nix store path. A restore is compared with a manifest observed from
    /// it at verification time.
    Tree(PathBuf),
    /// One state of the generated source history. It is materialised into a
    /// shared working tree immediately before it is ingested, exactly as the
    /// Git backend replays it into a working tree before committing it, and
    /// a restore is compared with the manifest observed from the staging
    /// tree when the history was generated.
    HistoryState(usize),
}

/// One published generation: everything a push adds and a pull then has to
/// deliver.
type Stage = Vec<Item>;

/// The two payload generations each scenario publishes.
///
/// The two halves of the source history are *every* retained version in that
/// range, not a sample of it. Git's bundles carry every commit, branch and
/// tag in the range they cover, so an Amber store holding two of them would
/// have been delivering a different — much smaller — set at a comparable
/// cost, and the two `pull_*` rows would not have described the same
/// outcome. Each pull's `delivered_references` counter makes the coverage
/// reviewable from the report alone.
fn stages(ctx: &Ctx, scenario: &str) -> Result<[Stage; 2], String> {
    match scenario {
        SCENARIO_BACKUP => {
            if ctx.corpus.generations.len() < 2 {
                return Err("the corpus has fewer than two generations".into());
            }
            let generation = |i: usize| -> Stage {
                vec![Item {
                    reference: format!("gen{i}"),
                    logical_bytes: ctx.corpus.generations[i].logical_bytes,
                    kind: ItemKind::Tree(ctx.corpus.generations[i].dir.clone()),
                }]
            };
            Ok([generation(0), generation(1)])
        }
        SCENARIO_HISTORY => {
            let steps = ctx.history.steps.len();
            if steps < 2 {
                return Err("the generated history has fewer than two states".into());
            }
            // The same split the Git backend publishes at, so the two
            // backends' initial and incremental publications cover the same
            // range of the same history.
            let split = (steps * 3 / 4).max(1);
            let range = |from: usize, to: usize| -> Stage {
                (from..to)
                    .map(|i| Item {
                        reference: ctx.history.steps[i].ref_name.clone(),
                        logical_bytes: ctx.history.state_bytes(i),
                        kind: ItemKind::HistoryState(i),
                    })
                    .collect()
            };
            Ok([range(0, split), range(split, steps)])
        }
        SCENARIO_NIX => {
            let fixture = ctx
                .nix
                .as_ref()
                .ok_or_else(|| "the Nix fixture closure was not built".to_string())?;
            let stage = |paths: &[String]| -> Stage {
                paths
                    .iter()
                    .map(|p| {
                        let name = Path::new(p)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        Item {
                            reference: format!("nixpath-{name}"),
                            logical_bytes: tree_bytes(Path::new(p)),
                            kind: ItemKind::Tree(PathBuf::from(p)),
                        }
                    })
                    .collect()
            };
            Ok([stage(&fixture.gen1_closure), stage(&fixture.gen2_closure)])
        }
        other => Err(format!("blob: unknown scenario {other}")),
    }
}

fn run_amber(
    ctx: &Ctx,
    blob: &BlobCtx,
    rec: &mut Recorder,
    dir: &Path,
    suffix: &str,
    base: &str,
    scenario: &str,
) {
    rec.transport(
        "harness transport, not a product protocol: neither core ships any \
         network client, so the harness publishes the store's on-disk pack \
         segments and reference database with `mc mirror`. Sealed segments are \
         immutable and named by id, so an incremental publication uploads only \
         new ones; the active segment and the reference database are \
         re-uploaded whenever they change. This is a file-level mirror, \
         directly comparable with Git's dumb publication and not with restic's \
         or Nix's native S3 clients.",
    );
    rec.note(
        "these cores cannot fetch a single reference: a client that wants one \
         published tree has to download the whole store. The delivered set \
         recorded on each pull therefore covers every reference published so \
         far, and every one of them is restored and verified — which is the \
         honest comparison against a backend that can fetch one snapshot."
            .to_string(),
    );
    let cli = AmberCli {
        bin: match ctx.tools.path(base) {
            Ok(b) => b,
            Err(e) => {
                rec.push(Op::failed("setup", e));
                return;
            }
        },
        store: dir.join("store"),
        segment_size: ctx.profile.segment_size,
        chunk: ctx.profile.chunk,
        jobs: ctx.jobs,
        timeout: ctx.timeout,
    };
    let work = dir.join("payload");
    let stages = match stages(ctx, scenario) {
        Ok(s) => s,
        Err(e) => {
            rec.push(Op::failed("setup", e));
            return;
        }
    };
    let [p0, p1] = stages;
    rec.note(format!(
        "delivered set: {} reference(s) in the initial publication and {} in \
         the incremental one, {} in total",
        p0.len(),
        p1.len(),
        p0.len() + p1.len()
    ));
    let remote = match blob.target.mc_path(MC_ALIAS, &format!("{suffix}/store")) {
        Ok(p) => p,
        Err(e) => {
            rec.push(Op::failed("setup", e));
            return;
        }
    };
    // Both directions mirror faithfully: `--remove` as well as `--overwrite`.
    //
    // An Amber store is not an append-only directory. Both cores keep their
    // references in an embedded database that rewrites, renames and deletes
    // its files as it goes, and a collection removes whole pack segments. A
    // purely additive mirror would leave the publication holding files the
    // store no longer has — for Pebble, including superseded manifests and
    // the marker that names the newest one — and a client that copied them
    // down would get a directory whose newest manifest describes a file set
    // that is no longer current. It would look like a successful sync that
    // delivered nothing. So each transfer makes its destination equal to its
    // source, which is what publishing and fetching a store actually mean,
    // and the deletions are part of the measured cost.
    let mirror_up = || {
        blob.mc_measured()
            .args(["mirror", "--overwrite", "--remove"])
            .arg(&cli.store)
            .arg(&remote)
    };
    let mirror_down = |dest: &Path| {
        blob.mc_measured()
            .args(["mirror", "--overwrite", "--remove"])
            .arg(&remote)
            .arg(dest)
    };

    // A history state is written into one shared working tree just before it
    // is ingested, as a delta against the state before it — the same way the
    // Git backend replays the history into a checkout before committing it.
    // Materialising every state into a directory of its own would need the
    // whole history on disk at once and would make each ingest look like a
    // fresh import rather than the incremental one it is.
    let mut previous: Option<usize> = None;
    let mut ingest_stage = |rec: &mut Recorder, stage: &Stage| -> bool {
        let mut ok = true;
        for item in stage {
            let source = match &item.kind {
                ItemKind::Tree(path) => path.clone(),
                ItemKind::HistoryState(index) => {
                    if let Err(e) = ctx.history.apply(*index, previous, &work) {
                        rec.push(Op::failed(
                            "ingest_payload",
                            format!("materialising history state {index}: {e}"),
                        ));
                        return false;
                    }
                    if let Err(e) = crate::dataset::gitgen::normalize_mtimes(&work) {
                        rec.push(Op::failed(
                            "ingest_payload",
                            format!("normalising history state {index}: {e}"),
                        ));
                        return false;
                    }
                    previous = Some(*index);
                    work.clone()
                }
            };
            if !setup_cmds(
                rec,
                "ingest_payload",
                "store the payload locally; only the publication that follows \
                 is measured",
                &[cli.ingest(&source, Some(&item.reference))],
            ) {
                ok = false;
                break;
            }
        }
        ok
    };

    if !ingest_stage(rec, &p0) {
        return;
    }
    transfer(
        blob,
        rec,
        "push_initial",
        "publish the whole store to object storage",
        suffix,
        &[mirror_up()],
        None,
    );
    transfer(
        blob,
        rec,
        "push_noop",
        "publish again with nothing changed",
        suffix,
        &[mirror_up()],
        None,
    );

    let pulled = dir.join("pulled-store");
    let _ = std::fs::create_dir_all(&pulled);
    let st = transfer(
        blob,
        rec,
        "pull_fresh",
        "download everything published so far to an empty client",
        suffix,
        &[mirror_down(&pulled)],
        Some(Delivery::delivered(
            format!(
                "the whole published store, which holds all {} reference(s) \
                 published so far; these cores have no selective fetch",
                p0.len()
            ),
            p0.len(),
            stage_bytes(&p0),
        )),
    );
    rec.verify(verify_amber_pull(
        ctx,
        &cli,
        &pulled,
        scenario,
        &[&p0],
        st == OpStatus::Ok,
        dir,
        "fresh_pull_restores_correctly",
    ));

    // Only the Nix scenario can be short of a second generation, and only
    // when a single closure root was supplied; see NixOrigin.
    if let Some(fixture) = ctx.nix.as_ref()
        && scenario == SCENARIO_NIX
        && !fixture.origin.has_churn()
    {
        for name in ["push_incremental", "pull_incremental", "retention_cleanup"] {
            rec.push(Op::unsupported(name, fixture.origin.no_churn_reason()));
        }
        let probe = dir.join("pulled-probe");
        let _ = std::fs::create_dir_all(&probe);
        let probe_cli = AmberCli {
            store: probe.clone(),
            ..cli.clone()
        };
        let checks: Vec<Run> = p0
            .iter()
            .map(|i| {
                probe_cli.export_stdout(&format!("ref:{}", i.reference), Path::new("/dev/null"))
            })
            .collect();
        let mut cmds = vec![mirror_down(&probe)];
        cmds.extend(checks);
        let v = fault_injection(blob, rec, suffix, "store/", &cmds);
        rec.verify(v);
        return;
    }

    if !ingest_stage(rec, &p1) {
        return;
    }
    transfer(
        blob,
        rec,
        "push_incremental",
        "publish after storing the next generation",
        suffix,
        &[mirror_up()],
        None,
    );

    let st = transfer(
        blob,
        rec,
        "pull_incremental",
        "download only what changed into the existing client",
        suffix,
        &[mirror_down(&pulled)],
        Some(Delivery::delivered(
            format!(
                "the store's new pack segments and reference database, \
                 bringing the client up to the {} reference(s) the \
                 incremental publication added",
                p1.len()
            ),
            p1.len(),
            stage_bytes(&p1),
        )),
    );
    // Both generations must be restorable from the client, not just the
    // newest: the delivered set is everything published so far.
    rec.verify(verify_amber_pull(
        ctx,
        &cli,
        &pulled,
        scenario,
        &[&p0, &p1],
        st == OpStatus::Ok,
        dir,
        "incremental_pull_restores_correctly",
    ));

    // Retention: drop generation 0's references, collect, and propagate the
    // deletions to the bucket.
    let keep: Vec<String> = p1.iter().map(|i| i.reference.clone()).collect();
    let doomed: Vec<String> = p0
        .iter()
        .map(|i| i.reference.clone())
        .filter(|n| !keep.contains(n))
        .collect();
    let local: Vec<Run> = doomed.iter().map(|n| cli.ref_rm(n)).collect();
    setup_cmds(
        rec,
        "retention_local",
        "drop generation 0's references",
        &local,
    );
    // Sealed segments have to be older than the cores' minimum pack age
    // before a collection can reap them; see age_sealed_segments.
    rec.push(super::age_segments(&cli));
    setup_cmds(
        rec,
        "retention_local",
        "collect locally, so the publication that follows has deletions to \
         propagate",
        &[cli.gc_run()],
    );
    transfer(
        blob,
        rec,
        "retention_cleanup",
        "propagate the collection to object storage, removing reaped segments",
        suffix,
        &[mirror_up()],
        Some(Delivery::retained(
            format!(
                "the {} reference(s) of the incremental publication; the {} of \
                 the initial one were dropped and collected",
                keep.len(),
                doomed.len()
            ),
            keep.len(),
            stage_bytes(&p1),
        )),
    );

    let after = dir.join("pulled-after-cleanup");
    let _ = std::fs::create_dir_all(&after);
    let ok = blob
        .mc_admin()
        .args(["mirror", "--overwrite", "--remove"])
        .arg(&remote)
        .arg(&after)
        .run()
        .map(|o| o.success())
        .unwrap_or(false);
    rec.verify(verify_amber_pull(
        ctx,
        &cli,
        &after,
        scenario,
        &[&p1],
        ok,
        dir,
        "retained_content_valid_after_cleanup",
    ));
    // And what retention dropped is gone from the published store, as that
    // store's own reference listing sees it.
    let client = AmberCli {
        store: after.clone(),
        ..cli.clone()
    };
    let listing = client
        .ref_list()
        .ok()
        .map(|o| crate::adapters::amber::parse_ref_list(&o.stdout_text()));
    let kept_names: Vec<&str> = keep.iter().map(String::as_str).collect();
    let dropped_names: Vec<&str> = doomed.iter().map(String::as_str).collect();
    rec.verify(super::check_retention(
        "dropped_references_are_absent",
        super::RetentionEvidence {
            listing,
            still_readable: Vec::new(),
        },
        &kept_names,
        &dropped_names,
    ));

    let probe = dir.join("pulled-probe");
    let _ = std::fs::create_dir_all(&probe);
    let probe_cli = AmberCli {
        store: probe.clone(),
        ..cli.clone()
    };
    let checks: Vec<Run> = p1
        .iter()
        .map(|i| probe_cli.export_stdout(&format!("ref:{}", i.reference), Path::new("/dev/null")))
        .collect();
    let mut cmds = vec![mirror_down(&probe)];
    cmds.extend(checks);
    let v = fault_injection(blob, rec, suffix, "store/", &cmds);
    rec.verify(v);
}

/// Restores the published references of every delivered generation from the
/// pulled store and compares each one with the bytes it was built from.
///
/// By default that is *every* delivered reference, which is what makes a
/// pull's `delivered_references` counter mean something. A profile that sets
/// `verify_version_budget` may check a spread of them instead — and then the
/// check is named `sampled_…` and reports how many it did not look at, so a
/// subset never gets to claim it checked everything.
///
/// For the Nix scenario it additionally recomputes each restored path's NAR
/// hash with `nix hash path` and compares it with the hash the source Nix
/// store recorded — evidence that needs no Nix support in either core.
#[allow(clippy::too_many_arguments)]
fn verify_amber_pull(
    ctx: &Ctx,
    cli: &AmberCli,
    pulled: &Path,
    scenario: &str,
    delivered: &[&Stage],
    pulled_ok: bool,
    dir: &Path,
    base_name: &str,
) -> Verification {
    let items: Vec<&Item> = delivered.iter().flat_map(|s| s.iter()).collect();
    let total = items.len();
    // Which of them to look at, and what the check may therefore be called.
    let picked: Vec<usize> = match ctx.profile.verify_version_budget {
        None => (0..total).collect(),
        Some(budget) => super::versions_to_verify(total, budget),
    };
    let sampled = picked.len() < total;
    let name = if sampled {
        format!("sampled_{base_name}")
    } else {
        base_name.to_string()
    };
    let name = name.as_str();
    if !pulled_ok {
        return Verification::fail(name, "the download failed, so there was nothing to verify");
    }
    // Read a *copy* of what was downloaded, never the download itself.
    // Opening either core's reference database rewrites it — Pebble
    // especially, which compacts and renames on open — so verifying in place
    // would leave the client's copy different from what was published, and
    // the incremental transfer measured next would be charged for the
    // harness's own verification. It would also, with Pebble, leave a
    // directory whose newest manifest describes the *pre-sync* file set, so
    // a later additive mirror would appear to have delivered nothing.
    let workspace = dir.join("verify-client");
    let _ = fsx::make_writable_tree(&workspace);
    let _ = std::fs::remove_dir_all(&workspace);
    if let Err(e) = fsx::copy_tree(pulled, &workspace) {
        return Verification::fail(
            name,
            format!("the downloaded store could not be copied for verification: {e}"),
        );
    }
    let client = AmberCli {
        store: workspace.clone(),
        ..cli.clone()
    };
    let hasher = if scenario == SCENARIO_NIX {
        ctx.tools.path("nix").ok().map(|bin| NixCli {
            bin,
            store: None,
            timeout: ctx.timeout,
            cache: ClientCache::Inherited,
        })
    } else {
        None
    };
    let mut problems = Vec::new();
    let mut checked = 0usize;
    let mut hashed = 0usize;
    for index in &picked {
        let item = items[*index];
        let reference = item.reference.as_str();
        let dest = dir.join("verify").join(reference);
        let _ = fsx::make_writable_tree(&dest);
        let _ = std::fs::remove_dir_all(&dest);
        if let Err(e) = std::fs::create_dir_all(&dest) {
            problems.push(format!("{reference}: {e}"));
            continue;
        }
        if let Err(e) = client.restore(&format!("ref:{reference}"), &dest).ok() {
            problems.push(format!(
                "{reference}: restore from the pulled store failed: {}",
                first_line(&e)
            ));
            continue;
        }
        // What this reference is supposed to contain. A live directory is
        // re-observed now; a history state is compared with the manifest
        // observed from the staging tree when the history was generated,
        // because the working tree it was ingested from has since moved on.
        let want = match &item.kind {
            ItemKind::Tree(source) => {
                Manifest::observe("source", source).map_err(|e| format!("source unreadable: {e}"))
            }
            ItemKind::HistoryState(state) => match ctx.history_manifests.get(*state) {
                Some(path) => Manifest::load(path)
                    .map_err(|e| format!("reference manifest {}: {e}", path.display())),
                None => Err(format!("no reference manifest for history state {state}")),
            },
        };
        let want = match want {
            Ok(m) => m,
            Err(e) => {
                problems.push(format!("{reference}: {e}"));
                continue;
            }
        };
        let v = want.verify_tree(name, &dest, Strictness::full());
        if v.passed {
            checked += 1;
        } else {
            problems.push(format!("{reference}: {}", v.detail));
            problems.extend(v.corrupt.into_iter().take(3));
        }
        // The Nix payload carries recorded NAR hashes; a tree store that
        // restored the same bytes must reproduce them.
        if let (Some(hasher), Some(fixture), ItemKind::Tree(source)) =
            (&hasher, ctx.nix.as_ref(), &item.kind)
        {
            let store_path = source.display().to_string();
            match fixture.details.get(&store_path) {
                Some((want_hash, _)) => {
                    let got = match hasher.hash_path(&dest).run() {
                        Ok(o) if o.success() => nixstore::parse_hash_path(&o.stdout_text()),
                        Ok(o) => Err(o.require_success().unwrap_err()),
                        Err(e) => Err(e),
                    };
                    match got {
                        Ok(hash) if &hash == want_hash => hashed += 1,
                        Ok(hash) => problems.push(format!(
                            "{reference}: recomputed NAR hash {hash} != source {want_hash}"
                        )),
                        Err(e) => problems.push(format!("{reference}: {}", first_line(&e))),
                    }
                }
                None => problems.push(format!(
                    "{reference}: {store_path} is not recorded in the source closure"
                )),
            }
        }
        let _ = fsx::make_writable_tree(&dest);
        let _ = std::fs::remove_dir_all(&dest);
    }
    let unverified = total - picked.len();
    let mut v = if problems.is_empty() {
        Verification::pass(
            name,
            format!(
                "{} of the {total} delivered reference(s) restored from the \
                 downloaded store and matched byte for byte{}{}",
                picked.len(),
                if hashed > 0 {
                    format!(
                        "; {hashed} of them also reproduced the NAR hash the \
                         source Nix store recorded"
                    )
                } else {
                    String::new()
                },
                if sampled {
                    format!(
                        ". {unverified} delivered reference(s) were NOT \
                         verified, because profile {:?} samples them. This is \
                         not a statement about them.",
                        ctx.profile.name
                    )
                } else {
                    ", which is every one of them; 0 unverified".to_string()
                }
            ),
        )
    } else {
        Verification::fail(
            name,
            format!(
                "{} of the {} verified delivered reference(s) did not verify \
                 ({checked} matched, {unverified} of {total} not verified at \
                 all)",
                problems.len(),
                picked.len(),
            ),
        )
    };
    v.corrupt = problems;
    let _ = fsx::make_writable_tree(&workspace);
    let _ = std::fs::remove_dir_all(&workspace);
    v
}

// ---------------------------------------------------------------------------
// shared verification helpers
// ---------------------------------------------------------------------------

fn verify_corpus(ctx: &Ctx, generation: usize, root: &Path, ok: bool, name: &str) -> Verification {
    if !ok {
        return Verification::fail(name, "the transfer failed, so there was nothing to verify");
    }
    match ctx.corpus.manifest(generation) {
        Ok(m) => m.verify_tree(name, root, Strictness::full()),
        Err(e) => Verification::fail(name, format!("reference manifest unreadable: {e}")),
    }
}

/// Verifies a closure that arrived in a destination Nix store.
///
/// Three separate things are checked, and the expected set drives all of
/// them so a path that never arrived cannot pass by not being iterated over:
///
/// 1. the destination registers every expected path, with the source store's
///    NAR hash and reference list, and registers nothing else;
/// 2. the NAR hash recomputed from the bytes on disk with `nix hash path`
///    equals the source store's — independent of what the destination
///    registered about itself;
/// 3. the closure is complete: every reference of a retained path is
///    retained too.
///
/// `root` is the top of the closure, passed explicitly: a closure is sorted
/// by store path, so its last element is not its root and querying that
/// would ask the wrong question.
#[allow(clippy::too_many_arguments)]
fn verify_nix_closure(
    ctx: &Ctx,
    cli: &NixCli,
    fixture: &super::NixFixture,
    root: &str,
    want: &[String],
    ok: bool,
    name: &str,
) -> Verification {
    if !ok {
        return Verification::fail(name, "the transfer failed, so there was nothing to verify");
    }
    let out = match cli.path_info_closure(root).run() {
        Ok(o) if o.success() => o,
        Ok(o) => return Verification::fail(name, o.require_success().unwrap_err()),
        Err(e) => return Verification::fail(name, e),
    };
    let got = match nixstore::parse_closure_details(&out.stdout_text()) {
        Ok(g) => g,
        Err(e) => return Verification::fail(name, e),
    };
    let store_dir = cli
        .store
        .clone()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("nix/store");
    let hasher = NixCli {
        bin: cli.bin.clone(),
        store: None,
        timeout: cli.timeout,
        cache: ClientCache::Inherited,
    };
    let mut problems = Vec::new();
    let mut hashed = 0usize;
    for path in want {
        let Some((want_hash, want_refs)) = fixture.details.get(path) else {
            problems.push(format!("{path}: not recorded in the source closure"));
            continue;
        };
        match got.get(path) {
            Some((hash, refs)) => {
                if hash != want_hash {
                    problems.push(format!("{path}: NAR hash {hash} != {want_hash}"));
                }
                if refs != want_refs {
                    problems.push(format!("{path}: references differ from the source store"));
                }
            }
            None => {
                problems.push(format!("{path}: missing from the downloaded store"));
                continue;
            }
        }
        // Recomputed independently of the destination's own registration.
        let here = store_dir.join(Path::new(path).file_name().unwrap_or_default());
        match hasher.hash_path(&here).run() {
            Ok(o) if o.success() => match nixstore::parse_hash_path(&o.stdout_text()) {
                Ok(hash) if &hash == want_hash => hashed += 1,
                Ok(hash) => problems.push(format!(
                    "{path}: NAR hash recomputed from the bytes on disk is \
                     {hash}, source recorded {want_hash}"
                )),
                Err(e) => problems.push(format!("{path}: {e}")),
            },
            Ok(o) => problems.push(format!(
                "{path}: rehashing the downloaded path failed: {}",
                first_line(&o.require_success().unwrap_err())
            )),
            Err(e) => problems.push(format!("{path}: {}", first_line(&e))),
        }
    }
    // Nothing unexpected either, and the closure has to be complete.
    let expected: std::collections::BTreeSet<&String> = want.iter().collect();
    for path in got.keys() {
        if !expected.contains(path) {
            problems.push(format!(
                "{path}: present in the destination but not expected"
            ));
        }
    }
    for path in want {
        if let Some((_, refs)) = fixture.details.get(path) {
            for r in refs {
                if !expected.contains(r) {
                    problems.push(format!(
                        "{path} references {r}, which is outside the delivered closure"
                    ));
                }
            }
        }
    }
    let _ = ctx;
    let mut v = if problems.is_empty() {
        Verification::pass(
            name,
            format!(
                "all {} paths of the closure arrived: the destination \
                 registers each one with the source store's NAR hash and \
                 reference list, {hashed} of them reproduce that hash when \
                 recomputed from the bytes on disk with `nix hash path`, the \
                 destination holds nothing extra, and every reference of \
                 every path is inside the delivered set",
                want.len()
            ),
        )
    } else {
        Verification::fail(name, format!("{} problem(s)", problems.len()))
    };
    v.corrupt = problems;
    v
}

/// Deletes one published object and requires the backend to notice.
///
/// A store that silently returns a short or empty result when an object has
/// gone missing is worse than one that fails, so this is a pass/fail check on
/// *detection*, not on recovery. It runs last, because it deliberately leaves
/// the published state broken.
fn fault_injection(
    blob: &BlobCtx,
    rec: &mut Recorder,
    suffix: &str,
    under: &str,
    detect: &[Run],
) -> Verification {
    const NAME: &str = "missing_object_is_detected";
    let scope = format!("{suffix}/{}", under.trim_end_matches('/'));
    let objects = match s3::list(&blob.mc, &blob.mc_config, &blob.target, &scope) {
        Ok(o) => o,
        Err(e) => return Verification::fail(NAME, format!("listing the published objects: {e}")),
    };
    // `mc ls` reports keys either relative to the bucket or relative to
    // the listed prefix depending on version, so both are mapped back to
    // a path under this repetition's own prefix before anything is
    // deleted. A key that maps outside it is refused by mc_path.
    let Some(victim) = objects.iter().filter(|o| o.size > 0).max_by_key(|o| o.size) else {
        return Verification::fail(
            NAME,
            format!("nothing was published under {scope} to remove"),
        );
    };
    let raw = victim.key.as_str();
    let key = if let Some(rest) = raw.strip_prefix(&format!("{}/", blob.target.prefix)) {
        rest.to_string()
    } else if raw.starts_with(&format!("{suffix}/")) {
        raw.to_string()
    } else {
        format!("{scope}/{}", raw.trim_start_matches('/'))
    };
    if let Err(e) = s3::remove_tree(&blob.mc, &blob.mc_config, &blob.target, &key) {
        return Verification::fail(NAME, format!("removing {key}: {e}"));
    }
    rec.push(
        Op::aside("inject_missing_object", Phase::Verify, Default::default())
            .describe(format!("delete the published object {key}"))
            .note("this deliberately breaks the published state; it is the last thing the repetition does"),
    );
    let mut detected = false;
    let mut detail = String::new();
    for cmd in detect {
        match cmd.run() {
            Ok(out) if out.success() => {}
            Ok(out) => {
                detected = true;
                detail = out.require_success().unwrap_err();
                break;
            }
            Err(e) => {
                detected = true;
                detail = e;
                break;
            }
        }
    }
    if detected {
        Verification::pass(
            NAME,
            format!(
                "after deleting {key}, the backend failed rather than returning \
                     an incomplete result: {}",
                first_line(&detail)
            ),
        )
    } else {
        Verification::fail(
            NAME,
            format!("after deleting {key}, the backend still reported success"),
        )
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// The publishable description of the object-storage setup.
pub fn target_info(blob: &BlobCtx) -> BTreeMap<String, serde_json::Value> {
    let mut m = BTreeMap::new();
    m.insert(
        "target".into(),
        serde_json::to_value(blob.target.info()).unwrap_or(serde_json::Value::Null),
    );
    m.insert(
        "service".into(),
        serde_json::Value::String(blob.service.clone()),
    );
    m.insert(
        "shaping".into(),
        serde_json::Value::String(blob.gateway.shaping().describe()),
    );
    m.insert(
        "gateway_endpoint".into(),
        serde_json::Value::String(blob.gateway.endpoint()),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scenario_lists_its_backends_and_includes_both_cores() {
        for s in SCENARIOS {
            let b = backends(s);
            assert!(b.len() >= 3, "{s}: {b:?}");
            assert!(b.contains(&"amber-rust-s3"), "{s}");
            assert!(b.contains(&"amber-go-s3"), "{s}");
        }
        assert!(backends("blob/nonsense").is_empty());
    }

    /// A blob row that said "unrecorded" for its compression or its
    /// durability would be hiding something the suite knows, and a reader
    /// comparing a Git bundle with an Amber publication needs both.
    #[test]
    fn every_blob_backend_states_the_storage_semantics_it_inherits() {
        for scenario in SCENARIOS {
            for backend in backends(scenario) {
                let s = semantics(storage_base(backend));
                for (field, value) in [
                    ("compression", &s.compression),
                    ("encryption", &s.encryption),
                    ("durability", &s.durability),
                    ("concurrency", &s.concurrency),
                ] {
                    assert_ne!(
                        value, "unrecorded",
                        "{backend} in {scenario} has no recorded {field}"
                    );
                }
            }
        }
        assert_eq!(storage_base("git-bundle-s3"), "git");
        assert_eq!(storage_base("nix-binary-cache"), "nix");
        assert_eq!(storage_base("restic-s3"), "restic");
        assert_eq!(storage_base("amber-go-s3"), "amber-go");
    }

    #[test]
    fn the_short_scenario_name_is_a_usable_path_component() {
        assert_eq!(short(SCENARIO_BACKUP), "backup-corpus");
        assert_eq!(short(SCENARIO_NIX), "nix-closure");
        for s in SCENARIOS {
            assert!(!short(s).contains('/'));
        }
    }

    #[test]
    fn a_nix_store_path_hash_is_extracted() {
        assert_eq!(
            hash_part("/nix/store/abc123-name").as_deref(),
            Some("abc123")
        );
        assert!(
            hash_part("/nix/store").is_none(),
            "a name with no dash has no hash part"
        );
    }

    #[test]
    fn only_the_first_line_of_an_error_is_quoted_back() {
        assert_eq!(first_line("boom\ndetails\nmore"), "boom");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn a_delivery_states_what_arrived_not_only_what_it_cost() {
        let d = Delivery::delivered("snapshot g0", 1, 4096);
        assert_eq!(d.references, 1);
        assert_eq!(d.logical_bytes, 4096);
        assert!(d.what.contains("g0"));
        assert_eq!(d.role, "delivered");
        assert!(d.sentence().starts_with("delivered to the destination"));
    }

    #[test]
    fn a_retention_statement_refuses_to_be_read_as_a_ranking() {
        let d = Delivery::retained("snapshot g1", 1, 4096);
        assert_eq!(d.role, "retained");
        let s = d.sentence();
        assert!(s.contains("retained by this retention step"), "{s}");
        assert!(s.contains("not a like-for-like ranking"), "{s}");
    }

    fn ctx() -> Ctx {
        let profile = crate::config::profile("smoke").unwrap();
        let history = crate::dataset::gitgen::plan(&profile.git, 11);
        Ctx {
            seed: 11,
            jobs: 1,
            scratch: PathBuf::from("/scratch"),
            out: PathBuf::from("/out"),
            tools: Default::default(),
            corpus: crate::dataset::corpus::Corpus {
                root: PathBuf::from("/scratch/corpus"),
                seed: 11,
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

    /// The delivered set of the source-history scenario has to be the same
    /// set Git's bundles carry, or the two `pull_*` rows would be pricing
    /// different outcomes.
    #[test]
    fn the_amber_source_history_stages_cover_every_retained_version() {
        let ctx = ctx();
        let steps = ctx.history.steps.len();
        let [p0, p1] = stages(&ctx, SCENARIO_HISTORY).expect("stages");
        assert_eq!(
            p0.len() + p1.len(),
            steps,
            "every retained version must be published, not a sample of them"
        );
        // The same split the Git backend publishes at.
        let split = (steps * 3 / 4).max(1);
        assert_eq!(p0.len(), split);
        assert_eq!(p1.len(), steps - split);

        // Each item names the history state it came from, in order, and
        // carries that state's payload size.
        let mut expected = 0..steps;
        for item in p0.iter().chain(p1.iter()) {
            let i = expected.next().unwrap();
            match item.kind {
                ItemKind::HistoryState(state) => assert_eq!(state, i),
                _ => panic!("{} is not a history state", item.reference),
            }
            assert_eq!(item.reference, ctx.history.steps[i].ref_name);
            assert_eq!(item.logical_bytes, ctx.history.state_bytes(i));
        }
        assert!(expected.next().is_none());
        // And the payload sizes add up to the same totals the Git backend
        // reports for the two halves it publishes.
        assert_eq!(stage_bytes(&p0), history_bytes(&ctx, 0, split));
        assert_eq!(stage_bytes(&p1), history_bytes(&ctx, split, steps));
    }

    #[test]
    fn the_corpus_scenario_publishes_one_generation_per_stage() {
        let mut ctx = ctx();
        assert!(
            stages(&ctx, SCENARIO_BACKUP).is_err(),
            "an empty corpus has no two generations to publish"
        );
        for i in 0..2 {
            ctx.corpus
                .generations
                .push(crate::dataset::corpus::Generation {
                    index: i,
                    dir: PathBuf::from(format!("/scratch/corpus/gen{i}")),
                    manifest_path: PathBuf::from("/m"),
                    logical_bytes: 100 + i as u64,
                    file_count: 1,
                    dir_count: 1,
                    symlink_count: 0,
                    manifest_digest: "d".into(),
                });
        }
        let [p0, p1] = stages(&ctx, SCENARIO_BACKUP).expect("stages");
        assert_eq!(p0[0].reference, "gen0");
        assert_eq!(p1[0].reference, "gen1");
        assert_eq!(stage_bytes(&p0), 100);
        assert_eq!(stage_bytes(&p1), 101);
    }

    #[test]
    fn an_unknown_scenario_has_no_stages() {
        assert!(stages(&ctx(), "blob/nonsense").is_err());
    }
}
