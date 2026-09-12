//! amberpack: the record codec and the wire-pack reader/writer.

use amber_store_core::amberpack::{self, RawRecord, Record};
use amber_store_core::fstree::Object;
use amber_store_core::key::{Key, Type};

use crate::cases::payload_set;
use crate::env::Env;
use crate::fixtures::{
    PayloadSet, digest, digest_strings, digest_vecs, fold_bool, fold_i64, fold_key, fold_u64,
    new_fold, sink_bytes,
};
use crate::harness::{Case, Dims, Recorder, State, WireInput, bytes_kind};

use super::fstree_cases::CountingWriter;

/// Fingerprints what a pack carries rather than how it was encoded: the key
/// and the payload of every object, in stream order. Both cores must agree on
/// this even though their compressed bytes differ.
pub fn pack_content_digest(objs: &[Object]) -> String {
    let mut items: Vec<Vec<u8>> = Vec::with_capacity(objs.len() * 2);
    for o in objs {
        items.push(o.key.as_bytes().to_vec());
        items.push(o.bytes.clone());
    }
    digest_vecs(&items)
}

pub fn pack_bytes(objs: &[Object]) -> i64 {
    objs.iter().map(|o| o.bytes.len() as i64).sum()
}

/// Pairs a record's parsed header with its bytes, so the decode case measures
/// only the payload decode.
struct ParsedRecord {
    h: Record,
    rec: Vec<u8>,
}

