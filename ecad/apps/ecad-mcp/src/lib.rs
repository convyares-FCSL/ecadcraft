//! ecad-mcp: ECAD over the Model Context Protocol, headless (brief 011).
//!
//! It composes CADCraft's MCP server (`cadcraft-mcp`) from ECAD's side, with no upstream change:
//! one headless CADCraft session with the ECAD commands installed and one ECAD project open, as
//! the trial does. On top of CADCraft's server it adds:
//!
//! - **a write gate** ([`policy`]): read-only unless started with `--write`, and read-only
//!   whatever the flag when the project opened read-only. Writes reach the project only through
//!   `execute`, so every refusal, check and transaction is the one a person gets;
//! - **agent authorship**: every transaction the agent makes (edits, undos, redos) is recorded as
//!   `Author::Agent(<client name>)`, from the client's `initialize` (or `--agent`);
//! - **the ECAD read tools** (`ecad_project`, `ecad_history`, `ecad_inspect`) and **`save`**
//!   (only with `--write`; save-as, open, new and close stay with the person);
//! - **caps**: request size ([`MAX_REQUEST_BYTES`]), batch length ([`MAX_BATCH`]) and
//!   transactions per call ([`MAX_TRANSACTIONS_PER_CALL`]).
//!
//! CADCraft's `command_line`, `script`, `cancel`, `new_drawing` and `open` tools are not offered:
//! typed lines could reach file commands and several commands per call.
//!
//! **Bridged mode** (brief 035, [`bridge`]): with `--connect PORT` one server is headless until
//! the ECAD window answers on the port, and bridged once one does: every call then goes to the
//! window, which runs the same gate ([`gate`]) on the person's own session between two frames,
//! recording the agent as the author for that call only. `--launch` opens the window
//! ([`launch`]). `ecad_connection` says which mode is in use.
//!
//! Excluded on purpose: several agents changing one project at once, proposals held outside the
//! log, library tools, and anything beyond loopback on this machine.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod args;
pub mod bridge;
pub mod demo;
pub mod gate;
pub mod launch;
pub mod policy;
pub mod tools;

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

use cadcraft_engine::Session;
use cadcraft_mcp::{Backend, Headless, Server};
use ecad_cadcraft::{Persistence, Project, ProjectHandle};
use ecad_model::Author;
use serde_json::{Value, json};

/// Longest request line, in bytes; a longer line is answered with an Invalid Request error and
/// skipped without being kept in memory.
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// Most messages in one JSON-RPC batch.
pub const MAX_BATCH: usize = 64;
/// Most transactions one tool call may record. One `execute` runs one command, which records at
/// most one; a call that records more is undone and refused.
pub const MAX_TRANSACTIONS_PER_CALL: usize = 1;
/// Longest agent name recorded as an author, in characters.
pub const MAX_AGENT_NAME: usize = 64;
/// The author name before (or without) a client name.
pub const UNNAMED_AGENT: &str = "unnamed MCP client";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const INVALID_PARAMS: i64 = -32602;

/// CADCraft's tools that ECAD's server offers; the others are refused as unknown tools.
pub const CADCRAFT_TOOLS: &[&str] = &["execute", "list_commands", "inspect_drawing", "query_entities", "find_text", "render"];

/// Said when an agent asks for a CADCraft tool ECAD's server does not offer.
pub fn not_offered(name: &str) -> String {
    let why = match name {
        "command_line" | "script" | "cancel" => " ECAD changes go through `execute` only (one command with JSON parameters per call).",
        "new_drawing" | "open" => " The project is opened by the person who starts the server.",
        _ => "",
    };
    format!("unknown tool `{name}`.{why} `tools/list` shows the tools this server offers.")
}

/// How the server was asked to run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Allow writes (`--write`): ECAD commands that change the project, and `save`.
    pub write: bool,
    /// The author name to record instead of the client's own name (`--agent`).
    pub agent: Option<String>,
}

/// Characters that change how text is shown without being visible: bidirectional overrides and
/// isolates, zero-width spaces and joiners, and the byte-order mark. An author name never holds
/// them, so a name cannot pose as another.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
    )
}

