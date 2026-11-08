//! The ECAD tools: `ecad_project`, `ecad_history` and `ecad_inspect` (read), and `save` (write).

use ecad_cadcraft::debug::tx_kind;
use ecad_model::{Author, derive};
use serde_json::{Map, Value, json};

use crate::{MAX_TRANSACTIONS_PER_CALL, State, author_text, tx_summary};

/// Names of the tools this module answers.
pub const ECAD_TOOLS: &[&str] = &["ecad_project", "ecad_history", "ecad_inspect", "save"];

/// Default and largest `limit` of the list tools.
const DEFAULT_LIMIT: u64 = 500;
const MAX_LIMIT: u64 = 10_000;
const DEFAULT_HISTORY: u64 = 100;
const MAX_HISTORY: u64 = 1_000;

pub fn is_ecad_tool(name: &str) -> bool {
    ECAD_TOOLS.contains(&name)
}

/// What the server says about itself at `initialize`.
pub fn instructions(writes: bool, agent: &str) -> String {
    let mode = if writes {
        "Writes are ON: ECAD commands through `execute` change the project, and `save` writes it to its file."
    } else {
        "Writes are OFF: the project can be read but not changed or saved (the server was started without --write, \
         or the project opened read-only)."
    };
    format!(
        "ECAD electrical design project (headless; CADCraft drawing engine). Read it with ecad_project (pages, \
         components, terminals, conductors, cables, nets), ecad_history (transactions and their authors) and \
         ecad_inspect, and look at the active page with render, query_entities and inspect_drawing. Change it only with \
         `execute` and an ECAD command (list_commands shows them with their parameters: wire, place, \
         convertcomponent, addterminal, connect, disconnect, cableslash, cableview, newpage, openpage, setprop, \
         ecaddelete, ...). Each change is one transaction recorded as `agent {agent}`, and `execute u` undoes it in one \
         step. {mode}"
    )
}

/// The `execute` tool's description in ECAD's server.
pub fn execute_description(writes: bool) -> String {
    let mode = if writes { "Writes are on." } else { "Writes are off: only read-only commands run." };
    format!(
        "Run one command with JSON parameters (never prompts). ECAD commands (list_commands shows ids and parameter \
         docs) change the project exactly as for a person: the same checks and refusals, and each change is one \
         transaction authored by this agent and one step of `u` (undo) / `redo`. Read-only commands: ecadinspect, \
         entities, texts, shapes, usage, loops, drawing.inspect, getvar, openpage, document.switch, cablerefs, \
         cablegoto. File and document commands (open, new, close, saveas, export, ...) are refused; save with the `save` tool. At most \
         {MAX_TRANSACTIONS_PER_CALL} transaction per call. {mode} Examples: wire {{points:[[0,0],[60,0]]}}, \
         cableslash {{from:[28,-3], to:[32,18]}}, cableview {{cable:\"<id>\", view:\"fan\"}}, u {{}}."
    )
}

/// The ECAD tool definitions.
pub fn definitions(writes: bool) -> Vec<Value> {
    let save = if writes {
        "Save the project to its .ecad file (QSAVE). Save As, open, new and close stay with the person who started the server."
    } else {
        "Save the project to its .ecad file. Refused now: writes are off (start the server with --write)."
    };
    vec![
        json!({"name": "ecad_project", "description": "The ECAD project: name, file, whether writes are on, head and unsaved changes, pages, components with their connection points (`terminals`), conductors with their ends and cable, cables with their members in order, and nets (derived from the stored attachments). Each list has `count` and at most `limit` entries (default 500).",
            "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer", "minimum": 0}}}}),
        json!({"name": "ecad_history", "description": "The project's transactions in order: seq, id, label, kind (edit, undo of #n, redo of #n), author (`user` or `agent <name>`), time and op count. Page with offset and limit (default: the last 100).",
            "inputSchema": {"type": "object", "properties": {"offset": {"type": "integer", "minimum": 0}, "limit": {"type": "integer", "minimum": 0}}}}),
        json!({"name": "ecad_inspect", "description": "ecadinspect's read-only JSON snapshot of the project (live, or at transaction `seq`).",
            "inputSchema": {"type": "object", "properties": {"seq": {"type": "integer", "minimum": 0}}}}),
        json!({"name": "save", "description": save, "inputSchema": {"type": "object", "properties": {}}}),
    ]
}

