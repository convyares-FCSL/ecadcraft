//! The inspector protocol, version 1 (design 029 §2).
//!
//! # Semantics
//! - A request is one JSON object: `{"v": 1, "id": "7", "op": "devices", …}`. The reply is
//!   `{"v": 1, "id": "7", "ok": …}` or `{"v": 1, "id": "7", "error": {"what", "why", "way_out"}}`.
//!   `id` is echoed as given (a string of at most 64 characters, or a number); clients match
//!   replies by `id`. Fields a server does not know are ignored, so later clients can add optional
//!   fields; removing or changing one raises [`PROTOCOL_VERSION`].
//! - Reserved for push (later): the `event` key of a message with no `id`, and the op names
//!   `subscribe` and `unsubscribe`, which version 1 answers with `unknown-op`.
//! - Lists are paged: `from` (default 0) and `count` (default 200, at most 1 000); replies carry
//!   `total`. `q` filters rows by a case-insensitive substring of their text.
//! - Replies are plain JSON values. The protocol never sends HTML and has no op that writes.
//!
//! [`handle`] is pure: no I/O beyond reading the source, and no threads.

use std::collections::{BTreeMap, BTreeSet};

use ecad_file::Store;
use ecad_file::schema::VersionKind;
use ecad_file::store::TABLES as TABLES_ORDER;
use ecad_file::verify::VerifyReport;
use ecad_model::{DeviceId, Document, Subject, TerminalId, WireId};
use serde_json::{Map, Value, json};

use crate::source::{Opened, Problem, Source, VerifyState, size_text};
use crate::text;

/// The protocol version this server speaks.
pub const PROTOCOL_VERSION: u64 = 1;

/// The server's name in `hello`.
pub const SERVER: &str = concat!("ecadinspect ", env!("CARGO_PKG_VERSION"));

/// Every op version 1 answers (`open` and `reload` are handled by the transport, which owns the
/// source).
pub const OPS: [&str; 19] = [
    "hello",
    "summary",
    "project",
    "pages",
    "devices",
    "nets",
    "wires",
    "cables",
    "branches",
    "log",
    "versions",
    "tables",
    "rows",
    "row",
    "check_chain",
    "invariants",
    "verify",
    "open",
    "reload",
];

/// The default and largest page of a list.
pub const PAGE: u64 = 200;
pub const MAX_PAGE: u64 = 1_000;
/// The most bytes of a stored row `row` returns raw.
pub const MAX_RAW: usize = 64 * 1024;
/// The largest reply (design 029 §5.1).
pub const MAX_REPLY: usize = 256 * 1024 * 1024;
/// The longest `id` and `q`.
const MAX_ID: usize = 64;
const MAX_Q: usize = 256;

/// A parsed request.
pub struct Request {
    pub id: Value,
    pub op: String,
    pub body: Map<String, Value>,
}

impl Request {
    fn u64(&self, name: &str, default: u64) -> u64 {
        self.body.get(name).and_then(Value::as_u64).unwrap_or(default)
    }

    fn str(&self, name: &str) -> Option<&str> {
        self.body.get(name).and_then(Value::as_str)
    }

    fn bool(&self, name: &str) -> bool {
        self.body.get(name).and_then(Value::as_bool).unwrap_or(false)
    }
}

/// Parse `text` as a request.
///
/// # Errors
/// The reply to send instead: `bad-request` or `protocol`.
pub fn parse(text: &str) -> Result<Request, String> {
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return Err(error(&Value::Null, &bad_request(format!("Not JSON: {e}.")))),
    };
    let Value::Object(body) = value else {
        return Err(error(&Value::Null, &bad_request("A request is a JSON object.")));
    };
    let id = match body.get("id") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) if s.chars().count() <= MAX_ID => Value::String(s.clone()),
        Some(Value::Number(n)) => Value::Number(n.clone()),
        Some(_) => return Err(error(&Value::Null, &bad_request(format!("`id` is a string of at most {MAX_ID} characters, or a number.")))),
    };
    match body.get("v") {
        None => {}
        Some(v) => match v.as_u64() {
            Some(n) if n <= PROTOCOL_VERSION => {}
            Some(n) => {
                return Err(error(
                    &id,
                    &Problem::new(
                        "protocol",
                        format!("The page speaks protocol {n}; this server speaks {PROTOCOL_VERSION}."),
                        "Use the page this server serves (reload it).",
                    ),
                ));
            }
            None => return Err(error(&id, &bad_request("`v` is the protocol version, a whole number."))),
        },
    }
    let Some(op) = body.get("op").and_then(Value::as_str).map(String::from) else {
        return Err(error(&id, &bad_request("The request has no `op`.")));
    };
    if let Some(q) = body.get("q").and_then(Value::as_str)
        && q.chars().count() > MAX_Q
    {
        return Err(error(&id, &bad_request(format!("`q` is at most {MAX_Q} characters."))));
    }
    Ok(Request { id, op, body })
}

