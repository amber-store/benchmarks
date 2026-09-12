//! Filesystem helpers: storage accounting, deterministic timestamps, and the
//! ownership rules that keep the harness from ever deleting something it did
//! not create.

use std::ffi::CString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The file that marks a directory as scratch space this harness created and
/// is therefore allowed to delete.
pub const OWNER_MARKER: &str = ".amber-bench-owned";

/// Storage occupancy of a directory tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct DirSizes {
    /// Sum of `st_size` over every non-directory entry: the bytes a reader
    /// would see.
    pub apparent_bytes: u64,
    /// Sum of `st_blocks * 512` over every entry including directories: the
    /// blocks the filesystem actually allocated. Sparse files and shared
    /// extents make this differ from `apparent_bytes` in both directions.
    pub allocated_bytes: u64,
    /// Regular files.
    pub files: u64,
    /// Directories, including the root.
    pub dirs: u64,
    /// Symbolic links.
    pub symlinks: u64,
}

/// Walks `root` with `lstat` and totals its occupancy. A missing root is
/// reported as all zeroes with `Ok`, because "the store does not exist yet"
/// is a legitimate before-state.
pub fn measure_dir(root: &Path) -> io::Result<DirSizes> {
    let mut acc = DirSizes::default();
    if !root.exists() {
        return Ok(acc);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let md = fs::symlink_metadata(&dir)?;
        acc.dirs += 1;
        acc.allocated_bytes += md.blocks() * 512;
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let md = fs::symlink_metadata(&path)?;
            let ft = md.file_type();
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            acc.allocated_bytes += md.blocks() * 512;
            if ft.is_symlink() {
                acc.symlinks += 1;
            } else {
                acc.files += 1;
                acc.apparent_bytes += md.len();
            }
        }
    }
    Ok(acc)
}

/// Every path under `root`, relative to it, sorted, excluding `root` itself.
pub fn walk_relative(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if fs::symlink_metadata(&path)?.is_dir() {
                stack.push(path.clone());
            }
            out.push(
                path.strip_prefix(root)
                    .map_err(|e| io::Error::other(e.to_string()))?
                    .to_path_buf(),
            );
        }
    }
    out.sort();
    Ok(out)
}

/// Copies a directory tree, preserving file modes and symlinks.
///
/// This exists so a verification can read a store without touching the one
/// the next measured operation will act on. Both Amber cores keep their
/// references in an embedded database — redb in Rust, Pebble in Go — and
/// opening one to read a single tree rewrites its files. A verification that
/// read the downloaded store in place would therefore leave it different
/// from what was published, and the incremental transfer measured next would
/// be pricing the harness's own verification as well as the change.
pub fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((from, to)) = stack.pop() {
        for entry in fs::read_dir(&from)? {
            let entry = entry?;
            let source = entry.path();
            let target = to.join(entry.file_name());
            let md = fs::symlink_metadata(&source)?;
            if md.is_dir() {
                fs::create_dir_all(&target)?;
                stack.push((source, target));
            } else if md.is_symlink() {
                let link = fs::read_link(&source)?;
                let _ = fs::remove_file(&target);
                std::os::unix::fs::symlink(link, &target)?;
            } else {
                fs::copy(&source, &target)?;
            }
        }
    }
    // Directory modes last: a read-only directory cannot be filled first.
    let mut dirs: Vec<PathBuf> = walk_relative(src)?
        .into_iter()
        .filter(|p| src.join(p).is_dir())
        .collect();
    dirs.sort();
    for rel in dirs.into_iter().rev() {
        let mode = fs::symlink_metadata(src.join(&rel))?.permissions();
        fs::set_permissions(dst.join(&rel), mode)?;
    }
    fs::set_permissions(dst, fs::symlink_metadata(src)?.permissions())
}

/// Creates `path` as a directory the harness owns, failing if it already
/// exists. Drops [`OWNER_MARKER`] inside so [`remove_owned`] will later agree
/// to delete it.
pub fn create_owned(path: &Path, run_id: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(path)?;
    let mut f = fs::File::create(path.join(OWNER_MARKER))?;
    writeln!(
        f,
        "amber-cas-bench scratch directory\nrun-id: {run_id}\ncreated-unix-nanos: {}",
        now_unix_nanos()
    )?;
    Ok(())
}

