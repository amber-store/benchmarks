//! A measuring gateway for the object-storage scenarios.
//!
//! Every backend's S3 traffic is pointed at this listener, which forwards
//! bytes to the real service *verbatim* while parsing the HTTP/1.1 framing as
//! it goes. Forwarding and parsing are separate: the proxy never rewrites a
//! byte, so a parser that loses the framing can misreport a count but can
//! never corrupt a transfer. If it does lose it, `parse_desync` is set and
//! stays set — a count is either trustworthy or explicitly flagged, never
//! quietly wrong.
//!
//! Because the counters are taken on the wire, a client's retries, redirects
//! and pre-flight probes are all counted: they are part of what the transfer
//! cost.
//!
//! Latency and bandwidth simulation are off by default. When enabled, the
//! model is deliberately simple and is written into the report verbatim so
//! nobody has to guess what "50 ms" meant.

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

/// Latency and bandwidth simulation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shaping {
    /// Milliseconds of delay injected once per HTTP message, in each
    /// direction: a request and its response therefore gain `2 * latency_ms`.
    pub latency_ms: u64,
    /// Bytes per second allowed per direction per connection. Zero disables
    /// the cap.
    pub bandwidth_bytes_per_s: u64,
}

impl Shaping {
    /// The exact model, for the report.
    pub fn describe(&self) -> String {
        if self.latency_ms == 0 && self.bandwidth_bytes_per_s == 0 {
            return "none: the loopback path to the local object store was \
                    measured as it is"
                .into();
        }
        format!(
            "simulated by the harness gateway: {} ms of delay injected once per \
             HTTP message in each direction (so about {} ms added per \
             request/response pair), and a {} bytes/s cap applied per direction \
             per connection by sleeping for the transfer time of each forwarded \
             batch. This is a coarse model, not a network emulator.",
            self.latency_ms,
            self.latency_ms * 2,
            self.bandwidth_bytes_per_s
        )
    }
}

/// What crossed the gateway.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Stats {
    /// TCP connections accepted.
    pub connections: u64,
    /// Requests seen, keyed by HTTP method.
    pub requests_by_method: BTreeMap<String, u64>,
    /// Final responses seen, keyed by status class (`2xx`, `4xx`, ...).
    /// Interim `1xx` responses are counted separately under `1xx`.
    pub responses_by_status: BTreeMap<String, u64>,
    /// All bytes sent by clients, including request headers.
    pub bytes_client_to_service: u64,
    /// All bytes sent by the service, including response headers.
    pub bytes_service_to_client: u64,
    /// Request body bytes only.
    pub request_body_bytes: u64,
    /// Response body bytes only.
    pub response_body_bytes: u64,
    /// Set if the HTTP parser ever lost the framing. When true, the request
    /// and body counts above are not trustworthy and must be reported as
    /// such; the byte totals remain exact because they are counted at the
    /// socket.
    pub parse_desync: bool,
}

impl Stats {
    /// Total requests across all methods.
    pub fn total_requests(&self) -> u64 {
        self.requests_by_method.values().sum()
    }

    /// Counters in the flat form the report uses.
    pub fn counters(&self, prefix: &str) -> BTreeMap<String, i64> {
        let mut m = BTreeMap::new();
        m.insert(format!("{prefix}connections"), self.connections as i64);
        m.insert(
            format!("{prefix}bytes_up"),
            self.bytes_client_to_service as i64,
        );
        m.insert(
            format!("{prefix}bytes_down"),
            self.bytes_service_to_client as i64,
        );
        m.insert(
            format!("{prefix}request_body_bytes"),
            self.request_body_bytes as i64,
        );
        m.insert(
            format!("{prefix}response_body_bytes"),
            self.response_body_bytes as i64,
        );
        for (k, v) in &self.requests_by_method {
            m.insert(format!("{prefix}requests_{k}"), *v as i64);
        }
        for (k, v) in &self.responses_by_status {
            m.insert(format!("{prefix}responses_{k}"), *v as i64);
        }
        m.insert(
            format!("{prefix}requests_total"),
            self.total_requests() as i64,
        );
        if self.parse_desync {
            m.insert(format!("{prefix}parse_desync"), 1);
        }
        m
    }
}

