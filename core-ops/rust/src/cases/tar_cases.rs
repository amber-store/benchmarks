//! tarexport and tarextract.

use std::fs;
use std::path::PathBuf;

use amber_store_core::{tarexport, tarextract};

use crate::env::Env;
use crate::fixtures::{digest, manifest};
use crate::harness::{Case, Recorder, State};
use crate::stores::work_dir;

#[derive(Default)]
struct CountingWriter {
    n: i64,
}

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.n += buf.len() as i64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn cases(env: &Env) -> Vec<Case> {
    let tar_len = env.fx.tar_bytes.len() as i64;
    vec![
        Case::new(
            "tar",
            "tarexport.write",
            "fixture-tree",
            1,
            1,
            Box::new(|_| Box::new(()) as State),
            Box::new(|env, _| {
                let mut c = CountingWriter::default();
                tarexport::write(&mut c, env.fx.tree_root, |k| env.fx.tree_mem.get(k)).unwrap();
                c.n as u64
            }),
        )
        .bytes(tar_len),
        Case::new(
            "tar",
            "tarextract.extract",
            "fixture-tree",
            1,
            1,
            Box::new(|env| Box::new(work_dir(env, "tar-extract")) as State),
            Box::new(|env, s| {
                let dir = s.downcast_ref::<PathBuf>().unwrap();
                tarextract::extract(&mut &env.fx.tar_bytes[..], dir).unwrap();
                env.fx.tar_bytes.len() as u64
            }),
        )
        .bytes(tar_len)
        .per_rep(Box::new(|_, s| {
            let dir = s.downcast::<PathBuf>().unwrap();
            let _ = fs::remove_dir_all(&*dir);
        })),
    ]
}

pub fn checks(env: &Env, rec: &mut Recorder) {
    let fx = &env.fx;

    // The PAX export is specified to be byte-identical across the two cores,
    // so its digest is a comparable statement.
    let mut buf: Vec<u8> = Vec::new();
    let res = tarexport::write(&mut buf, fx.tree_root, |k| fx.tree_mem.get(k));
    rec.want(
        "tar",
        "tarexport.write",
        "tar/export-bytes",
        res.is_ok() && buf == fx.tar_bytes,
        format!("export is not deterministic: {res:?}"),
        digest(&buf),
    );

    let mut sink = CountingWriter::default();
    let res = tarexport::write(&mut sink, fx.pack_objects[0].key, |k| fx.tree_mem.get(k));
    rec.want(
        "tar",
        "tarexport.write",
        "tar/export-rejects-non-directory",
        res.is_err(),
        "a Blob root was accepted as a directory",
        "rejected",
    );

    let dir = work_dir(env, "check-extract");
    if let Err(e) = tarextract::extract(&mut &fx.tar_bytes[..], &dir) {
        rec.fail("tar", "tarextract.extract", "tar/extract", e.to_string());
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let m = manifest(&dir);
    let m_digest = m.as_ref().cloned().unwrap_or_default();
    rec.want(
        "tar",
        "tarextract.extract",
        "tar/extract-manifest",
        m.is_ok(),
        format!("manifest of the extracted tree: {m:?}"),
        m_digest,
    );

    let mut same = true;
    let mut detail = String::new();
    for rel in [
        "data/d000/f000.bin",
        "data/d000/keep.tmp",
        "big.bin",
        "wide/w000000.bin",
    ] {
        let want = fs::read(fx.tree_v1.join(rel));
        let got = fs::read(dir.join(rel));
        match (&want, &got) {
            (Ok(a), Ok(b)) if a == b => {}
            _ => {
                same = false;
                detail = format!("{rel}: {:?} / {:?}", want.err(), got.err());
                break;
            }
        }
    }
    rec.want(
        "tar",
        "tarextract.extract",
        "tar/extract-content-matches-source",
        same,
        format!("an extracted file differs from the source tree: {detail}"),
        "ok",
    );

    let link = fs::read_link(dir.join("links").join("ln000"));
    rec.want(
        "tar",
        "tarextract.extract",
        "tar/extract-symlink",
        matches!(&link, Ok(p) if p.to_string_lossy() == "../data/d000/f000.bin"),
        format!("symlink target {link:?}"),
        link.map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    );

    rec.want(
        "tar",
        "tarextract.extract",
        "tar/extract-honours-ingest-filter",
        !dir.join("build").exists(),
        "the ignored build/ directory was extracted",
        "absent",
    );
    let _ = fs::remove_dir_all(&dir);
}
