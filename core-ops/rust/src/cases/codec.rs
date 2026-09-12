//! key, cbor, binaryfuse, chunkers and reference.

use amber_store_core::binaryfuse::BinaryFuse16;
use amber_store_core::cbor;
use amber_store_core::chunkers::{self, ByteOpts, ItemChunker};
use amber_store_core::key::{self, Key, Type};
use amber_store_core::reference;

use crate::cases::{PAYLOAD_SETS, payload_set};
use crate::env::Env;
use crate::fixtures::{digest, digest_keys, digest_strings, digest_vecs, random_bytes};
use crate::harness::{Case, Recorder, State};

// ---------------------------------------------------------------------------
// key
// ---------------------------------------------------------------------------

pub fn key_cases(env: &Env) -> Vec<Case> {
    let mut out = Vec::new();
    for i in PAYLOAD_SETS {
        let ps = payload_set(&env.fx, i);
        out.push(
            Case::new(
                "key",
                "key.new",
                &ps.name,
                1,
                ps.items.len(),
                Box::new(move |_| Box::new(i) as State),
                Box::new(move |env, s| {
                    let idx = *s.downcast_ref::<usize>().unwrap();
                    let ps = payload_set(&env.fx, idx);
                    let mut acc = 0u64;
                    for b in &ps.items {
                        let k = Key::new(Type::Blob, b.len() as u64, b);
                        acc += k.as_bytes()[0] as u64;
                    }
                    acc
                }),
            )
            .bytes(ps.bytes),
        );
    }

    out.push(Case::new(
        "key",
        "key.new_from_hash",
        "batch",
        1,
        env.fx.keys.len(),
        Box::new(|env| {
            let hashes: Vec<[u8; 32]> = env
                .fx
                .keys
                .iter()
                .map(|k| {
                    let mut h = [0u8; 32];
                    h[..k.hash().len()].copy_from_slice(k.hash());
                    h
                })
                .collect();
            Box::new(hashes) as State
        }),
        Box::new(|_, s| {
            let hashes = s.downcast_ref::<Vec<[u8; 32]>>().unwrap();
            let mut acc = 0u64;
            for (i, h) in hashes.iter().enumerate() {
                let k = Key::new_from_hash(Type::Blob, i as u64, *h);
                acc += k.as_bytes()[1] as u64;
            }
            acc
        }),
    ));

    out.push(Case::new(
        "key",
        "key.parse",
        "canonical",
        1,
        env.fx.key_bytes.len(),
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for b in &env.fx.key_bytes {
                let k = Key::parse(b).unwrap();
                acc += k.as_bytes()[2] as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "key",
        "key.parse",
        "malformed",
        1,
        env.fx.bad_key_bytes.len() * 256,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for _ in 0..256 {
                for b in &env.fx.bad_key_bytes {
                    if Key::parse(b).is_err() {
                        acc += 1;
                    }
                }
            }
            acc
        }),
    ));
    out.push(Case::new(
        "key",
        "key.validate",
        "canonical",
        1,
        env.fx.keys.len(),
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for k in &env.fx.keys {
                if k.validate().is_ok() {
                    acc += 1;
                }
            }
            acc
        }),
    ));
    // The header accessors are single field reads; timing each one separately
    // would measure the loop, not the core. They are batched into one grouped
    // case and the coverage matrix records that.
    out.push(Case::new(
        "key",
        "key.accessors",
        "type+length+length_size+hash",
        1,
        env.fx.keys.len() * 4,
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for k in &env.fx.keys {
                acc += k.type_() as u64 + k.length() + k.length_size() as u64 + k.hash()[0] as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "key",
        "key.string",
        "hex",
        1,
        env.fx.keys.len(),
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for k in &env.fx.keys {
                acc += k.to_string().len() as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "key",
        "key.type_string",
        "names",
        1,
        env.fx.keys.len(),
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for k in &env.fx.keys {
                acc += k.type_().to_string().len() as u64;
                if Type::is_valid(k.type_() as u8) {
                    acc += 1;
                }
            }
            acc
        }),
    ));
    out
}

