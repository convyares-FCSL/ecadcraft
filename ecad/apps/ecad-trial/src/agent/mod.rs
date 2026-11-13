//! Live agent access to the trial (brief 035, design §3–§8): an agent drives the running ECAD
//! window over a loopback port while the person watches.
//!
//! - **Preference** ([`ecad_cadcraft::agent_access`]): Off (default), Read only, Read and change,
//!   and a port; `--control PORT` turns it on (Read and change) for one run. When off, nothing
//!   listens.
//! - **Transport**: upstream CADCraft's own control server, included unchanged by path (bounded,
//!   token-checked, loopback only). Its requests never reach `CadApp` directly: the trial's drain
//!   answers ECAD's methods (`ecad.hello`, `ecad.execute`, `ecad.tool`, `ecad.bye`), forwards a
//!   short allowlist of reads to `CadApp`'s own channel, and refuses everything else unrun.
//! - **The gate** ([`ecad_mcp::gate`]): the same policy, caps and roll-back as headless, run on the
//!   person's own session between two frames, with `Author::Agent(<name>)` set for that one call
//!   and the person restored after it, also on refusal, error or panic, and checked again at the
//!   top of every frame.
//! - **The busy rule**: a call that may change the project waits (up to [`AGENT_HOLD`]) while the
//!   person is in the middle of a command, a dialog, a drag or typing, then is refused unrun.
//! - **The banner** ([`banner`]): a data contract the UI redesign places ([`banner::AgentBanner`]);
//!   today only a minimal, unstyled strip draws it.
//! - **Undo session** ([`undo_session`]): the person takes back an agent session's steps.
//!
//! Excluded: several writers at once, proposals outside the log, anything beyond loopback, and
//! the banner's final look (the UI shell).

pub mod banner;
pub mod undo_session;

// Upstream's loopback control server, unchanged (design 035 §17, 2): the same bounds, token and
// replies as CADCraft's own. `#[path]` is relative to this file's folder (`src/agent`).
#[allow(dead_code)]
#[path = "../../../../../apps/cadcraft/src/control_server.rs"]
mod control_server;

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use cadcraft_ui_egui::{ControlRequest, PendingReply};
use ecad_cadcraft::agent_access::{self, Mode, Preference, Status};
use ecad_cadcraft::views::Settings;
use ecad_mcp::gate::{self, Call, OwnedSteps};
use ecad_mcp::{agent_name, bridge, policy, tools};
use eframe::egui;
use serde_json::{Value, json};

use crate::Trial;
pub use banner::{AgentBanner, BannerAction, BannerState, LastChange};

/// How long a call that may change the project waits while the person is busy (design §6.2).
pub const AGENT_HOLD: Duration = Duration::from_secs(5);
/// Most agent requests answered in one frame, so a burst cannot freeze the window (design §5.3).
pub const MAX_AGENT_CALLS_PER_FRAME: usize = 8;
/// A bridge session without a call for this long is finished (its writer role released).
pub const SESSION_IDLE: Duration = Duration::from_secs(600);
/// Most bridge sessions remembered at once; the oldest finished one is forgotten first.
pub const MAX_SESSIONS: usize = 64;
/// How long the banner shows a write after it was accepted.
pub const WRITE_FLASH: Duration = Duration::from_secs(1);
/// Requests waiting in the trial at once (the listener's own queue holds 64 more).
pub const MAX_WAITING: usize = 256;
/// Longest agent name accepted, in bytes, before it is cleaned (larger is refused).
const MAX_NAME_BYTES: usize = 1024;

/// Said when access is off or was turned off.
pub const ACCESS_OFF: &str = "agent access is off in the ECAD window; the person turns it on in ECAD Dev ▸ Agent access";
/// Said for a write under the Read only preference.
pub const READ_ONLY: &str = "Agent access in the ECAD window is Read only.";
/// Said for a write while paused.
pub const PAUSED: &str = "Refused: paused by the designer; nothing was changed.";
/// Said to a session the designer disconnected.
pub const DISCONNECTED: &str = "the designer disconnected this agent; restart the MCP server to connect again";
/// Said when the author could not be reset (access stops for this run).
pub const STOPPED: &str = "agent access stopped in the ECAD window";
/// Logged when access stops because the author could not be reset.
pub const STOPPED_LOG: &str = "Agent access stopped: the author could not be reset.";

/// Said to a write by a session that is not the writer.
pub fn not_writer(writer: &str) -> String {
    format!("another agent ({writer}) is changing this project; this session can read")
}

/// Said after a write was held for [`AGENT_HOLD`].
pub fn held_refusal(said: &str) -> String {
    format!("Refused: {said}; nothing was changed. Try again when they finish.")
}

/// One bridge session (one `ecad-mcp` process), from `ecad.hello` to `ecad.bye` or idle.
#[derive(Debug)]
pub struct BridgeSession {
    pub name: String,
    /// The server allows writes (`--write`).
    pub write: bool,
    pub owned: OwnedSteps,
    pub last_call: Instant,
    /// The request line being answered and what it recorded so far.
    line: u64,
    line_used: usize,
    pub disconnected: bool,
    pub finished: bool,
    /// When the session said hello (the banner shows the latest).
    pub since: Instant,
}