/// An author name as recorded: control, bidirectional and zero-width characters dropped, spaces
/// trimmed, at most [`MAX_AGENT_NAME`] characters; empty becomes [`UNNAMED_AGENT`].
pub fn agent_name(given: &str) -> String {
    let s: String = given.chars().filter(|c| !c.is_control() && !invisible(*c)).take(MAX_AGENT_NAME).collect();
    let s = s.trim();
    if s.is_empty() { UNNAMED_AGENT.to_string() } else { s.to_string() }
}

/// How a transaction's author is shown (as the debugger shows it).
pub fn author_text(a: &Author) -> String {
    match a {
        Author::User => "user".into(),
        Author::Agent(n) => format!("agent {n}"),
    }
}

/// Said when the project cannot be read to check a call; the call is refused (fail closed).
pub const UNREADABLE: &str = "Refused: the project could not be checked just now (it is busy). Nothing was changed; try again.";

/// Opens the headless session and its project when the first headless call needs it
/// (`--connect`: a project the window may have open is never locked before then).
pub type Opener = Box<dyn FnMut() -> Result<(Headless, ProjectHandle), String>>;

/// The headless session and its project.
pub(crate) struct Local {
    pub(crate) headless: Headless,
    pub(crate) project: ProjectHandle,
}

/// The session, the project, the gate's settings and the bridge, shared by the server and its
/// backend.
pub(crate) struct State {
    local: Option<Local>,
    opener: Option<Opener>,
    pub(crate) write: bool,
    pub(crate) agent: String,
    /// Whether the author name was taken from a client's `initialize` (later ones are ignored).
    pub(crate) named: bool,
    /// Transactions recorded so far by the request line being answered: a batch counts as one
    /// call ([`MAX_TRANSACTIONS_PER_CALL`]).
    pub(crate) line_transactions: usize,
    /// Request lines answered so far (sent with bridged calls: the window counts per line).
    line: u64,
    /// `--connect`: the window's port and the mode (headless or bridged).
    link: Option<bridge::Link>,
}

/// Said when the headless project cannot be opened any more.
const NO_LOCAL: &str = "the headless ECAD project could not be opened";

impl State {
    /// The headless session and project, opened on first use.
    fn local(&mut self) -> Result<&mut Local, String> {
        if self.local.is_none() {
            let open = self.opener.as_mut().ok_or(NO_LOCAL)?;
            let (mut headless, project) = open()?;
            let name = self.agent.clone();
            project.configure(&mut headless.session, |p| p.author = Author::Agent(name))?;
            self.local = Some(Local { headless, project });
        }
        self.local.as_mut().ok_or_else(|| NO_LOCAL.to_string())
    }

    /// Whether the next call goes to the window (probing while undecided). Bridging closes the
    /// headless project, so its file's writer lock is released (review of PR #69, 1).
    fn bridged(&mut self) -> Result<bool, String> {
        let (agent, write) = (self.agent.clone(), self.write);
        let b = match self.link.as_mut() {
            None => false,
            Some(l) => l.decide(&agent, write)?,
        };
        if b {
            self.local = None;
        }
        Ok(b)
    }

    /// A `--connect` server keeps a headless project **file** open only once headless has
    /// recorded something (it then sticks). Until then the file is closed after every request
    /// line, so it is not locked while the person opens it in the window (review of PR #69, 1). A
    /// project in memory locks nothing and stays open.
    fn release_unstuck(&mut self) {
        if !self.link.as_ref().is_some_and(|l| l.mode != bridge::Mode::Headless { stuck: true }) {
            return;
        }
        let file =
            self.local.as_mut().is_some_and(|l| l.project.read(&mut l.headless.session, |p, _| p.file_status().path.is_some()).unwrap_or(true));
        if file {
            self.local = None;
        }
    }

    /// Headless recorded something: it never bridges from now on.
    fn stick_headless(&mut self) {
        if let Some(l) = self.link.as_mut() {
            l.stick_headless();
        }
    }

    /// Why the headless project cannot be written, if it cannot.
    ///
    /// # Errors
    /// [`UNREADABLE`] when the project cannot be read.
    pub(crate) fn read_only(&mut self) -> Result<Option<String>, String> {
        let l = self.local()?;
        gate::read_only(&mut l.headless.session, &l.project)
    }

