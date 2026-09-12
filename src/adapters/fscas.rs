//! The control: an uncompressed, whole-file SHA-256 filesystem CAS.
//!
//! No chunking, no compression, no encryption, no packing, no index — every
//! file's bytes are hashed whole and stored at `objects/ab/<hex>`, and a
//! reference is a JSON list of entries. Identical files cost nothing extra;
//! a file that differs by one byte costs a full copy.
//!
//! It exists so the other backends' numbers have a floor to be read against:
//! it is the simplest thing that could possibly work, and on some shapes of
//! data it wins.
//!
//! It runs as a subcommand of the harness binary, in its own child process,
//! so its wall time, CPU time and peak RSS are measured exactly the way every
//! other backend's are.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::util::fsx;
use crate::util::proc::Run;

/// One stored filesystem object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    /// `dir`, `file` or `symlink`.
    pub kind: String,
    pub mode: u32,
    pub mtime: i64,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
}

/// A stored reference: the whole tree, in path order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefFile {
    pub entries: Vec<Entry>,
}

/// A configured invocation of the baseline, as a child process.
#[derive(Debug, Clone)]
pub struct FsCasCli {
    /// The harness's own executable.
    pub bin: PathBuf,
    pub store: PathBuf,
    pub timeout: Duration,
}

impl FsCasCli {
    fn base(&self) -> Run {
        Run::new(&self.bin)
            .arg("fs-sha256")
            .arg("--store")
            .arg(&self.store)
            .timeout(self.timeout)
    }

    /// Stores `src` under reference `name`.
    pub fn ingest(&self, src: &Path, name: &str) -> Run {
        self.base().args(["ingest", "--ref", name]).arg(src)
    }

    /// Lists a reference's entries.
    pub fn list(&self, name: &str) -> Run {
        self.base().args(["list", "--ref", name])
    }

    /// Restores a reference into `dest`.
    pub fn restore(&self, name: &str, dest: &Path) -> Run {
        self.base().args(["restore", "--ref", name]).arg(dest)
    }

    /// Reads every object of a reference without writing anything, the
    /// read-throughput comparison.
    pub fn read(&self, name: &str) -> Run {
        self.base().args(["read", "--ref", name])
    }

    /// Deletes a reference.
    pub fn rm(&self, name: &str) -> Run {
        self.base().args(["rm", "--ref", name])
    }

    /// Prints one reference name per line: what the store still retains.
    pub fn refs(&self) -> Run {
        self.base().arg("refs")
    }

    /// Deletes every object no reference names.
    pub fn gc(&self) -> Run {
        self.base().arg("gc")
    }

    /// Re-hashes every object and checks every reference resolves.
    pub fn check(&self) -> Run {
        self.base().arg("check")
    }
}

fn objects_dir(store: &Path) -> PathBuf {
    store.join("objects")
}

fn refs_dir(store: &Path) -> PathBuf {
    store.join("refs")
}

fn object_path(store: &Path, hash: &str) -> PathBuf {
    objects_dir(store).join(&hash[..2]).join(hash)
}

fn ref_path(store: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return Err(format!("invalid reference name {name:?}"));
    }
    Ok(refs_dir(store).join(format!("{name}.json")))
}

