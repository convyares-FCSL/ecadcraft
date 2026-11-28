//! The ECAD tools: `ecad_project`, `ecad_history` and `ecad_inspect` (read), and `save` (write); and
//! the command list as an agent sees it ([`agent_commands`]).

use ecad_model::Author;
use serde_json::{Map, Value, json};

use crate::ProjectHandle;
use crate::cad::Session;

use crate::agent::MAX_TRANSACTIONS_PER_CALL;
use crate::agent::gate::{self, Call, tx_summary};
use crate::agent::policy;
use ecad_file::history::logged_author_text;

/// Names of the tools this module answers.
pub(crate) const ECAD_TOOLS: &[&str] = &["ecad_project", "ecad_history", "ecad_inspect", "describe_command", "save", "ecad_connection", "screenshot"];

/// Said by `screenshot` when no ECAD window is connected.
pub const NO_WINDOW: &str = "No ECAD window is connected; use render.";

/// Where an ECAD tool runs: the session and project, the call's write gate and the agent's name.
pub struct Host<'a> {
    pub session: &'a mut Session,
    pub project: &'a ProjectHandle,
    pub call: Call<'a>,
    pub agent: &'a str,
}

/// Whether the ECAD tool `name` may change the project (the bridge holds it while the person is
/// busy).
pub fn is_write_tool(name: &str) -> bool {
    name == "save"
}

/// Default and largest `limit` of the list tools.
const DEFAULT_LIMIT: u64 = 500;
const MAX_LIMIT: u64 = 10_000;
const DEFAULT_HISTORY: u64 = 100;
const MAX_HISTORY: u64 = 1_000;

pub fn is_ecad_tool(name: &str) -> bool {
    ECAD_TOOLS.contains(&name)
}

/// What the server says about itself at `initialize`.
pub fn instructions(writes: bool, agent: &str, live: bool) -> String {
    let mode = if writes {
        "Writes are ON: ECAD commands through `execute` change the project, and `save` writes it to its file."
    } else {
        "Writes are OFF: the project can be read but not changed or saved (the server was started without --write, \
         or the project opened read-only)."
    };
    let where_ = if live { " ECAD may be headless (no window) or live (the person watches). Call ecad_connection to know which." } else { "" };
    format!(
        "ECAD electrical design project (headless; CADCraft drawing engine).{where_} Read it with ecad_project (pages, \
         devices, terminals, conductors, cables, nets), ecad_history (transactions and their authors) and \
         ecad_inspect, and look at the active page with render, query_entities and inspect_drawing. Change it only with \
         `execute` and an ECAD command (list_commands shows them with their parameters: wire, place, \
         convertcomponent, addconnectionpoint, connect, disconnect, cableslash, cableview, newpage, openpage, setprop, \
         ecaddelete, ...). Each change is one transaction recorded as `agent {agent}`, and `execute u` undoes it in one \
         step. {mode}"
    )
}

/// The `execute` tool's description in ECAD's server.
pub fn execute_description(writes: bool) -> String {
    let mode = if writes { "Writes are on." } else { "Writes are off: only read-only commands run." };
    format!(
        "Run one command with JSON parameters (never prompts). Agents run ECAD's commands and a short list of drawing \
         commands (line, move, erase, u, ...); list_commands shows them with their parameter docs, each marked read or \
         write. They change the project exactly as for a person: the same checks and refusals, and each change is one \
         transaction authored by this agent and one step of `u` (undo) / `redo`. Read-only commands (ecadinspect, \
         entities, texts, openpage, nets, ...) run with writes off. Every other command is refused, and so are file \
         and document commands (open, new, close, saveas, export, lisp, ...); save with the `save` tool. At most \
         {MAX_TRANSACTIONS_PER_CALL} transaction per call. {mode} Examples: wire {{points:[[0,0],[60,0]]}}, \
         cableslash {{from:[28,-3], to:[32,18]}}, cableview {{cable:\"<id>\", view:\"fan\"}}, u {{}}."
    )
}

/// The `list_commands` description in ECAD's server.
pub const LIST_COMMANDS: &str = "List the commands this agent may run, with ids, labels, aliases and parameter docs; each is \
     marked `agent`: `read` (runs with writes off) or `write` (only with writes on). Optional `filter`: a substring of the \
     id, label or an alias. `all: true` lists every command of the engine instead, the refused ones marked `agent: \
     \"refused\"` and `enabled: false` with the reason.";