    /// Whether writes are on, without probing: the window's answer when bridged, else `--write`
    /// and a project known to be writable (before the headless project is open: `--write`).
    pub(crate) fn writes(&mut self) -> bool {
        if let Some(l) = &self.link {
            if l.mode == bridge::Mode::Bridged {
                return l.hello.as_ref().and_then(|h| h.get("writes")).and_then(Value::as_bool).unwrap_or(false);
            }
            if self.local.is_none() {
                return self.write;
            }
        }
        self.write && matches!(self.read_only(), Ok(None))
    }

    /// Record the agent's name as the author of the project's next transactions.
    pub(crate) fn set_agent(&mut self, name: &str) -> Result<(), String> {
        let name = agent_name(name);
        if let Some(l) = self.local.as_mut() {
            l.project.configure(&mut l.headless.session, |p| p.author = Author::Agent(name.clone()))?;
        }
        self.agent = name;
        Ok(())
    }

    /// `engine.execute` through the gate, headless.
    fn execute_local(&mut self, p: &Value) -> Result<Value, String> {
        let write = self.write;
        let mut used = self.line_transactions;
        let l = self.local()?;
        let mut call = gate::Call { write, refusal: None, line_transactions: &mut used, owned: None };
        let r = gate::execute(&mut l.headless.session, &l.project, &mut call, p);
        if used > self.line_transactions {
            self.stick_headless();
        }
        self.line_transactions = used;
        r
    }

    /// A control method through the gate, headless or bridged.
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        if self.bridged()? {
            let (agent, write, line) = (self.agent.clone(), self.write, self.line);
            let l = self.link.as_mut().ok_or(NO_LOCAL)?;
            return l.call(method, p, &agent, write, line);
        }
        match method {
            "engine.execute" | "command" => self.execute_local(&p),
            "cmdline.input" | "cmdline.script" | "cmdline.key" => Err(not_offered("command_line")),
            "app.open" => Err(not_offered("open")),
            "app.save" => Err("use the `save` tool (Save As stays with the person who started the server)".into()),
            "ui.render" if p.get("path").is_some_and(|v| !v.is_null()) => {
                Err("Refused: `render` returns the image; agents cannot write files. Nothing was written.".into())
            }
            _ => self.local()?.headless.call(method, p),
        }
    }

    /// What `ecad_connection` says.
    fn connection(&mut self) -> Value {
        let agent = self.agent.clone();
        match &self.link {
            None => json!({"mode": "headless", "port": null, "app": null, "agent": agent, "session": null, "writer": null}),
            Some(l) => {
                let app = (l.mode == bridge::Mode::Bridged).then(|| l.hello.clone()).flatten();
                let writer = app.as_ref().and_then(|h| h.get("writer")).cloned();
                json!({"mode": l.mode.word(), "port": l.port, "app": app, "agent": agent, "session": l.session, "writer": writer})
            }
        }
    }

    /// An ECAD tool (or `ecad_connection` and `screenshot`), headless or bridged. `Err` is a
    /// protocol error; a failing tool is an `isError` result.
    fn tool(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        if name == "ecad_connection" {
            // Asking which mode is in use decides it, as any other call would.
            if let Err(e) = self.bridged() {
                return Ok(tools::err_text(&e));
            }
            return Ok(tools::text(&self.connection()));
        }
        let bridged = match self.bridged() {
            Ok(b) => b,
            Err(e) => return Ok(tools::err_text(&e)),
        };
        if name == "screenshot" {
            if !bridged {
                return Ok(tools::err_text(tools::NO_WINDOW));
            }
            return Ok(match self.call("ui.screenshot", json!({})) {
                Ok(v) => match v.get("pngBase64").and_then(Value::as_str) {
                    Some(data) => json!({"content": [{"type": "image", "data": data, "mimeType": "image/png"}]}),
                    None => tools::text(&v),
                },
                Err(e) => tools::err_text(&e),
            });
        }
        if bridged {
            let (agent, write, line) = (self.agent.clone(), self.write, self.line);
            let l = self.link.as_mut().ok_or(NO_LOCAL)?;
            return match l.tool(name, args.clone(), &agent, write, line) {
                Ok(r) => r,
                Err(e) => Ok(tools::err_text(&e)),
            };
        }
        let (write, agent) = (self.write, self.agent.clone());
        let mut used = self.line_transactions;
        let l = match self.local() {
            Ok(l) => l,
            Err(e) => return Ok(tools::err_text(&e)),
        };
        let call = gate::Call { write, refusal: None, line_transactions: &mut used, owned: None };
        let mut host = tools::Host { session: &mut l.headless.session, project: &l.project, call, agent: &agent };
        let r = tools::call(&mut host, name, args);
        if name == "save" && r.as_ref().is_ok_and(|v| v.get("isError").is_none()) {
            self.stick_headless();
        }
        r
    }
}