/// The sockets of one accepted connection, kept so a shutdown can close
/// them and unblock the threads that are pumping them.
struct Live {
    client: TcpStream,
    service: TcpStream,
}

/// Everything a shutdown has to reach.
#[derive(Default)]
struct Registry {
    /// Open connections, by accept order. An entry is removed when its
    /// connection finishes, so a long run does not accumulate descriptors.
    open: BTreeMap<u64, Live>,
    /// Threads pumping those connections. Finished handles are pruned as new
    /// connections arrive.
    threads: Vec<std::thread::JoinHandle<()>>,
}

/// A running gateway.
pub struct Gateway {
    addr: SocketAddr,
    stats: Arc<Mutex<Stats>>,
    stop: Arc<AtomicBool>,
    shaping: Shaping,
    registry: Arc<Mutex<Registry>>,
    /// The accept loop, taken by the first shutdown.
    accept: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Gateway {
    /// Starts listening on an ephemeral loopback port and forwarding to
    /// `upstream`.
    pub fn start(upstream: SocketAddr, shaping: Shaping) -> std::io::Result<Gateway> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let stats = Arc::new(Mutex::new(Stats::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let registry: Arc<Mutex<Registry>> = Arc::new(Mutex::new(Registry::default()));
        let accept = {
            let stats = Arc::clone(&stats);
            let stop = Arc::clone(&stop);
            let registry = Arc::clone(&registry);
            listener.set_nonblocking(false)?;
            std::thread::spawn(move || {
                let mut next_id = 0u64;
                for conn in listener.incoming() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(client) = conn else { continue };
                    let stats = Arc::clone(&stats);
                    let registry_for_thread = Arc::clone(&registry);
                    let id = next_id;
                    next_id += 1;
                    let handle = std::thread::spawn(move || {
                        let _ = proxy(id, client, upstream, stats, shaping, &registry_for_thread);
                        // Whatever happened, this connection's sockets are no
                        // longer worth holding on to.
                        registry_for_thread
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .open
                            .remove(&id);
                    });
                    let mut reg = registry.lock().unwrap_or_else(|e| e.into_inner());
                    reg.threads.retain(|h| !h.is_finished());
                    reg.threads.push(handle);
                }
            })
        };
        Ok(Gateway {
            addr,
            stats,
            stop,
            shaping,
            registry,
            accept: Mutex::new(Some(accept)),
        })
    }

    /// The endpoint clients should be pointed at.
    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// A copy of the counters so far.
    pub fn snapshot(&self) -> Stats {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Zeroes the counters, so the next operation is measured on its own.
    /// `parse_desync` is deliberately *not* cleared: once the parser has lost
    /// the framing on a connection, later counts on it stay suspect.
    pub fn reset(&self) {
        let mut s = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let desync = s.parse_desync;
        *s = Stats::default();
        s.parse_desync = desync;
    }

    /// The shaping model in force.
    pub fn shaping(&self) -> Shaping {
        self.shaping
    }

    /// Stops the gateway completely: no new connections are accepted, every
    /// connection already accepted is closed in both directions, and every
    /// thread that was pumping one is joined before this returns.
    ///
    /// Closing the sockets is what makes the join finish: a pump thread is
    /// blocked in `read` on a socket that a well-behaved S3 client keeps
    /// alive, so merely stopping the accept loop would leave those threads —
    /// and their descriptors — behind for the rest of the process's life.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        // Unblock the accept loop, which then sees the stop flag.
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.accept.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = handle.join();
        }
        // Close both ends of every live connection, then wait for the
        // threads that were pumping them.
        let threads = {
            let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
            for live in reg.open.values() {
                let _ = live.client.shutdown(Shutdown::Both);
                let _ = live.service.shutdown(Shutdown::Both);
            }
            std::mem::take(&mut reg.threads)
        };
        for handle in threads {
            let _ = handle.join();
        }
        self.registry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .open
            .clear();
    }

