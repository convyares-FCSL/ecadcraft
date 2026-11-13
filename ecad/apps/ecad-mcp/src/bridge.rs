//! The client side of the live bridge (brief 035, design §9): one `ecad-mcp --connect PORT` is
//! headless until an ECAD window answers on the port, and bridged once one does.
//!
//! # Semantics
//! - [`Mode::Undecided`] at start. Each call while undecided probes `127.0.0.1:PORT` (a refused
//!   loopback connect returns at once, else 300 ms). It answers → `ecad.hello` → [`Mode::Bridged`],
//!   which sticks. Nothing → the call runs headless; headless sticks once it records a transaction
//!   or saves ([`Link::stick_headless`]).
//! - Every `ecad.*` request carries the session id (16 hex digits, chosen here) and the agent's
//!   name, so authorship never depends on the connection.
//! - Upstream's [`Remote`] carries the requests: the token, reply caps, reconnect and the
//!   `not-run` / `may-have-run` states. Its wording is turned into ECAD's
//!   ([`reword`]).
//!
//! Excluded: the window's side (the trial's `agent` module), and launching (see
//! [`crate::launch`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use cadcraft_mcp::{Backend, Remote};
use serde_json::{Value, json};

/// The port the trial listens on by default (design 035 §17, 1).
pub const DEFAULT_PORT: u16 = 47823;
/// How long the probe waits for the window to accept.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(300);
/// How long `ecad.bye` may take when the server stops.
pub const BYE_TIMEOUT: Duration = Duration::from_secs(1);

/// The upstream control methods the window answers over the bridge besides `ecad.*` (design
/// 035 §4.3); everything else is refused there without running.
pub const ALLOWED_METHODS: &[&str] = &["drawing.inspect", "engine.commands", "ui.render", "ui.screenshot", "ping", "control.methods"];

/// Where the server's calls go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Not known yet: the next call probes.
    Undecided,
    /// Headless; `stuck` once it recorded a transaction or saved (then it never probes again).
    Headless { stuck: bool },
    /// The ECAD window; sticks.
    Bridged,
}

impl Mode {
    /// The mode's word in `ecad_connection`.
    pub fn word(self) -> &'static str {
        match self {
            Mode::Undecided => "undecided",
            Mode::Headless { .. } => "headless",
            Mode::Bridged => "bridged",
        }
    }
}

/// The address of the window's port.
pub fn addr(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

/// Whether something accepts connections on `127.0.0.1:port` now.
pub fn probe(port: u16) -> bool {
    let sa = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&sa, PROBE_TIMEOUT).is_ok()
}

/// Said when the window went away after the server bridged to it.
pub fn not_answering(port: u16) -> String {
    format!("The ECAD window at {} is not answering (closed?). Reopen ECAD with agent access on; nothing was changed.", addr(port))
}

/// Upstream's [`Remote`] wording turned into ECAD's: `CADCraft at <addr>` becomes `The ECAD window
/// at <addr>`, a refusal the window says did not run loses upstream's resend note (the window's
/// own text already says nothing was changed), and a lost reply adds `check ecad_history`.
pub fn reword(method: &str, e: &str) -> String {
    let not_run = format!(": CADCraft did not start `{method}` and won't; it is safe to send again");
    let mut e = e.strip_suffix(&not_run).map_or_else(|| e.to_string(), str::to_string);
    e = e.replace("CADCraft at ", "The ECAD window at ");
    if e.contains("may or may not have been applied") {
        e.push_str("; check ecad_history");
    }
    e
}

/// A fresh bridge session id: 16 hex digits.
pub fn new_session_id() -> String {
    format!("{:016x}", ecad_cadcraft::session_seed())
}

/// One server's connection to the window.
pub struct Link {
    pub port: u16,
    /// Open the trial when nothing answers (`--launch`).
    pub launch: bool,
    pub mode: Mode,
    /// The session id sent with every `ecad.*` request.
    pub session: String,
    remote: Option<Remote>,
    /// The window's last `ecad.hello` reply.
    pub hello: Option<Value>,
}

/// Said when the window does not know the session (it restarted): the client says hello again.
pub const SAY_HELLO: &str = "say ecad.hello first";

impl Link {
    /// A link to `port`, undecided.
    pub fn new(port: u16, launch: bool) -> Self {
        Link { port, launch, mode: Mode::Undecided, session: new_session_id(), remote: None, hello: None }
    }

    /// Headless has recorded something: it never bridges from now on.
    pub fn stick_headless(&mut self) {
        if matches!(self.mode, Mode::Headless { .. } | Mode::Undecided) {
            self.mode = Mode::Headless { stuck: true };
        }
    }

    /// Decide where the next call goes: probe while undecided (or headless and not stuck), and
    /// bridge when the window answers. `Ok(true)` when bridged.
    ///
    /// # Errors
    /// The window answered but `ecad.hello` failed (the error in words); the mode is unchanged.
    pub fn decide(&mut self, name: &str, write: bool) -> Result<bool, String> {
        match self.mode {
            Mode::Bridged => return Ok(true),
            Mode::Headless { stuck: true } => return Ok(false),
            Mode::Undecided | Mode::Headless { stuck: false } => {}
        }
        if !probe(self.port) {
            if self.launch && self.mode == Mode::Undecided {
                crate::launch::launch_and_wait(self.port)?;
            } else {
                self.mode = Mode::Headless { stuck: false };
                return Ok(false);
            }
        }
        self.remote = Some(Remote::lazy(&addr(self.port), false));
        self.say_hello(name, write)?;
        self.mode = Mode::Bridged;
        Ok(true)
    }

