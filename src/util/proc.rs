//! A bounded subprocess runner that reports resource usage.
//!
//! Every backend under comparison is driven through its own shipped command
//! line, so the fairest thing the harness can measure is a whole child
//! process: `wait4(2)` then yields wall time, child user and system CPU time
//! and peak resident set size on exactly the same terms for the Rust core,
//! the Go core, Git, restic and Nix alike. Nothing here ever runs a shell, so
//! no argument is ever re-parsed, and every call carries a deadline: a hung
//! backend fails the run instead of hanging it.
//!
//! Every child is placed in a **process group of its own**, and the group —
//! not just the direct child — is what a deadline terminates and what is
//! swept when the child is reaped. A backend that leaves a grandchild behind
//! would otherwise keep the harness's inherited pipe open, so the run would
//! hang in the drain thread of a command that had already "finished", and a
//! timeout would leave the real worker running.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Resource usage of one finished child process.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Wall-clock nanoseconds from just before `fork` to just after reaping.
    pub wall_ns: u64,
    /// `ru_utime` of the child and its waited-for descendants.
    pub user_ns: u64,
    /// `ru_stime` of the child and its waited-for descendants.
    pub sys_ns: u64,
    /// `ru_maxrss` of the child, in bytes.
    pub max_rss_bytes: u64,
}

impl Usage {
    #[allow(clippy::should_implement_trait)]
    /// Sums two usages; wall time adds, which is what a caller wants when it
    /// charges several child processes to one logical operation.
    pub fn add(self, other: Usage) -> Usage {
        Usage {
            wall_ns: self.wall_ns + other.wall_ns,
            user_ns: self.user_ns + other.user_ns,
            sys_ns: self.sys_ns + other.sys_ns,
            max_rss_bytes: self.max_rss_bytes.max(other.max_rss_bytes),
        }
    }
}

/// What a finished child left behind.
#[derive(Debug, Clone)]
pub struct Output {
    /// The command line, for diagnostics and for the report's command log.
    pub display: String,
    /// Exit code, or `None` when the child was terminated by a signal.
    pub code: Option<i32>,
    /// Terminating signal, if any.
    pub signal: Option<i32>,
    /// Set when the deadline fired and the harness killed the child.
    pub timed_out: bool,
    /// Captured standard output (empty when redirected to a file).
    pub stdout: Vec<u8>,
    /// Captured standard error, always captured.
    pub stderr: Vec<u8>,
    /// Resource usage as reported by `wait4`.
    pub usage: Usage,
}

impl Output {
    /// True when the child exited 0 and was not killed.
    pub fn success(&self) -> bool {
        self.code == Some(0) && !self.timed_out
    }

    /// Standard output decoded as UTF-8 with invalid sequences replaced, and
    /// trailing whitespace trimmed.
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim_end().to_string()
    }

    /// The last line of standard output, which is how both Amber CLIs report
    /// a root key.
    pub fn last_stdout_line(&self) -> String {
        self.stdout_text()
            .lines()
            .next_back()
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    /// `Ok(self)` when the child succeeded, otherwise a message naming the
    /// command, how it ended and the tail of its standard error.
    pub fn require_success(self) -> Result<Output, String> {
        if self.success() {
            return Ok(self);
        }
        let how = if self.timed_out {
            "timed out".to_string()
        } else if let Some(sig) = self.signal {
            format!("killed by signal {sig}")
        } else {
            format!("exited {}", self.code.unwrap_or(-1))
        };
        let text = String::from_utf8_lossy(&self.stderr);
        let mut tail: Vec<&str> = text.trim_end().lines().rev().take(8).collect();
        tail.reverse();
        Err(format!("{}: {how}\n{}", self.display, tail.join("\n")))
    }
}

/// Where a child's standard output goes.
#[derive(Debug, Clone)]
enum Sink {
    Capture,
    File(PathBuf),
}

/// One child process to run.
#[derive(Debug, Clone)]
pub struct Run {
    prog: PathBuf,
    args: Vec<OsString>,
    envs: Vec<(OsString, Option<OsString>)>,
    cwd: Option<PathBuf>,
    timeout: Duration,
    stdout: Sink,
    stdin: Option<PathBuf>,
}

/// The deadline applied when a caller does not set one.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(900);