fn bad_request(why: impl Into<String>) -> Problem {
    Problem::new("bad-request", why, r#"Send {"v":1,"op":"hello"}."#)
}

/// A success reply.
pub fn ok(id: &Value, value: Value) -> String {
    json!({"v": PROTOCOL_VERSION, "id": id, "ok": value}).to_string()
}

/// An error reply.
pub fn error(id: &Value, p: &Problem) -> String {
    json!({"v": PROTOCOL_VERSION, "id": id, "error": p.json()}).to_string()
}

/// The reply to `request` from `source` (`None`: nothing is open).
pub fn handle(source: Option<&dyn Source>, request: &str) -> String {
    let req = match parse(request) {
        Ok(r) => r,
        Err(reply) => return reply,
    };
    reply_to(source, &req)
}

/// The reply to a parsed request.
pub fn reply_to(source: Option<&dyn Source>, req: &Request) -> String {
    let out = match source {
        _ if req.op == "hello" => Ok(hello(source)),
        _ if !OPS.contains(&req.op.as_str()) || req.op == "open" || req.op == "reload" => {
            if req.op == "open" || req.op == "reload" {
                Err(bad_request(format!("`{}` is answered by the server, which was not asked through its transport.", req.op)))
            } else {
                Err(Problem::new("unknown-op", format!("Unknown op `{}`; the ops are: {}.", req.op, OPS.join(", ")), r#"Send {"v":1,"op":"hello"}."#))
            }
        }
        None => Err(not_open()),
        Some(s) if s.changed() => {
            Err(Problem::new("changed", "The file changed on disk since it was opened.", "Reload (it re-opens the file; never automatic)."))
        }
        Some(s) => op(s, req),
    };
    let text = match out {
        Ok(v) => ok(&req.id, v),
        Err(p) => return error(&req.id, &p),
    };
    if text.len() > MAX_REPLY {
        return error(
            &req.id,
            &Problem::new("too-large", format!("The reply is {} , over the limit of 256 MiB.", size_text(text.len() as u64)), "Ask for fewer rows."),
        );
    }
    text
}

/// `not-open`.
pub fn not_open() -> Problem {
    Problem::new("not-open", "No file is open.", "Drop a .ecad file on the page, or start with: ecadinspect serve <file>")
}

/// What `source` offers.
fn capabilities(s: &dyn Source) -> Vec<&'static str> {
    let mut out = Vec::new();
    if s.store().is_some() {
        out.push("store");
        if s.problem().is_none() {
            out.push("verify");
        }
    }
    if s.opened().is_ok() {
        out.push("model");
    }
    out
}

fn hello(source: Option<&dyn Source>) -> Value {
    let src = source.map(|s| {
        let i = s.describe();
        json!({"kind": i.kind, "name": i.name, "size": i.size})
    });
    json!({
        "protocol": PROTOCOL_VERSION,
        "server": SERVER,
        "source": src,
        "ops": OPS,
        "capabilities": source.map(capabilities).unwrap_or_default(),
    })
}

fn op(s: &dyn Source, req: &Request) -> Result<Value, Problem> {
    match req.op.as_str() {
        "summary" => Ok(summary(s)),
        "project" => model(s).map(|o| project(&o.doc)),
        "pages" => model(s).map(|o| paged(req, pages(&o.doc))),
        "devices" => model(s).map(|o| paged(req, devices(&o.doc, req.bool("detail")))),
        "nets" => model(s).map(|o| paged(req, nets(&o.doc))),
        "wires" => model(s).map(|o| paged(req, wires(&o.doc))),
        "cables" => model(s).map(|o| paged(req, cables(&o.doc))),
        "branches" => store(s).and_then(|st| branches(st).map(|rows| json!({"total": rows.len(), "rows": rows}))),
        "log" => store(s).and_then(|st| log(st, s, req)),
        "versions" => store(s).and_then(versions).map(|rows| json!({"total": rows.len(), "rows": rows})),
        "tables" => store(s).and_then(tables),
        "rows" => store(s).and_then(|st| rows(st, req)),
        "row" => store(s).and_then(|st| row(st, req)),
        "check_chain" => match s.chain() {
            Some(c) => Ok(report_json(c)),
            None => Err(s
                .problem()
                .cloned()
                .unwrap_or_else(|| Problem::new("unavailable", "The chain was not checked.", "Open a version 2 project file."))),
        },
        "invariants" => model(s).map(|o| invariants(&o.doc)),
        "verify" => Ok(verify_json(&s.verify())),
        _ => Err(Problem::new("unknown-op", format!("Unknown op `{}`.", req.op), r#"Send {"v":1,"op":"hello"}."#)),
    }
}

fn model(s: &dyn Source) -> Result<&Opened, Problem> {
    s.opened().map_err(Problem::clone)
}

fn store(s: &dyn Source) -> Result<&Store, Problem> {
    s.store().ok_or_else(|| {
        s.problem()
            .cloned()
            .unwrap_or_else(|| Problem::new("unavailable", "The file has no tables this build reads.", "Open a version 2 project file."))
    })
}

fn engine(e: impl std::fmt::Display) -> Problem {
    Problem::new("unreadable", format!("The database could not be read: {e}."), "Check the app opens it; otherwise report the file.")
}

// ------------------------------------------------------------------ paging

/// The lower-case text of every string in `v` (for `q`).
fn text_of(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => {
            out.push_str(&s.to_lowercase());
            out.push('\u{1f}');
        }
        Value::Number(n) => {
            out.push_str(&n.to_string());
            out.push('\u{1f}');
        }
        Value::Array(a) => a.iter().for_each(|x| text_of(x, out)),
        Value::Object(o) => o.values().for_each(|x| text_of(x, out)),
        _ => {}
    }
}

fn window(req: &Request) -> (usize, usize) {
    let from = usize::try_from(req.u64("from", 0)).unwrap_or(usize::MAX);
    let count = usize::try_from(req.u64("count", PAGE).min(MAX_PAGE)).unwrap_or(0);
    (from, count)
}

/// `rows` filtered by `q` and cut to the requested window.
fn paged(req: &Request, rows: Vec<Value>) -> Value {
    let q = req.str("q").map(str::to_lowercase).filter(|q| !q.is_empty());
    let rows: Vec<Value> = match &q {
        Some(q) => rows
            .into_iter()
            .filter(|r| {
                let mut t = String::new();
                text_of(r, &mut t);
                t.contains(q.as_str())
            })
            .collect(),
        None => rows,
    };
    let (from, count) = window(req);
    let total = rows.len();
    let page: Vec<Value> = rows.into_iter().skip(from).take(count).collect();
    json!({"total": total, "from": from.min(total), "rows": page})
}

// ------------------------------------------------------------------ summary and checks

/// A Verify report as JSON.
pub fn report_json(r: &VerifyReport) -> Value {
    json!({
        "ok": r.ok,
        "lines": r.lines,
        "failures": r.failures,
        "transactions": r.transactions,
        "snapshots": r.snapshots,
        "full_checks": r.full_checks,
        "unreferenced_chunks": r.unreferenced_chunks,
    })
}

fn verify_json(v: &VerifyState) -> Value {
    match v {
        VerifyState::NotRun => json!({"state": "not-run"}),
        VerifyState::Running { elapsed_ms } => json!({"state": "running", "elapsed_ms": elapsed_ms}),
        VerifyState::Done { elapsed_ms, report } => {
            json!({"state": if report.ok { "done" } else { "failed" }, "elapsed_ms": elapsed_ms, "report": report_json(report)})
        }
    }
}

fn summary(s: &dyn Source) -> Value {
    let info = s.describe();
    let mut out = json!({
        "source": {"kind": info.kind, "name": info.name, "size": info.size, "size_text": size_text(info.size)},
        "problem": s.problem().map(Problem::json),
        "capabilities": capabilities(s),
        "chain": s.chain().map(|c| json!({"ok": c.ok, "failures": c.failures})),
        "verify": verify_json(&s.verify()),
    });
    if let Some(p) = s.problem() {
        match p.what {
            "not-ecad" | "unreadable" => {
                out["head_hex"] = json!(text::hex(s.head().get(..64).unwrap_or(s.head())));
            }
            _ => {}
        }
    }
    if let Some(st) = s.store()
        && let Ok(r) = st.reader()
    {
        let mut meta = Map::new();
        if let Ok(all) = r.meta_all() {
            for (k, v) in all {
                if k == "cache" {
                    continue;
                }
                let value = serde_json::from_slice::<Value>(&v).unwrap_or_else(|_| json!(String::from_utf8_lossy(&v)));
                meta.insert(k, value);
            }
        }
        out["meta"] = Value::Object(meta);
        if let Ok(b) = r.branches() {
            out["branches"] = json!(b.len());
        }
    }
    match s.opened() {
        Ok(o) => {
            let r = &o.report;
            let state = if r.damage.is_some() { "damaged" } else { "clean" };
            out["open"] = json!({
                "state": state,
                "branch": o.branch,
                "head": r.head,
                "verified_seq": r.verified_seq,
                "snapshot_seq": r.snapshot_seq,
                "tail": r.tail,
                "repaired": r.repaired,
                "damage": r.damage.as_ref().map(|d| json!({"seq": d.seq, "detail": d.detail})),
            });
            let d = &o.doc;
            out["counts"] = json!({
                "pages": d.pages.len(),
                "devices": d.devices.len(),
                "wires": d.wires.len(),
                "cables": d.cables.len(),
                "nets": d.connectivity().len(),
                "items": d.items.len(),
            });
        }
        Err(p) => out["open"] = json!({"state": "failed", "error": p.json()}),
    }
    out
}

fn invariants(d: &Document) -> Value {
    let check = d.check();
    let index = d.check_index();
    let mismatches = ecad_model::derive::mismatches(d, 1e-6);
    let shown: Vec<Value> = mismatches
        .iter()
        .take(200)
        .map(|m| json!({"route": format!("({})", m.route.short()), "end": format!("{:?}", m.end), "point": point_name(d, m.terminal), "page": page_number(d, m.page)}))
        .collect();
    json!({
        "ok": check.is_ok() && index.is_ok() && mismatches.is_empty(),
        "invariants": check.as_ref().err().map(ToString::to_string),
        "index": index.as_ref().err().map(ToString::to_string),
        "mismatches": mismatches.len(),
        "mismatch_rows": shown,
    })
}

// ------------------------------------------------------------------ model views

fn page_number(d: &Document, p: ecad_model::PageId) -> String {
    ecad_model::book::page_number(d, p)
}

fn tag_of(d: &ecad_model::Device) -> Option<&str> {
    d.tag()
}

fn device_name(d: &Document, c: DeviceId) -> String {
    d.devices.get(&c).and_then(tag_of).map(str::to_string).unwrap_or_else(|| format!("({})", c.short()))
}

fn point_name(d: &Document, t: TerminalId) -> String {
    match d.terminals.get(&t) {
        Some(term) => {
            let own = term.label.clone().or_else(|| term.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| t.short());
            format!("{}:{own}", device_name(d, term.component))
        }
        None => format!("({})", t.short()),
    }
}

fn wire_number(d: &Document, c: WireId) -> Option<String> {
    d.wires.get(&c).and_then(|w| w.number_text()).map(str::to_string)
}

fn wire_name(d: &Document, c: WireId) -> String {
    wire_number(d, c).unwrap_or_else(|| format!("({})", c.short()))
}

fn project(d: &Document) -> Value {
    let mut props = Map::new();
    for (k, v) in &d.properties {
        props.insert(k.clone(), json!(v));
    }
    // Design 072 §5.3 (A15): the revision rows, numbered from 1.
    let revisions: BTreeMap<String, Value> = d.project.revisions.iter().zip(1u32..).map(|(r, n)| (n.to_string(), json!(r))).collect();
    // Design 072 §5.3 (A15): the location and function lists and the project's defaults.
    let list = |l: Vec<(String, String, Option<String>, Vec<String>)>| -> Vec<Value> {
        l.into_iter().map(|(id, code, tag, descriptions)| json!({"id": id, "code": code, "tag": tag, "descriptions": descriptions})).collect()
    };
    let entry = |id: String, code: &ecad_model::ListCode, tag: &Option<ecad_model::ListCode>, ds: &[ecad_model::ListDescription]| {
        (id, code.to_string(), tag.as_ref().map(ToString::to_string), ds.iter().map(ToString::to_string).collect())
    };
    let locations = list(d.project.locations.iter().map(|e| entry(e.id.to_string(), &e.code, &e.tag, &e.descriptions)).collect());
    let functions = list(d.project.functions.iter().map(|e| entry(e.id.to_string(), &e.code, &e.tag, &e.descriptions)).collect());
    json!({
        "properties": props,
        "info": d.project.info,
        // Design 072 §5.2 (A15): each folder with its place and start number.
        "folders": d.folder_order.iter().filter_map(|id| d.folders.get(id).map(|f| json!({
            "id": id.to_string(),
            "name": f.name,
            "parent": f.parent.and_then(|p| d.folders.get(&p)).map(|p| p.name.clone()),
            "start": f.start,
        }))).collect::<Vec<_>>(),
        "locations": locations,
        "functions": functions,
        "location": d.project.location.map(|l| location_code(d, l)),
        "function": d.project.function.map(|f| function_code(d, f)),
        "revisions": revisions,
        "reference_style": format!("{:?}", d.settings.reference_style()),
        // Design 072 F40 (A15): what one wire number counts.
        "number_by": ecad_model::Word::word(d.settings.wiring.number_by),
        "versions": d.versions,
        "counts": {
            "pages": d.pages.len(),
            "folders": d.folders.len(),
            "devices": d.devices.len(),
            "connection_points": d.terminals.len(),
            "wires": d.wires.len(),
            "cables": d.cables.len(),
            "nets": d.connectivity().len(),
            "definitions": d.definitions.len(),
            "items": d.items.len(),
        },
    })
}

/// A location id as its code, or `? (id)` when the list has no such entry.
fn location_code(d: &Document, id: ecad_model::LocationId) -> String {
    d.project.location(id).map_or_else(|| format!("? ({id})"), |e| e.code.to_string())
}

/// A function id as its code, or `? (id)` when the list has no such entry.
fn function_code(d: &Document, id: ecad_model::FunctionId) -> String {
    d.project.function(id).map_or_else(|| format!("? ({id})"), |e| e.code.to_string())
}

fn pages(d: &Document) -> Vec<Value> {
    let mut items: BTreeMap<ecad_model::PageId, usize> = BTreeMap::new();
    for it in d.items.values() {
        *items.entry(it.page).or_default() += 1;
    }
    let mut devices: BTreeMap<ecad_model::PageId, BTreeSet<DeviceId>> = BTreeMap::new();
    let mut wires: BTreeMap<ecad_model::PageId, BTreeSet<WireId>> = BTreeMap::new();
    for r in d.representations.values() {
        match r.subject {
            Subject::Device(c) => {
                devices.entry(r.page).or_default().insert(c);
            }
            Subject::Wire(c) => {
                wires.entry(r.page).or_default().insert(c);
            }
            _ => {}
        }
    }
    d.page_order
        .iter()
        .filter_map(|id| d.pages.get(id).map(|p| (id, p)))
        .map(|(id, p)| {
            let folder = ecad_model::book::folder_of(d, *id).and_then(|f| d.folders.get(&f)).map(|f| f.name.clone());
            json!({
                "id": id.to_string(),
                "number": page_number(d, *id),
                "title": p.title,
                "folder": folder,
                "kind": p.kind.generated_kind().map_or("schematic", ecad_model::Word::word),
                "strip": p.kind.strip().map(|c| c.to_string()),
                "page_type": p.page_type.as_ref().map(ToString::to_string),
                "sheet": p.sheet.map(|s| s.text()),
                "title_block": p.title_block.as_ref().map(ToString::to_string),
                "location": p.location.map(|l| location_code(d, l)),
                "function": p.function.map(|f| function_code(d, f)),
                "display": p.display,
                "items": items.get(id).copied().unwrap_or(0),
                "devices": devices.get(id).map_or(0, BTreeSet::len),
                "wires": wires.get(id).map_or(0, BTreeSet::len),
            })
        })
        .collect()
}

fn devices(d: &Document, detail: bool) -> Vec<Value> {
    let mut tags: BTreeMap<&str, usize> = BTreeMap::new();
    for c in d.devices.values() {
        if let Some(t) = tag_of(c) {
            *tags.entry(t).or_default() += 1;
        }
    }
    let mut attached: BTreeMap<TerminalId, Vec<WireId>> = BTreeMap::new();
    for (id, w) in &d.wires {
        for t in &w.ends {
            attached.entry(*t).or_default().push(*id);
        }
    }
    let mut on_pages: BTreeMap<DeviceId, BTreeSet<usize>> = BTreeMap::new();
    let order: BTreeMap<ecad_model::PageId, usize> = d.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    for r in d.representations.values() {
        if let Subject::Device(c) = r.subject
            && let Some(i) = order.get(&r.page)
        {
            on_pages.entry(c).or_default().insert(*i);
        }
    }
    let mut out: Vec<(String, Value)> = d
        .devices
        .iter()
        .map(|(id, c)| {
            let tag = tag_of(c);
            let points: Vec<Value> = d
                .terminals_of(*id)
                .map(|(t, term)| {
                    let name = term.label.clone().or_else(|| term.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| t.short());
                    let wires: Vec<String> = attached.get(&t).into_iter().flatten().map(|w| wire_name(d, *w)).collect();
                    // Design 072 F38 (A15): how many conductors it accepts.
                    json!({"name": name, "wires": wires, "accepts": term.accepts.get()})
                })
                .collect();
            let pages: Vec<String> =
                on_pages.get(id).into_iter().flatten().filter_map(|i| d.page_order.get(*i)).map(|p| page_number(d, *p)).collect();
            let mut row = json!({
                "id": id.to_string(),
                "tag": tag,
                "duplicate": tag.as_ref().is_some_and(|t| tags.get(t).copied().unwrap_or(0) > 1),
                "function": c.function.map(|f| function_code(d, f)),
                "function_fixed": c.function_fixed,
                "location": c.location.map(|l| location_code(d, l)),
                "location_fixed": c.location_fixed,
                "definition": c.definition.and_then(|def| d.definitions.get(&def)).map(|def| def.name.clone()),
                "pages": pages,
                "points": points,
            });
            if detail {
                // Design 072 §2.1: the device's text fields, then what is left in its bag.
                for f in ecad_model::TextField::ALL {
                    row[f.key()] = json!(c.text(f));
                }
                row["properties"] = json!(c.properties);
            }
            (tag.unwrap_or_default().to_string(), row)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, v)| v).collect()
}

fn nets(d: &Document) -> Vec<Value> {
    let mut out: Vec<(Option<String>, Value)> = d
        .connectivity()
        .nets()
        .map(|(_, n)| {
            let number = ecad_model::wiring::net_number(d, n);
            let pot = ecad_model::wiring::net_potential(d, n);
            let (page, column) = ecad_model::wiring::net_place(d, n);
            let points: Vec<String> = n.terminals.iter().map(|t| point_name(d, *t)).collect();
            let row = json!({
                "number": number,
                "fixed": ecad_model::wiring::net_fixed(n),
                "potential": pot.name,
                "place": format!("{page}/{column}"),
                "points": points,
                "wires": n.wires.len(),
            });
            (number, row)
        })
        .collect();
    out.sort_by(|a, b| (a.0.is_none(), &a.0).cmp(&(b.0.is_none(), &b.0)));
    out.into_iter().map(|(_, v)| v).collect()
}

fn wires(d: &Document) -> Vec<Value> {
    let mut in_cable: BTreeMap<WireId, (String, Option<String>)> = BTreeMap::new();
    for (id, k) in &d.cables {
        let tag = k.properties.get(ecad_model::tag::TAG_KEY).cloned().unwrap_or_else(|| format!("({})", id.short()));
        for w in k.cores.iter().filter_map(|m| m.wire()) {
            in_cable.insert(w, (tag.clone(), k.cores.iter().find(|m| m.wire() == Some(w)).and_then(|m| m.label_str().map(str::to_string))));
        }
    }
    d.wires
        .iter()
        .map(|(id, w)| {
            let ends: Vec<String> = w.ends.iter().map(|t| point_name(d, *t)).collect();
            let cable = in_cable.get(id);
            json!({
                "id": id.to_string(),
                "number": wire_number(d, *id),
                "ends": ends,
                "net": w.net.to_string(),
                "conductor": w.conductor.to_string(),
                "net_order": w.net_order,
                "order_fixed": w.order_fixed,
                "size": w.size.map(|s| s.to_string()),
                "colours": w.colours.as_ref().map(ToString::to_string),
                "fixed": w.number_fixed(),
                "potential": w.potential_name(),
                "rating": w.rating.values().into_iter().map(|(k, v)| format!("{k}: {v}")).chain(w.load.map(|a| format!("load: {a}"))).collect::<Vec<_>>(),
                "protected_by": w.protected_by.map(|p| protected_name(d, p)),
                "cable": cable.map(|c| c.0.clone()),
                "core": cable.and_then(|c| c.1.clone()),
            })
        })
        .collect()
}

/// The device a wire or cable names as its protection, in words: `-F01:3`, or the id of a device
/// that is not there.
fn protected_name(d: &Document, p: ecad_model::wire::ProtectedBy) -> String {
    let device = if d.devices.contains_key(&p.device) { device_name(d, p.device) } else { format!("{} (not in the project)", p.device.short()) };
    p.position.map_or_else(|| device.clone(), |n| format!("{device}:{n}"))
}

/// Where a cable core is, in words: its wire, or the line it waits on with its landed end
/// (owner ruling, 2026-11-18), or unplaced.
fn core_place(d: &Document, p: ecad_model::CorePosition) -> String {
    match p {
        ecad_model::CorePosition::Wire(w) => wire_name(d, w),
        ecad_model::CorePosition::Route { route, landed: Some(t) } => {
            format!("line {} (no wire yet, landed at {})", route.short(), point_name(d, t))
        }
        ecad_model::CorePosition::Route { route, landed: None } => format!("line {} (no wire yet, no end landed)", route.short()),
        ecad_model::CorePosition::Unplaced => "unplaced".into(),
    }
}

fn cables(d: &Document) -> Vec<Value> {
    d.cables
        .iter()
        .map(|(id, k)| {
            let members: Vec<Value> = k
                .cores
                .iter()
                .map(|m| {
                    json!({
                        "wire": core_place(d, m.position),
                        "core": m.label_str(),
                        "purpose": if m.is_spare() { "spare" } else { "in use" },
                        "colours": m.colours.as_ref().map(ToString::to_string),
                    })
                })
                .collect();
            let mut ends: [BTreeSet<String>; 2] = [BTreeSet::new(), BTreeSet::new()];
            for m in &k.cores {
                if let Some(w) = m.wire().and_then(|w| d.wires.get(&w)) {
                    for (side, t) in ends.iter_mut().zip(w.ends.iter()) {
                        if let Some(term) = d.terminals.get(t) {
                            side.insert(device_name(d, term.component));
                        }
                    }
                }
            }
            json!({
                "id": id.to_string(),
                "tag": k.properties.get(ecad_model::tag::TAG_KEY),
                "shield": k.shield.map(ecad_model::words::Word::word),
                "protected_by": k.protected_by.map(|p| protected_name(d, p)),
                "members": members,
                "ends": ends,
            })
        })
        .collect()
}

// ------------------------------------------------------------------ store views

fn branches(st: &Store) -> Result<Vec<Value>, Problem> {
    let r = st.reader().map_err(engine)?;
    let all = r.branches().map_err(engine)?;
    let names: BTreeMap<String, String> = all.iter().map(|(id, b)| (id.hex(), b.name.clone())).collect();
    all.iter()
        .map(|(id, b)| {
            let rows = r.tx_count(*id).map_err(engine)?;
            let fork = match (&b.parent, &b.fork_point) {
                (Some(p), Some(f)) => json!({"parent": names.get(p).cloned().unwrap_or_else(|| p.clone()), "seq": f.seq}),
                _ => Value::Null,
            };
            Ok(json!({
                "name": b.name,
                "id": id.hex(),
                "status": format!("{:?}", b.status),
                "release_line": b.release_line,
                "head": b.head.seq,
                "fork": fork,
                "rows": rows,
            }))
        })
        .collect()
}

fn log(st: &Store, s: &dyn Source, req: &Request) -> Result<Value, Problem> {
    let r = st.reader().map_err(engine)?;
    let default = s.opened().map(|o| o.branch.clone()).unwrap_or_else(|_| "main".into());
    let name = req.str("branch").map(String::from).unwrap_or(default);
    let id = text::branch_named(&r, &name).map_err(|e| Problem::new("not-found", e.to_string(), "Pick a branch from the History tab."))?;
    let head = r.last_seq(id).map_err(engine)?.unwrap_or(0);
    let (from, count) = window(req);
    let from = from as u64;
    let hi = head.saturating_sub(from);
    let lo = hi.saturating_sub(count as u64).saturating_add(1).max(1);
    let mut rows = Vec::new();
    if hi >= 1 && count > 0 {
        for (seq, flag, chain, body) in r.tx_range(id, lo..=hi).map_err(engine)?.into_iter().rev() {
            let record = ecad_file::pack::unpack(flag, &body);
            let value = record.as_ref().ok().and_then(|b| serde_json::from_slice::<Value>(b).ok());
            let v = value.clone().unwrap_or(Value::Null);
            let mut row = json!({
                "seq": seq,
                "time_ms": v.get("time_ms"),
                "author": v.get("author"),
                "kind": v.get("kind"),
                "label": v.get("label"),
                "ops": v.get("ops").and_then(Value::as_array).map(Vec::len),
                "stored": body.len() + 33,
                "compression": text::flag_name(flag),
                "chain": ecad_file::chain::hex(&chain),
                "decodes": value.is_some(),
            });
            if req.bool("detail") {
                row["record"] = v;
            }
            rows.push(row);
        }
    }
    Ok(json!({"branch": name, "total": head, "from": from.min(head), "rows": rows}))
}

fn versions(st: &Store) -> Result<Vec<Value>, Problem> {
    let r = st.reader().map_err(engine)?;
    let mut out = Vec::new();
    for (id, b) in r.branches().map_err(engine)? {
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto, VersionKind::Merge] {
            for v in r.versions(id, kind).map_err(engine)? {
                out.push(json!({
                    "branch": b.name,
                    "kind": format!("{kind:?}"),
                    "name": v.name,
                    "seq": v.point.seq,
                    "chunks": v.chunks.len(),
                    "undo": v.undo.len() as u64 + v.undo_hidden,
                    "redo": v.redo.len() as u64 + v.redo_hidden,
                    "release": v.release.as_ref().map(|rel| json!(rel.fields)),
                    "description": v.description,
                    "merge": v.merge.as_ref().map(|m| json!({"source": m.source, "source_head": m.source_head.seq})),
                }));
            }
        }
    }
    Ok(out)
}

fn tables(st: &Store) -> Result<Value, Problem> {
    let r = st.reader().map_err(engine)?;
    let rows: Vec<Value> = r
        .stats()
        .map_err(engine)?
        .iter()
        .map(|t| json!({"name": t.name, "rows": t.rows, "stored": t.stored_bytes, "metadata": t.metadata_bytes, "fragmented": t.fragmented_bytes}))
        .collect();
    let meta: Map<String, Value> = r
        .meta_all()
        .map_err(engine)?
        .into_iter()
        .map(|(k, v)| (k, serde_json::from_slice::<Value>(&v).unwrap_or_else(|_| json!(String::from_utf8_lossy(&v)))))
        .collect();
    Ok(json!({"tables": rows, "meta": meta}))
}

/// A stored key in words, by table.
fn key_text(table: &str, key: &[u8]) -> String {
    match table {
        "meta" => String::from_utf8_lossy(key).into_owned(),
        "transactions" => match (key.get(..16), key.get(16..24).and_then(|b| <[u8; 8]>::try_from(b).ok())) {
            (Some(b), Some(seq)) => format!("{} #{}", text::hex(b), u64::from_be_bytes(seq)),
            _ => text::hex(key),
        },
        "versions" => match (key.get(..16), key.get(16), key.get(17..)) {
            (Some(b), Some(kind), Some(name)) => format!("{} {} {}", text::hex(b), kind_name(*kind), String::from_utf8_lossy(name)),
            _ => text::hex(key),
        },
        _ => text::hex(key),
    }
}

fn kind_name(byte: u8) -> String {
    [VersionKind::User, VersionKind::Revision, VersionKind::Auto, VersionKind::Merge]
        .into_iter()
        .find(|k| k.byte() == byte)
        .map_or_else(|| format!("kind {byte}"), |k| format!("{k:?}"))
}

fn table_of(req: &Request) -> Result<&'static str, Problem> {
    let want = req.str("table").unwrap_or_default();
    TABLES_ORDER.iter().copied().find(|t| *t == want).ok_or_else(|| {
        Problem::new("bad-request", format!("`table` is one of: {}.", TABLES_ORDER.join(", ")), r#"Send {"v":1,"op":"tables"} for the list."#)
    })
}

fn stored_flag(table: &str, value: &[u8]) -> Option<&'static str> {
    match table {
        "transactions" | "chunks" => value.first().map(|f| text::flag_name(*f)),
        _ => None,
    }
}

fn rows(st: &Store, req: &Request) -> Result<Value, Problem> {
    let table = table_of(req)?;
    let r = st.reader().map_err(engine)?;
    let q = req.str("q").map(str::to_lowercase).filter(|q| !q.is_empty());
    let (from, count) = window(req);
    let mut total = 0usize;
    let mut out = Vec::new();
    r.raw_rows(|t, key, value| {
        if t != table {
            return Ok(());
        }
        let text = key_text(t, key);
        if let Some(q) = &q
            && !text.to_lowercase().contains(q.as_str())
        {
            return Ok(());
        }
        if total >= from && out.len() < count {
            out.push(json!({"key": text::hex(key), "key_text": text, "size": value.len(), "compression": stored_flag(t, value)}));
        }
        total += 1;
        Ok(())
    })
    .map_err(engine)?;
    Ok(json!({"table": table, "total": total, "from": from.min(total), "rows": out}))
}

fn row(st: &Store, req: &Request) -> Result<Value, Problem> {
    let table = table_of(req)?;
    let want = req.str("key").unwrap_or_default().to_ascii_lowercase();
    let raw = req.str("form") == Some("raw");
    let r = st.reader().map_err(engine)?;
    let mut found: Option<Value> = None;
    r.raw_rows(|t, key, value| {
        if found.is_some() || t != table || text::hex(key) != want {
            return Ok(());
        }
        let mut v = json!({"table": t, "key": want, "key_text": key_text(t, key), "size": value.len(), "compression": stored_flag(t, value)});
        if raw {
            let cut = value.get(..MAX_RAW.min(value.len())).unwrap_or_default();
            v["form"] = json!("raw");
            v["hex"] = json!(text::hex(cut));
            v["truncated"] = json!(value.len() > MAX_RAW);
        } else {
            v["form"] = json!("readable");
            match text::decode_raw(t, value) {
                Ok((_, decoded)) => v["decoded"] = decoded,
                Err(e) => v["decode_error"] = json!(e),
            }
        }
        found = Some(v);
        Ok(())
    })
    .map_err(engine)?;
    found.ok_or_else(|| Problem::new("not-found", format!("No row of `{table}` has the key {want}."), "List the keys with the `rows` op."))
}