/// Stores the tree at `src` under `name`.
pub fn ingest(store: &Path, src: &Path, name: &str) -> Result<(), String> {
    fs::create_dir_all(objects_dir(store)).map_err(e)?;
    fs::create_dir_all(refs_dir(store)).map_err(e)?;
    let mut entries = Vec::new();
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut children: Vec<_> = fs::read_dir(&dir)
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        children.sort_by_key(|c| c.file_name());
        for child in children {
            let path = child.path();
            let rel = path
                .strip_prefix(src)
                .map_err(|x| x.to_string())?
                .to_string_lossy()
                .into_owned();
            let md = fs::symlink_metadata(&path).map_err(e)?;
            let ft = md.file_type();
            if ft.is_dir() {
                stack.push(path.clone());
                entries.push(Entry {
                    path: rel,
                    kind: "dir".into(),
                    mode: md.mode() & 0o7777,
                    mtime: md.mtime(),
                    size: 0,
                    sha256: None,
                    link_target: None,
                });
            } else if ft.is_symlink() {
                let target = fs::read_link(&path)
                    .map_err(e)?
                    .to_string_lossy()
                    .into_owned();
                entries.push(Entry {
                    path: rel,
                    kind: "symlink".into(),
                    mode: md.mode() & 0o7777,
                    mtime: md.mtime(),
                    size: target.len() as u64,
                    sha256: None,
                    link_target: Some(target),
                });
            } else {
                let hash = fsx::sha256_file(&path).map_err(e)?;
                store_object(store, &path, &hash)?;
                entries.push(Entry {
                    path: rel,
                    kind: "file".into(),
                    mode: md.mode() & 0o7777,
                    mtime: md.mtime(),
                    size: md.len(),
                    sha256: Some(hash),
                    link_target: None,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let json = serde_json::to_vec(&RefFile { entries }).map_err(e)?;
    let path = ref_path(store, name)?;
    write_durable(&path, &json)
}

/// Copies a file into the object store unless it is already there. The
/// dedup test is the hash alone, which is the whole point of the baseline.
fn store_object(store: &Path, src: &Path, hash: &str) -> Result<(), String> {
    let dest = object_path(store, hash);
    if dest.exists() {
        return Ok(());
    }
    fs::create_dir_all(dest.parent().unwrap()).map_err(e)?;
    let bytes = fs::read(src).map_err(e)?;
    write_durable(&dest, &bytes)
}

fn write_durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(e)?;
        f.write_all(bytes).map_err(e)?;
        f.sync_all().map_err(e)?;
    }
    fs::rename(&tmp, path).map_err(e)
}

fn load_ref(store: &Path, name: &str) -> Result<RefFile, String> {
    let path = ref_path(store, name)?;
    let bytes = fs::read(&path).map_err(|x| format!("{}: {x}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(e)
}

/// Restores `name` into `dest`.
pub fn restore(store: &Path, name: &str, dest: &Path) -> Result<(), String> {
    let r = load_ref(store, name)?;
    fs::create_dir_all(dest).map_err(e)?;
    for entry in &r.entries {
        let path = dest.join(&entry.path);
        match entry.kind.as_str() {
            "dir" => fs::create_dir_all(&path).map_err(e)?,
            "symlink" => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(e)?;
                }
                let target = entry.link_target.clone().unwrap_or_default();
                let _ = fs::remove_file(&path);
                std::os::unix::fs::symlink(&target, &path).map_err(e)?;
            }
            _ => {
                let hash = entry
                    .sha256
                    .as_deref()
                    .ok_or_else(|| format!("{}: file entry without a hash", entry.path))?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(e)?;
                }
                let src = object_path(store, hash);
                fs::copy(&src, &path).map_err(|x| format!("{}: {x}", src.display()))?;
                fs::set_permissions(&path, fs::Permissions::from_mode(entry.mode)).map_err(e)?;
            }
        }
    }
    // Timestamps after contents, deepest directories last.
    for entry in &r.entries {
        if entry.kind != "dir" {
            fsx::set_mtime(&dest.join(&entry.path), entry.mtime, 0).map_err(e)?;
        }
    }
    let mut dirs: Vec<&Entry> = r.entries.iter().filter(|x| x.kind == "dir").collect();
    dirs.sort_by_key(|x| std::cmp::Reverse(x.path.len()));
    for entry in dirs {
        let path = dest.join(&entry.path);
        fs::set_permissions(&path, fs::Permissions::from_mode(entry.mode)).map_err(e)?;
        fsx::set_mtime(&path, entry.mtime, 0).map_err(e)?;
    }
    Ok(())
}

/// Reads every object a reference names, without writing anything.
pub fn read_all(store: &Path, name: &str) -> Result<u64, String> {
    let r = load_ref(store, name)?;
    let mut total = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    for entry in &r.entries {
        let Some(hash) = entry.sha256.as_deref() else {
            continue;
        };
        let mut f = fs::File::open(object_path(store, hash)).map_err(e)?;
        loop {
            let n = f.read(&mut buf).map_err(e)?;
            if n == 0 {
                break;
            }
            total += n as u64;
        }
    }
    Ok(total)
}

/// Prints one line per entry, the listing comparison.
pub fn list(store: &Path, name: &str, out: &mut impl Write) -> Result<u64, String> {
    let r = load_ref(store, name)?;
    for entry in &r.entries {
        writeln!(
            out,
            "{} {:o} {} {} {}",
            entry.kind,
            entry.mode,
            entry.size,
            entry.sha256.as_deref().unwrap_or("-"),
            entry.path
        )
        .map_err(e)?;
    }
    Ok(r.entries.len() as u64)
}

/// Every reference the store still holds, sorted.
///
/// This is what lets a scenario prove that a dropped reference is really
/// gone, instead of inferring it from a restore that failed for some other
/// reason.
pub fn list_refs(store: &Path) -> Result<Vec<String>, String> {
    let dir = refs_dir(store);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).map_err(e)? {
        let entry = entry.map_err(e)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(stripped) = name.strip_suffix(".json") {
            out.push(stripped.to_string());
        }
    }
    out.sort();
    Ok(out)
}

