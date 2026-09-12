//! The case registry, one module per public core module.

pub mod codec;
pub mod fstree_cases;
pub mod gc_cases;
pub mod ingest_cases;
pub mod pack;
pub mod packstore_cases;
pub mod stores_cases;
pub mod tar_cases;

use std::collections::BTreeSet;

use crate::env::Env;
use crate::fixtures::{
    Fixtures, PayloadSet, digest, digest_keys, digest_strings, digest_vecs, manifest,
};
use crate::harness::{Case, Recorder};

/// The five batched payload sets, addressed by index so a case can name one
/// without borrowing the fixtures at construction time.
pub fn payload_set(fx: &Fixtures, i: usize) -> &PayloadSet {
    match i {
        0 => &fx.tiny,
        1 => &fx.small,
        2 => &fx.text,
        3 => &fx.large,
        _ => &fx.rand,
    }
}

pub const PAYLOAD_SETS: [usize; 5] = [0, 1, 2, 3, 4];

/// The registry, in module order. Every entry maps to one public Rust module,
/// which is how the coverage matrix stays checkable against the two cores'
/// public surfaces.
pub fn registry(env: &Env, only: &Option<BTreeSet<String>>) -> Vec<Case> {
    let want = |g: &str| only.as_ref().is_none_or(|s| s.contains(g));
    let mut out = Vec::new();
    if want("key") {
        out.extend(codec::key_cases(env));
    }
    if want("cbor") {
        out.extend(codec::cbor_cases(env));
    }
    if want("binaryfuse") {
        out.extend(codec::binaryfuse_cases(env));
    }
    if want("chunkers") {
        out.extend(codec::chunker_cases(env));
    }
    if want("fstree") {
        out.extend(fstree_cases::cases(env));
    }
    if want("amberignore") {
        out.extend(ingest_cases::ignore_cases(env));
    }
    if want("ingest") {
        out.extend(ingest_cases::ingest_cases(env));
    }
    if want("amberpack") {
        out.extend(pack::cases(env));
    }
    if want("packstore") {
        out.extend(packstore_cases::cases(env));
    }
    if want("reference") {
        out.extend(codec::reference_cases(env));
    }
    if want("refstore") {
        out.extend(stores_cases::refstore_cases(env));
    }
    if want("inbox") {
        out.extend(stores_cases::inbox_cases(env));
    }
    if want("tar") {
        out.extend(tar_cases::cases(env));
    }
    if want("gc") {
        out.extend(gc_cases::cases(env));
    }
    out
}

pub fn run_checks(env: &Env, rec: &mut Recorder, only: &Option<BTreeSet<String>>) {
    let want = |g: &str| only.as_ref().is_none_or(|s| s.contains(g));
    fixture_checks(env, rec);
    if want("key") {
        codec::key_checks(env, rec);
    }
    if want("cbor") {
        codec::cbor_checks(env, rec);
    }
    if want("binaryfuse") {
        codec::binaryfuse_checks(env, rec);
    }
    if want("chunkers") {
        codec::chunker_checks(env, rec);
    }
    if want("fstree") {
        fstree_cases::checks(env, rec);
    }
    if want("amberignore") {
        ingest_cases::ignore_checks(env, rec);
    }
    if want("ingest") {
        ingest_cases::ingest_checks(env, rec);
    }
    if want("amberpack") {
        pack::checks(env, rec);
    }
    if want("packstore") {
        packstore_cases::checks(env, rec);
    }
    if want("reference") {
        codec::reference_checks(env, rec);
    }
    if want("refstore") {
        stores_cases::refstore_checks(env, rec);
    }
    if want("inbox") {
        stores_cases::inbox_checks(env, rec);
    }
    if want("tar") {
        tar_cases::checks(env, rec);
    }
    if want("gc") {
        gc_cases::checks(env, rec);
    }
}

/// Records the canonical digests of the fixtures themselves. The report
/// compares each of these against the other core's value and refuses to pair
/// any timing unless they match.
fn fixture_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    rec.pass(
        "fixture",
        "fixture.corpus",
        "fixture/corpus-random",
        digest(&fx.corpus_random),
    );
    rec.pass(
        "fixture",
        "fixture.corpus",
        "fixture/corpus-text",
        digest(&fx.corpus_text),
    );
    rec.pass(
        "fixture",
        "fixture.payloads",
        "fixture/payloads-tiny",
        digest_vecs(&fx.tiny.items),
    );
    rec.pass(
        "fixture",
        "fixture.payloads",
        "fixture/payloads-small",
        digest_vecs(&fx.small.items),
    );
    rec.pass(
        "fixture",
        "fixture.payloads",
        "fixture/payloads-text",
        digest_vecs(&fx.text.items),
    );
    rec.pass(
        "fixture",
        "fixture.payloads",
        "fixture/payloads-large",
        digest_vecs(&fx.large.items),
    );
    rec.pass(
        "fixture",
        "fixture.payloads",
        "fixture/payloads-rand",
        digest_vecs(&fx.rand.items),
    );
    rec.pass(
        "fixture",
        "fixture.keys",
        "fixture/keys",
        digest_keys(&fx.keys),
    );
    rec.pass(
        "fixture",
        "fixture.entries",
        "fixture/entries-large",
        digest(&fx.enc_dir_leaf_large),
    );
    rec.pass(
        "fixture",
        "fixture.tree",
        "fixture/tree-v1",
        manifest(&fx.tree_v1).unwrap(),
    );
    rec.pass(
        "fixture",
        "fixture.tree",
        "fixture/tree-v2",
        manifest(&fx.tree_v2).unwrap(),
    );
    rec.pass(
        "fixture",
        "fixture.tree",
        "fixture/ignore-dir",
        manifest(&fx.ignore_dir).unwrap(),
    );
    // The wire pack embeds zstd-compressed payloads, which the two cores
    // produce with different encoders; its digest is a within-core anchor,
    // not a cross-core one.
    rec.pass_local(
        "fixture",
        "fixture.pack",
        "fixture/wire-pack-bytes",
        digest(&fx.wire_pack),
    );
    rec.pass(
        "fixture",
        "fixture.pack",
        "fixture/wire-pack-contents",
        pack::pack_content_digest(&fx.pack_objects),
    );
    rec.pass(
        "fixture",
        "fixture.store",
        "fixture/store-keys",
        digest_keys(&fx.store_keys),
    );
    rec.pass(
        "fixture",
        "fixture.tar",
        "fixture/tar-archive",
        digest(&fx.tar_bytes),
    );
    rec.pass(
        "fixture",
        "fixture.mem",
        "fixture/wide-root",
        fx.wide_root.to_string(),
    );
    rec.pass(
        "fixture",
        "fixture.mem",
        "fixture/deep-root",
        fx.deep_root.to_string(),
    );
    rec.pass(
        "fixture",
        "fixture.mem",
        "fixture/file-root",
        fx.file_root.to_string(),
    );
    rec.pass(
        "fixture",
        "fixture.gc",
        "fixture/gc-live-root",
        fx.gc_live_root.to_string(),
    );
    let _ = digest_strings::<String>(&[]);
}