impl Run {
    /// A new child running `prog` with the default deadline.
    pub fn new(prog: impl AsRef<Path>) -> Run {
        Run {
            prog: prog.as_ref().to_path_buf(),
            args: Vec::new(),
            envs: Vec::new(),
            cwd: None,
            timeout: DEFAULT_TIMEOUT,
            stdout: Sink::Capture,
            stdin: None,
        }
    }

    /// Appends one argument.
    pub fn arg(mut self, a: impl AsRef<OsStr>) -> Run {
        self.args.push(a.as_ref().to_os_string());
        self
    }

    /// Appends several arguments.
    pub fn args<I, S>(mut self, it: I) -> Run
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in it {
            self.args.push(a.as_ref().to_os_string());
        }
        self
    }

    /// Sets an environment variable for the child.
    pub fn env(mut self, k: impl AsRef<OsStr>, v: impl AsRef<OsStr>) -> Run {
        self.envs
            .push((k.as_ref().to_os_string(), Some(v.as_ref().to_os_string())));
        self
    }

    /// Removes an environment variable the harness itself inherited.
    pub fn env_remove(mut self, k: impl AsRef<OsStr>) -> Run {
        self.envs.push((k.as_ref().to_os_string(), None));
        self
    }

    /// Sets the child's working directory.
    pub fn cwd(mut self, dir: impl AsRef<Path>) -> Run {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Sets the deadline after which the child is killed.
    pub fn timeout(mut self, d: Duration) -> Run {
        self.timeout = d;
        self
    }

    /// Redirects standard output to `path` instead of capturing it, for
    /// commands whose output is bulk data (a tar export, a pack stream).
    pub fn stdout_to(mut self, path: impl AsRef<Path>) -> Run {
        self.stdout = Sink::File(path.as_ref().to_path_buf());
        self
    }

    /// Feeds the child's standard input from `path`.
    pub fn stdin_from(mut self, path: impl AsRef<Path>) -> Run {
        self.stdin = Some(path.as_ref().to_path_buf());
        self
    }

    /// The command line as it is recorded in reports. Arguments are shown
    /// space separated and quoted only when they contain whitespace; the
    /// harness never passes a credential in argv, so this is safe to publish.
    pub fn display(&self) -> String {
        let mut s = self.prog.display().to_string();
        for a in &self.args {
            let a = a.to_string_lossy();
            s.push(' ');
            if a.contains(char::is_whitespace) {
                s.push('"');
                s.push_str(&a);
                s.push('"');
            } else {
                s.push_str(&a);
            }
        }
        s
    }

    /// Runs the child to completion (or to its deadline) and reaps it with
    /// `wait4`, so the returned [`Usage`] is the kernel's own accounting.
    pub fn run(&self) -> Result<Output, String> {
        let display = self.display();
        let mut cmd = std::process::Command::new(&self.prog);
        cmd.args(&self.args);
        for (k, v) in &self.envs {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        match &self.stdin {
            Some(p) => {
                let f =
                    File::open(p).map_err(|e| format!("{display}: stdin {}: {e}", p.display()))?;
                cmd.stdin(Stdio::from(f));
            }
            None => {
                cmd.stdin(Stdio::null());
            }
        }
        match &self.stdout {
            Sink::Capture => {
                cmd.stdout(Stdio::piped());
            }
            Sink::File(p) => {
                let f = File::create(p)
                    .map_err(|e| format!("{display}: stdout {}: {e}", p.display()))?;
                cmd.stdout(Stdio::from(f));
            }
        }
        cmd.stderr(Stdio::piped());
        // A group of its own, so the deadline and the post-mortem sweep can
        // reach every descendant rather than only the process the harness
        // forked.
        cmd.process_group(0);

        let started = Instant::now();
        let mut child = cmd.spawn().map_err(|e| format!("{display}: spawn: {e}"))?;
        let pid = child.id() as libc::pid_t;

        // Drain both pipes concurrently: a child that fills one while the
        // harness waits on the other would deadlock.
        let out_thread = child.stdout.take().map(drain_thread);
        let err_thread = child.stderr.take().map(drain_thread);

        let timed_out = wait_with_deadline(pid, self.timeout);
        // The direct child is reapable but deliberately not yet reaped, so
        // its pid — and with it the id of the group it leads — cannot have
        // been recycled. Sweeping the group now kills anything it left
        // behind; without this, a lingering grandchild holding the inherited
        // stdout pipe open would block the drain threads below for ever.
        terminate_group(pid);
        let (code, signal, usage_raw) = reap(pid);

        let stdout = out_thread
            .map(|t| t.join().unwrap_or_default())
            .unwrap_or_default();
        let stderr = err_thread
            .map(|t| t.join().unwrap_or_default())
            .unwrap_or_default();

        let mut usage = usage_raw;
        usage.wall_ns = started.elapsed().as_nanos() as u64;

        Ok(Output {
            display,
            code,
            signal,
            timed_out,
            stdout,
            stderr,
            usage,
        })
    }

    /// Runs the child and fails unless it exited 0.
    pub fn ok(&self) -> Result<Output, String> {
        self.run()?.require_success()
    }
}

fn drain_thread<R: Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        buf
    })
}