pub fn key_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let built: Vec<Key> = fx
        .small
        .items
        .iter()
        .map(|b| Key::new(Type::Blob, b.len() as u64, b))
        .collect();
    rec.pass("key", "key.new", "key/new-small", digest_keys(&built));

    let from_hash: Vec<Key> = [
        Type::Blob,
        Type::FileNode,
        Type::DirLeaf,
        Type::DirNode,
        Type::XattrSet,
    ]
    .iter()
    .enumerate()
    .map(|(i, t)| {
        let h: [u8; 32] = random_bytes(env.profile.seed + 12345 + i as u64, 32)
            .try_into()
            .unwrap();
        Key::new_from_hash(*t, i as u64 * 777, h)
    })
    .collect();
    rec.pass(
        "key",
        "key.new_from_hash",
        "key/new-from-hash",
        digest_keys(&from_hash),
    );

    let k0 = built[0];
    let rt = Key::parse(k0.as_bytes());
    rec.want(
        "key",
        "key.parse",
        "key/parse-roundtrip",
        matches!(&rt, Ok(k) if *k == k0),
        format!("parse round trip failed: {rt:?}"),
        k0.to_string(),
    );
    rec.want(
        "key",
        "key.accessors",
        "key/accessors",
        k0.type_() == Type::Blob
            && k0.length() == fx.small.items[0].len() as u64
            && k0.length_size() >= 1
            && k0.hash().len() == key::SIZE - 1 - k0.length_size(),
        "accessors disagree with the constructed key",
        format!("{}/{}/{}", k0.type_(), k0.length(), k0.length_size()),
    );

    // The rejection paths are part of the contract; each malformed key must
    // fail with its own error.
    let wants = [
        key::Error::ReservedBitSet,
        key::Error::ReservedType(5),
        key::Error::NonCanonicalLength,
        key::Error::BadKeyLength(31),
    ];
    let mut ok = true;
    let mut details = Vec::new();
    for (i, b) in fx.bad_key_bytes.iter().enumerate() {
        let got = Key::parse(b);
        let matched = matches!(
            (&got, &wants[i]),
            (Err(key::Error::ReservedBitSet), key::Error::ReservedBitSet)
                | (
                    Err(key::Error::ReservedType(_)),
                    key::Error::ReservedType(_)
                )
                | (
                    Err(key::Error::NonCanonicalLength),
                    key::Error::NonCanonicalLength
                )
                | (
                    Err(key::Error::BadKeyLength(_)),
                    key::Error::BadKeyLength(_)
                )
        );
        if !matched {
            ok = false;
            details.push(format!("case {i}: got {got:?}, want {:?}", wants[i]));
        }
    }
    rec.want(
        "key",
        "key.parse",
        "key/parse-rejects",
        ok,
        details.join("; "),
        digest_strings(&[
            "ErrReservedBitSet",
            "ErrReservedType",
            "ErrNonCanonicalLength",
            "ErrBadKeyLength",
        ]),
    );
    rec.want(
        "key",
        "key.type_string",
        "key/type-names",
        Type::Blob.to_string() == "Blob"
            && Type::XattrSet.to_string() == "XattrSet"
            && !Type::is_valid(7),
        "type names or validity disagree",
        digest_strings(&[
            Type::Blob.to_string(),
            Type::FileNode.to_string(),
            Type::DirLeaf.to_string(),
            Type::DirNode.to_string(),
            Type::XattrSet.to_string(),
        ]),
    );
}

// ---------------------------------------------------------------------------
// cbor
// ---------------------------------------------------------------------------

const CODEC_REPS: usize = 64;