/// A request waiting in the trial: not begun yet, so the client's own wait still applies.
struct Waiting {
    req: ControlRequest,
    since: Instant,
}

/// A request forwarded to `CadApp`'s own channel, waiting for its reply.
struct Forwarded {
    req: ControlRequest,
    pending: PendingReply,
    method: String,
}

/// What the ECAD Dev row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listen {
    Off,
    Listening(u16),
    InUse(u16),
}

/// The trial's agent access state.
pub struct Agent {
    /// The preference in effect (the file's, or `--control`'s for this run).
    pub pref: Preference,
    /// `--control` turned access on for this run (the preference file is not changed).
    pub this_run: bool,
    settings: Settings,
    rx: Option<Receiver<ControlRequest>>,
    /// The port the listener was started on (it keeps it until the trial exits).
    listen_port: Option<u16>,
    bind_failed: bool,
    /// Into `CadApp`'s own control channel (the allowlisted reads only).
    cad_tx: Option<SyncSender<ControlRequest>>,
    waiting: VecDeque<Waiting>,
    forwarded: Vec<Forwarded>,
    pub sessions: BTreeMap<String, BridgeSession>,
    pub writer: Option<String>,
    pub paused: bool,
    /// Access stopped for this run (the author could not be reset).
    pub stopped: Option<String>,
    pub banner: Option<AgentBanner>,
    /// How many times the frame check found the author not reset (tests read it; 0 normally).
    pub frame_fixes: u64,
    /// The Port field's text, the field's warning, and the restart note.
    pub port_text: String,
    pub port_warning: bool,
    pub restart_note: bool,
    /// Undo session's confirmation, when open.
    pub undo_confirm: Option<undo_session::Confirm>,
    /// The (branch, log length) last seen, to recompute the banner's steps only after an undo,
    /// redo or branch switch.
    seen_log: Option<(String, usize)>,
    /// A warning about the preference file, said once.
    pending_warning: Option<&'static str>,
}

impl Agent {
    /// The preference read from the user's settings folder (Off when missing or unreadable).
    pub fn from_settings(settings: Settings) -> Self {
        let loaded = agent_access::load(&settings);
        Agent {
            pref: loaded.preference,
            this_run: false,
            settings,
            rx: None,
            listen_port: None,
            bind_failed: false,
            cad_tx: None,
            waiting: VecDeque::new(),
            forwarded: Vec::new(),
            sessions: BTreeMap::new(),
            writer: None,
            paused: false,
            stopped: None,
            banner: None,
            frame_fixes: 0,
            port_text: loaded.preference.port.to_string(),
            port_warning: false,
            restart_note: false,
            undo_confirm: None,
            seen_log: None,
            pending_warning: loaded.warning,
        }
    }

    /// Give `CadApp`'s channel's sender (the allowlisted reads go there).
    pub fn set_cad_channel(&mut self, tx: SyncSender<ControlRequest>) {
        self.cad_tx = Some(tx);
    }

    /// `--control PORT`: on, Read and change, on `port`, for this run only.
    pub fn control_flag(&mut self, port: u16) {
        self.pref = Preference { mode: Mode::Write, port };
        self.this_run = true;
        self.port_text = port.to_string();
    }

    /// Whether access is on now.
    pub fn on(&self) -> bool {
        self.pref.mode != Mode::Off && self.stopped.is_none()
    }

    /// What the ECAD Dev row's status says.
    pub fn listen(&self) -> Listen {
        match self.listen_port {
            Some(p) if self.bind_failed => Listen::InUse(p),
            Some(p) if self.on() => Listen::Listening(p),
            _ => Listen::Off,
        }
    }

    /// The status text (design §3.2).
    pub fn status_text(&self) -> String {
        if let Some(why) = &self.stopped {
            return format!("● {why}");
        }
        match self.listen() {
            Listen::Off => "Off".into(),
            Listen::InUse(p) => format!("● Port {p} is in use by another program; agent access is off in this window"),
            Listen::Listening(p) if self.this_run => format!("● Listening on 127.0.0.1:{p} (this run only, --control)"),
            Listen::Listening(p) => format!("● Listening on 127.0.0.1:{p}"),
        }
    }

    fn status(&self) -> Status {
        Status { preference: self.pref, listening: matches!(self.listen(), Listen::Listening(_)), text: self.status_text(), this_run: self.this_run }
    }

    /// The person chose a mode in ECAD Dev (or `agentaccess {mode}`). Saved unless `--control`.
    pub fn set_mode(&mut self, mode: Mode) -> Result<(), String> {
        if self.this_run {
            return Err("Set by --control for this run.".into());
        }
        self.pref.mode = mode;
        if mode == Mode::Off {
            self.end_all_sessions();
        }
        agent_access::store(&self.settings, &self.pref)
    }

