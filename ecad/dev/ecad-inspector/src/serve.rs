//! `ecadinspect serve` (design 029 §5): the page at `GET /` and the protocol over a WebSocket at
//! `/ws`, on one port bound to `127.0.0.1` only.
//!
//! # Semantics
//! - Every request must carry the per-run token in `?t=`, and a `Host` of `127.0.0.1:<port>` or
//!   `localhost:<port>` (DNS rebinding). The upgrade's `Origin`, when present, must be
//!   `http://127.0.0.1:<port>` or `http://localhost:<port>`: browsers do not apply CORS to
//!   WebSockets, so this is what stops another page connecting. No CORS headers are sent.
//! - One page at a time: a second WebSocket is closed with code 1013.
//! - A text frame is one request and gets one reply. `{"op":"open","name","size"}` followed by a
//!   binary frame replaces the source with the dropped bytes (held in memory only); `reload`
//!   re-opens the file named on the command line. The server never takes a path from the page.
//! - Read-only: the file is opened with `Store::open(path, false)`, and no op writes.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::{Message, WebSocket};

use crate::protocol::{self, Request};
use crate::source::{FileSource, Problem, Source, size_text};

/// The page, served at `GET /`.
pub const PAGE: &str = include_str!("../page/index.html");

/// The most bytes of an HTTP request head.
pub const MAX_HEAD: usize = 16 * 1024;
/// The largest text message in (one request).
pub const MAX_TEXT: usize = 1024 * 1024;
/// The largest file dropped on the page (design 029 §6 row 12).
pub const MAX_DROP: u64 = 1 << 30;

/// What the server shares between connections.
struct Shared {
    token: String,
    port: u16,
    path: Option<PathBuf>,
    source: Mutex<Option<Arc<FileSource>>>,
    busy: AtomicBool,
}

impl Shared {
    fn current(&self) -> Option<Arc<FileSource>> {
        self.source.lock().ok().and_then(|g| g.clone())
    }

    fn replace(&self, s: FileSource) -> Arc<FileSource> {
        let s = Arc::new(s);
        if let Ok(mut g) = self.source.lock() {
            *g = Some(s.clone());
        }
        s
    }
}

/// A bound, not yet running, inspector server.
pub struct Server {
    listener: TcpListener,
    shared: Arc<Shared>,
}

/// A random-enough per-run token (design 029 §5.2): 96 bits of SHA-256 over the time, the process
/// id, the port, a stack address and std's randomly keyed hasher. Not cryptographically strong; a
/// local developer tool does not need it to be.
fn token(port: u16) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = Sha256::new();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    h.update(nanos.to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    h.update(port.to_le_bytes());
    let here = 0u8;
    h.update((&here as *const u8 as usize).to_le_bytes());
    let mut keyed = std::collections::hash_map::RandomState::new().build_hasher();
    keyed.write_u128(nanos);
    h.update(keyed.finish().to_le_bytes());
    h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
}

impl Server {
    /// Bind `127.0.0.1:<port>` (0: a free port) and open `file`, if given.
    ///
    /// # Errors
    /// The port could not be bound.
    pub fn bind(file: Option<&Path>, port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
        let port = listener.local_addr()?.port();
        let source = file.map(|p| Arc::new(FileSource::from_path(p)));
        let shared = Arc::new(Shared {
            token: token(port),
            port,
            path: file.map(Path::to_path_buf),
            source: Mutex::new(source),
            busy: AtomicBool::new(false),
        });
        Ok(Self { listener, shared })
    }

    pub fn port(&self) -> u16 {
        self.shared.port
    }

    pub fn token(&self) -> &str {
        &self.shared.token
    }

    /// The address it listens on (always 127.0.0.1).
    ///
    /// # Errors
    /// The socket's address could not be read.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// The address to open.
    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}/?t={}", self.shared.port, self.shared.token)
    }

    /// The source being served (tests).
    pub fn source(&self) -> Option<Arc<FileSource>> {
        self.shared.current()
    }

    /// What `ecadinspect serve` prints.
    pub fn banner(&self) -> String {
        let what = match self.shared.current() {
            Some(s) => {
                let i = s.describe();
                format!("serving {} ({}) read-only", i.name, size_text(i.size))
            }
            None => "serving no file yet (drop one on the page) read-only".into(),
        };
        format!("ecadinspect: {what}\nopen {} (Ctrl+C to stop)", self.address())
    }

    /// Serve until the process stops; each connection on its own thread.
    pub fn run(self) {
        for stream in self.listener.incoming() {
            let Ok(stream) = stream else { continue };
            let shared = self.shared.clone();
            let _ = std::thread::Builder::new().name("ecadinspect-conn".into()).spawn(move || connection(&shared, stream));
        }
    }
}

