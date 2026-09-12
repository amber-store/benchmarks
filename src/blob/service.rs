//! The local S3-compatible object store.
//!
//! The blob scenarios run against a real object-storage service speaking real
//! HTTP, started and torn down by the harness: Garage, pinned by the
//! benchmark flake. Nothing here copies files between directories and calls
//! it a network transfer.
//!
//! A remote endpoint can be used instead, opt in only. A remote run must name
//! a unique prefix and the harness never touches an object outside it.

use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::util::proc::Run;
use crate::util::rng::Stream;

use super::s3::Target;

/// A running local Garage instance.
pub struct LocalService {
    child: Child,
    config: PathBuf,
    garage: PathBuf,
    /// Address the S3 API listens on.
    pub api_addr: SocketAddr,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    /// Version string, for the report.
    pub version: String,
}

impl LocalService {
    /// Starts Garage under `dir`, creates a bucket and a key, and waits until
    /// the S3 API answers.
    pub fn start(garage: &Path, dir: &Path, seed: u64) -> Result<LocalService, String> {
        std::fs::create_dir_all(dir.join("meta")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dir.join("data")).map_err(|e| e.to_string())?;

        let api_addr = free_addr()?;
        let rpc_addr = free_addr()?;
        let admin_addr = free_addr()?;
        let region = "garage".to_string();
        let bucket = "amber-cas-bench".to_string();
        // The RPC secret only protects the loopback cluster of one node, but
        // it still must not be predictable across runs on a shared host.
        let rpc_secret = hex::encode(Stream::new(seed ^ fresh_entropy(), "garage-rpc").bytes(32));
        let admin_token =
            hex::encode(Stream::new(seed ^ fresh_entropy(), "garage-admin").bytes(32));

        let config = dir.join("garage.toml");
        let toml = format!(
            "metadata_dir = \"{meta}\"\n\
             data_dir = \"{data}\"\n\
             db_engine = \"sqlite\"\n\
             replication_factor = 1\n\
             rpc_bind_addr = \"{rpc}\"\n\
             rpc_public_addr = \"{rpc}\"\n\
             rpc_secret = \"{rpc_secret}\"\n\
             \n\
             [s3_api]\n\
             s3_region = \"{region}\"\n\
             api_bind_addr = \"{api}\"\n\
             root_domain = \".s3.amber-bench.localhost\"\n\
             \n\
             [admin]\n\
             api_bind_addr = \"{admin}\"\n\
             admin_token = \"{admin_token}\"\n",
            meta = dir.join("meta").display(),
            data = dir.join("data").display(),
            rpc = rpc_addr,
            api = api_addr,
            admin = admin_addr,
        );
        write_private(&config, &toml)?;

        let log = std::fs::File::create(dir.join("garage.log")).map_err(|e| e.to_string())?;
        let child = Command::new(garage)
            .arg("server")
            .env("GARAGE_CONFIG_FILE", &config)
            .env("GARAGE_ALLOW_WORLD_READABLE_SECRETS", "false")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| format!("starting garage: {e}"))?;

        let mut svc = LocalService {
            child,
            config,
            garage: garage.to_path_buf(),
            api_addr,
            bucket,
            region,
            access_key: String::new(),
            secret_key: String::new(),
            version: String::new(),
        };
        if let Err(e) = svc.provision(rpc_addr) {
            svc.stop();
            let log = std::fs::read_to_string(dir.join("garage.log")).unwrap_or_default();
            let tail: Vec<&str> = log.lines().rev().take(15).collect();
            return Err(format!(
                "{e}\ngarage log tail:\n{}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            ));
        }
        Ok(svc)
    }

    fn cli(&self) -> Run {
        Run::new(&self.garage)
            .env("GARAGE_CONFIG_FILE", &self.config)
            .timeout(Duration::from_secs(120))
    }

    fn provision(&mut self, rpc_addr: SocketAddr) -> Result<(), String> {
        wait_for_port(rpc_addr, Duration::from_secs(60))?;
        self.version = self
            .cli()
            .arg("--version")
            .run()?
            .stdout_text()
            .lines()
            .next()
            .unwrap_or("")
            .to_string();

        // A single-node cluster still needs a layout before it will serve.
        let node = self.cli().args(["node", "id", "-q"]).ok()?.stdout_text();
        let node = node.split('@').next().unwrap_or(&node).trim().to_string();
        self.cli()
            .args(["layout", "assign", "-z", "bench", "-c", "200G", &node])
            .ok()?;
        self.cli()
            .args(["layout", "apply", "--version", "1"])
            .ok()?;

        // The layout takes a moment to become effective.
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if self
                .cli()
                .args(["bucket", "create", &self.bucket])
                .run()?
                .success()
            {
                break;
            }
            if Instant::now() > deadline {
                return Err("garage did not accept a bucket within 60 s".into());
            }
            std::thread::sleep(Duration::from_millis(250));
        }

        let out = self.cli().args(["key", "create", "amber-cas-bench"]).ok()?;
        let text = out.stdout_text();
        self.access_key = field(&text, "Key ID:")
            .ok_or_else(|| "garage key create: no key id in output".to_string())?;
        self.secret_key = field(&text, "Secret key:")
            .ok_or_else(|| "garage key create: no secret in output".to_string())?;
        self.cli()
            .args([
                "bucket",
                "allow",
                "--read",
                "--write",
                "--owner",
                &self.bucket,
                "--key",
                "amber-cas-bench",
            ])
            .ok()?;
        wait_for_port(self.api_addr, Duration::from_secs(60))?;
        Ok(())
    }