pub fn cbor_cases(env: &Env) -> Vec<Case> {
    let mut out = Vec::new();
    for (name, large) in [("xattrs-3", false), ("xattrs-64", true)] {
        let enc_len = if large {
            env.fx.xattrs_large_enc.len()
        } else {
            env.fx.xattrs_small_enc.len()
        };
        out.push(
            Case::new(
                "cbor",
                "cbor.encode_xattrs",
                name,
                1,
                CODEC_REPS,
                Box::new(move |_| Box::new(large) as State),
                Box::new(move |env, s| {
                    let large = *s.downcast_ref::<bool>().unwrap();
                    let m = if large {
                        &env.fx.xattrs_large
                    } else {
                        &env.fx.xattrs_small
                    };
                    let mut acc = 0u64;
                    for _ in 0..CODEC_REPS {
                        acc += cbor::encode_xattrs(m).len() as u64;
                    }
                    acc
                }),
            )
            .bytes((CODEC_REPS * enc_len) as i64),
        );
        out.push(
            Case::new(
                "cbor",
                "cbor.decode_xattrs",
                name,
                1,
                CODEC_REPS,
                Box::new(move |_| Box::new(large) as State),
                Box::new(move |env, s| {
                    let large = *s.downcast_ref::<bool>().unwrap();
                    let enc = if large {
                        &env.fx.xattrs_large_enc
                    } else {
                        &env.fx.xattrs_small_enc
                    };
                    let mut acc = 0u64;
                    for _ in 0..CODEC_REPS {
                        acc += cbor::decode_xattrs(enc).unwrap().len() as u64;
                    }
                    acc
                }),
            )
            .bytes((CODEC_REPS * enc_len) as i64),
        );
    }
    // Rust-only: the CBOR head and byte-string primitives are exported here
    // and unexported in Go's cborx. They are measured, and the report keeps
    // them in the unpaired table.
    out.push(Case::new(
        "cbor",
        "cbor.head_primitives",
        "append+read/rust-only",
        1,
        4096 * 4,
        Box::new(|_| Box::new(()) as State),
        Box::new(|_, _| {
            let mut acc = 0u64;
            let mut buf = Vec::with_capacity(1 << 16);
            for i in 0..4096u64 {
                buf.clear();
                cbor::append_head(&mut buf, cbor::MAJOR_ARRAY, i);
                cbor::append_bstr(&mut buf, &i.to_be_bytes());
                let (major, n, rest) = cbor::read_head(&buf).unwrap();
                acc += major as u64 + n;
                let (b, _) = cbor::read_bstr(rest).unwrap();
                acc += b.len() as u64;
            }
            acc
        }),
    ));
    out
}

pub fn cbor_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    for (name, m) in [
        ("xattrs-3", &fx.xattrs_small),
        ("xattrs-64", &fx.xattrs_large),
    ] {
        let enc = cbor::encode_xattrs(m);
        let back = cbor::decode_xattrs(&enc);
        rec.want(
            "cbor",
            "cbor.decode_xattrs",
            &format!("cbor/roundtrip-{name}"),
            matches!(&back, Ok(b) if b == m),
            format!("xattr round trip failed: {back:?}"),
            digest(&enc),
        );
    }
    let first = cbor::encode_xattrs(&fx.xattrs_large);
    let stable = (0..32).all(|_| cbor::encode_xattrs(&fx.xattrs_large) == first);
    rec.want(
        "cbor",
        "cbor.encode_xattrs",
        "cbor/canonical-stable",
        stable,
        "encoding is not stable across repeated calls",
        digest(&first),
    );
    let mut trailing = fx.xattrs_small_enc.clone();
    trailing.push(0x00);
    rec.want(
        "cbor",
        "cbor.decode_xattrs",
        "cbor/rejects-trailing",
        cbor::decode_xattrs(&trailing).is_err(),
        "trailing bytes were accepted",
        "rejected",
    );
    // The Rust-only primitives round trip.
    let mut buf = Vec::new();
    cbor::append_head(&mut buf, cbor::MAJOR_ARRAY, 1000);
    cbor::append_bstr(&mut buf, b"amber");
    let (major, n, rest) = cbor::read_head(&buf).unwrap();
    let (b, tail) = cbor::read_bstr(rest).unwrap();
    rec.pass_local(
        "cbor",
        "cbor.head_primitives",
        "cbor/head-primitives-roundtrip",
        format!("{major}/{n}/{}/{}", String::from_utf8_lossy(b), tail.len()),
    );
}

// ---------------------------------------------------------------------------
// binaryfuse (Rust-only)
// ---------------------------------------------------------------------------

fn fuse_keys(env: &Env) -> Vec<u64> {
    env.fx
        .store_keys
        .iter()
        .map(|k| u64::from_be_bytes(k.as_bytes()[..8].try_into().unwrap()))
        .collect()
}