/// Why a refused command is disabled in the full list (the refusal itself says more).
const NEVER_REASON: &str = "not available to agents: files, documents and the process stay with the person";
const NOT_OFFERED_REASON: &str = "not available to agents";

/// The engine's command list (`engine.commands`, an array of command entries) as an agent sees it:
/// by default only the commands the gate offers ([`policy::offered`]), each marked `"agent": "read"`
/// or `"write"`; with `all`, every command, the refused ones marked `"agent": "refused"` and
/// `"enabled": false` with a short reason as `disabled_reason`.
pub fn agent_commands(list: Value, all: bool) -> Value {
    let Value::Array(items) = list else { return list };
    let items = items.into_iter().filter_map(|mut c| {
        let id = c.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
        let offer = policy::offered(&id);
        if offer.is_none() && !all {
            return None;
        }
        if let Some(o) = c.as_object_mut() {
            match offer {
                Some(offer) => {
                    o.insert("agent".into(), json!(offer.word()));
                }
                None => {
                    let why = if policy::NEVER_COMMANDS.contains(&id.as_str()) { NEVER_REASON } else { NOT_OFFERED_REASON };
                    o.insert("agent".into(), json!("refused"));
                    o.insert("enabled".into(), json!(false));
                    o.insert("disabled_reason".into(), json!(why));
                }
            }
        }
        Some(c)
    });
    Value::Array(items.collect())
}

/// The ECAD tool definitions (`live`: a `--connect` server, which also lists `screenshot`).
pub fn definitions(writes: bool, live: bool) -> Vec<Value> {
    let save = if writes {
        "Save the project to its .ecad file (QSAVE). Save As, open, new and close stay with the person who started the server."
    } else {
        "Save the project to its .ecad file. Refused now: writes are off (start the server with --write)."
    };
    let mut v = vec![
        json!({"name": "ecad_project", "description": "The ECAD project: name, file, whether writes are on, head and unsaved changes, pages, components with their connection points (`connection_points`), conductors with their ends and cable, cables with their members in order, and nets as the app, wire numbers and checks see them (joined by conductor ends, definition bridges, jumper bars and continuations of one name; each with its connection points, conductors, continuation names and wire numbers). Each list has `count` and at most `limit` entries (default 500).",
            "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer", "minimum": 0}}}}),
        json!({"name": "ecad_history", "description": "The project's transactions in order: seq, id, label, kind (edit, undo of #n, redo of #n), author (`user` or `agent <name>`), time and op count. Page with offset and limit (default: the last 100).",
            "inputSchema": {"type": "object", "properties": {"offset": {"type": "integer", "minimum": 0}, "limit": {"type": "integer", "minimum": 0}}}}),
        json!({"name": "ecad_inspect", "description": "ecadinspect's read-only JSON snapshot of the project (live, or at transaction `seq`).",
            "inputSchema": {"type": "object", "properties": {"seq": {"type": "integer", "minimum": 0}}}}),
        json!({"name": "describe_command", "description": "One command offered to agents: its parameters text, the JSON Schema of its parameters (`params`) and of its reply (`reply`; null while that command still answers untyped JSON), and whether it reads or writes (`agent`).",
            "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}}),
        json!({"name": "save", "description": save, "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "ecad_connection", "description": "Where this server's calls go: `mode` headless (no window) or bridged (the ECAD window, the person watches), the port, the window's project and whether it lets this agent write (`app.writes`, `app.why`), the agent name, the session id and whether this session is the window's writer.",
            "inputSchema": {"type": "object", "properties": {}}}),
    ];
    if live {
        // A `--connect` server lists it in both modes, so its tool list never changes with the mode.
        v.push(json!({"name": "screenshot", "description": "A PNG of the ECAD window (live mode only; headless, use render).", "inputSchema": {"type": "object", "properties": {}}}));
    }
    v
}

/// A JSON value as a text tool result.
pub fn text(v: &Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default()}]})
}

/// An `isError` tool result.
pub fn err_text(e: &str) -> Value {
    json!({"content": [{"type": "text", "text": e}], "isError": true})
}