    /// The person typed a port (applied on Enter or focus loss). Saved; used at the next start
    /// when the listener is already running on another port.
    pub fn set_port_text(&mut self) -> Result<(), String> {
        let Some(port) = agent_access::parse_port(&self.port_text) else {
            self.port_warning = true;
            self.port_text = self.pref.port.to_string();
            return Ok(());
        };
        self.port_warning = false;
        if port == self.pref.port || self.this_run {
            return Ok(());
        }
        self.pref.port = port;
        self.restart_note = self.listen_port.is_some_and(|p| p != port);
        agent_access::store(&self.settings, &self.pref)
    }

    /// Access turned off (or stopped): every bridge session ends, the writer is released and the
    /// banner goes.
    fn end_all_sessions(&mut self) {
        for s in self.sessions.values_mut() {
            s.finished = true;
        }
        self.writer = None;
        self.banner = None;
        self.undo_confirm = None;
    }

    /// Start the listener the first time access is on (upstream's server has no stop: it keeps
    /// its socket until the trial exits, answering nothing while access is off).
    fn ensure_listener(&mut self, ctx: &egui::Context) {
        if self.rx.is_some() || !self.on() {
            return;
        }
        let port = self.pref.port;
        self.rx = Some(control_server::start(port, ctx.clone()));
        self.listen_port = Some(port);
    }

    /// Revoke the token file (the trial is exiting).
    pub fn shut_down(&mut self) {
        if let (Some(port), false) = (self.listen_port, self.bind_failed) {
            cadcraft_mcp::control_token::revoke(port);
        }
    }

    /// Take what arrived from the listener into the waiting queue (bounded).
    fn take_arrivals(&mut self) {
        let Some(rx) = &self.rx else { return };
        loop {
            match rx.try_recv() {
                Ok(req) => {
                    if self.waiting.len() >= MAX_WAITING {
                        if req.begin() {
                            let _ = req.reply.send(not_run("busy: too many waiting agent requests"));
                        }
                        continue;
                    }
                    self.waiting.push_back(Waiting { req, since: Instant::now() });
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The listener could not bind (or issue its token): nothing will arrive.
                    self.bind_failed = true;
                    break;
                }
            }
        }
    }

    /// Answer forwarded reads whose reply came back from `CadApp`.
    fn finish_forwarded(&mut self) {
        let mut i = 0;
        while i < self.forwarded.len() {
            let Some(f) = self.forwarded.get(i) else { break };
            match f.pending.try_reply() {
                None => i += 1,
                Some(v) => {
                    let f = self.forwarded.swap_remove(i);
                    let v = if f.method == "ui.screenshot" { inline_screenshot(v) } else { v };
                    let _ = f.req.reply.send(v);
                }
            }
        }
    }

    /// Sessions idle for [`SESSION_IDLE`] are finished; the oldest finished ones are forgotten.
    fn expire_sessions(&mut self) {
        let now = Instant::now();
        let mut finished_writer = false;
        for (id, s) in &mut self.sessions {
            if !s.finished && now.duration_since(s.last_call) >= SESSION_IDLE {
                s.finished = true;
                finished_writer |= self.writer.as_deref() == Some(id.as_str());
            }
        }
        if finished_writer {
            self.writer = None;
        }
        if let Some(b) = &mut self.banner
            && self.sessions.get(&b.session).is_some_and(|s| s.finished)
            && !matches!(b.state, BannerState::Finished | BannerState::Error)
        {
            b.state = BannerState::Finished;
        }
        while self.sessions.len() > MAX_SESSIONS {
            let oldest = self.sessions.iter().filter(|(_, s)| s.finished).min_by_key(|(_, s)| s.last_call).map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    self.sessions.remove(&k);
                }
                None => break,
            }
        }
    }

    /// Whether the session may write now, or the refusal said instead of `WRITES_OFF` (design
    /// §5.4: Read only, no `--write` (the gate's own text), not the writer, paused). Takes the
    /// writer role when it is free.
    fn write_gate(&mut self, session: &str) -> (bool, Option<String>) {
        if self.pref.mode != Mode::Write {
            return (false, Some(READ_ONLY.into()));
        }
        let Some(s) = self.sessions.get(session) else { return (false, None) };
        if !s.write {
            return (false, None);
        }
        match &self.writer {
            Some(w) if w != session => {
                let name = self.sessions.get(w).map_or_else(|| w.clone(), |o| o.name.clone());
                return (false, Some(not_writer(&name)));
            }
            Some(_) => {}
            None => self.writer = Some(session.to_string()),
        }
        if self.paused {
            return (false, Some(PAUSED.into()));
        }
        (true, None)
    }

    /// Other connected sessions than the banner's.
    fn others(&self, session: &str) -> usize {
        self.sessions.iter().filter(|(k, s)| k.as_str() != session && !s.finished && !s.disconnected).count()
    }
}

/// A refusal the client may send again: nothing ran.
fn not_run(e: &str) -> Value {
    json!({"ok": false, "error": e, "state": "not-run"})
}

fn ok(v: Value) -> Value {
    json!({"ok": true, "result": v})
}

fn failed(e: &str) -> Value {
    json!({"ok": false, "error": e})
}