/// Waits for `pid` to become reapable, killing it once `timeout` elapses.
/// Returns whether the deadline fired. The process is *not* reaped here.
/// Waits for `pid` to become reapable, killing it once `timeout` elapses.
/// Returns whether the deadline fired. The process is deliberately *not*
/// reaped here: `WNOWAIT` leaves the zombie in place so [`reap`] can collect
/// its `rusage` with a real `wait4` afterwards.
fn wait_with_deadline(pid: libc::pid_t, timeout: Duration) -> bool {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            );
        }
        let _ = tx.send(());
    });
    match rx.recv_timeout(timeout) {
        Ok(()) => false,
        Err(_) => {
            // The whole group, not only the direct child: a backend that
            // forked workers must not survive its own deadline.
            terminate_group(pid);
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
            // The child is now guaranteed to terminate; block for it.
            let _ = rx.recv();
            true
        }
    }
}

/// `SIGKILL`s every member of the process group `pid` leads.
///
/// Safe by construction: the harness gave this child a group of its own with
/// `setpgid`, the group id equals the child's pid, and the child is still
/// unreaped whenever this is called — so the id cannot name some unrelated
/// process's group. A group that no longer has members fails with `ESRCH`,
/// which is exactly the "nothing left to kill" case.
fn terminate_group(pid: libc::pid_t) {
    if pid <= 1 {
        return;
    }
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}

/// Reaps `pid` and converts its `rusage`.
fn reap(pid: libc::pid_t) -> (Option<i32>, Option<i32>, Usage) {
    let mut status: libc::c_int = 0;
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
    if rc < 0 {
        return (None, None, Usage::default());
    }
    let exited = libc::WIFEXITED(status);
    let code = if exited {
        Some(libc::WEXITSTATUS(status))
    } else {
        None
    };
    let signal = if libc::WIFSIGNALED(status) {
        Some(libc::WTERMSIG(status))
    } else {
        None
    };
    (code, signal, usage_from(&ru))
}

fn usage_from(ru: &libc::rusage) -> Usage {
    let tv = |t: libc::timeval| t.tv_sec as u64 * 1_000_000_000 + t.tv_usec as u64 * 1_000;
    Usage {
        wall_ns: 0,
        user_ns: tv(ru.ru_utime),
        sys_ns: tv(ru.ru_stime),
        max_rss_bytes: max_rss_bytes(ru.ru_maxrss),
    }
}

// ru_maxrss is kilobytes on Linux and bytes on Darwin.
#[cfg(target_os = "linux")]
fn max_rss_bytes(raw: libc::c_long) -> u64 {
    (raw.max(0) as u64).saturating_mul(1024)
}

