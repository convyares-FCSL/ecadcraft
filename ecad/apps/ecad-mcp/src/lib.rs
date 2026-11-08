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
//! Excluded on purpose: the bridged mode (an agent driving the running trial; a later brief),
//! proposals held outside the log, library tools and anything beyond stdio on this machine.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod args;
pub mod demo;
pub mod policy;
mod tools;

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

use cadcraft_engine::Session;
use cadcraft_mcp::{Backend, Headless, Server};
use ecad_cadcraft::{Persistence, Project, ProjectHandle};
use ecad_model::{Author, Transaction, TxKind};
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
pub const CADCRAFT_TOOLS: &[&str] =
    &["execute", "list_commands", "inspect_drawing", "query_entities", "find_text", "render"];

/// Said when an agent asks for a CADCraft tool ECAD's server does not offer.
pub fn not_offered(name: &str) -> String {
    let why = match name {
        "command_line" | "script" | "cancel" => {
            " ECAD changes go through `execute` only (one command with JSON parameters per call)."
        }
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
pub const UNREADABLE: &str =
    "Refused: the project could not be checked just now (it is busy). Nothing was changed; try again.";

/// The session, the project and the gate, shared by the server and its backend.
pub(crate) struct State {
    pub(crate) headless: Headless,
    pub(crate) project: ProjectHandle,
    pub(crate) write: bool,
    pub(crate) agent: String,
    /// Whether the author name was taken from a client's `initialize` (later ones are ignored).
    pub(crate) named: bool,
    /// Transactions recorded so far by the request line being answered: a batch counts as one
    /// call ([`MAX_TRANSACTIONS_PER_CALL`]).
    pub(crate) line_transactions: usize,
}

impl State {
    pub(crate) fn session(&mut self) -> &mut Session {
        &mut self.headless.session
    }

    /// Why the project cannot be written, if it cannot (damaged, or another session edits it).
    ///
    /// # Errors
    /// [`UNREADABLE`] when the project cannot be read (callers refuse: fail closed).
    pub(crate) fn read_only(&mut self) -> Result<Option<String>, String> {
        let p = self.project.clone();
        p.read(self.session(), |p, _| p.file_status().read_only).map_err(|_| UNREADABLE.to_string())
    }

    /// Whether writes are on: `--write` and a project known to be writable.
    pub(crate) fn writes(&mut self) -> bool {
        self.write && matches!(self.read_only(), Ok(None))
    }

    /// Refuse a write when writes are off.
    pub(crate) fn may_write(&mut self) -> Result<(), String> {
        if !self.write {
            return Err(policy::WRITES_OFF.into());
        }
        match self.read_only()? {
            Some(why) => Err(policy::project_read_only(&why)),
            None => Ok(()),
        }
    }

    fn log_len(&mut self) -> Result<usize, String> {
        let p = self.project.clone();
        p.read(self.session(), |p, _| p.ledger.log().len()).map_err(|_| UNREADABLE.to_string())
    }

    fn log_since(&mut self, from: usize) -> Result<Vec<Transaction>, String> {
        let p = self.project.clone();
        p.read(self.session(), |p, _| p.ledger.log().get(from..).map(<[Transaction]>::to_vec).unwrap_or_default())
    }

    /// Record the agent's name as the author of the project's next transactions.
    pub(crate) fn set_agent(&mut self, name: &str) -> Result<(), String> {
        let name = agent_name(name);
        let p = self.project.clone();
        p.configure(self.session(), |p| p.author = Author::Agent(name.clone()))?;
        self.agent = name;
        Ok(())
    }

    /// Make a page of the project the active document, as the trial does before saving.
    pub(crate) fn focus_project(&mut self) -> Result<(), String> {
        let s = self.session();
        let owned = |s: &Session, uid: u64| s.ext.owner_of(uid).is_some();
        if s.state().is_ok_and(|st| owned(s, st.uid)) {
            return Ok(());
        }
        let i = s.docs.iter().position(|d| owned(s, d.uid)).ok_or("no ECAD page is open")?;
        s.execute("document.switch", &json!({ "index": i })).map(|_| ()).map_err(|e| e.to_string())
    }

    /// Take back `txs` (oldest first), newest first: an undo is redone, an edit or a redo is
    /// undone. Returns how many were taken back, or how many were before the first failure and
    /// that failure.
    fn roll_back(&mut self, txs: &[Transaction]) -> Result<usize, (usize, String)> {
        for (done, t) in txs.iter().rev().enumerate() {
            let step = match t.kind {
                TxKind::Undo { .. } => "redo",
                TxKind::Edit | TxKind::Redo { .. } => "u",
            };
            if let Err(e) = self.session().execute(step, &json!({})) {
                return Err((done, format!("{} failed: {e}", step.to_ascii_uppercase())));
            }
        }
        Ok(txs.len())
    }

    /// `engine.execute` through the gate, with the caps and the transactions it recorded.
    fn execute(&mut self, p: &Value) -> Result<Value, String> {
        let given = p.get("command").and_then(Value::as_str).ok_or("missing `command`")?.to_string();
        let params = p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({}));
        let id = cadcraft_engine::find_command(&given).map_or_else(|| given.to_ascii_lowercase(), |c| c.id.to_string());
        let read_only = self.read_only()?;
        policy::check(&id, &params, self.write, read_only.as_deref())?;
        // Fail closed: a call whose transactions cannot be counted does not run.
        let before = self.log_len()?;
        let r = self.headless.call("engine.execute", json!({ "command": given, "params": params }));
        let name = id.to_ascii_uppercase();
        let new = self.log_since(before).map_err(|e| {
            format!("{name} ran, but its transactions could not be read back ({e}); check ecad_history.")
        })?;
        let used = self.line_transactions;
        if used.saturating_add(new.len()) > MAX_TRANSACTIONS_PER_CALL {
            let what = if used > 0 {
                format!("this request line had already recorded {used}, and {name} recorded {}", new.len())
            } else {
                format!("{name} recorded {}", new.len())
            };
            let seqs: Vec<String> = new.iter().map(|t| format!("#{}", t.seq)).collect();
            let seqs = seqs.join(", ");
            return Err(match self.roll_back(&new) {
                Ok(n) => format!(
                    "Refused: one call (one request line, batches included) may record at most \
                     {MAX_TRANSACTIONS_PER_CALL} transaction; {what} ({seqs}). They were taken back by {n} \
                     undo/redo transaction(s) recorded after them, so the design is as before {name}; the history \
                     keeps both."
                ),
                Err((done, e)) => format!(
                    "Refused: one call (one request line, batches included) may record at most \
                     {MAX_TRANSACTIONS_PER_CALL} transaction; {what} ({seqs}). Taking them back stopped after {done} \
                     of {}: {e}. The project is NOT as before {name}; check ecad_history.",
                    new.len()
                ),
            });
        }
        self.line_transactions = used.saturating_add(new.len());
        let v = r?;
        if new.is_empty() {
            return Ok(v);
        }
        let txs: Vec<Value> = new.iter().map(tx_summary).collect();
        Ok(match v {
            Value::Object(mut o) => {
                o.insert("ecadTransactions".into(), Value::Array(txs));
                Value::Object(o)
            }
            other => json!({ "result": other, "ecadTransactions": txs }),
        })
    }

    /// A control method through the gate.
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        match method {
            "engine.execute" | "command" => self.execute(&p),
            "cmdline.input" | "cmdline.script" | "cmdline.key" => Err(not_offered("command_line")),
            "app.open" => Err(not_offered("open")),
            "app.save" => Err("use the `save` tool (Save As stays with the person who started the server)".into()),
            "ui.render" if p.get("path").is_some_and(|v| !v.is_null()) => {
                Err("Refused: `render` returns the image; agents cannot write files. Nothing was written.".into())
            }
            _ => self.headless.call(method, p),
        }
    }
}