/// Said when a call arrives while another is still using the session (not expected over stdio).
const BUSY: &str = "the ECAD session is busy with another call";

/// CADCraft's backend role, served by the shared state through the gate.
struct Gated(Rc<RefCell<State>>);

impl Backend for Gated {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.0.try_borrow_mut().map_err(|_| BUSY.to_string())?.call(method, params)
    }
    fn has_ui(&self) -> bool {
        false
    }
    fn describe(&self) -> String {
        "ECAD project".into()
    }
}

fn response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}
fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

/// The ECAD MCP server: CADCraft's server with the ECAD project, gate and tools.
pub struct EcadMcp {
    inner: Server,
    state: Rc<RefCell<State>>,
    agent_override: Option<String>,
}

/// Install the ECAD commands, or say why they are missing.
fn install() -> Result<(), String> {
    ecad_cadcraft::install();
    // Brief 024: `printbook` plots through CADCraft's PDF plotter.
    ecad_cadcraft::printbook::install_io();
    if cadcraft_engine::find_command("wire").is_none() {
        return Err("the ECAD commands could not be installed (the command list was built first)".into());
    }
    Ok(())
}

/// Open the project file at `path` headless; also what opening found, in words.
fn open_local(path: &str, write: bool) -> Result<(Headless, ProjectHandle, String), String> {
    install()?;
    let path = ecad_cadcraft::normalize(path)?;
    let mut headless = Headless { session: Session::empty() };
    // Writes off: the file is opened for reading only, so it is never changed, not even by the
    // database's own bookkeeping (brief 021: a version 2 file is a redb database).
    let seed = ecad_cadcraft::session_seed();
    let (project, summary) = if write {
        ProjectHandle::open_file(&mut headless.session, &path, seed)?
    } else {
        ProjectHandle::open_file_read_only(&mut headless.session, &path, seed)?
    };
    Ok((headless, project, summary))
}

/// Create the project file at `path` headless (it must not exist yet), saved straight away.
fn create_local(path: &str) -> Result<(Headless, ProjectHandle), String> {
    install()?;
    let path = ecad_cadcraft::normalize(path)?;
    if std::path::Path::new(&path).exists() {
        return Err(format!("{path} already exists; open it without --create"));
    }
    if let Some(dir) = std::path::Path::new(&path).parent().filter(|d| !d.as_os_str().is_empty())
        && !dir.is_dir()
    {
        return Err(format!("the folder {} does not exist; create it first. Nothing was written.", dir.display()));
    }
    let mut headless = Headless { session: Session::empty() };
    let name = std::path::Path::new(&path).file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let project = Project::create_with(&mut headless.session, &name, ecad_cadcraft::session_seed(), Persistence::File)?;
    headless.session.execute("saveas", &json!({ "path": path })).map_err(|e| e.to_string())?;
    Ok((headless, project))
}

/// A new project in memory, headless.
fn memory_local(name: &str, seed: u64) -> Result<(Headless, ProjectHandle), String> {
    install()?;
    let mut headless = Headless { session: Session::empty() };
    let project = Project::create_with(&mut headless.session, name, seed, Persistence::File)?;
    Ok((headless, project))
}