pub fn cases(env: &Env) -> Vec<Case> {
    let fx = &env.fx;
    let mut out = Vec::new();

    // Encoding is swept over the whole (size, content) grid, because this is
    // where the compressor's behaviour is the measurement: random bytes are
    // stored, text shrinks, and duplicates shrink the same way as text but
    // with a warm dictionary.
    //
    // The denominator is the *input* payload, which is byte-identical in both
    // cores. The encoded size is not, and is reported separately rather than
    // divided by anything.
    for i in 0..fx.payloads.len() {
        let lim = payload_set(fx, i).limit(4 << 20);
        let n_items = lim.items.len();
        out.push(
            Case::new(
                "amberpack",
                "amberpack.encode_record",
                &lim.name,
                1,
                n_items,
                Box::new(move |env| {
                    let lim = payload_set(&env.fx, i).limit(4 << 20);
                    let keys: Vec<Key> = lim
                        .items
                        .iter()
                        .map(|b| Key::new(Type::Blob, b.len() as u64, b))
                        .collect();
                    Box::new((lim, keys)) as State
                }),
                Box::new(|_, s| {
                    let (lim, keys) = s.downcast_ref::<(PayloadSet, Vec<Key>)>().unwrap();
                    let mut acc = new_fold();
                    for (n, b) in lim.items.iter().enumerate() {
                        acc = sink_bytes(acc, &amberpack::encode_record(keys[n], b).unwrap());
                    }
                    acc
                }),
            )
            .bytes(lim.bytes)
            .dims(lim.dims()),
        );
    }

    // Every reader and decoder case runs once per producing core, over the
    // pack that core wrote. Both drivers read the same two files, so
    // `producer-go` means the same bytes in both documents and the comparison
    // isolates the decoder instead of comparing two encoders.
    for (wi, w) in fx.wire.iter().enumerate() {
        let suffix = format!("/producer-{}", w.producer);
        let rec_dims = Dims {
            items: w.records.len() as i64,
            objects: w.objects as i64,
            content: "structured".into(),
            ..Default::default()
        };
        out.push(
            Case::new(
                "amberpack",
                "amberpack.parse_record",
                &format!("mixed-records{suffix}"),
                1,
                w.records.len(),
                Box::new(move |_| Box::new(wi) as State),
                Box::new(|env, s| {
                    let w = &env.fx.wire[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for rec in &w.records {
                        let r = amberpack::parse_record(rec).unwrap();
                        // The whole parsed header: key, both lengths and the
                        // flags. Fixed size, so this is constant work.
                        acc = fold_key(acc, &r.key);
                        acc = fold_u64(acc, r.slen as u64);
                        acc = fold_u64(acc, r.ulen as u64);
                        acc = fold_u64(acc, r.flags as u64);
                    }
                    acc
                }),
            )
            .bytes_of(w.bytes, bytes_kind::ENCODED)
            .dims(rec_dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "amberpack",
                "amberpack.decode_payload",
                &format!("mixed-records{suffix}"),
                1,
                w.records.len(),
                Box::new(move |env| {
                    let w = &env.fx.wire[wi];
                    let parsed: Vec<ParsedRecord> = w
                        .records
                        .iter()
                        .map(|rec| ParsedRecord {
                            h: amberpack::parse_record(rec).unwrap(),
                            rec: rec.clone(),
                        })
                        .collect();
                    Box::new(parsed) as State
                }),
                Box::new(|_, s| {
                    let parsed = s.downcast_ref::<Vec<ParsedRecord>>().unwrap();
                    let mut acc = new_fold();
                    for p in parsed {
                        let body = amberpack::decode_payload(
                            p.h.flags,
                            p.h.ulen,
                            &p.rec[amberpack::REC_HEADER_SIZE
                                ..amberpack::REC_HEADER_SIZE + p.h.slen as usize],
                        )
                        .unwrap();
                        acc = sink_bytes(acc, &body);
                    }
                    acc
                }),
            )
            .bytes_of(w.bytes, bytes_kind::ENCODED)
            .dims(rec_dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "amberpack",
                "amberpack.reader_all",
                &format!("mixed-objects{suffix}"),
                1,
                w.objects,
                Box::new(move |_| Box::new(wi) as State),
                Box::new(|env, s| {
                    let w = &env.fx.wire[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for item in amberpack::Reader::new(&w.data[..]) {
                        let (k, bytes) = item.unwrap();
                        acc = fold_key(acc, &k);
                        acc = sink_bytes(acc, &bytes);
                    }
                    acc
                }),
            )
            .bytes_of(w.bytes, bytes_kind::ENCODED)
            .dims(rec_dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "amberpack",
                "amberpack.reader_records",
                &format!("mixed-objects{suffix}"),
                1,
                w.objects,
                Box::new(move |_| Box::new(wi) as State),
                Box::new(|env, s| {
                    let w = &env.fx.wire[*s.downcast_ref::<usize>().unwrap()];
                    let mut acc = new_fold();
                    for item in amberpack::Reader::new(&w.data[..]).records() {
                        let rec: RawRecord = item.unwrap();
                        acc = fold_key(acc, &rec.record.key);
                        acc = sink_bytes(acc, &rec.bytes);
                    }
                    acc
                }),
            )
            .bytes_of(w.bytes, bytes_kind::ENCODED)
            .dims(rec_dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "amberpack",
                "amberpack.writer_add_record",
                &format!("pre-encoded-records{suffix}"),
                1,
                w.records.len(),
                Box::new(move |_| Box::new(wi) as State),
                Box::new(|env, s| {
                    let w = &env.fx.wire[*s.downcast_ref::<usize>().unwrap()];
                    let mut sink = CountingWriter::default();
                    {
                        let mut wr = amberpack::Writer::new(&mut sink);
                        for rec in &w.records {
                            wr.add_record(rec).unwrap();
                        }
                        wr.finish().unwrap();
                    }
                    fold_i64(new_fold(), sink.n)
                }),
            )
            .bytes_of(w.bytes, bytes_kind::ENCODED)
            .dims(rec_dims.clone())
            .cross(),
        );
        out.push(
            Case::new(
                "amberpack",
                "amberpack.reader_all",
                &format!("truncated-stream{suffix}"),
                1,
                64,
                Box::new(move |_| Box::new(wi) as State),
                Box::new(|env, s| {
                    let w = &env.fx.wire[*s.downcast_ref::<usize>().unwrap()];
                    let half = &w.data[..w.data.len() / 2];
                    let mut acc = new_fold();
                    for _ in 0..64 {
                        for item in amberpack::Reader::new(half) {
                            acc = fold_bool(acc, item.is_err());
                            if let Ok((k, _)) = item {
                                acc = fold_key(acc, &k);
                            }
                        }
                    }
                    acc
                }),
            )
            .dims(Dims {
                items: 64,
                item_bytes: w.bytes / 2,
                content: "structured".into(),
                ..Default::default()
            })
            .cross(),
        );
        out.push(
            Case::new(
                "amberpack",
                "amberpack.parse_record",
                &format!("corrupt-crc{suffix}"),
                1,
                256,
                Box::new(move |env| {
                    let mut bad = env.fx.wire[wi].records[0].clone();
                    bad[amberpack::REC_HEADER_SIZE] ^= 0xFF;
                    Box::new(bad) as State
                }),
                Box::new(|_, s| {
                    let bad = s.downcast_ref::<Vec<u8>>().unwrap();
                    let mut acc = new_fold();
                    for _ in 0..256 {
                        acc = fold_bool(acc, amberpack::parse_record(bad).is_err());
                    }
                    acc
                }),
            )
            .dims(Dims {
                items: 256,
                content: "structured".into(),
                ..Default::default()
            })
            .cross(),
        );
    }

    // The encoding side, this core's own writer.
    out.push(
        Case::new(
            "amberpack",
            "amberpack.writer_add",
            "mixed-objects",
            1,
            fx.pack_objects.len(),
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut sink = CountingWriter::default();
                {
                    let mut w = amberpack::Writer::new(&mut sink);
                    for o in &env.fx.pack_objects {
                        w.add(o.key, &o.bytes).unwrap();
                    }
                    w.finish().unwrap();
                }
                fold_i64(new_fold(), sink.n)
            }),
        )
        .bytes(pack_bytes(&fx.pack_objects))
        .dims(Dims {
            items: fx.pack_objects.len() as i64,
            objects: fx.pack_objects.len() as i64,
            content: "structured".into(),
            ..Default::default()
        }),
    );
    out
}