    /// True when no connection thread is still running. Used by the tests
    /// and by the runner's teardown check.
    pub fn idle(&self) -> bool {
        let reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.open.is_empty() && reg.threads.iter().all(|h| h.is_finished())
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn proxy(
    id: u64,
    client: TcpStream,
    upstream: SocketAddr,
    stats: Arc<Mutex<Stats>>,
    shaping: Shaping,
    registry: &Mutex<Registry>,
) -> std::io::Result<()> {
    let service = TcpStream::connect(upstream)?;
    client.set_nodelay(true)?;
    service.set_nodelay(true)?;
    // Register both ends before any blocking read, so a shutdown that races
    // with a new connection can still close it.
    registry
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .open
        .insert(
            id,
            Live {
                client: client.try_clone()?,
                service: service.try_clone()?,
            },
        );
    stats.lock().unwrap_or_else(|e| e.into_inner()).connections += 1;

    // Response framing needs to know which request it answers (a HEAD has no
    // body), so the two directions share a queue of pending methods.
    let pending: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));

    let c2 = client.try_clone()?;
    let s2 = service.try_clone()?;
    let up_stats = Arc::clone(&stats);
    let up_pending = Arc::clone(&pending);
    let up = std::thread::spawn(move || {
        pump(
            c2,
            s2,
            Framing::request(up_pending),
            up_stats,
            Direction::Up,
            shaping,
        )
    });
    pump(
        service,
        client,
        Framing::response(pending),
        stats,
        Direction::Down,
        shaping,
    );
    let _ = up.join();
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
}

fn pump(
    mut src: TcpStream,
    mut dst: TcpStream,
    mut framing: Framing,
    stats: Arc<Mutex<Stats>>,
    dir: Direction,
    shaping: Shaping,
) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let observed = framing.feed(&buf[..n]);
        {
            let mut s = stats.lock().unwrap_or_else(|e| e.into_inner());
            match dir {
                Direction::Up => {
                    s.bytes_client_to_service += n as u64;
                    s.request_body_bytes += observed.body_bytes;
                    for m in &observed.starts {
                        *s.requests_by_method.entry(m.clone()).or_insert(0) += 1;
                    }
                }
                Direction::Down => {
                    s.bytes_service_to_client += n as u64;
                    s.response_body_bytes += observed.body_bytes;
                    for m in &observed.starts {
                        *s.responses_by_status.entry(m.clone()).or_insert(0) += 1;
                    }
                }
            }
            if observed.desync {
                s.parse_desync = true;
            }
        }
        if shaping.latency_ms > 0 && !observed.starts.is_empty() {
            std::thread::sleep(Duration::from_millis(shaping.latency_ms));
        }
        if shaping.bandwidth_bytes_per_s > 0 {
            let secs = n as f64 / shaping.bandwidth_bytes_per_s as f64;
            std::thread::sleep(Duration::from_secs_f64(secs));
        }
        if dst.write_all(&buf[..n]).is_err() {
            break;
        }
    }
    let _ = dst.shutdown(Shutdown::Write);
}

/// What one `feed` observed.
#[derive(Default)]
struct Observed {
    /// Method names (requests) or status classes (responses) that started.
    starts: Vec<String>,
    /// Body bytes seen in this batch.
    body_bytes: u64,
    /// The parser lost the framing.
    desync: bool,
}

/// HTTP/1.1 message framing, observed rather than enforced.
///
/// The parser keeps only unparsed header bytes in `carry`; body bytes are
/// counted and dropped as they stream past, so memory stays flat no matter
/// how large an upload is.
enum State {
    /// Accumulating a header block.
    Head,
    /// Remaining body bytes; `u64::MAX` means "until the connection closes".
    Body(u64),
    /// Expecting a chunk-size line.
    ChunkSize,
    /// Remaining payload bytes of the current chunk.
    ChunkData(u64),
    /// Remaining bytes of the CRLF that terminates a chunk's payload.
    ChunkCrlf(u64),
    /// After the final chunk: trailer lines, then a blank line.
    Trailers,
    /// The framing was lost; counts on this connection are no longer valid.
    Lost,
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Request,
    Response,
}