#[cfg(not(target_os = "linux"))]
fn max_rss_bytes(raw: libc::c_long) -> u64 {
    raw.max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Run {
        Run::new("/bin/sh").arg("-c").arg(script)
    }

    #[test]
    fn captures_output_and_status() {
        let out = sh("printf 'a\\nb\\n'; printf 'oops\\n' >&2; exit 3")
            .run()
            .expect("spawn");
        assert_eq!(out.code, Some(3));
        assert!(!out.success());
        assert_eq!(out.stdout_text(), "a\nb");
        assert_eq!(out.last_stdout_line(), "b");
        assert!(String::from_utf8_lossy(&out.stderr).contains("oops"));
    }

    #[test]
    fn reports_cpu_time_for_a_busy_child() {
        let out = sh("i=0; while [ $i -lt 200000 ]; do i=$((i+1)); done")
            .run()
            .expect("spawn");
        assert!(out.success(), "{out:?}");
        assert!(out.usage.user_ns + out.usage.sys_ns > 0, "no CPU accounted");
        assert!(out.usage.wall_ns > 0);
        assert!(out.usage.max_rss_bytes > 0, "no peak RSS accounted");
    }

    #[test]
    fn kills_a_child_that_overruns_its_deadline() {
        let out = sh("sleep 30")
            .timeout(Duration::from_millis(200))
            .run()
            .expect("spawn");
        assert!(out.timed_out);
        assert!(!out.success());
        assert_eq!(out.signal, Some(libc::SIGKILL));
        let err = out.require_success().unwrap_err();
        assert!(err.contains("timed out"), "{err}");
    }

    /// True while `pid` is a process that can still do something.
    ///
    /// A killed descendant whose parent has already gone is reparented, and
    /// stays a zombie until whatever adopted it reaps it — so `kill(pid, 0)`
    /// alone would report a terminated process as alive. The scheduler state
    /// from `/proc` is what actually answers the question.
    fn alive(pid: libc::pid_t) -> bool {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        // The second field is the executable name in parentheses and may
        // itself contain spaces and parentheses, so the state follows the
        // last ')'.
        let Some(rest) = stat.rsplit_once(')') else {
            return false;
        };
        !matches!(rest.1.split_whitespace().next(), Some("Z") | None)
    }

    fn wait_until_gone(pid: libc::pid_t, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if !alive(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        !alive(pid)
    }

    #[test]
    fn a_grandchild_holding_stdout_open_does_not_hang_a_finished_command() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("grandchild.pid");
        // The shell exits immediately, leaving a child that inherited its
        // stdout and will hold the pipe open for 300 seconds. Without a
        // process-group sweep, reading stdout to end of file would block
        // until that child died.
        let out = sh(&format!(
            "sleep 300 & echo $! > {}; printf 'parent-done\\n'; exit 0",
            pidfile.display()
        ))
        .timeout(Duration::from_secs(30))
        .run()
        .expect("spawn");
        assert!(out.success(), "{out:?}");
        assert_eq!(out.stdout_text(), "parent-done");
        let pid: libc::pid_t = std::fs::read_to_string(&pidfile)
            .expect("pid file")
            .trim()
            .parse()
            .expect("pid");
        assert!(
            wait_until_gone(pid, Duration::from_secs(5)),
            "the grandchild {pid} outlived the command it belonged to"
        );
    }

    #[test]
    fn a_deadline_kills_descendants_not_only_the_direct_child() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("worker.pid");
        let out = sh(&format!(
            "sleep 300 & echo $! > {}; sleep 300",
            pidfile.display()
        ))
        .timeout(Duration::from_millis(300))
        .run()
        .expect("spawn");
        assert!(out.timed_out);
        let pid: libc::pid_t = std::fs::read_to_string(&pidfile)
            .expect("pid file")
            .trim()
            .parse()
            .expect("pid");
        assert!(
            wait_until_gone(pid, Duration::from_secs(5)),
            "the descendant {pid} survived its parent's deadline"
        );
    }

    #[test]
    fn require_success_names_the_command_and_stderr() {
        let err = sh("echo boom >&2; exit 1").ok().unwrap_err();
        assert!(err.contains("/bin/sh"), "{err}");
        assert!(err.contains("boom"), "{err}");
    }

    #[test]
    fn redirects_stdout_to_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let out = sh("printf 'hello'").stdout_to(&path).ok().unwrap();
        assert!(out.stdout.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn passes_environment_and_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let out = sh("printf '%s|%s' \"$AMBER_BENCH_T\" \"$PWD\"")
            .env("AMBER_BENCH_T", "v")
            .cwd(dir.path())
            .ok()
            .unwrap();
        let text = out.stdout_text();
        assert!(text.starts_with("v|"), "{text}");
    }

    #[test]
    fn display_quotes_only_arguments_with_spaces() {
        let r = Run::new("/bin/echo").arg("a").arg("b c");
        assert_eq!(r.display(), "/bin/echo a \"b c\"");
    }
}