    fn raw(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let port = self.port;
        let r = self.remote.get_or_insert_with(|| Remote::lazy(&addr(port), false));
        r.call(method, params).map_err(|e| {
            if e.contains("not answering") || e.contains("connection refused") || e.contains("actively refused") {
                not_answering(port)
            } else {
                reword(method, &e)
            }
        })
    }

    /// `ecad.hello`.
    ///
    /// # Errors
    /// The window's refusal or a transport error.
    pub fn say_hello(&mut self, name: &str, write: bool) -> Result<Value, String> {
        let p = json!({"session": self.session, "name": name, "write": write, "client_version": env!("CARGO_PKG_VERSION")});
        let v = self.raw("ecad.hello", p)?;
        self.hello = Some(v.clone());
        Ok(v)
    }

    /// An `ecad.*` request with the session and name; a window that forgot the session (it
    /// restarted) gets a new hello and the request once more (it did not run).
    fn ecad(&mut self, method: &str, mut params: Value, name: &str, write: bool) -> Result<Value, String> {
        if let Some(o) = params.as_object_mut() {
            o.insert("session".into(), json!(self.session));
            o.insert("name".into(), json!(name));
        }
        match self.raw(method, params.clone()) {
            Err(e) if e.contains(SAY_HELLO) => {
                self.say_hello(name, write)?;
                self.raw(method, params)
            }
            r => r,
        }
    }

    /// A control method of the server, bridged: `engine.execute` becomes `ecad.execute`, the
    /// allowlisted reads go as they are, anything else is refused here.
    ///
    /// # Errors
    /// The refusal or the window's error, in words.
    pub fn call(&mut self, method: &str, params: Value, name: &str, write: bool, line: u64) -> Result<Value, String> {
        match method {
            "engine.execute" | "command" => {
                let command = params.get("command").cloned().unwrap_or(Value::Null);
                let p = params.get("params").cloned().unwrap_or_else(|| json!({}));
                self.ecad("ecad.execute", json!({"line": line, "command": command, "params": p}), name, write)
            }
            m if ALLOWED_METHODS.contains(&m) => {
                if matches!(m, "ui.render" | "ui.screenshot") && params.get("path").is_some_and(|v| !v.is_null()) {
                    return Err("Refused: the image comes back in the reply; agents cannot write files. Nothing was written.".into());
                }
                self.raw(m, params)
            }
            other => Err(not_offered(other)),
        }
    }

    /// An ECAD tool, bridged (`ecad.tool`). `Ok(Ok(result))` is the tool's MCP result,
    /// `Ok(Err(msg))` a protocol error (invalid arguments).
    ///
    /// # Errors
    /// The transport or the window refused the call (an `isError` result for the agent).
    pub fn tool(&mut self, tool: &str, arguments: Value, name: &str, write: bool, line: u64) -> Result<Result<Value, String>, String> {
        let v = self.ecad("ecad.tool", json!({"line": line, "tool": tool, "arguments": arguments}), name, write)?;
        if let Some(e) = v.get("protocolError").and_then(Value::as_str) {
            return Ok(Err(e.to_string()));
        }
        Ok(Ok(v))
    }

    /// `ecad.bye`, best effort within [`BYE_TIMEOUT`] (on a fresh connection, so a hung one cannot
    /// hold the server's exit).
    pub fn bye(&mut self) {
        if self.mode != Mode::Bridged {
            return;
        }
        let Ok(token) = cadcraft_mcp::control_token::read(self.port) else { return };
        let sa = SocketAddr::from(([127, 0, 0, 1], self.port));
        let Ok(mut s) = TcpStream::connect_timeout(&sa, BYE_TIMEOUT) else { return };
        let _ = s.set_read_timeout(Some(BYE_TIMEOUT));
        let _ = s.set_write_timeout(Some(BYE_TIMEOUT));
        let line = json!({"id": 0, "method": "ecad.bye", "params": {"session": self.session}, "token": token});
        if writeln!(s, "{line}").is_ok() {
            let mut reply = String::new();
            let _ = BufReader::new(&s).take(4096).read_line(&mut reply);
        }
    }
}

/// Said when a method is not offered over ECAD's port (also by the window, design 035 §4.3).
pub fn not_offered(method: &str) -> String {
    let why = match method {
        m if m.starts_with("cmdline.") => "ECAD changes go through execute only",
        "app.open" | "app.save" | "app.quit" => "files stay with the person",
        m if m.starts_with("ui.") => "the agent does not use the person's mouse and keyboard",
        m if m.starts_with("engine.") => "ECAD changes go through ecad.execute only",
        _ => "not one of ECAD's methods",
    };
    format!("not offered over ECAD's port: {why}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_words_become_ecads() {
        let e = "busy: CADCraft did not start `ecad.execute` and won't; it is safe to send again";
        assert_eq!(reword("ecad.execute", e), "busy");
        assert_eq!(reword("x", "CADCraft at 127.0.0.1:1: reset"), "The ECAD window at 127.0.0.1:1: reset");
        assert!(reword("x", "`x` may or may not have been applied. Inspect the drawing first").ends_with("check ecad_history"));
        assert_eq!(new_session_id().len(), 16);
        assert!(not_offered("cmdline.input").contains("execute only"));
        assert!(not_offered("ui.click").contains("mouse and keyboard"));
    }
}