/// Why a path may not be deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotOwned {
    /// The path is relative; only absolute paths are accepted.
    Relative,
    /// The path is too close to the filesystem root to be scratch space.
    TooShallow,
    /// The path is a symbolic link, which could point anywhere.
    Symlink,
    /// The path does not exist.
    Missing,
    /// The path is not a directory.
    NotADirectory,
    /// The directory carries no [`OWNER_MARKER`].
    NoMarker,
}

impl std::fmt::Display for NotOwned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NotOwned::Relative => "path is not absolute",
            NotOwned::TooShallow => "path is too close to the filesystem root",
            NotOwned::Symlink => "path is a symbolic link",
            NotOwned::Missing => "path does not exist",
            NotOwned::NotADirectory => "path is not a directory",
            NotOwned::NoMarker => "directory carries no .amber-bench-owned marker",
        };
        f.write_str(s)
    }
}

/// Checks the ownership rules without touching anything.
pub fn check_owned(path: &Path) -> Result<(), NotOwned> {
    if !path.is_absolute() {
        return Err(NotOwned::Relative);
    }
    // "/", "/a" and "/a/b" are never scratch space; require at least three
    // components below the root.
    if path.components().count() < 4 {
        return Err(NotOwned::TooShallow);
    }
    let md = fs::symlink_metadata(path).map_err(|_| NotOwned::Missing)?;
    if md.file_type().is_symlink() {
        return Err(NotOwned::Symlink);
    }
    if !md.is_dir() {
        return Err(NotOwned::NotADirectory);
    }
    if !path.join(OWNER_MARKER).exists() {
        return Err(NotOwned::NoMarker);
    }
    Ok(())
}

/// Recursively deletes `path`, but only if [`check_owned`] accepts it.
pub fn remove_owned(path: &Path) -> Result<(), String> {
    check_owned(path).map_err(|e| format!("refusing to delete {}: {e}", path.display()))?;
    // Nix store paths inside the scratch tree are read-only directories.
    make_writable_tree(path).map_err(|e| format!("{}: {e}", path.display()))?;
    fs::remove_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Restores owner write permission throughout a tree. Isolated Nix stores and
/// restored Nix closures contain `r-xr-xr-x` directories that `remove_dir_all`
/// cannot descend into otherwise.
pub fn make_writable_tree(root: &Path) -> io::Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let md = fs::symlink_metadata(&dir)?;
        if md.is_dir() {
            let mut perm = md.permissions();
            let mode = std::os::unix::fs::PermissionsExt::mode(&perm);
            std::os::unix::fs::PermissionsExt::set_mode(&mut perm, mode | 0o700);
            let _ = fs::set_permissions(&dir, perm);
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if fs::symlink_metadata(&path)?.is_dir() {
                    stack.push(path);
                }
            }
        }
    }
    Ok(())
}

/// Sets a file's access and modification times without following symlinks, so
/// generated corpora carry timestamps that are a function of the seed rather
/// than of when the run happened. Both Amber cores hash mtime into directory
/// objects, so this is what makes their root keys reproducible.
pub fn set_mtime(path: &Path, unix_secs: i64, nanos: u32) -> io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let ts = libc::timespec {
        tv_sec: unix_secs as libc::time_t,
        tv_nsec: nanos as _,
    };
    let times = [ts, ts];
    let rc = unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            c.as_ptr(),
            times.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// SHA-256 of a file's contents, streamed.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// SHA-256 of a byte slice.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Bytes free on the filesystem holding `path`, for the unprivileged user.
pub fn free_bytes(path: &Path) -> io::Result<u64> {
    let c = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(st.f_bavail as u64 * st.f_frsize as u64)
}

/// Flushes the page cache for dirty data to disk. Called between an operation
/// and the storage measurement that follows it, never inside a measured
/// window, so it is charged to neither.
pub fn sync_all() {
    unsafe { libc::sync() };
}