pub fn binaryfuse_cases(env: &Env) -> Vec<Case> {
    let n = env.fx.store_keys.len();
    vec![
        Case::new(
            "binaryfuse",
            "binaryfuse.new",
            "store-keys/rust-only",
            1,
            n,
            Box::new(|env| Box::new(fuse_keys(env)) as State),
            Box::new(|_, s| {
                let keys = s.downcast_ref::<Vec<u64>>().unwrap();
                BinaryFuse16::new(keys).unwrap().section_bytes().len() as u64
            }),
        ),
        Case::new(
            "binaryfuse",
            "binaryfuse.contains",
            "store-keys/rust-only",
            1,
            n,
            Box::new(|env| {
                let keys = fuse_keys(env);
                let f = BinaryFuse16::new(&keys).unwrap();
                Box::new((f, keys)) as State
            }),
            Box::new(|_, s| {
                let (f, keys) = s.downcast_ref::<(BinaryFuse16, Vec<u64>)>().unwrap();
                let mut acc = 0u64;
                for k in keys {
                    if f.contains(*k) {
                        acc += 1;
                    }
                }
                acc
            }),
        ),
        Case::new(
            "binaryfuse",
            "binaryfuse.section_bytes",
            "store-keys/rust-only",
            1,
            16,
            Box::new(|env| {
                let keys = fuse_keys(env);
                Box::new(BinaryFuse16::new(&keys).unwrap()) as State
            }),
            Box::new(|_, s| {
                let f = s.downcast_ref::<BinaryFuse16>().unwrap();
                let mut acc = 0u64;
                for _ in 0..16 {
                    acc += f.section_bytes().len() as u64;
                }
                acc
            }),
        ),
        Case::new(
            "binaryfuse",
            "binaryfuse.parse_section",
            "store-keys/rust-only",
            1,
            16,
            Box::new(|env| {
                let keys = fuse_keys(env);
                Box::new(BinaryFuse16::new(&keys).unwrap().section_bytes()) as State
            }),
            Box::new(|_, s| {
                let b = s.downcast_ref::<Vec<u8>>().unwrap();
                let mut acc = 0u64;
                for _ in 0..16 {
                    acc += BinaryFuse16::parse_section(b).unwrap().contains(1) as u64 + 1;
                }
                acc
            }),
        ),
    ]
}

pub fn binaryfuse_checks(env: &Env, rec: &mut Recorder) {
    let keys = fuse_keys(env);
    let f = BinaryFuse16::new(&keys).unwrap();
    let all = keys.iter().all(|k| f.contains(*k));
    // binaryfuse is a Rust-only module, so its checks have no counterpart to
    // be compared against.
    rec.want_local(
        "binaryfuse",
        "binaryfuse.contains",
        "binaryfuse/no-false-negatives",
        all,
        "the filter reported a stored key as absent",
        format!("n={}", keys.len()),
    );
    let section = f.section_bytes();
    let parsed = BinaryFuse16::parse_section(&section);
    rec.want_local(
        "binaryfuse",
        "binaryfuse.parse_section",
        "binaryfuse/section-roundtrip",
        matches!(&parsed, Ok(p) if keys.iter().all(|k| p.contains(*k))),
        format!("section round trip failed: {:?}", parsed.err()),
        digest(&section),
    );
}

// ---------------------------------------------------------------------------
// chunkers
// ---------------------------------------------------------------------------

pub fn chunker_cases(env: &Env) -> Vec<Case> {
    let mut out = Vec::new();
    // 0 = random corpus, 1 = compressible corpus, 2 = the first 64 KiB of it.
    for (which, name, len) in [
        (0usize, "random/default-sizes", env.fx.corpus_random.len()),
        (1, "compressible/default-sizes", env.fx.corpus_text.len()),
        (2, "tiny-64KiB/default-sizes", 64 << 10),
    ] {
        out.push(
            Case::new(
                "chunkers",
                "chunkers.split_bytes",
                name,
                1,
                1,
                Box::new(move |_| Box::new(which) as State),
                Box::new(move |env, s| {
                    let which = *s.downcast_ref::<usize>().unwrap();
                    let data: &[u8] = match which {
                        0 => &env.fx.corpus_random,
                        1 => &env.fx.corpus_text,
                        _ => &env.fx.corpus_text[..64 << 10],
                    };
                    let mut acc = 0u64;
                    chunkers::split_bytes(data, None, |c| {
                        acc += c.len() as u64;
                        Ok::<(), std::io::Error>(())
                    })
                    .unwrap();
                    acc
                }),
            )
            .bytes(len as i64),
        );
    }
    out.push(
        Case::new(
            "chunkers",
            "chunkers.split_bytes",
            "compressible/4-16-64KiB",
            1,
            1,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let opts = ByteOpts {
                    min_size: 4 << 10,
                    normal_size: 16 << 10,
                    max_size: 64 << 10,
                    key: Vec::new(),
                };
                let mut acc = 0u64;
                chunkers::split_bytes(&env.fx.corpus_text[..], Some(&opts), |c| {
                    acc += c.len() as u64;
                    Ok::<(), std::io::Error>(())
                })
                .unwrap();
                acc
            }),
        )
        .bytes(env.fx.corpus_text.len() as i64),
    );
    out.push(Case::new(
        "chunkers",
        "chunkers.item_chunker",
        "is_boundary/bits-7",
        1,
        env.fx.item_encodings.len(),
        Box::new(|_| Box::new(ItemChunker::new(7)) as State),
        Box::new(|env, s| {
            let ic = s.downcast_ref::<ItemChunker>().unwrap();
            let mut acc = 0u64;
            let mut run = 0usize;
            for enc in &env.fx.item_encodings {
                run += 1;
                if ic.is_boundary(enc, run) {
                    acc += 1;
                    run = 0;
                }
            }
            acc
        }),
    ));
    out.push(Case::new(
        "chunkers",
        "chunkers.new_item_chunker",
        "bits-4..12",
        1,
        9 * 256,
        Box::new(|_| Box::new(()) as State),
        Box::new(|_, _| {
            let mut acc = 0u64;
            for _ in 0..256 {
                for bits in 4..=12u32 {
                    let ic = ItemChunker::new(bits);
                    acc += (ic.min_run + ic.max_run) as u64;
                }
            }
            acc
        }),
    ));
    out
}

