//! What machine produced the numbers.
//!
//! Nothing here is used to adjust a measurement; it is recorded so a reader
//! can tell whether two reports are comparable at all. The cache policy is
//! stated explicitly rather than implied, because a benchmark that reads back
//! data it just wrote is reading it from RAM unless something says otherwise.

use std::path::Path;

use serde::Serialize;

use crate::util::fsx;

/// Facts about the host and the run environment.
#[derive(Debug, Clone, Serialize)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    /// `uname -srv` equivalent.
    pub kernel: String,
    pub cpu_model: String,
    pub cpu_logical_cores: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_total_bytes: Option<u64>,
    /// Where the scratch data lives, and how much room it had at the start.
    pub scratch_path: String,
    pub scratch_free_bytes_at_start: u64,
    /// 1-, 5- and 15-minute load averages when the run started.
    ///
    /// A benchmark host that was already busy produces numbers that are not
    /// comparable with one that was idle, and the only honest way to let a
    /// reader judge that is to write down what the machine was doing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_average_at_start: Option<(f64, f64, f64)>,
    /// Exactly what was and was not done to the page cache.
    pub cache_policy: String,
}

impl HostInfo {
    /// Collects host facts. Anything unavailable is reported as such rather
    /// than guessed.
    pub fn collect(scratch: &Path, dropped_caches: bool) -> HostInfo {
        HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            kernel: kernel(),
            cpu_model: cpu_model(),
            cpu_logical_cores: std::thread::available_parallelism().map_or(0, |n| n.get()),
            memory_total_bytes: memory_total_bytes(),
            scratch_path: scratch.display().to_string(),
            scratch_free_bytes_at_start: fsx::free_bytes(scratch).unwrap_or(0),
            load_average_at_start: load_average(),
            cache_policy: cache_policy(dropped_caches),
        }
    }
}

/// The kernel's 1-, 5- and 15-minute load averages, or `None` where the
/// system does not publish them. Never a fabricated zero: an idle host and an
/// unknown one are different facts.
fn load_average() -> Option<(f64, f64, f64)> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/loadavg").ok()?;
        let mut fields = text.split_whitespace();
        let mut next = || fields.next()?.parse::<f64>().ok();
        Some((next()?, next()?, next()?))
    }
    #[cfg(not(target_os = "linux"))]
    None
}

fn cache_policy(dropped: bool) -> String {
    if dropped {
        "The page cache was dropped before each measured read operation \
         (/proc/sys/vm/drop_caches). Read numbers are cold."
            .into()
    } else {
        "The page cache was NOT dropped: dropping it needs root, which this \
         harness does not ask for. Every store is freshly created per \
         repetition, and reads follow the writes that filled them, so read \
         and restore numbers are WARM-CACHE numbers and must not be described \
         as cold. Write numbers are unaffected. Enable --drop-caches on a host \
         where the harness can write /proc/sys/vm/drop_caches to change this."
            .into()
    }
}

fn kernel() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
            let v = std::fs::read_to_string("/proc/sys/kernel/version").unwrap_or_default();
            return format!("{} {}", s.trim(), v.trim());
        }
    }
    std::env::consts::OS.to_string()
}

fn cpu_model() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in s.lines() {
                if let Some((k, v)) = line.split_once(':')
                    && k.trim() == "model name"
                {
                    return v.trim().to_string();
                }
            }
        }
    }
    "unavailable".into()
}

fn memory_total_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
                return Some(kb * 1024);
            }
        }
    }
    None
}

/// Attempts to drop the page cache. Returns whether it worked; the caller
/// records the answer rather than assuming it.
pub fn drop_caches() -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open("/proc/sys/vm/drop_caches")
        .map_err(|e| format!("/proc/sys/vm/drop_caches: {e}"))?;
    fsx::sync_all();
    f.write_all(b"3")
        .map_err(|e| format!("/proc/sys/vm/drop_caches: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_info_never_claims_a_cold_cache_it_did_not_arrange() {
        let d = tempfile::tempdir().unwrap();
        let warm = HostInfo::collect(d.path(), false);
        assert!(
            warm.cache_policy.contains("WARM-CACHE"),
            "{}",
            warm.cache_policy
        );
        assert!(!warm.cache_policy.contains("are cold"));
        let cold = HostInfo::collect(d.path(), true);
        assert!(cold.cache_policy.contains("cold"), "{}", cold.cache_policy);
    }

    #[test]
    fn host_info_records_the_scratch_filesystem() {
        let d = tempfile::tempdir().unwrap();
        let h = HostInfo::collect(d.path(), false);
        assert_eq!(h.scratch_path, d.path().display().to_string());
        assert!(h.scratch_free_bytes_at_start > 0);
        assert!(h.cpu_logical_cores > 0);
    }

    /// A busy host and an idle one do not produce comparable numbers, so the
    /// load has to be in the report — and where it cannot be read it has to
    /// be absent rather than zero.
    #[test]
    fn host_info_records_what_the_machine_was_doing() {
        let d = tempfile::tempdir().unwrap();
        let h = HostInfo::collect(d.path(), false);
        #[cfg(target_os = "linux")]
        {
            let (one, five, fifteen) = h
                .load_average_at_start
                .expect("a Linux host has /proc/loadavg");
            for v in [one, five, fifteen] {
                assert!(v >= 0.0 && v.is_finite(), "{v}");
            }
        }
        let json = serde_json::to_string(&h).unwrap();
        if h.load_average_at_start.is_none() {
            assert!(
                !json.contains("load_average"),
                "an unknown load must be absent, not zero: {json}"
            );
        }
    }
}