fn text(v: &Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default()}]})
}

fn err_text(e: &str) -> Value {
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
pub fn call(st: &mut State, name: &str, args: &Value) -> Result<Value, String> {
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
            Ok(match st.session().execute("ecadinspect", &params) {
                Ok(v) => text(&v),
                Err(e) => err_text(&e.to_string()),
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
        other => Err(crate::not_offered(other)),
    }
}

fn save(st: &mut State) -> Result<Value, String> {
    st.may_write()?;
    st.focus_project()?;
    st.session().execute("qsave", &json!({})).map_err(|e| e.to_string())?;
    let p = st.project.clone();
    p.read(st.session(), |p, _| {
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

fn project(st: &mut State, limit: u64) -> Result<Value, String> {
    let write = st.write;
    let read_only = st.read_only()?;
    let agent = st.agent.clone();
    let p = st.project.clone();
    p.read(st.session(), |p, _| {
        let doc = p.ledger.document();
        let pages = doc.page_order.iter().map(|id| {
            json!({
                "id": id.hex(),
                "title": doc.pages.get(id).map(|pg| pg.title.clone()),
                "open": p.pages.get(id).is_some_and(|v| v.uid.is_some()),
            })
        });
        let components = doc.components.iter().map(|(id, c)| {
            let terminals: Vec<Value> =
                doc.terminals_of(*id).map(|(t, term)| json!({"id": t.hex(), "label": term.label})).collect();
            json!({"id": id.hex(), "properties": c.properties, "terminals": terminals})
        });
        let cable_of = |c| doc.cables.iter().find(|(_, k)| k.members.iter().any(|m| m.conductor == c)).map(|(id, _)| id.hex());
        let conductors = doc.conductors.iter().map(|(id, c)| {
            json!({
                "id": id.hex(),
                "properties": c.properties,
                "ends": {"A": c.end(ecad_model::End::A).map(|t| t.hex()), "B": c.end(ecad_model::End::B).map(|t| t.hex())},
                "cable": cable_of(*id),
            })
        });
        let cables = doc.cables.iter().map(|(id, c)| {
            let members: Vec<Value> =
                c.members.iter().map(|m| json!({"conductor": m.conductor.hex(), "core": m.core})).collect();
            json!({"id": id.hex(), "properties": c.properties, "members": members})
        });
        let nets = derive::nets(doc);
        let net_count = nets.len();
        let nets = nets.into_iter().map(|n| {
            json!({
                "terminals": n.terminals.iter().map(|t| t.hex()).collect::<Vec<_>>(),
                "conductors": n.conductors.iter().map(|c| c.hex()).collect::<Vec<_>>(),
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
            "pages": listed(pages, doc.page_order.len(), limit),
            "components": listed(components, doc.components.len(), limit),
            "conductors": listed(conductors, doc.conductors.len(), limit),
            "cables": listed(cables, doc.cables.len(), limit),
            "nets": listed(nets, net_count, limit),
        })
    })
}

fn history(st: &mut State, offset: Option<u64>, limit: Option<u64>) -> Result<Value, String> {
    let p = st.project.clone();
    p.read(st.session(), |p, _| {
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
                    o.insert("kind".into(), json!(tx_kind(t, log)));
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
            "authors": log.iter().map(|t| author_text(&t.author)).collect::<std::collections::BTreeSet<_>>(),
            "transactions": txs,
        })
    })
}
