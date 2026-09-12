//! Scratch directories and store handles shared by the on-disk cases.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use amber_store_core::packstore;
use amber_store_core::refstore;

use crate::env::Env;
use crate::fixtures::copy_tree;
use crate::fixtures_build::store_options;

static WORK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh, empty scratch directory under the driver's scratch root (ext4,
/// not the session tmpfs). It is removed when the case releases it.
pub fn work_dir(env: &Env, name: &str) -> PathBuf {
    let n = WORK_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    let dir = env.scratch.join("work").join(format!("{name}-{n:06}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Duplicates a template directory without opening anything.
pub fn copied_dir(env: &Env, template: &Path, name: &str) -> PathBuf {
    let dir = work_dir(env, name);
    copy_tree(template, &dir).unwrap();
    dir
}

/// An open packstore plus the directory it owns.
pub struct StoreHandle {
    pub dir: PathBuf,
    pub st: Option<Arc<packstore::Store>>,
}

impl StoreHandle {
    pub fn store(&self) -> &packstore::Store {
        self.st.as_ref().expect("store is open")
    }

    pub fn close(mut self) {
        if let Some(st) = self.st.take() {
            let _ = st.close();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub fn open_store(env: &Env, dir: PathBuf, sync: bool) -> StoreHandle {
    let st = packstore::Store::open_with(&dir, store_options(&env.profile).sync(sync)).unwrap();
    StoreHandle {
        dir,
        st: Some(Arc::new(st)),
    }
}

pub fn fresh_store(env: &Env, name: &str) -> StoreHandle {
    let d = work_dir(env, name);
    open_store(env, d, false)
}

pub fn fresh_store_sync(env: &Env, name: &str) -> StoreHandle {
    let d = work_dir(env, name);
    open_store(env, d, true)
}

/// Duplicates a prebuilt template and opens the copy, so a destructive
/// operation never consumes the template. The copy happens in setup, outside
/// every measured interval.
pub fn copied_store(env: &Env, template: &Path, name: &str) -> StoreHandle {
    let dir = copied_dir(env, template, name);
    open_store(env, dir, false)
}

/// An open refstore plus the directory it owns.
pub struct RefHandle {
    pub dir: PathBuf,
    pub st: Option<refstore::Store>,
}

impl RefHandle {
    pub fn store(&self) -> &refstore::Store {
        self.st.as_ref().expect("refstore is open")
    }

    pub fn close(mut self) {
        drop(self.st.take());
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub fn fresh_refs(env: &Env, name: &str, sync: bool) -> RefHandle {
    let dir = work_dir(env, name);
    let st = refstore::Store::open(&dir, sync).unwrap();
    RefHandle { dir, st: Some(st) }
}

pub fn copied_refs(env: &Env, template: &Path, name: &str, sync: bool) -> RefHandle {
    let dir = copied_dir(env, template, name);
    let st = refstore::Store::open(&dir, sync).unwrap();
    RefHandle { dir, st: Some(st) }
}

/// Total bytes of every file below `dir`.
pub fn dir_bytes(dir: &Path) -> i64 {
    let mut n = 0i64;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            match e.file_type() {
                Ok(ft) if ft.is_dir() => n += dir_bytes(&e.path()),
                Ok(_) => {
                    if let Ok(md) = e.metadata() {
                        n += md.len() as i64;
                    }
                }
                Err(_) => {}
            }
        }
    }
    n
}