/// A parsed request head.
struct Head {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn param(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// Read up to the end of the head (`\r\n\r\n`): everything read, and where the head ends.
fn read_head(stream: &mut TcpStream) -> Result<(Vec<u8>, usize), u16> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok((buf, i + 4));
        }
        if buf.len() > MAX_HEAD {
            return Err(431);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(400),
            Ok(n) => buf.extend_from_slice(chunk.get(..n).unwrap_or_default()),
        }
    }
}

fn parse_head(bytes: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target, version) = (first.next()?, first.next()?, first.next()?);
    if !version.starts_with("HTTP/1.") || first.next().is_some() {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let query = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| p.split_once('=').unwrap_or((p, "")))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut headers = Vec::new();
    for l in lines.take_while(|l| !l.is_empty()) {
        let (k, v) = l.split_once(':')?;
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    Some(Head { method: method.to_string(), path: path.to_string(), query, headers })
}

fn respond(stream: &mut TcpStream, code: u16, extra: &[(&str, String)], body: &[u8], content_type: &str) {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn refuse(stream: &mut TcpStream, code: u16, why: &str) {
    respond(stream, code, &[], why.as_bytes(), "text/plain; charset=utf-8");
}

fn host_ok(h: &Head, port: u16) -> bool {
    h.header("host").is_some_and(|v| {
        let v = v.to_ascii_lowercase();
        v == format!("127.0.0.1:{port}") || v == format!("localhost:{port}")
    })
}

fn origin_ok(h: &Head, port: u16) -> bool {
    match h.header("origin") {
        None => true,
        Some(o) => {
            let o = o.to_ascii_lowercase();
            o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}")
        }
    }
}

/// One connection: the page, a refusal, or a WebSocket session.
fn connection(shared: &Shared, mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let (bytes, end) = match read_head(&mut stream) {
        Ok(x) => x,
        Err(code) => return refuse(&mut stream, code, "bad request"),
    };
    let Some(head) = parse_head(bytes.get(..end).unwrap_or_default()) else {
        return refuse(&mut stream, 400, "bad request");
    };
    if !host_ok(&head, shared.port) {
        return refuse(&mut stream, 403, "the Host must be 127.0.0.1 or localhost with this port");
    }
    if head.method != "GET" {
        return refuse(&mut stream, 404, "not found");
    }
    let token_ok = head.param("t") == Some(shared.token.as_str());
    match head.path.as_str() {
        "/" if token_ok => page(shared, &mut stream),
        "/ws" if token_ok => {
            let upgrade = head.header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
            if !upgrade {
                return refuse(&mut stream, 400, "a WebSocket upgrade is expected here");
            }
            if !origin_ok(&head, shared.port) {
                return refuse(&mut stream, 403, "another site's page may not connect");
            }
            let _ = stream.set_read_timeout(None);
            session(shared, Prefixed { pre: std::io::Cursor::new(bytes), inner: stream });
        }
        "/" | "/ws" => refuse(&mut stream, 403, "open the address ecadinspect printed (it carries the token)"),
        _ => refuse(&mut stream, 404, "not found"),
    }
}

/// The page, with a fresh nonce for its script: a Content-Security-Policy that runs only that
/// script, so an injected handler would not run even if a value escaped `esc()`.
fn page(shared: &Shared, stream: &mut TcpStream) {
    let nonce = token(shared.port);
    let body = PAGE.replacen("<script>", &format!("<script nonce=\"{nonce}\">"), 1);
    let port = shared.port;
    let csp = format!(
        "default-src 'none'; script-src 'nonce-{nonce}'; style-src 'unsafe-inline'; img-src data:; connect-src ws://127.0.0.1:{port} ws://localhost:{port}; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
    );
    respond(stream, 200, &[("Content-Security-Policy", csp)], body.as_bytes(), "text/html; charset=utf-8");
}

/// The bytes already read, then the socket.
struct Prefixed {
    pre: std::io::Cursor<Vec<u8>>,
    inner: TcpStream,
}

impl Read for Prefixed {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.pre.read(buf)?;
        if n > 0 { Ok(n) } else { self.inner.read(buf) }
    }
}