pub fn chunker_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    for (name, data) in [
        ("random", &fx.corpus_random),
        ("compressible", &fx.corpus_text),
    ] {
        let mut chunks: Vec<Vec<u8>> = Vec::new();
        let mut total = 0usize;
        let res = chunkers::split_bytes(&data[..], None, |c| {
            total += c.len();
            chunks.push(c);
            Ok::<(), std::io::Error>(())
        });
        let ok = res.is_ok() && total == data.len();
        rec.want(
            "chunkers",
            "chunkers.split_bytes",
            &format!("chunkers/split-{name}"),
            ok,
            format!(
                "split failed: {res:?} (covered {total} of {} bytes)",
                data.len()
            ),
            digest_vecs(&chunks),
        );
        if ok {
            let last = chunks.len() - 1;
            let sizes = chunks.iter().enumerate().all(|(i, c)| {
                c.len() <= chunkers::DEFAULT_MAX_SIZE
                    && (i == last || c.len() >= chunkers::DEFAULT_MIN_SIZE)
            });
            rec.want(
                "chunkers",
                "chunkers.split_bytes",
                &format!("chunkers/split-bounds-{name}"),
                sizes,
                "a chunk fell outside the configured min/max bounds",
                format!("n={}", chunks.len()),
            );
        }
    }
    let mut n = 0;
    let empty: &[u8] = &[];
    let res = chunkers::split_bytes(empty, None, |_| {
        n += 1;
        Ok::<(), std::io::Error>(())
    });
    rec.want(
        "chunkers",
        "chunkers.split_bytes",
        "chunkers/split-empty",
        res.is_ok() && n == 0,
        format!("empty reader produced {n} chunks ({res:?})"),
        "0",
    );
    let res = chunkers::split_bytes(&fx.corpus_text[..], None, |_| {
        Err::<(), std::io::Error>(std::io::Error::other("consumer stop"))
    });
    rec.want(
        "chunkers",
        "chunkers.split_bytes",
        "chunkers/split-propagates-error",
        matches!(res, Err(chunkers::SplitError::Callback(_))),
        format!("consumer error was not propagated: {res:?}"),
        "propagated",
    );

    let ic = ItemChunker::new(7);
    rec.want(
        "chunkers",
        "chunkers.new_item_chunker",
        "chunkers/item-bounds",
        ic.min_run == 32 && ic.max_run == 512,
        format!("item chunker bounds are {}/{}", ic.min_run, ic.max_run),
        format!("{}/{}", ic.min_run, ic.max_run),
    );
    let mut bounds: Vec<String> = Vec::new();
    let mut run = 0usize;
    for (i, enc) in fx.item_encodings.iter().enumerate() {
        run += 1;
        if ic.is_boundary(enc, run) {
            bounds.push(i.to_string());
            run = 0;
        }
    }
    rec.pass(
        "chunkers",
        "chunkers.item_chunker",
        "chunkers/item-boundaries",
        digest_strings(&bounds),
    );
}

// ---------------------------------------------------------------------------
// reference
// ---------------------------------------------------------------------------

const REF_REPS: usize = 256;