/// What opens the headless project for a `--connect` server: `project` (created first with
/// `create`), else an unsaved project in memory.
pub fn opener(project: Option<String>, create: bool, write: bool) -> Opener {
    let mut created = false;
    Box::new(move || match project.as_deref() {
        Some(p) if create && !created => {
            created = true;
            create_local(p)
        }
        Some(p) => open_local(p, write).map(|(h, pr, _)| (h, pr)),
        None => memory_local("Untitled", ecad_cadcraft::session_seed()),
    })
}

impl EcadMcp {
    fn build(local: Option<Local>, opener: Option<Opener>, link: Option<bridge::Link>, opts: Options) -> Result<Self, String> {
        let state = Rc::new(RefCell::new(State {
            local,
            opener,
            write: opts.write,
            agent: String::new(),
            named: false,
            line_transactions: 0,
            line: 0,
            link,
        }));
        let inner = Server::new(Box::new(Gated(Rc::clone(&state))));
        let mut me = EcadMcp { inner, state, agent_override: opts.agent.clone() };
        // No agent edit is ever recorded as the person's, even before `initialize`.
        let name = opts.agent.unwrap_or_else(|| UNNAMED_AGENT.to_string());
        me.state_mut()?.set_agent(&name)?;
        Ok(me)
    }

    fn with(headless: Headless, project: ProjectHandle, opts: Options) -> Result<Self, String> {
        Self::build(Some(Local { headless, project }), None, None, opts)
    }

    /// A `--connect` server (brief 035): headless until an ECAD window answers on `port`, bridged
    /// once one does; `opener` opens the headless project at the first headless call. With
    /// `launch`, a call that finds no window opens one first.
    ///
    /// # Errors
    /// The ECAD commands could not be installed.
    pub fn connect(port: u16, launch: bool, opener: Opener, opts: Options) -> Result<Self, String> {
        install()?;
        Self::build(None, Some(opener), Some(bridge::Link::new(port, launch)), opts)
    }

    /// Open the project file at `path` (`~` and a missing `.ecad` are handled as in the trial).
    /// Returns the server and what opening found, in words. A project another session is editing,
    /// or a damaged one, opens read-only: writes are then refused whatever `opts.write` says.
    ///
    /// # Errors
    /// The path is refused, or the file cannot be opened.
    pub fn open(path: &str, opts: Options) -> Result<(Self, String), String> {
        let (headless, project, summary) = open_local(path, opts.write)?;
        Ok((Self::with(headless, project, opts)?, summary))
    }

    /// Create a new project and save it as `path` straight away (the person's explicit Save As,
    /// recorded as the person's). The file must not exist yet.
    ///
    /// # Errors
    /// The path is refused, the file exists, or creating or saving failed.
    pub fn create(path: &str, opts: Options) -> Result<Self, String> {
        let (headless, project) = create_local(path)?;
        Self::with(headless, project, opts)
    }

    /// A new project in memory, never saved (`save` says it needs a file), with the ID seed given:
    /// two servers with the same seed allocate the same IDs.
    ///
    /// # Errors
    /// The project could not be created.
    pub fn in_memory(name: &str, seed: u64, opts: Options) -> Result<Self, String> {
        let (headless, project) = memory_local(name, seed)?;
        Self::with(headless, project, opts)
    }