impl Write for Prefixed {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Clears `busy` when the session ends, however it ends.
struct Busy<'a>(&'a AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn config() -> WebSocketConfig {
    let max = usize::try_from(MAX_DROP).unwrap_or(usize::MAX).saturating_add(MAX_TEXT);
    WebSocketConfig::default().max_message_size(Some(max)).max_frame_size(Some(max))
}

fn send(ws: &mut WebSocket<Prefixed>, text: String) -> bool {
    ws.send(Message::text(text)).is_ok()
}

fn request(id: Value, op: &str) -> Request {
    Request { id, op: op.to_string(), body: Map::new() }
}

/// The reply to `open` or `reload`: the new source's summary.
fn summary_of(id: Value, s: &FileSource) -> String {
    protocol::reply_to(Some(s as &dyn Source), &request(id, "summary"))
}

fn session(shared: &Shared, stream: Prefixed) {
    let Ok(mut ws) = tungstenite::accept_with_config(stream, Some(config())) else { return };
    if shared.busy.swap(true, Ordering::SeqCst) {
        let frame = CloseFrame { code: CloseCode::Again, reason: "another inspector page is connected; close it first".into() };
        let _ = ws.close(Some(frame));
        // Let the close handshake finish (bounded: the peer may not answer).
        for _ in 0..8 {
            if ws.read().is_err() {
                break;
            }
        }
        return;
    }
    let _busy = Busy(&shared.busy);
    // An `open` waiting for its bytes: (id, name, size).
    let mut pending: Option<(Value, String, u64)> = None;
    loop {
        let msg = match ws.read() {
            Ok(m) => m,
            Err(_) => return,
        };
        let reply = match msg {
            Message::Text(t) => {
                if t.len() > MAX_TEXT {
                    protocol::error(&Value::Null, &Problem::new("too-large", "A request is at most 1 MiB.", "Send a smaller request."))
                } else {
                    match protocol::parse(t.as_str()) {
                        Err(reply) => reply,
                        Ok(req) => match req.op.as_str() {
                            "open" => match open_request(&req) {
                                Ok(p) => {
                                    pending = Some(p);
                                    continue;
                                }
                                Err(p) => protocol::error(&req.id, &p),
                            },
                            "reload" => match &shared.path {
                                Some(path) => summary_of(req.id.clone(), &shared.replace(FileSource::from_path(path))),
                                None => protocol::error(
                                    &req.id,
                                    &Problem::new("bad-request", "Nothing to reload: the file was dropped on the page.", "Drop the file again."),
                                ),
                            },
                            _ => {
                                let src = shared.current();
                                protocol::reply_to(src.as_deref().map(|s| s as &dyn Source), &req)
                            }
                        },
                    }
                }
            }
            Message::Binary(b) => match pending.take() {
                Some((id, name, size)) if b.len() as u64 == size => summary_of(id, &shared.replace(FileSource::from_bytes(&name, b.to_vec()))),
                Some((id, _, size)) => protocol::error(
                    &id,
                    &Problem::new("bad-request", format!("The file sent has {} bytes; `open` announced {size}.", b.len()), "Drop the file again."),
                ),
                None => protocol::error(
                    &Value::Null,
                    &Problem::new("bad-request", "A file must follow an `open` request.", r#"Send {"v":1,"op":"open","name":…,"size":…} first."#),
                ),
            },
            Message::Close(_) => return,
            _ => continue,
        };
        if !send(&mut ws, reply) {
            return;
        }
    }
}

/// `open`'s name and size, checked before the bytes arrive (design 029 §6 row 12).
fn open_request(req: &Request) -> Result<(Value, String, u64), Problem> {
    let name = req.body.get("name").and_then(Value::as_str).filter(|n| !n.is_empty() && n.chars().count() <= 255);
    let size = req.body.get("size").and_then(Value::as_u64);
    let (Some(name), Some(size)) = (name, size) else {
        return Err(Problem::new("bad-request", "`open` needs the file's `name` (at most 255 characters) and `size`.", "Drop the file again."));
    };
    if size > MAX_DROP {
        return Err(Problem::new(
            "too-large",
            format!("{} is over the drop limit of 1 GiB.", size_text(size)),
            "Start the server with the path instead: `ecadinspect serve <file>` (no limit).",
        ));
    }
    // A file name only: never a path the server would read.
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name).to_string();
    Ok((req.id.clone(), name, size))
}