/// Nanoseconds since the Unix epoch.
pub fn now_unix_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Writes `bytes` to `path`, creating parent directories.
pub fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_dir_totals_files_and_links() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("sub")).unwrap();
        fs::write(d.path().join("sub/a"), vec![7u8; 4096]).unwrap();
        std::os::unix::fs::symlink("a", d.path().join("sub/l")).unwrap();
        let s = measure_dir(d.path()).unwrap();
        assert_eq!(s.files, 1);
        assert_eq!(s.symlinks, 1);
        assert_eq!(s.dirs, 2);
        assert_eq!(s.apparent_bytes, 4096);
        assert!(s.allocated_bytes >= 4096);
    }

    #[test]
    fn measure_dir_reports_zero_for_a_missing_root() {
        let d = tempfile::tempdir().unwrap();
        let s = measure_dir(&d.path().join("nope")).unwrap();
        assert_eq!(s, DirSizes::default());
    }

    #[test]
    fn create_owned_refuses_an_existing_directory() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("scratch");
        create_owned(&p, "run").unwrap();
        let err = create_owned(&p, "run").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn remove_owned_deletes_only_marked_directories() {
        let d = tempfile::tempdir().unwrap();
        let owned = d.path().join("scratch");
        create_owned(&owned, "run").unwrap();
        fs::write(owned.join("x"), b"data").unwrap();
        remove_owned(&owned).unwrap();
        assert!(!owned.exists());

        let foreign = d.path().join("precious");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("keep"), b"data").unwrap();
        let err = remove_owned(&foreign).unwrap_err();
        assert!(err.contains("no .amber-bench-owned marker"), "{err}");
        assert!(foreign.join("keep").exists(), "foreign tree was touched");
    }

    #[test]
    fn ownership_rules_reject_dangerous_paths() {
        assert_eq!(check_owned(Path::new("relative")), Err(NotOwned::Relative));
        assert_eq!(check_owned(Path::new("/")), Err(NotOwned::TooShallow));
        assert_eq!(check_owned(Path::new("/tmp")), Err(NotOwned::TooShallow));
        assert_eq!(check_owned(Path::new("/a/b")), Err(NotOwned::TooShallow));
    }

    #[test]
    fn remove_owned_refuses_a_symlink_to_a_marked_directory() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        create_owned(&real, "run").unwrap();
        let link = d.path().join("a").join("b").join("link");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = remove_owned(&link).unwrap_err();
        assert!(err.contains("symbolic link"), "{err}");
        assert!(real.exists());
    }

    #[test]
    fn set_mtime_is_exact_and_does_not_follow_links() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("f");
        fs::write(&f, b"x").unwrap();
        let l = d.path().join("l");
        std::os::unix::fs::symlink("f", &l).unwrap();
        set_mtime(&f, 1_000_000, 0).unwrap();
        set_mtime(&l, 2_000_000, 0).unwrap();
        assert_eq!(fs::symlink_metadata(&f).unwrap().mtime(), 1_000_000);
        assert_eq!(fs::symlink_metadata(&l).unwrap().mtime(), 2_000_000);
    }

    #[test]
    fn sha256_matches_the_published_empty_string_digest() {
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// A verification reads a copy so it never disturbs the store the next
    /// measured transfer will sync against.
    #[test]
    fn a_copied_tree_reproduces_contents_modes_and_symlinks() {
        use std::os::unix::fs::PermissionsExt;
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let dst = dst.path().join("copy");

        fs::create_dir_all(src.path().join("packstore")).unwrap();
        fs::create_dir_all(src.path().join("refs/nested")).unwrap();
        write_file(&src.path().join("packstore/0001.seg"), b"segment").unwrap();
        write_file(&src.path().join("refs/nested/000005.sst"), b"table").unwrap();
        std::os::unix::fs::symlink("0001.seg", src.path().join("packstore/link")).unwrap();
        let exe = src.path().join("packstore/tool");
        write_file(&exe, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        // A read-only directory: the copy has to fill it before locking it.
        fs::set_permissions(
            src.path().join("refs/nested"),
            fs::Permissions::from_mode(0o555),
        )
        .unwrap();

        copy_tree(src.path(), &dst).unwrap();
        assert_eq!(
            walk_relative(src.path()).unwrap(),
            walk_relative(&dst).unwrap()
        );
        assert_eq!(
            fs::read(dst.join("packstore/0001.seg")).unwrap(),
            b"segment"
        );
        assert_eq!(
            fs::read(dst.join("refs/nested/000005.sst")).unwrap(),
            b"table"
        );
        assert_eq!(
            fs::read_link(dst.join("packstore/link")).unwrap(),
            std::path::Path::new("0001.seg")
        );
        assert_eq!(
            fs::symlink_metadata(dst.join("packstore/tool"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::symlink_metadata(dst.join("refs/nested"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o555
        );

        // Writing in the copy leaves the original alone, which is the whole
        // point: opening either core's reference database rewrites it.
        make_writable_tree(&dst).unwrap();
        write_file(&dst.join("refs/nested/000006.sst"), b"new").unwrap();
        fs::remove_file(dst.join("refs/nested/000005.sst")).unwrap();
        assert!(src.path().join("refs/nested/000005.sst").exists());
        assert!(!src.path().join("refs/nested/000006.sst").exists());
    }
}