/// A transaction as the tools report it.
pub(crate) fn tx_summary(t: &Transaction) -> Value {
    json!({
        "seq": t.seq,
        "id": t.id.hex(),
        "label": t.label,
        "author": author_text(&t.author),
        "ops": t.ops.len(),
    })
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
        "headless ECAD project".into()
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
    if cadcraft_engine::find_command("wire").is_none() {
        return Err("the ECAD commands could not be installed (the command list was built first)".into());
    }
    Ok(())
}

impl EcadMcp {
    fn with(headless: Headless, project: ProjectHandle, opts: Options) -> Result<Self, String> {
        let state = Rc::new(RefCell::new(State {
            headless,
            project,
            write: opts.write,
            agent: String::new(),
            named: false,
            line_transactions: 0,
        }));
        let inner = Server::new(Box::new(Gated(Rc::clone(&state))));
        let mut me = EcadMcp { inner, state, agent_override: opts.agent.clone() };
        // No agent edit is ever recorded as the person's, even before `initialize`.
        let name = opts.agent.unwrap_or_else(|| UNNAMED_AGENT.to_string());
        me.state_mut()?.set_agent(&name)?;
        Ok(me)
    }

    /// Open the project file at `path` (`~` and a missing `.ecad` are handled as in the trial).
    /// Returns the server and what opening found, in words. A project another session is editing,
    /// or a damaged one, opens read-only: writes are then refused whatever `opts.write` says.
    ///
    /// # Errors
    /// The path is refused, or the file cannot be opened.
    pub fn open(path: &str, opts: Options) -> Result<(Self, String), String> {
        install()?;
        let path = ecad_cadcraft::normalize(path)?;
        let mut headless = Headless { session: Session::empty() };
        let (project, summary) = ProjectHandle::open_file(&mut headless.session, &path, ecad_cadcraft::session_seed())?;
        Ok((Self::with(headless, project, opts)?, summary))
    }