/// A pathless screenshot's reply (a private file upstream) turned into the PNG inline, as
/// `ui.render` replies; the file is removed.
fn inline_screenshot(v: Value) -> Value {
    let Some(path) = v.pointer("/result/path").and_then(Value::as_str).map(str::to_string) else { return v };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return failed(&format!("the screenshot could not be read back: {e}")),
    };
    let _ = std::fs::remove_file(&path);
    let w = v.pointer("/result/width").cloned().unwrap_or(Value::Null);
    let h = v.pointer("/result/height").cloned().unwrap_or(Value::Null);
    ok(json!({"pngBase64": base64(&bytes), "width": w, "height": h, "bytes": bytes.len()}))
}

/// Standard base64 with padding.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3).saturating_mul(4));
    for c in data.chunks(3) {
        let b = [c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= c.len() {
                out.push(char::from(T.get(((n >> shift) & 63) as usize).copied().unwrap_or(b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// What `control.methods` answers over ECAD's port (design §4.2, §4.3).
fn methods() -> Value {
    json!([
        {"method": "ecad.hello", "params": "{session, name, write, client_version}", "description": "Start a bridge session."},
        {"method": "ecad.execute", "params": "{session, name, line, command, params}", "description": "One command through ECAD's gate."},
        {"method": "ecad.tool", "params": "{session, name, line, tool, arguments}", "description": "ecad_project, ecad_history, ecad_inspect or save."},
        {"method": "ecad.bye", "params": "{session}", "description": "End the bridge session."},
        {"method": "drawing.inspect", "params": "{entities?, limit?}", "description": "The active page's drawing."},
        {"method": "engine.commands", "params": "", "description": "Every command."},
        {"method": "ui.render", "params": "{width?, height?, fit?, window?, background?}", "description": "Render the active page (pngBase64)."},
        {"method": "ui.screenshot", "params": "", "description": "PNG of the window, inline."},
        {"method": "ping", "params": "", "description": "pong"},
        {"method": "control.methods", "params": "", "description": "This list."},
    ])
}

/// A request's kind for the busy rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Answered at once.
    Read,
    /// Held while the person is busy.
    Write,
}

fn kind(method: &str, params: &Value) -> Kind {
    match method {
        "ecad.execute" => Kind::Write,
        "ecad.tool" if params.get("tool").and_then(Value::as_str).is_some_and(tools::is_write_tool) => Kind::Write,
        _ => Kind::Read,
    }
}

fn session_of(params: &Value) -> Option<String> {
    params.get("session").and_then(Value::as_str).filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_ascii_lowercase)
}

impl Trial {
    /// Brief 035, at the top of every frame before any input of the frame is handled: the author
    /// check, the preference's requests, the listener, and the agent requests that may be
    /// answered now.
    pub fn agent_frame(&mut self, ctx: &egui::Context) {
        self.agent_author_check();
        self.agent_apply_request();
        if let Some(w) = self.agent.pending_warning.take() {
            self.cad.session.echo(w.to_string());
        }
        self.agent.ensure_listener(ctx);
        self.agent.take_arrivals();
        self.agent.finish_forwarded();
        self.agent.expire_sessions();
        self.agent_sync_status();
        self.agent_steps_after_change();
        let busy = self.agent_busy(ctx);
        let mut answered = 0usize;
        let mut kept: VecDeque<Waiting> = VecDeque::new();
        // Sessions with a request held this frame: their later requests keep their order.
        let mut blocked: Vec<String> = Vec::new();
        let mut held_by: Option<String> = None;
        while let Some(w) = self.agent.waiting.pop_front() {
            if answered >= MAX_AGENT_CALLS_PER_FRAME {
                kept.push_back(w);
                continue;
            }
            let session = session_of(&w.req.params);
            if session.as_ref().is_some_and(|s| blocked.contains(s)) {
                kept.push_back(w);
                continue;
            }
            let k = kind(&w.req.method, &w.req.params);
            if k == Kind::Write
                && self.agent.on()
                && let Some(said) = &busy
            {
                if w.since.elapsed() < AGENT_HOLD {
                    if let Some(s) = session {
                        blocked.push(s);
                    }
                    held_by = Some(said.clone());
                    kept.push_back(w);
                    continue;
                }
                if w.req.begin() {
                    let why = held_refusal(said);
                    self.agent_refused(session.as_deref(), &why);
                    let _ = w.req.reply.send(not_run(&why));
                }
                answered += 1;
                continue;
            }
            if !w.req.begin() {
                // The client gave up on it and was told it did not run.
                continue;
            }
            let reply = self.agent_answer(w.req);
            if let Some((req, v)) = reply {
                let _ = req.reply.send(v);
            }
            answered += 1;
        }
        self.agent.waiting = kept;
        if let Some(b) = &mut self.agent.banner {
            b.held = held_by;
        }
        if !self.agent.waiting.is_empty() || !self.agent.forwarded.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.agent.banner.as_ref().is_some_and(|b| b.flash_until.is_some_and(|t| t > Instant::now())) {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }

    /// The frame invariant (design §5.2): the person is the author and no budget is set.
    fn agent_author_check(&mut self) {
        let Some(p) = self.project.clone() else { return };
        if !p.is_attached(&self.cad.session) {
            return;
        }
        if gate::author_is_user(&mut self.cad.session, &p) == Some(false) {
            self.agent.frame_fixes = self.agent.frame_fixes.saturating_add(1);
            if gate::restore_user(&mut self.cad.session, &p) {
                self.cad.session.echo("Agent access: the author was found set to the agent between frames and was reset to you.".to_string());
            } else {
                self.agent_stop();
            }
        }
    }

    /// Access stops for this run: the author could not be reset.
    fn agent_stop(&mut self) {
        self.agent.stopped = Some("agent access stopped: the author could not be reset".into());
        self.cad.session.echo(STOPPED_LOG.to_string());
        let sessions: Vec<String> = self.agent.sessions.keys().cloned().collect();
        for k in sessions {
            if let Some(s) = self.agent.sessions.get_mut(&k) {
                s.finished = true;
            }
        }
        self.agent.writer = None;
        if let Some(b) = &mut self.agent.banner {
            b.state = BannerState::Error;
            b.error = Some("the author could not be reset".into());
        }
    }

    /// Apply what `agentaccess {mode?, port?}` asked for.
    fn agent_apply_request(&mut self) {
        let Some(p) = self.project.clone() else { return };
        if !p.is_attached(&self.cad.session) {
            return;
        }
        let Ok(Some(want)) = p.configure(&mut self.cad.session, |p| p.agent_access_request.take()) else { return };
        if want.port != self.agent.pref.port {
            self.agent.port_text = want.port.to_string();
            if let Err(e) = self.agent.set_port_text() {
                self.cad.session.echo(format!("Agent access: the port could not be saved: {e}"));
            }
        }
        if want.mode != self.agent.pref.mode
            && let Err(e) = self.agent.set_mode(want.mode)
        {
            self.cad.session.echo(format!("Agent access: {e}"));
        }
    }

    /// Tell the project what `agentaccess` reports (only when it changed).
    fn agent_sync_status(&mut self) {
        let Some(p) = self.project.clone() else { return };
        if !p.is_attached(&self.cad.session) {
            return;
        }
        let st = self.agent.status();
        let _ = p.configure(&mut self.cad.session, |p| {
            if p.agent_access_status.as_ref() != Some(&st) {
                p.agent_access_status = Some(st);
            }
        });
    }

    /// The person is busy, and in what words (design §6.2), or `None`.
    pub fn agent_busy(&self, ctx: &egui::Context) -> Option<String> {
        let s = &self.cad.session;
        if let Some(r) = s.running.as_ref().or(s.suspended.as_ref()) {
            return Some(format!("the designer is in the middle of {}", r.id.to_ascii_uppercase()));
        }
        if let Some(title) = self.open_dialog_title() {
            return Some(format!("the designer has the {title} dialog open"));
        }
        let dragging =
            ctx.input(|i| i.pointer.any_down() && i.pointer.interact_pos().is_some_and(|at| self.cad.canvas.rect.is_some_and(|r| r.contains(at))));
        if dragging {
            return Some("the designer is dragging on the drawing".into());
        }
        if !self.command.trim().is_empty() || !self.cad.cmd.buffer.trim().is_empty() {
            return Some("the designer is typing a command".into());
        }
        None
    }

    /// The title of a trial dialog that is open, if any.
    fn open_dialog_title(&self) -> Option<&'static str> {
        if let Some(d) = &self.dialog {
            return Some(match d.kind {
                crate::DialogKind::Open => "Open",
                crate::DialogKind::SaveAs => "Save As",
            });
        }
        if self.held.is_some() || self.cad.closing.is_some() || self.cad.save_loss.is_some() {
            return Some("Save changes");
        }
        if self.merge.is_some() {
            return Some("Merge");
        }
        if self.renumber.is_some() {
            return Some("Renumber");
        }
        if self.release.is_some() {
            return Some("Release");
        }
        if self.project_props.is_some() {
            return Some("Project Properties");
        }
        if self.name_box.is_some() {
            return Some("Page name");
        }
        if self.issue_prompt.is_some() {
            return Some("Issue revision");
        }
        if self.convert_offer.is_some() {
            return Some("Convert");
        }
        if self.agent.undo_confirm.is_some() {
            return Some("Undo agent session");
        }
        None
    }

    /// Answer one begun request. Returns the request and its reply, or `None` when it was
    /// forwarded to `CadApp` (answered later).
    fn agent_answer(&mut self, req: ControlRequest) -> Option<(ControlRequest, Value)> {
        let method = req.method.clone();
        let params = req.params.clone();
        if !self.agent.on() {
            let why = if self.agent.stopped.is_some() { STOPPED } else { ACCESS_OFF };
            return Some((req, not_run(why)));
        }
        let v = match method.as_str() {
            "ecad.hello" => self.agent_hello(&params),
            "ecad.execute" | "ecad.tool" => self.agent_call(&method, &params),
            "ecad.bye" => self.agent_bye(&params),
            "control.methods" => ok(methods()),
            "ping" => ok(json!("pong")),
            m if bridge::ALLOWED_METHODS.contains(&m) => {
                if matches!(m, "ui.render" | "ui.screenshot") && params.get("path").is_some_and(|v| !v.is_null()) {
                    not_run("not offered over ECAD's port: the image comes back in the reply; agents cannot write files")
                } else {
                    return self.agent_forward(&method, params, req);
                }
            }
            other => not_run(&bridge::not_offered(other)),
        };
        Some((req, v))
    }

    /// Forward an allowlisted read to `CadApp`'s own channel.
    fn agent_forward(&mut self, method: &str, params: Value, req: ControlRequest) -> Option<(ControlRequest, Value)> {
        let Some(tx) = &self.agent.cad_tx else { return Some((req, not_run("the window cannot answer this just now"))) };
        let (fwd, pending) = ControlRequest::new(method, params);
        match tx.try_send(fwd) {
            Ok(()) => {
                self.agent.forwarded.push(Forwarded { req, pending, method: method.to_string() });
                None
            }
            Err(_) => Some((req, not_run("busy: too many queued requests"))),
        }
    }

    /// `ecad.hello {session, name, write, client_version}`.
    fn agent_hello(&mut self, params: &Value) -> Value {
        let Some(session) = session_of(params) else { return not_run("`session` must be 16 hex digits") };
        let given = params.get("name").and_then(Value::as_str).unwrap_or("");
        if given.len() > MAX_NAME_BYTES {
            return not_run("the agent name is too long");
        }
        let name = agent_name(given);
        let write = params.get("write").and_then(Value::as_bool).unwrap_or(false);
        let now = Instant::now();
        let known = self.agent.sessions.get(&session).map(|s| (s.disconnected, s.name.clone()));
        match known {
            Some((true, _)) => return not_run(DISCONNECTED),
            Some((false, n)) if n != name => return not_run("this session said hello with another name"),
            Some(_) => {
                if let Some(s) = self.agent.sessions.get_mut(&session) {
                    s.write = write;
                    s.finished = false;
                    s.last_call = now;
                }
            }
            None => {
                self.agent.sessions.insert(
                    session.clone(),
                    BridgeSession {
                        name: name.clone(),
                        write,
                        owned: OwnedSteps::default(),
                        last_call: now,
                        line: 0,
                        line_used: 0,
                        disconnected: false,
                        finished: false,
                        since: now,
                    },
                );
            }
        }
        // The first session whose hello may write, while access is Read and change, becomes the
        // writer (design §5.5).
        let (writes, why) = match self.agent.write_gate(&session) {
            (true, _) => (true, None),
            (false, Some(why)) => (false, Some(why)),
            (false, None) => (false, Some(policy::WRITES_OFF.to_string())),
        };
        let (project, file) = self.agent_project_facts();
        // The banner shows the writer, else the newest session.
        let replace = match &self.agent.banner {
            None => true,
            Some(b) => {
                b.session != session
                    && (matches!(b.state, BannerState::Finished | BannerState::Error) || self.agent.writer.as_deref() != Some(b.session.as_str()))
            }
        };
        if replace {
            self.agent.banner = Some(AgentBanner::new(&session, &name));
        }
        if let Some(b) = &mut self.agent.banner
            && b.session == session
        {
            b.state = if self.agent.paused { BannerState::Paused } else { BannerState::Connected };
            b.writes_off = why.clone();
        }
        self.agent_refresh_others();
        ok(json!({
            "writes": writes,
            "why": why,
            "project": {"name": project, "file": file},
            "writer": self.agent.writer.as_deref() == Some(session.as_str()),
            "app_version": env!("CARGO_PKG_VERSION"),
        }))
    }

    fn agent_refresh_others(&mut self) {
        if let Some(session) = self.agent.banner.as_ref().map(|b| b.session.clone()) {
            let n = self.agent.others(&session);
            if let Some(b) = &mut self.agent.banner {
                b.others = n;
            }
        }
    }

    fn agent_project_facts(&mut self) -> (String, Option<String>) {
        let Some(p) = self.project.clone() else { return (String::new(), None) };
        p.read(&mut self.cad.session, |p, _| (p.name.clone(), p.file_status().path.clone())).unwrap_or_default()
    }

    /// `ecad.bye {session}`.
    fn agent_bye(&mut self, params: &Value) -> Value {
        let Some(session) = session_of(params) else { return ok(json!({})) };
        if let Some(s) = self.agent.sessions.get_mut(&session) {
            s.finished = true;
        }
        if self.agent.writer.as_deref() == Some(session.as_str()) {
            self.agent.writer = None;
        }
        if let Some(b) = &mut self.agent.banner
            && b.session == session
        {
            b.state = BannerState::Finished;
        }
        self.agent_refresh_others();
        ok(json!({}))
    }

    /// The session's checks for `ecad.execute` and `ecad.tool`: known, same name, not
    /// disconnected. `Err` is the reply.
    fn agent_session_check(&mut self, params: &Value) -> Result<(String, String), Value> {
        let Some(session) = session_of(params) else { return Err(not_run("`session` must be 16 hex digits")) };
        let Some(s) = self.agent.sessions.get_mut(&session) else { return Err(not_run(bridge::SAY_HELLO)) };
        if s.disconnected {
            return Err(not_run(DISCONNECTED));
        }
        let given = params.get("name").and_then(Value::as_str).unwrap_or("");
        if given.len() > MAX_NAME_BYTES || agent_name(given) != s.name {
            return Err(not_run("this call's agent name is not the one this session said hello with"));
        }
        s.last_call = Instant::now();
        s.finished = false;
        let line = params.get("line").and_then(Value::as_u64).unwrap_or(0);
        if line != s.line {
            s.line = line;
            s.line_used = 0;
        }
        Ok((session, s.name.clone()))
    }

    /// `ecad.execute` and `ecad.tool` through the gate, as the agent, for this call only.
    fn agent_call(&mut self, method: &str, params: &Value) -> Value {
        let (session, name) = match self.agent_session_check(params) {
            Ok(x) => x,
            Err(v) => return v,
        };
        let Some(p) = self.project.clone() else { return failed("no ECAD project is open") };
        let (write, refusal) = self.agent.write_gate(&session);
        let Some(bs) = self.agent.sessions.get_mut(&session) else { return not_run(bridge::SAY_HELLO) };
        let mut line_used = bs.line_used;
        let mut owned = std::mem::take(&mut bs.owned);
        let tool = (method == "ecad.tool").then(|| params.get("tool").and_then(Value::as_str).unwrap_or("").to_string());
        let args = params.get("arguments").cloned().unwrap_or(Value::Null);
        let scoped = gate::as_agent(&mut self.cad.session, &p, &name, |s| {
            let call = Call { write, refusal: refusal.as_deref(), line_transactions: &mut line_used, owned: Some(&mut owned) };
            match &tool {
                Some(t) if ["ecad_project", "ecad_history", "ecad_inspect", "save"].contains(&t.as_str()) => {
                    let mut host = tools::Host { session: s, project: &p, call, agent: &name };
                    match tools::call(&mut host, t, &args) {
                        Ok(v) => Ok(v),
                        Err(m) => Ok(json!({"protocolError": m})),
                    }
                }
                Some(t) => Ok(json!({"protocolError": ecad_mcp::not_offered(t)})),
                None => {
                    let mut call = call;
                    gate::execute(s, &p, &mut call, params)
                }
            }
        });
        if let Some(bs) = self.agent.sessions.get_mut(&session) {
            bs.line_used = line_used;
            bs.owned = owned;
        }
        let r = match scoped {
            Err(e) => Err(e),
            Ok(sc) => {
                if !sc.restored {
                    self.agent_stop();
                }
                sc.result
                    .unwrap_or_else(|| Err("Refused: the call failed unexpectedly inside ECAD (the failure was caught); check ecad_history.".into()))
            }
        };
        let what = tool.clone().unwrap_or_else(|| params.get("command").and_then(Value::as_str).unwrap_or("").to_ascii_uppercase());
        match r {
            Ok(v) => {
                let refused_tool = v.get("isError").and_then(Value::as_bool) == Some(true);
                if refused_tool {
                    let text = v.pointer("/content/0/text").and_then(Value::as_str).unwrap_or("refused").to_string();
                    self.agent_refused(Some(&session), &text);
                    self.agent_echo(&name, &what, None, Some(&text));
                } else {
                    let last = v.get("ecadTransactions").and_then(Value::as_array).and_then(|a| a.last()).cloned();
                    self.agent_accepted(&session, last.as_ref());
                    self.agent_echo(&name, &what, last.as_ref(), None);
                }
                ok(v)
            }
            Err(e) => {
                self.agent_refused(Some(&session), &e);
                self.agent_echo(&name, &what, None, Some(&e));
                failed(&e)
            }
        }
    }

    /// One command-history line per agent call (design §5.1).
    fn agent_echo(&mut self, name: &str, what: &str, last: Option<&Value>, refused: Option<&str>) {
        let line = match (last, refused) {
            (_, Some(e)) => format!("Agent {name}: {what} refused: {}", e.lines().next().unwrap_or("")),
            (Some(t), None) => format!(
                "Agent {name}: {what}  #{} \"{}\"",
                t.get("seq").and_then(Value::as_u64).unwrap_or(0),
                t.get("label").and_then(Value::as_str).unwrap_or("")
            ),
            (None, None) => format!("Agent {name}: {what}"),
        };
        self.cad.session.echo(line);
    }

    /// An agent call was accepted: the banner's last change, steps and write flash.
    fn agent_accepted(&mut self, session: &str, last: Option<&Value>) {
        let branch = self.agent_branch();
        let steps = self.agent_steps(session);
        if let Some(b) = &mut self.agent.banner
            && b.session == session
        {
            b.last_refusal = None;
            if let Some(t) = last {
                b.last_change = Some(LastChange {
                    seq: t.get("seq").and_then(Value::as_u64).unwrap_or(0),
                    label: t.get("label").and_then(Value::as_str).unwrap_or("").to_string(),
                    time_ms: u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
                    branch,
                });
                b.flash_until = Some(Instant::now() + WRITE_FLASH);
            }
            b.steps = steps;
            if !matches!(b.state, BannerState::Paused) {
                b.state = BannerState::Connected;
            }
        }
    }

    /// An agent call was refused: the banner's last refusal.
    fn agent_refused(&mut self, session: Option<&str>, why: &str) {
        if let Some(b) = &mut self.agent.banner
            && session.is_some_and(|s| s == b.session)
        {
            b.last_refusal = Some(why.lines().next().unwrap_or(why).to_string());
        }
    }

    fn agent_branch(&mut self) -> String {
        let Some(p) = self.project.clone() else { return String::new() };
        p.read(&mut self.cad.session, |p, _| ecad_cadcraft::history::current(p).hex()).unwrap_or_default()
    }

    /// This session's transactions still on the log (on the undo stack of the branch shown).
    fn agent_steps(&mut self, session: &str) -> usize {
        let Some(p) = self.project.clone() else { return 0 };
        let Some(s) = self.agent.sessions.get(session) else { return 0 };
        let owned = &s.owned;
        if owned.is_empty() {
            return 0;
        }
        p.read(&mut self.cad.session, |p, _| p.ledger.undo_stack().iter().filter(|(_, id)| owned.contains(id)).count()).unwrap_or(0)
    }

    /// The banner's update map (design §7.5): `steps` is recomputed only after an undo, redo or
    /// branch switch by anyone; a person's ordinary edit changes nothing in the banner.
    fn agent_steps_after_change(&mut self) {
        let Some(session) = self.agent.banner.as_ref().map(|b| b.session.clone()) else { return };
        let Some(p) = self.project.clone() else { return };
        let seen = self.agent.seen_log.clone();
        let Ok((now, step_changed)) = p.read(&mut self.cad.session, |p, _| {
            let log = p.ledger.log();
            let now = (ecad_cadcraft::history::current(p).hex(), log.len());
            let changed = match &seen {
                Some((b, n)) if *b == now.0 => log.get(*n..).is_some_and(|new| new.iter().any(|t| !matches!(t.kind, ecad_model::TxKind::Edit))),
                _ => true,
            };
            (now, changed)
        }) else {
            return;
        };
        if seen.as_ref() == Some(&now) {
            return;
        }
        self.agent.seen_log = Some(now);
        if step_changed {
            let steps = self.agent_steps(&session);
            if let Some(b) = &mut self.agent.banner {
                b.steps = steps;
            }
        }
    }

    /// The agent banner's data (design §7.1), for the UI to place; `None` when no agent is shown.
    pub fn agent_banner(&self) -> Option<&AgentBanner> {
        self.agent.banner.as_ref()
    }

    /// A banner control (design §7.3).
    pub fn agent_action(&mut self, a: BannerAction) {
        match a {
            BannerAction::Pause => {
                self.agent.paused = true;
                if let Some(b) = &mut self.agent.banner {
                    b.state = BannerState::Paused;
                }
            }
            BannerAction::Resume => {
                self.agent.paused = false;
                if let Some(b) = &mut self.agent.banner {
                    b.state = BannerState::Connected;
                }
            }
            BannerAction::Disconnect => {
                let Some(session) = self.agent.banner.as_ref().map(|b| b.session.clone()) else { return };
                if let Some(s) = self.agent.sessions.get_mut(&session) {
                    s.disconnected = true;
                    s.finished = true;
                }
                if self.agent.writer.as_deref() == Some(session.as_str()) {
                    self.agent.writer = None;
                }
                if let Some(b) = &mut self.agent.banner {
                    b.state = BannerState::Finished;
                }
            }
            BannerAction::Dismiss => {
                if self.agent.banner.as_ref().is_some_and(|b| matches!(b.state, BannerState::Finished | BannerState::Error)) {
                    self.agent.banner = None;
                }
            }
            BannerAction::UndoSession => self.agent_undo_session(),
            BannerAction::OpenLast => {
                if let Some(seq) = self.agent.banner.as_ref().and_then(|b| b.last_change.as_ref()).map(|c| c.seq) {
                    self.open_history_at(seq);
                }
            }
        }
    }

    /// The ECAD Dev row's mode choice.
    pub fn agent_set_mode(&mut self, mode: Mode) {
        if let Err(e) = self.agent.set_mode(mode) {
            self.cad.session.echo(format!("Agent access: {e}"));
        }
    }

    /// The ECAD Dev row's port field was applied.
    pub fn agent_apply_port(&mut self) {
        if let Err(e) = self.agent.set_port_text() {
            self.cad.session.echo(format!("Agent access: the port could not be saved: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn requests_that_may_change_the_project_are_held() {
        assert_eq!(kind("ecad.execute", &json!({})), Kind::Write);
        assert_eq!(kind("ecad.tool", &json!({"tool": "save"})), Kind::Write);
        assert_eq!(kind("ecad.tool", &json!({"tool": "ecad_project"})), Kind::Read);
        assert_eq!(kind("ui.render", &json!({})), Kind::Read);
        assert_eq!(session_of(&json!({"session": "00112233AABBCCDD"})).as_deref(), Some("00112233aabbccdd"));
        assert_eq!(session_of(&json!({"session": "xyz"})), None);
    }
}