/// The arguments as an object, with each named integer argument checked (absent or `null` is not
/// given). `Err` is the invalid-params message.
fn object(name: &str, args: &Value, ints: &[&str]) -> Result<Map<String, Value>, String> {
    let o = match args {
        Value::Null => Map::new(),
        Value::Object(o) => o.clone(),
        _ => return Err(format!("`arguments` of `{name}` must be an object")),
    };
    for k in ints {
        if let Some(v) = o.get(*k).filter(|v| !v.is_null())
            && v.as_u64().is_none()
        {
            return Err(format!("`{k}` of `{name}` must be a non-negative integer"));
        }
    }
    Ok(o)
}

fn int(o: &Map<String, Value>, k: &str) -> Option<u64> {
    o.get(k).and_then(Value::as_u64)
}

/// Run an ECAD tool. `Err` is a protocol error (invalid arguments); a failing tool is an
/// `isError` result.
pub fn call(st: &mut Host<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "ecad_project" => {
            let o = object(name, args, &["limit"])?;
            let limit = int(&o, "limit").unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
            Ok(match project(st, limit) {
                Ok(v) => text(&v),
                Err(e) => err_text(&e),
            })
        }
        "ecad_history" => {
            let o = object(name, args, &["offset", "limit"])?;
            Ok(match history(st, int(&o, "offset"), int(&o, "limit")) {
                Ok(v) => text(&v),
                Err(e) => err_text(&e),
            })
        }
        "ecad_inspect" => {
            let o = object(name, args, &["seq"])?;
            let params = int(&o, "seq").map_or_else(|| json!({}), |s| json!({ "seq": s }));
            Ok(match st.session.execute("ecadinspect", &params) {
                Ok(v) => text(&v),
                Err(e) => err_text(&e.to_string()),
            })
        }
        "describe_command" => {
            let o = object(name, args, &[])?;
            let id = o.get("id").and_then(Value::as_str).ok_or("`id` of `describe_command` must be a command id")?;
            Ok(match describe(id) {
                Ok(v) => text(&v),
                Err(e) => err_text(&e),
            })
        }
        "save" => {
            let o = object(name, args, &[])?;
            if o.get("path").is_some_and(|v| !v.is_null()) {
                return Ok(err_text(
                    "Refused: `save` writes the project to its own file; Save As stays with the person who started the \
                     server. Nothing was written.",
                ));
            }
            Ok(match save(st) {
                Ok(v) => text(&v),
                Err(e) => err_text(&e),
            })
        }
        other => Err(crate::agent::not_offered(other)),
    }
}

/// `describe_command`: an offered typed command's parameters text, params schema, reply schema and
/// access.
///
/// # Errors
/// A command that is not offered to agents, or whose input is not typed.
fn describe(id: &str) -> Result<Value, String> {
    let offer = policy::offered(id).ok_or_else(|| crate::agent::not_offered(id))?;
    let params = crate::commands::schema_of(id).ok_or_else(|| format!("`{id}` has no typed parameters; list_commands shows its parameter docs."))?;
    Ok(json!({
        "id": id,
        "agent": offer.word(),
        "help": crate::commands::help_of(id),
        "params": params,
        "reply": crate::commands::reply_schema_of(id),
    }))
}

fn save(st: &mut Host<'_>) -> Result<Value, String> {
    gate::may_write(st.session, st.project, &st.call)?;
    gate::focus_project(st.session)?;
    st.session.execute("qsave", &json!({})).map_err(|e| e.to_string())?;
    st.project.read(st.session, |p, _| {
        json!({
            "saved": true,
            "path": p.file_status().path,
            "head": p.ledger.head(),
            "unsaved": p.unsaved(),
        })
    })
}

/// At most `limit` entries of `items`, with the full count.
fn listed(items: impl Iterator<Item = Value>, count: usize, limit: u64) -> Value {
    let take = usize::try_from(limit).unwrap_or(usize::MAX);
    let items: Vec<Value> = items.take(take).collect();
    json!({ "count": count, "truncated": items.len() < count, "items": items })
}