    /// Create a new project and save it as `path` straight away (the person's explicit Save As,
    /// recorded as the person's). The file must not exist yet.
    ///
    /// # Errors
    /// The path is refused, the file exists, or creating or saving failed.
    pub fn create(path: &str, opts: Options) -> Result<Self, String> {
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
        let name =
            std::path::Path::new(&path).file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let project =
            Project::create_with(&mut headless.session, &name, ecad_cadcraft::session_seed(), Persistence::File)?;
        headless.session.execute("saveas", &json!({ "path": path })).map_err(|e| e.to_string())?;
        Self::with(headless, project, opts)
    }

    /// A new project in memory, never saved (`save` says it needs a file), with the ID seed given:
    /// two servers with the same seed allocate the same IDs.
    ///
    /// # Errors
    /// The project could not be created.
    pub fn in_memory(name: &str, seed: u64, opts: Options) -> Result<Self, String> {
        install()?;
        let mut headless = Headless { session: Session::empty() };
        let project = Project::create_with(&mut headless.session, name, seed, Persistence::File)?;
        Self::with(headless, project, opts)
    }

    fn state_mut(&mut self) -> Result<std::cell::RefMut<'_, State>, String> {
        self.state.try_borrow_mut().map_err(|_| BUSY.to_string())
    }

    /// Run `f` with the session and the project, for embedders and tests (the agent never gets
    /// this).
    ///
    /// # Errors
    /// The session is busy.
    pub fn with_session<R>(&mut self, f: impl FnOnce(&mut Session, &ProjectHandle) -> R) -> Result<R, String> {
        let mut st = self.state_mut()?;
        let p = st.project.clone();
        Ok(f(st.session(), &p))
    }

    /// Whether writes are on (`--write` and a writable project).
    pub fn writes(&mut self) -> bool {
        self.state_mut().is_ok_and(|mut s| s.writes())
    }

    /// The author name the agent's transactions are recorded with.
    pub fn agent(&self) -> String {
        self.state.try_borrow().map(|s| s.agent.clone()).unwrap_or_default()
    }

    /// Serve newline-delimited JSON-RPC until `input` closes. A line longer than
    /// [`MAX_REQUEST_BYTES`] is answered with an Invalid Request error and skipped.
    ///
    /// # Errors
    /// Reading `input` or writing `output` failed.
    pub fn serve(&mut self, mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            let (read, over) = read_line_capped(&mut input, &mut line, MAX_REQUEST_BYTES)?;
            if read == 0 {
                return Ok(());
            }
            let reply = if over {
                Some(
                    error(Value::Null, INVALID_REQUEST, format!("request too large (over {MAX_REQUEST_BYTES} bytes)"))
                        .to_string(),
                )
            } else {
                match std::str::from_utf8(&line) {
                    Ok(text) => self.handle_line(text),
                    Err(_) => {
                        Some(error(Value::Null, PARSE_ERROR, "parse error: the request is not UTF-8").to_string())
                    }
                }
            };
            if let Some(reply) = reply {
                output.write_all(reply.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }

    /// Answer one request line (a message or a batch); `None` for notifications and responses.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        if line.len() > MAX_REQUEST_BYTES {
            return Some(
                error(Value::Null, INVALID_REQUEST, format!("request too large (over {MAX_REQUEST_BYTES} bytes)"))
                    .to_string(),
            );
        }
        // One request line is one call for the transaction cap, batches included.
        if let Ok(mut st) = self.state_mut() {
            st.line_transactions = 0;
        }
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
        reply.map(|r| r.to_string())
    }

    /// Answer one JSON-RPC message, as one call (its own request line).
    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        if let Ok(mut st) = self.state_mut() {
            st.line_transactions = 0;
        }
        self.handle_one(msg)
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
                if let Some(res) = r.get_mut("result").and_then(Value::as_object_mut) {
                    res.insert(
                        "serverInfo".into(),
                        json!({"name": "ecad", "title": "ECAD (headless)", "version": env!("CARGO_PKG_VERSION")}),
                    );
                    res.insert("instructions".into(), json!(tools::instructions(writes, &agent)));
                }
                Some(r)
            }
            "tools/list" => Some(response(id, self.tool_list())),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else { return self.inner.handle(msg) };
                if tools::is_ecad_tool(name) {
                    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                    let r = self.state_mut().and_then(|mut s| tools::call(&mut s, name, &args));
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
        tools.extend(tools::definitions(writes));
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