    /// A target pointing backends at `endpoint` (normally the measuring
    /// gateway) and the harness's own calls straight at the service.
    pub fn target(&self, endpoint: &str, prefix: &str) -> Target {
        Target {
            endpoint: endpoint.to_string(),
            admin_endpoint: format!("http://{}", self.api_addr),
            region: self.region.clone(),
            bucket: self.bucket.clone(),
            prefix: prefix.to_string(),
            access_key: self.access_key.clone(),
            secret_key: self.secret_key.clone(),
            remote: false,
        }
    }

    /// Stops the service.
    pub fn stop(&mut self) {
        unsafe {
            libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for LocalService {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The credentials a remote run needs, from the environment.
///
/// They are read here and passed in explicitly rather than being fetched
/// deep inside the target builder, so nothing that constructs a target has
/// to depend on process-wide state.
pub fn credentials_from_env() -> Result<(String, String), String> {
    let access_key = std::env::var("AWS_ACCESS_KEY_ID")
        .map_err(|_| "remote S3 requires AWS_ACCESS_KEY_ID in the environment".to_string())?;
    let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
        .map_err(|_| "remote S3 requires AWS_SECRET_ACCESS_KEY in the environment".to_string())?;
    if access_key.trim().is_empty() || secret_key.trim().is_empty() {
        return Err("remote S3 credentials are present but empty".into());
    }
    Ok((access_key, secret_key))
}

/// Builds a target for a user-supplied remote endpoint. The prefix must be
/// explicit and non-empty, so a remote run can never be pointed at the root
/// of somebody's bucket.
pub fn remote_target(
    endpoint: &str,
    region: &str,
    bucket: &str,
    prefix: &str,
    credentials: (String, String),
) -> Result<Target, String> {
    let (access_key, secret_key) = credentials;
    let prefix = prefix.trim().trim_matches('/');
    if prefix.is_empty() {
        return Err("remote S3 requires an explicit, unique --remote-s3-prefix".into());
    }
    if prefix.contains("..") {
        return Err(format!("remote S3 prefix {prefix:?} must not contain '..'"));
    }
    if bucket.trim().is_empty() {
        return Err("remote S3 requires --remote-s3-bucket".into());
    }
    // An endpoint given without a scheme becomes HTTPS. Guessing plain HTTP
    // for somebody's remote object store would send their credentials over
    // the wire in the clear.
    let endpoint = endpoint.trim().trim_end_matches('/');
    let endpoint = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!("https://{endpoint}")
    };
    Ok(Target {
        endpoint: endpoint.clone(),
        admin_endpoint: endpoint,
        region: region.to_string(),
        bucket: bucket.to_string(),
        prefix: prefix.to_string(),
        access_key,
        secret_key,
        remote: true,
    })
}

fn field(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
        .filter(|v| !v.is_empty())
}

fn free_addr() -> Result<SocketAddr, String> {
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    l.local_addr().map_err(|e| e.to_string())
}

fn fresh_entropy() -> u64 {
    crate::util::fsx::now_unix_nanos() as u64 ^ std::process::id() as u64
}

fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(contents.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn wait_for_port(addr: SocketAddr, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("{addr} did not start listening within {timeout:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garage_key_output_is_parsed() {
        let out = "Key name: amber-cas-bench\n\
                   Key ID: GK932c527adbf10ee0\n\
                   Secret key: 0300c7733c11803f\n\
                   Can create buckets: false\n";
        assert_eq!(field(out, "Key ID:").as_deref(), Some("GK932c527adbf10ee0"));
        assert_eq!(
            field(out, "Secret key:").as_deref(),
            Some("0300c7733c11803f")
        );
        assert!(field(out, "Nonexistent:").is_none());
    }

    #[test]
    fn a_remote_target_demands_an_explicit_prefix() {
        let creds = || ("k".to_string(), "s".to_string());
        assert!(
            remote_target("https://s3.example", "us-east-1", "b", "  ", creds())
                .unwrap_err()
                .contains("unique --remote-s3-prefix")
        );
        assert!(
            remote_target("https://s3.example", "us-east-1", "b", "a/../b", creds())
                .unwrap_err()
                .contains("'..'")
        );
        assert!(
            remote_target("https://s3.example", "us-east-1", "", "p", creds())
                .unwrap_err()
                .contains("--remote-s3-bucket")
        );
        let t =
            remote_target("https://s3.example/", "us-east-1", "b", "/runs/x/", creds()).unwrap();
        assert_eq!(t.prefix, "runs/x");
        assert_eq!(t.endpoint, "https://s3.example");
        assert!(t.remote);
        // A scheme-less endpoint is assumed to be TLS, never downgraded.
        let t = remote_target("s3.example.com", "us-east-1", "b", "p", creds()).unwrap();
        assert_eq!(t.endpoint, "https://s3.example.com");
        assert_eq!(t.scheme(), "https");
        // An explicit plain-HTTP endpoint is respected as given.
        let t = remote_target("http://127.0.0.1:9000", "r", "b", "p", creds()).unwrap();
        assert_eq!(t.scheme(), "http");
    }

    #[test]
    fn a_remote_run_without_credentials_is_an_error_not_a_silent_skip() {
        // SAFETY: the environment is only read here, and this is the one test
        // that touches these variables.
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        }
        let err = credentials_from_env().unwrap_err();
        assert!(err.contains("AWS_ACCESS_KEY_ID"), "{err}");
    }

    #[test]
    fn free_addr_returns_a_usable_loopback_port() {
        let a = free_addr().unwrap();
        assert!(a.ip().is_loopback());
        assert!(a.port() > 0);
    }
}
