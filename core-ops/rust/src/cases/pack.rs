//! amberpack: the record codec and the wire-pack reader/writer.

use amber_store_core::amberpack::{self, RawRecord, Record};
use amber_store_core::fstree::Object;
use amber_store_core::key::{Key, Type};

use crate::cases::{PAYLOAD_SETS, payload_set};
use crate::env::Env;
use crate::fixtures::{digest, digest_strings, digest_vecs};
use crate::harness::{Case, Recorder, State};

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

    for i in PAYLOAD_SETS {
        let ps = payload_set(fx, i);
        out.push(
            Case::new(
                "amberpack",
                "amberpack.encode_record",
                &ps.name,
                1,
                ps.items.len(),
                Box::new(move |env| {
                    let ps = payload_set(&env.fx, i);
                    let keys: Vec<Key> = ps
                        .items
                        .iter()
                        .map(|b| Key::new(Type::Blob, b.len() as u64, b))
                        .collect();
                    Box::new((i, keys)) as State
                }),
                Box::new(|env, s| {
                    let (i, keys) = s.downcast_ref::<(usize, Vec<Key>)>().unwrap();
                    let ps = payload_set(&env.fx, *i);
                    let mut acc = 0u64;
                    for (n, b) in ps.items.iter().enumerate() {
                        acc += amberpack::encode_record(keys[n], b).unwrap().len() as u64;
                    }
                    acc
                }),
            )
            .bytes(ps.bytes),
        );
    }

    out.push(Case::new(
        "amberpack",
        "amberpack.parse_record",
        "mixed-records",
        1,
        fx.wire_records.len(),
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for rec in &env.fx.wire_records {
                acc += amberpack::parse_record(rec).unwrap().slen as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "amberpack",
        "amberpack.parse_record",
        "corrupt-crc",
        1,
        256,
        Box::new(|env| {
            let mut bad = env.fx.wire_records[0].clone();
            bad[amberpack::REC_HEADER_SIZE] ^= 0xFF;
            Box::new(bad) as State
        }),
        Box::new(|_, s| {
            let bad = s.downcast_ref::<Vec<u8>>().unwrap();
            let mut acc = 0u64;
            for _ in 0..256 {
                if amberpack::parse_record(bad).is_err() {
                    acc += 1;
                }
            }
            acc
        }),
    ));
    out.push(Case::new(
        "amberpack",
        "amberpack.decode_payload",
        "mixed-records",
        1,
        fx.wire_records.len(),
        Box::new(|env| {
            let v: Vec<ParsedRecord> = env
                .fx
                .wire_records
                .iter()
                .map(|rec| ParsedRecord {
                    h: amberpack::parse_record(rec).unwrap(),
                    rec: rec.clone(),
                })
                .collect();
            Box::new(v) as State
        }),
        Box::new(|_, s| {
            let v = s.downcast_ref::<Vec<ParsedRecord>>().unwrap();
            let mut acc = 0u64;
            for p in v {
                let stored = &p.rec
                    [amberpack::REC_HEADER_SIZE..amberpack::REC_HEADER_SIZE + p.h.slen as usize];
                acc += amberpack::decode_payload(p.h.flags, p.h.ulen, stored)
                    .unwrap()
                    .len() as u64;
            }
            acc
        }),
    ));
    out.push(
        Case::new(
            "amberpack",
            "amberpack.writer_add",
            "mixed-objects",
            1,
            fx.pack_objects.len(),
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut sink = CountingSink::default();
                {
                    let mut w = amberpack::Writer::new(&mut sink);
                    for o in &env.fx.pack_objects {
                        w.add(o.key, &o.bytes).unwrap();
                    }
                    w.finish().unwrap();
                }
                sink.n as u64
            }),
        )
        .bytes(pack_bytes(&fx.pack_objects)),
    );
    out.push(
        Case::new(
            "amberpack",
            "amberpack.writer_add_record",
            "pre-encoded-records",
            1,
            fx.wire_records.len(),
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut sink = CountingSink::default();
                {
                    let mut w = amberpack::Writer::new(&mut sink);
                    for rec in &env.fx.wire_records {
                        w.add_record(rec).unwrap();
                    }
                    w.finish().unwrap();
                }
                sink.n as u64
            }),
        )
        .bytes(pack_bytes(&fx.pack_objects)),
    );
    out.push(
        Case::new(
            "amberpack",
            "amberpack.reader_all",
            "mixed-objects",
            1,
            fx.pack_objects.len(),
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut acc = 0u64;
                for item in amberpack::Reader::new(&env.fx.wire_pack[..]) {
                    let (_, bytes) = item.unwrap();
                    acc += bytes.len() as u64;
                }
                acc
            }),
        )
        .bytes(pack_bytes(&fx.pack_objects)),
    );
    out.push(Case::new(
        "amberpack",
        "amberpack.reader_records",
        "mixed-objects",
        1,
        fx.pack_objects.len(),
        Box::new(|_| Box::new(()) as State),
        Box::new(|env, _| {
            let mut acc = 0u64;
            for item in amberpack::Reader::new(&env.fx.wire_pack[..]).records() {
                let r: RawRecord = item.unwrap();
                acc += r.bytes.len() as u64;
            }
            acc
        }),
    ));
    out.push(Case::new(
        "amberpack",
        "amberpack.reader_all",
        "truncated-stream",
        1,
        64,
        Box::new(|env| Box::new(env.fx.wire_pack[..env.fx.wire_pack.len() / 2].to_vec()) as State),
        Box::new(|_, s| {
            let truncated = s.downcast_ref::<Vec<u8>>().unwrap();
            let mut acc = 0u64;
            for _ in 0..64 {
                for item in amberpack::Reader::new(&truncated[..]) {
                    if item.is_err() {
                        acc += 1;
                    }
                }
            }
            acc
        }),
    ));
    out
}

#[derive(Default)]
struct CountingSink {
    n: i64,
}

impl std::io::Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.n += buf.len() as i64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;

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

    let mut read_back: Vec<Object> = Vec::new();
    let mut reader_ok = true;
    for item in amberpack::Reader::new(&fx.wire_pack[..]) {
        match item {
            Ok((key, bytes)) => read_back.push(Object { key, bytes }),
            Err(e) => {
                rec.fail(
                    "amberpack",
                    "amberpack.reader_all",
                    "amberpack/reader-roundtrip",
                    e.to_string(),
                );
                reader_ok = false;
                break;
            }
        }
    }
    if reader_ok {
        let same = read_back.len() == fx.pack_objects.len()
            && read_back
                .iter()
                .zip(&fx.pack_objects)
                .all(|(a, b)| a.key == b.key && a.bytes == b.bytes);
        rec.want(
            "amberpack",
            "amberpack.reader_all",
            "amberpack/reader-roundtrip",
            same,
            "the reader did not return the objects the writer wrote",
            pack_content_digest(&read_back),
        );
    }

    let raws: Vec<Vec<u8>> = amberpack::Reader::new(&fx.wire_pack[..])
        .records()
        .map(|r| r.unwrap().bytes)
        .collect();
    let mut rebuilt = Vec::new();
    {
        let mut w = amberpack::Writer::new(&mut rebuilt);
        for r in &raws {
            w.add_record(r).unwrap();
        }
        w.finish().unwrap();
    }
    rec.want_local(
        "amberpack",
        "amberpack.writer_add_record",
        "amberpack/record-passthrough",
        rebuilt == fx.wire_pack,
        "re-adding read records did not reproduce the pack byte for byte",
        digest(&rebuilt),
    );

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