struct Framing {
    side: Side,
    state: State,
    carry: Vec<u8>,
    pending: Arc<Mutex<VecDeque<String>>>,
}

const MAX_HEAD: usize = 256 * 1024;

impl Framing {
    fn request(pending: Arc<Mutex<VecDeque<String>>>) -> Framing {
        Framing {
            side: Side::Request,
            state: State::Head,
            carry: Vec::new(),
            pending,
        }
    }

    fn response(pending: Arc<Mutex<VecDeque<String>>>) -> Framing {
        Framing {
            side: Side::Response,
            state: State::Head,
            carry: Vec::new(),
            pending,
        }
    }

    fn feed(&mut self, data: &[u8]) -> Observed {
        let mut out = Observed::default();
        if matches!(self.state, State::Lost) {
            out.desync = true;
            return out;
        }
        self.carry.extend_from_slice(data);
        let mut pos = 0usize;
        loop {
            let rest = &self.carry[pos..];
            if rest.is_empty() {
                break;
            }
            match self.state {
                State::Lost => {
                    out.desync = true;
                    self.carry.clear();
                    return out;
                }
                State::Head => {
                    let Some(i) = find(rest, b"\r\n\r\n") else {
                        if rest.len() > MAX_HEAD {
                            self.state = State::Lost;
                            out.desync = true;
                            self.carry.clear();
                            return out;
                        }
                        break;
                    };
                    let head = rest[..i + 4].to_vec();
                    pos += i + 4;
                    match self.parse_head(&head) {
                        Ok(Some(next)) => {
                            if let Some(label) = next.label {
                                out.starts.push(label);
                            }
                            self.state = next.state;
                        }
                        // An interim 1xx response: no body, another head follows.
                        Ok(None) => {
                            out.starts.push("1xx".into());
                            self.state = State::Head;
                        }
                        Err(()) => {
                            self.state = State::Lost;
                            out.desync = true;
                            self.carry.clear();
                            return out;
                        }
                    }
                }
                State::Body(ref mut left) => {
                    let take = if *left == u64::MAX {
                        rest.len()
                    } else {
                        (*left).min(rest.len() as u64) as usize
                    };
                    out.body_bytes += take as u64;
                    pos += take;
                    if *left != u64::MAX {
                        *left -= take as u64;
                        if *left == 0 {
                            self.state = State::Head;
                        }
                    }
                }
                State::ChunkSize => {
                    let Some(i) = find(rest, b"\r\n") else {
                        if rest.len() > 1024 {
                            self.state = State::Lost;
                            out.desync = true;
                            self.carry.clear();
                            return out;
                        }
                        break;
                    };
                    let line = String::from_utf8_lossy(&rest[..i]).to_string();
                    pos += i + 2;
                    let hex = line.split(';').next().unwrap_or("").trim();
                    match u64::from_str_radix(hex, 16) {
                        Ok(0) => self.state = State::Trailers,
                        Ok(n) => self.state = State::ChunkData(n),
                        Err(_) => {
                            self.state = State::Lost;
                            out.desync = true;
                            self.carry.clear();
                            return out;
                        }
                    }
                }
                State::ChunkData(ref mut left) => {
                    let take = (*left).min(rest.len() as u64) as usize;
                    out.body_bytes += take as u64;
                    pos += take;
                    *left -= take as u64;
                    if *left == 0 {
                        self.state = State::ChunkCrlf(2);
                    }
                }
                State::ChunkCrlf(ref mut left) => {
                    let take = (*left).min(rest.len() as u64) as usize;
                    pos += take;
                    *left -= take as u64;
                    if *left == 0 {
                        self.state = State::ChunkSize;
                    }
                }
                State::Trailers => {
                    let Some(i) = find(rest, b"\r\n") else {
                        break;
                    };
                    pos += i + 2;
                    if i == 0 {
                        self.state = State::Head;
                    }
                }
            }
        }
        self.carry.drain(..pos);
        out
    }