fn project(st: &mut Host<'_>, limit: u64) -> Result<Value, String> {
    let write = st.call.write;
    let read_only = gate::read_only(st.session, st.project)?;
    let agent = st.agent.to_string();
    st.project.read(st.session, |p, _| {
        let doc = p.ledger.document();
        let pages = doc.page_order().iter().map(|id| {
            json!({
                "id": id.hex(),
                "title": doc.pages().get(id).map(|pg| pg.title.clone()),
                "open": p.pages.get(id).is_some_and(|v| v.uid.is_some()),
            })
        });
        let devices = doc.devices().iter().map(|(id, c)| {
            let connection_points: Vec<Value> = doc.connection_points_of(*id).map(|(t, term)| json!({"id": t.hex(), "label": term.label})).collect();
            json!({"id": id.hex(), "properties": c.properties, "connection_points": connection_points})
        });
        let cable_of = |c| doc.cables().iter().find(|(_, k)| k.cores.iter().any(|m| m.wire() == Some(c))).map(|(id, _)| id.hex());
        let conductors = doc.wires().iter().map(|(id, c)| {
            json!({
                "id": id.hex(),
                "values": c.values().into_iter().map(|(k, v)| (k.to_string(), Value::String(v))).collect::<serde_json::Map<_, _>>(),
                "display": c.display,
                "ends": {"A": c.end(ecad_model::End::A).hex(), "B": c.end(ecad_model::End::B).hex()},
                "cable": cable_of(*id),
            })
        });
        let cables = doc.cables().iter().map(|(id, c)| {
            let members: Vec<Value> = c
                .cores
                .iter()
                .map(|m| json!({"conductor": m.wire().map(|w| w.hex()), "line": m.route().map(|r| r.hex()), "core": m.label_str(), "spare": m.is_spare()}))
                .collect();
            json!({"id": id.hex(), "tag": c.tag_shown(), "tag_state": c.tag.state().map(ecad_model::naming::words::Word::word), "description": c.description(), "properties": c.properties, "members": members})
        });
        // The nets the app, wire numbers and checks use: every net with at least one conductor,
        // ordered by its lowest conductor ID (as the `nets` command lists them).
        let conn = doc.connectivity();
        let mut nets: Vec<_> = conn.nets().map(|(_, n)| n).filter(|n| !n.wires.is_empty()).collect();
        nets.sort_by_key(|n| n.first_conductor());
        let net_count = nets.len();
        let nets = nets.into_iter().map(|n| {
            json!({
                "connection_points": n.connection_points.iter().map(|t| t.hex()).collect::<Vec<_>>(),
                "conductors": n.wires.iter().map(|c| c.hex()).collect::<Vec<_>>(),
                "continuations": n.names,
                "numbers": n.numbers.keys().collect::<Vec<_>>(),
            })
        });
        let status = p.file_status();
        json!({
            "name": p.name,
            "path": status.path,
            "readOnly": read_only,
            "writes": write && read_only.is_none(),
            "agent": agent,
            "head": p.ledger.head(),
            "unsaved": p.unsaved(),
            "unsavedTransactions": p.unsaved_transactions(),
            "pages": listed(pages, doc.page_order().len(), limit),
            "devices": listed(devices, doc.devices().len(), limit),
            "conductors": listed(conductors, doc.wires().len(), limit),
            "cables": listed(cables, doc.cables().len(), limit),
            "nets": listed(nets, net_count, limit),
        })
    })
}

fn history(st: &mut Host<'_>, offset: Option<u64>, limit: Option<u64>) -> Result<Value, String> {
    st.project.read(st.session, |p, _| {
        let log = p.ledger.log();
        let n = log.len();
        let limit = usize::try_from(limit.unwrap_or(DEFAULT_HISTORY).min(MAX_HISTORY)).unwrap_or(usize::MAX);
        let offset = match offset {
            Some(o) => usize::try_from(o).unwrap_or(usize::MAX).min(n),
            None => n.saturating_sub(limit),
        };
        let txs: Vec<Value> = log
            .iter()
            .skip(offset)
            .take(limit)
            .map(|t| {
                let mut v = tx_summary(t);
                if let Some(o) = v.as_object_mut() {
                    o.insert("kind".into(), json!(ecad_file::history::tx_kind(t, log)));
                    o.insert("timeMs".into(), json!(t.time_ms));
                    o.insert("byAgent".into(), json!(matches!(t.author, Author::Agent(_))));
                }
                v
            })
            .collect();
        json!({
            "count": n,
            "offset": offset,
            "head": p.ledger.head(),
            "persisted": p.ledger.persisted(),
            "authors": log.iter().map(|t| logged_author_text(&t.author)).collect::<std::collections::BTreeSet<_>>(),
            "transactions": txs,
        })
    })
}