    fn state_mut(&mut self) -> Result<std::cell::RefMut<'_, State>, String> {
        self.state.try_borrow_mut().map_err(|_| BUSY.to_string())
    }

    /// Run `f` with the headless session and the project, for embedders and tests (the agent
    /// never gets this). Opens the headless project if it is not open yet.
    ///
    /// # Errors
    /// The session is busy, or the headless project cannot be opened.
    pub fn with_session<R>(&mut self, f: impl FnOnce(&mut Session, &ProjectHandle) -> R) -> Result<R, String> {
        let mut st = self.state_mut()?;
        let l = st.local()?;
        Ok(f(&mut l.headless.session, &l.project))
    }

    /// Whether writes are on (`--write` and a writable project; bridged: the window's answer).
    pub fn writes(&mut self) -> bool {
        self.state_mut().is_ok_and(|mut s| s.writes())
    }

    /// The author name the agent's transactions are recorded with.
    pub fn agent(&self) -> String {
        self.state.try_borrow().map(|s| s.agent.clone()).unwrap_or_default()
    }

    /// The mode of a `--connect` server (`None` for a headless-only server).
    pub fn mode(&self) -> Option<bridge::Mode> {
        self.state.try_borrow().ok().and_then(|s| s.link.as_ref().map(|l| l.mode))
    }

    /// Tell the window this server is going (`ecad.bye`, best effort). Called when input closes.
    pub fn bye(&mut self) {
        if let Ok(mut s) = self.state_mut()
            && let Some(l) = s.link.as_mut()
        {
            l.bye();
        }
    }

    /// Serve newline-delimited JSON-RPC until `input` closes, then say `ecad.bye` when bridged. A
    /// line longer than [`MAX_REQUEST_BYTES`] is answered with an Invalid Request error and
    /// skipped.
    ///
    /// # Errors
    /// Reading `input` or writing `output` failed.
    pub fn serve(&mut self, mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            let (read, over) = read_line_capped(&mut input, &mut line, MAX_REQUEST_BYTES)?;
            if read == 0 {
                self.bye();
                return Ok(());
            }
            let reply = if over {
                Some(error(Value::Null, INVALID_REQUEST, format!("request too large (over {MAX_REQUEST_BYTES} bytes)")).to_string())
            } else {
                match std::str::from_utf8(&line) {
                    Ok(text) => self.handle_line(text),
                    Err(_) => Some(error(Value::Null, PARSE_ERROR, "parse error: the request is not UTF-8").to_string()),
                }
            };
            if let Some(reply) = reply {
                output.write_all(reply.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }

    /// A new request line starts: it is one call for the transaction cap, batches included.
    fn new_line(&mut self) {
        if let Ok(mut st) = self.state_mut() {
            st.line_transactions = 0;
            st.line = st.line.wrapping_add(1);
        }
    }

    /// Answer one request line (a message or a batch); `None` for notifications and responses.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        if line.len() > MAX_REQUEST_BYTES {
            return Some(error(Value::Null, INVALID_REQUEST, format!("request too large (over {MAX_REQUEST_BYTES} bytes)")).to_string());
        }
        self.new_line();
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(batch)) if batch.is_empty() => Some(error(Value::Null, INVALID_REQUEST, "empty batch")),
            Ok(Value::Array(batch)) if batch.len() > MAX_BATCH => {
                Some(error(Value::Null, INVALID_REQUEST, format!("batch too long (at most {MAX_BATCH} messages)")))
            }
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.into_iter().filter_map(|m| self.handle_one(m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(msg) => self.handle_one(msg),
            Err(e) => Some(error(Value::Null, PARSE_ERROR, format!("parse error: {e}"))),
        };
        self.end_line();
        reply.map(|r| r.to_string())
    }

    /// Answer one JSON-RPC message, as one call (its own request line).
    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        self.new_line();
        let r = self.handle_one(msg);
        self.end_line();
        r
    }

    /// A request line was answered.
    fn end_line(&mut self) {
        if let Ok(mut st) = self.state_mut() {
            st.release_unstuck();
        }
    }

    /// Answer one message of the request line being answered.
    fn handle_one(&mut self, msg: Value) -> Option<Value> {
        let method = msg.get("method").and_then(Value::as_str).map(str::to_string);
        let (Some(method), Some(id)) = (method, msg.get("id").cloned()) else { return self.inner.handle(msg) };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match method.as_str() {
            "initialize" => {
                // The first `initialize` names the author; later ones never rename it.
                let client = params.pointer("/clientInfo/name").and_then(Value::as_str).unwrap_or(UNNAMED_AGENT);
                let name = self.agent_override.clone().unwrap_or_else(|| client.to_string());
                let named = self.state_mut().and_then(|mut s| {
                    if s.named {
                        return Ok(());
                    }
                    s.set_agent(&name)?;
                    s.named = true;
                    Ok(())
                });
                if let Err(e) = named {
                    return Some(error(id, -32603, e));
                }
                let mut r = self.inner.handle(msg)?;
                let writes = self.writes();
                let agent = self.agent();
                let live = self.mode().is_some();
                if let Some(res) = r.get_mut("result").and_then(Value::as_object_mut) {
                    let title = if live { "ECAD" } else { "ECAD (headless)" };
                    res.insert("serverInfo".into(), json!({"name": "ecad", "title": title, "version": env!("CARGO_PKG_VERSION")}));
                    res.insert("instructions".into(), json!(tools::instructions(writes, &agent, live)));
                }
                Some(r)
            }
            "tools/list" => Some(response(id, self.tool_list())),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else { return self.inner.handle(msg) };
                if tools::is_ecad_tool(name) && (name != "screenshot" || self.mode().is_some()) {
                    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                    let r = self.state_mut().and_then(|mut s| s.tool(name, &args));
                    return Some(match r {
                        Ok(v) => response(id, v),
                        Err(m) => error(id, INVALID_PARAMS, m),
                    });
                }
                if CADCRAFT_TOOLS.contains(&name) {
                    return self.inner.handle(msg);
                }
                Some(error(id, INVALID_PARAMS, not_offered(name)))
            }
            _ => self.inner.handle(msg),
        }
    }

    /// The `tools/list` payload: CADCraft's offered tools (adapted) and the ECAD tools.
    fn tool_list(&mut self) -> Value {
        let theirs = self
            .inner
            .handle(json!({"jsonrpc": "2.0", "id": 0, "method": "tools/list"}))
            .and_then(|r| r.pointer("/result/tools").cloned())
            .and_then(|t| match t {
                Value::Array(a) => Some(a),
                _ => None,
            })
            .unwrap_or_default();
        let writes = self.writes();
        let mut tools: Vec<Value> = theirs
            .into_iter()
            .filter(|t| t["name"].as_str().is_some_and(|n| CADCRAFT_TOOLS.contains(&n)))
            .map(|mut t| {
                match t["name"].as_str() {
                    Some("execute") => {
                        if let Some(o) = t.as_object_mut() {
                            o.insert("description".into(), json!(tools::execute_description(writes)));
                        }
                    }
                    Some("render") => {
                        if let Some(p) = t.pointer_mut("/inputSchema/properties").and_then(Value::as_object_mut) {
                            p.remove("path");
                        }
                    }
                    _ => {}
                }
                t
            })
            .collect();
        let live = self.mode().is_some();
        tools.extend(tools::definitions(writes, live));
        json!({ "tools": tools })
    }
}