    /// Parses one header block. Returns the next state and the label to
    /// count, `None` for an interim `1xx` response, or `Err` when the bytes
    /// are not an HTTP message at all.
    fn parse_head(&mut self, head: &[u8]) -> Result<Option<Next>, ()> {
        let text = String::from_utf8_lossy(head);
        let mut lines = text.split("\r\n");
        let first = lines.next().ok_or(())?;
        let mut content_length: Option<u64> = None;
        let mut chunked = false;
        for line in lines {
            if line.is_empty() {
                break;
            }
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            if k == "content-length" {
                content_length = v.parse().ok();
            } else if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            }
        }
        match self.side {
            Side::Request => {
                let method = first.split(' ').next().unwrap_or("").to_string();
                if method.is_empty() || !method.chars().all(|c| c.is_ascii_uppercase()) {
                    return Err(());
                }
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_back(method.clone());
                let state = if chunked {
                    State::ChunkSize
                } else {
                    State::Body(content_length.unwrap_or(0))
                };
                Ok(Some(Next {
                    label: Some(method),
                    state,
                }))
            }
            Side::Response => {
                if !first.starts_with("HTTP/") {
                    return Err(());
                }
                let code: u16 = first
                    .split(' ')
                    .nth(1)
                    .and_then(|c| c.parse().ok())
                    .ok_or(())?;
                if (100..200).contains(&code) {
                    return Ok(None);
                }
                // A HEAD response and a 204/304 carry no body whatever their
                // headers claim, so the method that is being answered has to
                // be known here.
                let method = self
                    .pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .pop_front()
                    .unwrap_or_default();
                let bodyless = method == "HEAD" || code == 204 || code == 304;
                let state = if bodyless {
                    State::Head
                } else if chunked {
                    State::ChunkSize
                } else {
                    match content_length {
                        Some(n) => State::Body(n),
                        // No framing at all: the body runs to end of stream.
                        None => State::Body(u64::MAX),
                    }
                };
                Ok(Some(Next {
                    label: Some(format!("{}xx", code / 100)),
                    state,
                }))
            }
        }
    }
}