pub fn checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;

    // Record what this core's encoder made of every point of the payload
    // grid. The two cores' numbers are printed side by side and never
    // divided by one another.
    for i in 0..fx.payloads.len() {
        let lim = fx.payloads[i].limit(4 << 20);
        let mut enc = 0i64;
        for b in &lim.items {
            let k = Key::new(Type::Blob, b.len() as u64, b);
            enc += amberpack::encode_record(k, b).unwrap().len() as i64;
        }
        rec.encoded(
            "amberpack.encode_record",
            &lim.name,
            lim.bytes,
            enc,
            lim.items.len() as i64,
        );
    }
    rec.encoded(
        "amberpack.writer_add",
        "mixed-objects",
        pack_bytes(&fx.pack_objects),
        fx.wire_pack.len() as i64,
        fx.pack_objects.len() as i64,
    );

    // Every wire pack this driver was handed, with its producer and hash.
    // Both drivers read the same files, so these rows are what proves a
    // decode comparison used identical bytes.
    for w in &fx.wire {
        rec.wire_inputs.push(WireInput {
            producer: w.producer.clone(),
            sha256: w.sha256.clone(),
            bytes: w.bytes,
            objects: w.objects as i64,
        });
        // The hash of each shared input is a comparable statement: if the
        // two drivers somehow read different files, the run fails here
        // rather than publishing a decode comparison of unlike inputs.
        rec.pass(
            "amberpack",
            "amberpack.reader_all",
            &format!("amberpack/wire-input-{}", w.producer),
            w.sha256.clone(),
        );
    }

    // Interoperability, which is the thing the decode comparison rests on:
    // this core reads *every* producer's pack and must recover exactly the
    // same objects from each. The digest is over the (key, payload) list, so
    // it is comparable across cores even though the packs are not.
    for w in &fx.wire {
        let mut got: Vec<Object> = Vec::new();
        let mut rerr: Option<String> = None;
        for item in amberpack::Reader::new(&w.data[..]) {
            match item {
                Ok((key, bytes)) => got.push(Object { key, bytes }),
                Err(e) => {
                    rerr = Some(e.to_string());
                    break;
                }
            }
        }
        let same = rerr.is_none()
            && got.len() == fx.pack_objects.len()
            && got
                .iter()
                .zip(&fx.pack_objects)
                .all(|(a, b)| a.key == b.key && a.bytes == b.bytes);
        rec.want(
            "amberpack",
            "amberpack.reader_all",
            &format!("amberpack/reads-{}-pack", w.producer),
            same,
            format!(
                "reading the {} core's pack did not return the shared object population: {rerr:?}",
                w.producer
            ),
            pack_content_digest(&got),
        );
    }

    let mut headers: Vec<String> = Vec::new();
    let mut ok = true;
    for (i, o) in fx.pack_objects.iter().enumerate() {
        let r = amberpack::encode_record(o.key, &o.bytes);
        let Ok(record) = r else {
            ok = false;
            break;
        };
        let Ok(h) = amberpack::parse_record(&record) else {
            ok = false;
            break;
        };
        if h.key != o.key || h.ulen != o.bytes.len() as u32 {
            ok = false;
            break;
        }
        let stored =
            &record[amberpack::REC_HEADER_SIZE..amberpack::REC_HEADER_SIZE + h.slen as usize];
        match amberpack::decode_payload(h.flags, h.ulen, stored) {
            Ok(back) if back == o.bytes => {}
            _ => {
                ok = false;
                break;
            }
        }
        if i < 32 {
            headers.push(format!("{}/{}", h.key, h.ulen));
        }
    }
    rec.want(
        "amberpack",
        "amberpack.encode_record",
        "amberpack/record-roundtrip",
        ok,
        "a record did not round trip through encode/parse/decode",
        digest_strings(&headers),
    );
    rec.pass_local(
        "amberpack",
        "amberpack.encode_record",
        "amberpack/record-bytes",
        digest_vecs(&fx.wire_records),
    );

    let rb = &fx.rand.items[0];
    let tb = &fx.large.items[0];
    let rand_rec = amberpack::encode_record(Key::new(Type::Blob, rb.len() as u64, rb), rb).unwrap();
    let text_rec = amberpack::encode_record(Key::new(Type::Blob, tb.len() as u64, tb), tb).unwrap();
    let rh = amberpack::parse_record(&rand_rec).unwrap();
    let th = amberpack::parse_record(&text_rec).unwrap();
    rec.want(
        "amberpack",
        "amberpack.encode_record",
        "amberpack/compresses-only-when-smaller",
        rh.slen == rh.ulen && th.slen < th.ulen,
        format!(
            "random {}/{}, text {}/{}",
            rh.slen, rh.ulen, th.slen, th.ulen
        ),
        format!(
            "random-stored={} text-compressed={}",
            rh.slen == rh.ulen,
            th.slen < th.ulen
        ),
    );

    let mut bad = fx.wire_records[0].clone();
    bad[amberpack::REC_HEADER_SIZE] ^= 0xFF;
    let err = amberpack::parse_record(&bad);
    rec.want(
        "amberpack",
        "amberpack.parse_record",
        "amberpack/detects-crc-corruption",
        matches!(&err, Err(e) if e.is_corrupt()),
        format!("a flipped payload byte must be reported as corrupt, got {err:?}"),
        "ErrCorrupt",
    );
    let mut bad_key = fx.wire_records[0].clone();
    bad_key[1] |= 0x08;
    let err = amberpack::parse_record(&bad_key);
    rec.want(
        "amberpack",
        "amberpack.parse_record",
        "amberpack/rejects-non-canonical-key",
        matches!(&err, Err(e) if e.is_corrupt()),
        format!("a non-canonical key must be reported as corrupt, got {err:?}"),
        "ErrCorrupt",
    );

    // Records() hands over the same records undecoded, and re-adding them
    // verbatim reproduces the stream -- for every producer's pack, which is
    // what the writer_add_record workloads measure.
    for w in &fx.wire {
        let raws: Vec<Vec<u8>> = amberpack::Reader::new(&w.data[..])
            .records()
            .map(|r| r.unwrap().bytes)
            .collect();
        let mut rebuilt = Vec::new();
        {
            let mut wr = amberpack::Writer::new(&mut rebuilt);
            for r in &raws {
                wr.add_record(r).unwrap();
            }
            wr.finish().unwrap();
        }
        rec.want(
            "amberpack",
            "amberpack.writer_add_record",
            &format!("amberpack/record-passthrough-{}", w.producer),
            rebuilt == w.data,
            "re-adding read records did not reproduce the pack byte for byte",
            // The pack is the same file in both drivers, so this digest is a
            // comparable statement about a byte-identical artefact.
            w.sha256.clone(),
        );
    }

    let truncated = &fx.wire_pack[..fx.wire_pack.len() - 1];
    let saw_err = amberpack::Reader::new(truncated).any(|i| i.is_err());
    rec.want(
        "amberpack",
        "amberpack.reader_all",
        "amberpack/rejects-truncated",
        saw_err,
        "a truncated pack was read as a clean stream",
        "rejected",
    );
    let mut legacy = fx.wire_pack.clone();
    legacy[7] = 0x01;
    let saw_malformed =
        amberpack::Reader::new(&legacy[..]).any(|i| matches!(&i, Err(e) if e.is_malformed()));
    rec.want(
        "amberpack",
        "amberpack.reader_all",
        "amberpack/rejects-legacy-magic",
        saw_malformed,
        "a version-1 pack magic was accepted",
        "ErrMalformed",
    );
    let mut empty = Vec::new();
    {
        let w = amberpack::Writer::new(&mut empty);
        w.finish().unwrap();
    }
    let n = amberpack::Reader::new(&empty[..]).count();
    rec.want(
        "amberpack",
        "amberpack.writer_add",
        "amberpack/empty-pack",
        n == 0 && empty.len() == 9,
        format!(
            "an empty pack read back {n} objects from {} bytes",
            empty.len()
        ),
        format!("{}", empty.len()),
    );
}