/// Deletes a reference. Objects are untouched until a collection runs.
pub fn remove_ref(store: &Path, name: &str) -> Result<(), String> {
    let path = ref_path(store, name)?;
    fs::remove_file(&path).map_err(|x| format!("{}: {x}", path.display()))
}

/// What a collection did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcResult {
    pub deleted_objects: u64,
    pub reclaimed_bytes: u64,
    pub kept_objects: u64,
}

/// Deletes every object no surviving reference names.
pub fn gc(store: &Path) -> Result<GcResult, String> {
    let mut live: BTreeSet<String> = BTreeSet::new();
    if refs_dir(store).exists() {
        for entry in fs::read_dir(refs_dir(store)).map_err(e)? {
            let entry = entry.map_err(e)?;
            let bytes = fs::read(entry.path()).map_err(e)?;
            let r: RefFile = serde_json::from_slice(&bytes).map_err(e)?;
            for item in r.entries {
                if let Some(h) = item.sha256 {
                    live.insert(h);
                }
            }
        }
    }
    let mut result = GcResult {
        deleted_objects: 0,
        reclaimed_bytes: 0,
        kept_objects: 0,
    };
    if !objects_dir(store).exists() {
        return Ok(result);
    }
    for shard in fs::read_dir(objects_dir(store)).map_err(e)? {
        let shard = shard.map_err(e)?;
        if !shard.path().is_dir() {
            continue;
        }
        for object in fs::read_dir(shard.path()).map_err(e)? {
            let object = object.map_err(e)?;
            let name = object.file_name().to_string_lossy().into_owned();
            if live.contains(&name) {
                result.kept_objects += 1;
                continue;
            }
            let size = object.metadata().map(|m| m.len()).unwrap_or(0);
            fs::remove_file(object.path()).map_err(e)?;
            result.deleted_objects += 1;
            result.reclaimed_bytes += size;
        }
    }
    Ok(result)
}

/// Re-hashes every object and checks every reference resolves. This is the
/// integrity check: it detects a flipped bit, a truncated object and a
/// reference pointing at an object that is gone.
pub fn check(store: &Path) -> Result<u64, String> {
    let mut checked = 0u64;
    if objects_dir(store).exists() {
        for shard in fs::read_dir(objects_dir(store)).map_err(e)? {
            let shard = shard.map_err(e)?;
            if !shard.path().is_dir() {
                continue;
            }
            for object in fs::read_dir(shard.path()).map_err(e)? {
                let object = object.map_err(e)?;
                let name = object.file_name().to_string_lossy().into_owned();
                let actual = fsx::sha256_file(&object.path()).map_err(e)?;
                if actual != name {
                    return Err(format!(
                        "corrupt object {}: content hashes to {actual}",
                        object.path().display()
                    ));
                }
                checked += 1;
            }
        }
    }
    if refs_dir(store).exists() {
        for entry in fs::read_dir(refs_dir(store)).map_err(e)? {
            let entry = entry.map_err(e)?;
            let bytes = fs::read(entry.path()).map_err(e)?;
            let r: RefFile = serde_json::from_slice(&bytes).map_err(e)?;
            for item in r.entries {
                if let Some(h) = item.sha256
                    && !object_path(store, &h).exists()
                {
                    return Err(format!(
                        "reference {} names missing object {h} for {}",
                        entry.path().display(),
                        item.path
                    ));
                }
            }
        }
    }
    Ok(checked)
}

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