struct Next {
    label: Option<String>,
    state: State,
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    /// A minimal HTTP/1.1 origin server: answers every request, echoes the
    /// request body length back, and supports HEAD and chunked uploads.
    fn origin() -> (SocketAddr, std::thread::JoinHandle<()>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            for conn in l.incoming() {
                let Ok(stream) = conn else { break };
                std::thread::spawn(move || serve(stream));
            }
        });
        (addr, h)
    }

    fn serve(mut s: TcpStream) {
        let mut reader = std::io::BufReader::new(s.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let method = line.split(' ').next().unwrap_or("").to_string();
            let mut len = 0usize;
            let mut chunked = false;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 {
                    return;
                }
                if h == "\r\n" {
                    break;
                }
                let lower = h.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
                    chunked = true;
                }
            }
            if chunked {
                loop {
                    let mut size = String::new();
                    reader.read_line(&mut size).unwrap();
                    let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
                    let mut body = vec![0u8; n + 2];
                    reader.read_exact(&mut body).unwrap();
                    if n == 0 {
                        break;
                    }
                }
            } else if len > 0 {
                let mut body = vec![0u8; len];
                reader.read_exact(&mut body).unwrap();
            }
            let payload = b"hello-from-origin";
            if method == "HEAD" {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        payload.len()
                    )
                    .as_bytes(),
                );
            } else if method == "DELETE" {
                let _ = s.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
            } else {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        payload.len()
                    )
                    .as_bytes(),
                );
                let _ = s.write_all(payload);
            }
        }
    }

    fn request(addr: SocketAddr, raw: &[u8]) -> Vec<u8> {
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(raw).unwrap();
        s.shutdown(Shutdown::Write).unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn counts_requests_by_method_and_bytes_in_both_directions() {
        let (up, _h) = origin();
        let gw = Gateway::start(up, Shaping::default()).unwrap();
        let addr: SocketAddr = gw.endpoint().trim_start_matches("http://").parse().unwrap();

        let body = vec![b'x'; 1000];
        let mut raw = format!(
            "PUT /bucket/key HTTP/1.1\r\nHost: h\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        raw.extend_from_slice(&body);
        let resp = request(addr, &raw);
        assert!(String::from_utf8_lossy(&resp).contains("hello-from-origin"));

        request(addr, b"GET /bucket/key HTTP/1.1\r\nHost: h\r\n\r\n");
        request(addr, b"HEAD /bucket/key HTTP/1.1\r\nHost: h\r\n\r\n");
        request(addr, b"DELETE /bucket/key HTTP/1.1\r\nHost: h\r\n\r\n");
        std::thread::sleep(Duration::from_millis(150));

        let s = gw.snapshot();
        assert!(!s.parse_desync, "parser lost framing: {s:?}");
        assert_eq!(s.requests_by_method.get("PUT"), Some(&1));
        assert_eq!(s.requests_by_method.get("GET"), Some(&1));
        assert_eq!(s.requests_by_method.get("HEAD"), Some(&1));
        assert_eq!(s.requests_by_method.get("DELETE"), Some(&1));
        assert_eq!(s.total_requests(), 4);
        assert_eq!(s.responses_by_status.get("2xx"), Some(&4));
        assert_eq!(s.request_body_bytes, 1000);
        // GET and PUT return a body; HEAD and DELETE do not.
        assert_eq!(s.response_body_bytes, 2 * 17);
        assert!(s.bytes_client_to_service > 1000);
        assert!(s.bytes_service_to_client >= 34);
        assert_eq!(s.connections, 4);
    }

    #[test]
    fn counts_a_chunked_upload_without_losing_framing() {
        let (up, _h) = origin();
        let gw = Gateway::start(up, Shaping::default()).unwrap();
        let addr: SocketAddr = gw.endpoint().trim_start_matches("http://").parse().unwrap();
        let mut raw =
            b"POST /b/k HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        raw.extend_from_slice(b"10\r\n0123456789abcdef\r\n");
        raw.extend_from_slice(b"4\r\ntail\r\n");
        raw.extend_from_slice(b"0\r\n\r\n");
        request(addr, &raw);
        std::thread::sleep(Duration::from_millis(150));
        let s = gw.snapshot();
        assert!(!s.parse_desync, "{s:?}");
        assert_eq!(s.requests_by_method.get("POST"), Some(&1));
        assert_eq!(s.request_body_bytes, 20);
    }

    #[test]
    fn keeps_counting_across_a_kept_alive_connection() {
        let (up, _h) = origin();
        let gw = Gateway::start(up, Shaping::default()).unwrap();
        let addr: SocketAddr = gw.endpoint().trim_start_matches("http://").parse().unwrap();
        let mut s = TcpStream::connect(addr).unwrap();
        for _ in 0..3 {
            s.write_all(b"GET /b/k HTTP/1.1\r\nHost: h\r\n\r\n")
                .unwrap();
            let mut buf = [0u8; 64];
            let _ = s.read(&mut buf).unwrap();
        }
        s.shutdown(Shutdown::Both).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let st = gw.snapshot();
        assert_eq!(st.connections, 1);
        assert_eq!(st.requests_by_method.get("GET"), Some(&3), "{st:?}");
        assert!(!st.parse_desync);
    }

    #[test]
    fn forwards_bytes_verbatim_even_when_the_parser_gives_up() {
        let (up, _h) = origin();
        let gw = Gateway::start(up, Shaping::default()).unwrap();
        let addr: SocketAddr = gw.endpoint().trim_start_matches("http://").parse().unwrap();
        // Not HTTP at all: the parser must flag itself rather than guess.
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"not-http nonsense\r\n\r\n").unwrap();
        s.shutdown(Shutdown::Write).unwrap();
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        std::thread::sleep(Duration::from_millis(150));
        let st = gw.snapshot();
        assert!(st.parse_desync, "desync must be reported");
        assert!(st.bytes_client_to_service > 0, "bytes are still counted");
    }

    #[test]
    fn reset_clears_counts_but_never_clears_a_desync_flag() {
        let (up, _h) = origin();
        let gw = Gateway::start(up, Shaping::default()).unwrap();
        let addr: SocketAddr = gw.endpoint().trim_start_matches("http://").parse().unwrap();
        request(addr, b"GET /b/k HTTP/1.1\r\nHost: h\r\n\r\n");
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(gw.snapshot().total_requests(), 1);
        gw.reset();
        assert_eq!(gw.snapshot().total_requests(), 0);

        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"garbage\r\n\r\n").unwrap();
        let _ = s.shutdown(Shutdown::Write);
        std::thread::sleep(Duration::from_millis(150));
        assert!(gw.snapshot().parse_desync);
        gw.reset();
        assert!(
            gw.snapshot().parse_desync,
            "a desync must survive a counter reset"
        );
    }

    #[test]
    fn shutdown_closes_live_connections_and_joins_their_threads() {
        let (up, _h) = origin();
        let gw = Gateway::start(up, Shaping::default()).unwrap();
        let addr: SocketAddr = gw.endpoint().trim_start_matches("http://").parse().unwrap();

        // A keep-alive client, exactly like the S3 clients the harness
        // drives: it makes a request and then leaves the connection open.
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET /b/k HTTP/1.1\r\nHost: h\r\n\r\n")
            .unwrap();
        let mut buf = [0u8; 64];
        let n = client.read(&mut buf).unwrap();
        assert!(n > 0);
        assert!(!gw.idle(), "the connection thread should still be running");

        let start = std::time::Instant::now();
        gw.shutdown();
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "shutdown must not wait for an idle client to hang up"
        );
        assert!(
            gw.idle(),
            "every connection thread must have finished by the time shutdown returns"
        );

        // The client's socket really was closed, not just forgotten.
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut rest = Vec::new();
        let _ = client.read_to_end(&mut rest);

        // And nothing new is accepted.
        if let Ok(mut late) = TcpStream::connect(addr) {
            let _ = late.write_all(b"GET /b/k HTTP/1.1\r\nHost: h\r\n\r\n");
            late.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut out = Vec::new();
            let _ = late.read_to_end(&mut out);
            assert!(
                out.is_empty(),
                "a connection after shutdown must not be served: {:?}",
                String::from_utf8_lossy(&out)
            );
        }
    }

    #[test]
    fn a_second_shutdown_is_harmless() {
        let (up, _h) = origin();
        let gw = Gateway::start(up, Shaping::default()).unwrap();
        gw.shutdown();
        gw.shutdown();
        assert!(gw.idle());
    }

    #[test]
    fn shaping_describes_itself_precisely() {
        assert!(Shaping::default().describe().contains("none"));
        let s = Shaping {
            latency_ms: 25,
            bandwidth_bytes_per_s: 1_000_000,
        };
        let d = s.describe();
        assert!(d.contains("25 ms"), "{d}");
        assert!(d.contains("50 ms"), "{d}");
        assert!(d.contains("1000000 bytes/s"), "{d}");
    }

    #[test]
    fn counters_are_flattened_with_a_prefix() {
        let mut s = Stats::default();
        s.requests_by_method.insert("PUT".into(), 3);
        s.bytes_client_to_service = 99;
        let c = s.counters("s3_");
        assert_eq!(c.get("s3_requests_PUT"), Some(&3));
        assert_eq!(c.get("s3_requests_total"), Some(&3));
        assert_eq!(c.get("s3_bytes_up"), Some(&99));
        assert!(!c.contains_key("s3_parse_desync"));
    }
}