fn ref_names() -> Vec<String> {
    (0..256)
        .map(|i| format!("bench/name/{i}/{}", "x".repeat(i % 64)))
        .collect()
}

pub fn reference_cases(env: &Env) -> Vec<Case> {
    let enc_len = env.fx.ref_record_enc.len();
    vec![
        Case::new(
            "reference",
            "reference.encode",
            "signed",
            1,
            REF_REPS,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut acc = 0u64;
                for _ in 0..REF_REPS {
                    acc += env.fx.ref_record.encode().unwrap().len() as u64;
                }
                acc
            }),
        )
        .bytes((REF_REPS * enc_len) as i64),
        Case::new(
            "reference",
            "reference.decode",
            "signed",
            1,
            REF_REPS,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut acc = 0u64;
                for _ in 0..REF_REPS {
                    acc += reference::Reference::decode(&env.fx.ref_record_enc)
                        .unwrap()
                        .name
                        .len() as u64;
                }
                acc
            }),
        )
        .bytes((REF_REPS * enc_len) as i64),
        Case::new(
            "reference",
            "reference.signature_payload",
            "signed",
            1,
            REF_REPS,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut acc = 0u64;
                for _ in 0..REF_REPS {
                    acc += env.fx.ref_record.signature_payload().unwrap().len() as u64;
                }
                acc
            }),
        ),
        Case::new(
            "reference",
            "reference.validate_name",
            "valid",
            1,
            256,
            Box::new(|_| Box::new(ref_names()) as State),
            Box::new(|_, s| {
                let names = s.downcast_ref::<Vec<String>>().unwrap();
                let mut acc = 0u64;
                for n in names {
                    if reference::validate_name(n).is_ok() {
                        acc += 1;
                    }
                }
                acc
            }),
        ),
        Case::new(
            "reference",
            "reference.validate_user",
            "valid",
            1,
            256,
            Box::new(|_| Box::new(ref_names()) as State),
            Box::new(|_, s| {
                let names = s.downcast_ref::<Vec<String>>().unwrap();
                let mut acc = 0u64;
                for n in names {
                    if reference::validate_user(&format!("{n}@example.org")).is_ok() {
                        acc += 1;
                    }
                }
                acc
            }),
        ),
    ]
}

pub fn reference_checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;
    let enc = fx.ref_record.encode();
    rec.want(
        "reference",
        "reference.encode",
        "reference/encode",
        enc.is_ok(),
        format!("encode failed: {enc:?}"),
        digest(enc.as_deref().unwrap_or(&[])),
    );
    let enc = enc.unwrap();
    let back = reference::Reference::decode(&enc);
    rec.want(
        "reference",
        "reference.decode",
        "reference/roundtrip",
        matches!(&back, Ok(b) if *b == fx.ref_record),
        format!("reference round trip failed: {back:?}"),
        digest(&enc),
    );
    let sp = fx.ref_record.signature_payload();
    rec.want(
        "reference",
        "reference.signature_payload",
        "reference/signature-payload",
        sp.is_ok(),
        format!("signature payload failed: {sp:?}"),
        digest(sp.as_deref().unwrap_or(&[])),
    );
    let sp = sp.unwrap();
    let contains = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
    rec.want(
        "reference",
        "reference.signature_payload",
        "reference/signature-payload-excludes-sig",
        !contains(&sp, &fx.ref_record.signature) && contains(&sp, &fx.ref_record.public_key),
        "signature payload does not bind exactly the expected fields",
        "ok",
    );
    let mut trailing = enc.clone();
    trailing.push(0x00);
    rec.want(
        "reference",
        "reference.decode",
        "reference/rejects-trailing",
        reference::Reference::decode(&trailing).is_err(),
        "trailing bytes were accepted",
        "rejected",
    );
    let bad = [
        String::new(),
        "with@at".to_string(),
        "with\u{1}control".to_string(),
        "n".repeat(reference::MAX_NAME_LEN + 1),
    ];
    rec.want(
        "reference",
        "reference.validate_name",
        "reference/name-rules",
        bad.iter().all(|n| reference::validate_name(n).is_err()),
        "an invalid reference name was accepted",
        "rejected",
    );
    rec.want(
        "reference",
        "reference.validate_user",
        "reference/user-rules",
        reference::validate_user("a@b.example").is_ok() && reference::validate_user("").is_err(),
        "user validation disagrees with the documented rules",
        "ok",
    );
}