/// The `fs-sha256` subcommand of the harness binary.
pub fn main(args: &[String]) -> Result<(), String> {
    let mut store: Option<PathBuf> = None;
    let mut reference: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut command: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--store" => store = it.next().map(PathBuf::from),
            "--ref" => reference = it.next().cloned(),
            other if other.starts_with("--") => {
                return Err(format!("fs-sha256: unknown flag {other}"));
            }
            other if command.is_none() => command = Some(other.to_string()),
            other => positional.push(other.to_string()),
        }
    }
    let store = store.ok_or_else(|| "fs-sha256: --store is required".to_string())?;
    let command = command.ok_or_else(|| "fs-sha256: a subcommand is required".to_string())?;
    let need_ref = || {
        reference
            .clone()
            .ok_or_else(|| "fs-sha256: --ref is required".to_string())
    };
    match command.as_str() {
        "ingest" => {
            let src = positional
                .first()
                .ok_or_else(|| "fs-sha256 ingest: a source path is required".to_string())?;
            ingest(&store, Path::new(src), &need_ref()?)
        }
        "restore" => {
            let dest = positional
                .first()
                .ok_or_else(|| "fs-sha256 restore: a destination is required".to_string())?;
            restore(&store, &need_ref()?, Path::new(dest))
        }
        "read" => read_all(&store, &need_ref()?).map(|n| println!("{n}")),
        "list" => {
            let stdout = io::stdout();
            let mut w = io::BufWriter::new(stdout.lock());
            list(&store, &need_ref()?, &mut w).map(|_| ())
        }
        "rm" => remove_ref(&store, &need_ref()?),
        "refs" => list_refs(&store).map(|names| {
            for name in names {
                println!("{name}");
            }
        }),
        "gc" => gc(&store).map(|r| {
            println!(
                "deleted {} objects, reclaimed {} bytes, kept {}",
                r.deleted_objects, r.reclaimed_bytes, r.kept_objects
            )
        }),
        "check" => check(&store).map(|n| println!("checked {n} objects")),
        other => Err(format!("fs-sha256: unknown subcommand {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::manifest::{Manifest, Strictness};

    fn sample(root: &Path) {
        fs::create_dir_all(root.join("d/sub")).unwrap();
        fs::write(root.join("d/a.txt"), b"hello").unwrap();
        fs::write(root.join("d/copy.txt"), b"hello").unwrap();
        fs::write(root.join("d/sub/b.bin"), vec![9u8; 5000]).unwrap();
        fs::set_permissions(root.join("d/sub/b.bin"), fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("a.txt", root.join("d/l")).unwrap();
        for p in ["d/a.txt", "d/copy.txt", "d/sub/b.bin", "d/l", "d/sub", "d"] {
            fsx::set_mtime(&root.join(p), 1_700_000_000, 0).unwrap();
        }
    }

    #[test]
    fn a_round_trip_restores_the_tree_exactly() {
        let src = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        sample(src.path());
        ingest(store.path(), src.path(), "v0").unwrap();
        restore(store.path(), "v0", out.path()).unwrap();
        let want = Manifest::observe("src", src.path()).unwrap();
        let v = want.verify_tree("restore", out.path(), Strictness::full());
        assert!(v.passed, "{v:?}");
    }

    #[test]
    fn identical_files_are_stored_once() {
        let src = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        sample(src.path());
        ingest(store.path(), src.path(), "v0").unwrap();
        // "hello" appears twice in the tree but only once in the store.
        let mut count = 0;
        for shard in fs::read_dir(objects_dir(store.path())).unwrap() {
            count += fs::read_dir(shard.unwrap().path()).unwrap().count();
        }
        assert_eq!(count, 2, "expected two distinct objects, found {count}");
    }

    #[test]
    fn collection_keeps_what_a_surviving_reference_names() {
        let src = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        sample(src.path());
        ingest(store.path(), src.path(), "v0").unwrap();
        fs::write(src.path().join("d/extra.bin"), vec![3u8; 4096]).unwrap();
        ingest(store.path(), src.path(), "v1").unwrap();

        let before = gc(store.path()).unwrap();
        assert_eq!(before.deleted_objects, 0, "nothing is garbage yet");

        remove_ref(store.path(), "v1").unwrap();
        let after = gc(store.path()).unwrap();
        assert_eq!(after.deleted_objects, 1);
        assert_eq!(after.reclaimed_bytes, 4096);
        // v0 must still restore.
        let out = tempfile::tempdir().unwrap();
        restore(store.path(), "v0", out.path()).unwrap();
        assert!(out.path().join("d/a.txt").exists());
        check(store.path()).unwrap();
    }

    #[test]
    fn the_integrity_check_catches_a_corrupted_object() {
        let src = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        sample(src.path());
        ingest(store.path(), src.path(), "v0").unwrap();
        check(store.path()).unwrap();

        let victim = fs::read_dir(objects_dir(store.path()))
            .unwrap()
            .flat_map(|s| fs::read_dir(s.unwrap().path()).unwrap())
            .map(|o| o.unwrap().path())
            .next()
            .unwrap();
        fs::write(&victim, b"tampered").unwrap();
        let err = check(store.path()).unwrap_err();
        assert!(err.contains("corrupt object"), "{err}");
    }

    #[test]
    fn the_integrity_check_catches_a_missing_object() {
        let src = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        sample(src.path());
        ingest(store.path(), src.path(), "v0").unwrap();
        let victim = fs::read_dir(objects_dir(store.path()))
            .unwrap()
            .flat_map(|s| fs::read_dir(s.unwrap().path()).unwrap())
            .map(|o| o.unwrap().path())
            .next()
            .unwrap();
        fs::remove_file(&victim).unwrap();
        let err = check(store.path()).unwrap_err();
        assert!(err.contains("missing object"), "{err}");
    }

    #[test]
    fn reference_names_cannot_escape_the_store() {
        let store = tempfile::tempdir().unwrap();
        assert!(ref_path(store.path(), "../../etc/passwd").is_err());
        assert!(ref_path(store.path(), "").is_err());
        assert!(ref_path(store.path(), "ok-name").is_ok());
    }

    #[test]
    fn the_subcommand_parser_reports_what_is_missing() {
        assert!(main(&["ingest".into()]).unwrap_err().contains("--store"));
        let err = main(&[
            "ingest".into(),
            "--store".into(),
            "/tmp/x".into(),
            "/tmp/src".into(),
        ])
        .unwrap_err();
        assert!(err.contains("--ref"), "{err}");
        let err = main(&["frobnicate".into(), "--store".into(), "/tmp/x".into()]).unwrap_err();
        assert!(err.contains("unknown subcommand"), "{err}");
    }

    #[test]
    fn reading_a_reference_returns_the_logical_byte_count() {
        let src = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        sample(src.path());
        ingest(store.path(), src.path(), "v0").unwrap();
        // 5 + 5 + 5000 logical bytes across three file entries.
        assert_eq!(read_all(store.path(), "v0").unwrap(), 5010);
    }

    #[test]
    fn listing_prints_one_line_per_entry() {
        let src = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        sample(src.path());
        ingest(store.path(), src.path(), "v0").unwrap();
        let mut buf = Vec::new();
        let n = list(store.path(), "v0", &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert_eq!(text.lines().count() as u64, n);
        assert!(text.contains("symlink"), "{text}");
        assert!(text.contains("d/sub/b.bin"), "{text}");
    }
}