/// Read one line into `buf` (newline included), keeping at most `max` bytes: a longer line is
/// still consumed, but dropped. Returns the bytes read (0 at the end of input) and whether the
/// line was too long.
fn read_line_capped(r: &mut impl BufRead, buf: &mut Vec<u8>, max: usize) -> std::io::Result<(usize, bool)> {
    let mut total = 0usize;
    let mut over = false;
    loop {
        let avail = match r.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if avail.is_empty() {
            return Ok((total, over));
        }
        let (chunk, done) = match avail.iter().position(|b| *b == b'\n') {
            Some(i) => (avail.get(..=i).unwrap_or(avail), true),
            None => (avail, false),
        };
        let len = chunk.len();
        if !over {
            if buf.len().saturating_add(len) > max.saturating_add(1) {
                over = true;
                buf.clear();
                buf.shrink_to_fit();
            } else {
                buf.extend_from_slice(chunk);
            }
        }
        total = total.saturating_add(len);
        r.consume(len);
        if done {
            return Ok((total, over));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_lines_are_consumed_but_not_kept() {
        let input = format!("{}\n{{}}\n", "x".repeat(100));
        let mut r = std::io::Cursor::new(input.into_bytes());
        let mut buf = Vec::new();
        assert_eq!(read_line_capped(&mut r, &mut buf, 10).ok(), Some((101, true)));
        assert!(buf.is_empty());
        buf.clear();
        assert_eq!(read_line_capped(&mut r, &mut buf, 10).ok(), Some((3, false)));
        assert_eq!(buf, b"{}\n");
    }

    #[test]
    fn agent_names() {
        assert_eq!(agent_name("  claude-code\n"), "claude-code");
        assert_eq!(agent_name("\u{7}"), UNNAMED_AGENT);
        assert_eq!(agent_name(&"a".repeat(500)).chars().count(), MAX_AGENT_NAME);
    }
}
