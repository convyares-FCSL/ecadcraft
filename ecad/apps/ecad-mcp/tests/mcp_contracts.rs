//! Brief 011 acceptance: ECAD over MCP, headless. Every test drives the server through JSON-RPC
//! lines (the stdio framing), in-process; `stdio_*` spawns the built binary.
//!
//! Criteria: 1 tools and commands listed; 2 the brief 004/005 examples give the JSON forms'
//! transactions with an agent author, and `u` undoes each in one step; 3 writes off refuse every
//! write; 4 raw CADCraft commands cannot get round ECAD's refusals; 5 save only with `--write`;
//! 6 hostile input; 7 the binary over real stdio (run by CI on Linux and Windows).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use cadcraft_engine::Session;
use ecad_cadcraft::{Persistence, Project, ProjectHandle};
use ecad_mcp::{EcadMcp, MAX_BATCH, MAX_REQUEST_BYTES, Options, policy};
use ecad_model::{Author, Document, Transaction};
use serde_json::{Value, json};

const AGENT: &str = "test-agent";
const SEED: u64 = 0x0E_CAD0_0110;

struct Client {
    s: EcadMcp,
    next: u64,
}

impl Client {
    fn new(s: EcadMcp) -> Self {
        let mut c = Client { s, next: 1 };
        let r = c.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": AGENT, "version": "1"}}),
        );
        assert!(r.get("result").is_some(), "{r}");
        assert!(c.s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        c.no_settings();
        c
    }

    /// Cable views are remembered nowhere in these tests.
    fn no_settings(&mut self) {
        self.s
            .with_session(|s, p| p.configure(s, |p| p.settings = ecad_cadcraft::views::Settings::none()))
            .unwrap()
            .unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        let reply = self.s.handle_line(&line).expect("a reply");
        let v: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(v["id"], json!(id));
        v
    }

    /// A tool call's `result` (the call must not be a protocol error).
    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        assert!(r.get("error").is_none(), "{tool} {r}");
        r["result"].clone()
    }

    /// A successful tool call's JSON.
    fn ok(&mut self, tool: &str, args: Value) -> Value {
        let r = self.call(tool, args.clone());
        assert_ne!(r["isError"], json!(true), "{tool} {args}: {r}");
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    /// A refused tool call's message.
    fn refused(&mut self, tool: &str, args: Value) -> String {
        let r = self.call(tool, args.clone());
        assert_eq!(r["isError"], json!(true), "{tool} {args} should be refused: {r}");
        r["content"][0]["text"].as_str().unwrap().to_string()
    }

    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.ok("execute", json!({"command": command, "params": params}))
    }

    fn log(&mut self) -> Vec<Transaction> {
        self.s.with_session(|s, p| p.read(s, |p, _| p.ledger.log().to_vec()).unwrap()).unwrap()
    }

    fn doc(&mut self) -> Document {
        self.s.with_session(|s, p| p.read(s, |p, _| p.ledger.document().clone()).unwrap()).unwrap()
    }
}

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("mcp", SEED, Options { write: true, agent: None }).unwrap())
}

fn reader() -> Client {
    Client::new(EcadMcp::in_memory("mcp", SEED, Options { write: false, agent: None }).unwrap())
}

/// The same project driven directly through the JSON forms (as a person's typed commands are).
struct Direct {
    s: Session,
    p: ProjectHandle,
}

impl Direct {
    fn new() -> Self {
        ecad_cadcraft::install();
        let mut s = Session::empty();
        let p = Project::create_with(&mut s, "mcp", SEED, Persistence::File).unwrap();
        p.configure(&mut s, |p| p.settings = ecad_cadcraft::views::Settings::none()).unwrap();
        Direct { s, p }
    }
    fn log(&mut self) -> Vec<Transaction> {
        self.p.read(&mut self.s, |p, _| p.ledger.log().to_vec()).unwrap()
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("ecad-mcp-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------- criterion 1 ----------

#[test]
fn a_client_lists_the_ecad_commands_with_their_docs_and_the_ecad_tools() {
    let mut c = reader();
    let tools = c.request("tools/list", json!({}));
    let names: Vec<&str> =
        tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in [
        "ecad_project",
        "ecad_history",
        "ecad_inspect",
        "save",
        "execute",
        "list_commands",
        "inspect_drawing",
        "query_entities",
        "render",
    ] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    for hidden in ["command_line", "script", "cancel", "new_drawing", "open", "screenshot", "ui_click"] {
        assert!(!names.contains(&hidden), "{hidden} must not be offered: {names:?}");
    }
    for t in tools["result"]["tools"].as_array().unwrap() {
        assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()), "{t}");
        assert_eq!(t["inputSchema"]["type"], json!("object"), "{t}");
    }
    let render = tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "render").unwrap();
    assert!(render["inputSchema"]["properties"].get("path").is_none(), "render writes no file here");

    let catalog = c.ok("list_commands", json!({}));
    let catalog = catalog.as_array().unwrap();
    for spec in ecad_cadcraft::commands::specs() {
        let info = catalog.iter().find(|i| i["id"] == spec.id).unwrap_or_else(|| panic!("{} not listed", spec.id));
        assert_eq!(info["params"].as_str(), Some(spec.params), "{}", spec.id);
        assert!(info["params"].as_str().is_some_and(|p| !p.is_empty()), "{} has no parameter docs", spec.id);
    }
    let filtered = c.ok("list_commands", json!({"filter": "cable"}));
    let ids: Vec<&str> = filtered.as_array().unwrap().iter().filter_map(|i| i["id"].as_str()).collect();
    assert!(ids.contains(&"cableslash") && ids.contains(&"cableview") && ids.contains(&"cableadd"), "{ids:?}");

    // The read tools answer.
    let project = c.ok("ecad_project", json!({}));
    assert_eq!(project["pages"]["count"], json!(1));
    assert_eq!(project["writes"], json!(false));
    assert_eq!(project["agent"], json!(AGENT));
    let history = c.ok("ecad_history", json!({}));
    assert_eq!(history["count"], json!(1));
    assert_eq!(history["transactions"][0]["author"], json!("user"), "the person created the project");
    assert!(c.ok("ecad_inspect", json!({})).is_object());

    // `initialize` names ECAD and says whether writes are on.
    let mut c2 = Client { s: EcadMcp::in_memory("x", 1, Options::default()).unwrap(), next: 1 };
    let init = c2.request("initialize", json!({"protocolVersion": "2025-06-18", "clientInfo": {"name": "probe"}}));
    assert_eq!(init["result"]["serverInfo"]["name"], json!("ecad"));
    assert!(init["result"]["instructions"].as_str().unwrap().contains("Writes are OFF"));
}

// ---------- criterion 2 ----------

/// The brief 004 and 005 examples (quick start 5A and 5B), as JSON forms. `None` marks where the
/// cable's ID is filled in.
fn examples(cable: Option<&str>) -> Vec<(&'static str, Value)> {
    let cable = cable.unwrap_or_default();
    vec![
        ("wire", json!({"points": [[0, 0], [60, 0]]})),
        ("wire", json!({"points": [[0, 5], [60, 5]]})),
        ("wire", json!({"points": [[0, 10], [60, 10]]})),
        // A plain CADCraft line on the page goes through ECAD's commit gate like a typed one.
        ("line", json!({"points": [[0, 15], [60, 15]]})),
        ("cableslash", json!({"from": [28, -3], "to": [32, 18]})),
        ("cableview", json!({"cable": cable, "view": "fan"})),
        ("cableview", json!({"cable": cable, "view": "traditional"})),
        ("cableview", json!({"cable": cable, "view": "fan"})),
    ]
}

#[test]
fn the_cable_examples_through_mcp_record_the_json_forms_transactions_as_the_agent() {
    // Directly, as a person's JSON forms.
    let mut d = Direct::new();
    let mut cable = None;
    for (cmd, params) in examples(None) {
        let params =
            if cmd == "cableview" { json!({"cable": cable.clone().unwrap(), "view": params["view"]}) } else { params };
        d.s.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd}: {e}"));
        if cmd == "cableslash" {
            cable = d.p.read(&mut d.s, |p, _| p.ledger.document().cables.keys().next().unwrap().hex()).ok();
        }
    }
    let direct = d.log();

    // Through MCP.
    let mut c = writer();
    let mut docs = vec![c.doc()];
    for (cmd, params) in examples(cable.as_deref()) {
        let before = c.log().len();
        let r = c.exec(cmd, params.clone());
        let after = c.log();
        if cmd == "cableview" {
            assert_eq!(after.len(), before, "a view switch records nothing: {r}");
            assert!(r.get("ecadTransactions").is_none(), "{r}");
        } else {
            assert_eq!(after.len(), before + 1, "{cmd} records one transaction");
            let txs = r["ecadTransactions"].as_array().unwrap_or_else(|| panic!("{cmd}: {r}"));
            assert_eq!(txs.len(), 1);
            assert_eq!(txs[0]["author"], json!(format!("agent {AGENT}")));
            assert_eq!(txs[0]["seq"], json!(after.last().unwrap().seq));
            docs.push(c.doc());
        }
    }
    let agent = c.log();

    assert_eq!(agent.len(), direct.len(), "the same number of transactions");
    for (a, b) in agent.iter().zip(&direct) {
        assert_eq!((a.seq, &a.label, a.kind), (b.seq, &b.label, b.kind), "seq {}", a.seq);
        assert_eq!(a.ops, b.ops, "the same ops (IDs included) at seq {}", a.seq);
        if a.seq == 1 {
            assert_eq!(a.author, Author::User, "creating the project is the person's");
        } else {
            assert_eq!(a.author, Author::Agent(AGENT.into()), "seq {}", a.seq);
            assert_eq!(b.author, Author::User);
        }
    }
    let view = c.exec("cableview", json!({}));
    assert!(view.to_string().contains("fan"), "the cable is a fan: {view}");

    // `u` undoes each edit in one step, newest first, as the agent.
    docs.pop();
    while let Some(expected) = docs.pop() {
        let before = c.log().len();
        let r = c.exec("u", json!({}));
        let log = c.log();
        assert_eq!(log.len(), before + 1, "one undo transaction: {r}");
        let last = log.last().unwrap();
        assert!(matches!(last.kind, ecad_model::TxKind::Undo { .. }));
        assert_eq!(last.author, Author::Agent(AGENT.into()));
        assert_eq!(c.doc(), expected, "one step back");
    }
    // Every agent edit is undone; the project is as created.
    let mut fresh = Direct::new();
    let created = fresh.p.read(&mut fresh.s, |p, _| p.ledger.document().clone()).unwrap();
    assert_eq!(c.doc(), created);
    let history = c.ok("ecad_history", json!({"limit": 1000}));
    assert!(history["authors"].as_array().unwrap().contains(&json!(format!("agent {AGENT}"))));
}

#[test]
fn the_agent_name_comes_from_the_client_or_from_the_agent_option() {
    let mut c = writer();
    c.exec("wire", json!({"points": [[0, 0], [10, 0]]}));
    assert_eq!(c.log().last().unwrap().author, Author::Agent(AGENT.into()));

    let s = EcadMcp::in_memory("x", 2, Options { write: true, agent: Some("ci bot".into()) }).unwrap();
    let mut c = Client::new(s);
    c.exec("wire", json!({"points": [[0, 0], [10, 0]]}));
    assert_eq!(c.log().last().unwrap().author, Author::Agent("ci bot".into()), "--agent wins over the client's name");

    // Before `initialize`, an agent's edit is still never recorded as the person's.
    let mut c = Client { s: EcadMcp::in_memory("x", 3, Options { write: true, agent: None }).unwrap(), next: 1 };
    c.no_settings();
    c.exec("wire", json!({"points": [[0, 0], [10, 0]]}));
    assert_eq!(c.log().last().unwrap().author, Author::Agent(ecad_mcp::UNNAMED_AGENT.into()));
}

// ---------- criterion 3 ----------

#[test]
fn with_writes_off_every_write_is_refused_and_the_head_is_unchanged() {
    let mut c = reader();
    let before = c.log();
    let doc = c.doc();
    let writes: &[(&str, Value)] = &[
        ("wire", json!({"points": [[0, 0], [60, 0]]})),
        ("line", json!({"points": [[0, 15], [60, 15]]})),
        ("circle", json!({"center": [0, 0], "radius": 5})),
        ("cableslash", json!({"from": [28, -3], "to": [32, 18]})),
        ("cableview", json!({})),
        ("cableadd", json!({"conductors": []})),
        ("place", json!({"at": [100, 0]})),
        ("convertcomponent", json!({"handles": []})),
        ("addterminal", json!({"handle": "1", "at": [0, 0]})),
        ("connect", json!({"conductor": "1", "end": "A", "terminal": "2"})),
        ("disconnect", json!({"conductor": "1", "end": "A"})),
        ("setprop", json!({"component": "1", "key": "tag", "value": "K1"})),
        ("ecaddelete", json!({"cable": "1"})),
        ("newpage", json!({"title": "2"})),
        ("u", json!({})),
        ("undo", json!({})),
        ("redo", json!({})),
        ("erase", json!({"handles": []})),
    ];
    for (cmd, params) in writes {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
        assert_eq!(c.log(), before, "{cmd} moved the head");
    }
    let m = c.refused("save", json!({}));
    assert_eq!(m, policy::WRITES_OFF);
    // File commands are refused in every mode, with their own message.
    for (cmd, params) in [("saveas", json!({})), ("open", json!({})), ("close", json!({})), ("new", json!({}))] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert!(m.starts_with("Refused:") && m.contains("not available to agents"), "{cmd}: {m}");
    }
    let m = c.refused("execute", json!({"command": "saveas", "params": {"path": "x.ecad"}}));
    assert!(m.contains("names a file"), "{m}");
    assert_eq!(c.log(), before);
    assert_eq!(c.doc(), doc);

    // Reading works.
    c.ok("ecad_project", json!({}));
    c.ok("ecad_history", json!({}));
    c.ok("ecad_inspect", json!({}));
    c.ok("inspect_drawing", json!({}));
    c.ok("query_entities", json!({}));
    c.exec("ecadinspect", json!({}));
    c.exec("entities", json!({}));
    assert_eq!(c.log(), before);
}

#[test]
fn a_project_opened_read_only_refuses_writes_even_with_the_write_flag() {
    let dir = TempDir::new("readonly");
    let path = dir.file("held.ecad");
    let mut first = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    first.exec("wire", json!({"points": [[0, 0], [10, 0]]}));
    first.ok("save", json!({}));

    // A second server while the first still holds the writer lock.
    let (s, summary) = EcadMcp::open(&path, Options { write: true, agent: None }).unwrap();
    assert!(summary.contains("read-only"), "{summary}");
    let mut second = Client::new(s);
    assert!(!second.s.writes());
    let before = second.log();
    let m = second.refused("execute", json!({"command": "wire", "params": {"points": [[0, 5], [10, 5]]}}));
    assert!(m.starts_with("Refused: the project opened read-only"), "{m}");
    let m = second.refused("save", json!({}));
    assert!(m.starts_with("Refused: the project opened read-only"), "{m}");
    assert_eq!(second.log(), before);
    assert_eq!(second.ok("ecad_project", json!({}))["writes"], json!(false));
}

// ---------- criterion 4 ----------

#[test]
fn raw_cadcraft_commands_cannot_get_round_ecads_refusals() {
    let mut d = Direct::new();
    let mut c = writer();
    let before = c.log();
    for cmd in ecad_cadcraft::project::REFUSED_COMMANDS {
        let ui = d.s.execute(cmd, &json!({})).expect_err("refused for a person").to_string();
        let typed = d.s.cmdline(cmd).expect_err("refused when typed").to_string();
        let m = c.refused("execute", json!({"command": cmd, "params": {}}));
        assert_eq!(m, ui, "{cmd}: the agent gets the person's refusal");
        let reason = if matches!(*cmd, "lisp" | "appload") {
            ecad_cadcraft::project::LISP_REFUSED.to_string()
        } else {
            format!("{} is not available on ECAD pages", cmd.to_ascii_uppercase())
        };
        assert!(m.contains(&reason) && typed.contains(&reason), "{cmd}: {m} / {typed}");
        assert_eq!(c.log(), before, "{cmd} recorded nothing");
    }
    // Upper case or not, the same command.
    let m = c.refused("execute", json!({"command": "BEDIT", "params": {}}));
    assert!(m.contains("BEDIT is not available on ECAD pages"), "{m}");

    // A raw drawing command still goes through ECAD's commit gate: one transaction, the agent's.
    let r = c.exec("line", json!({"points": [[0, 0], [10, 0]]}));
    assert_eq!(r["ecadTransactions"][0]["author"], json!(format!("agent {AGENT}")), "{r}");
    assert_eq!(c.log().len(), before.len() + 1);
    // The command line is not offered at all.
    let r = c.request("tools/call", json!({"name": "command_line", "arguments": {"text": "bedit"}}));
    assert_eq!(r["error"]["code"], json!(-32602));
    assert!(r["error"]["message"].as_str().unwrap().contains("`execute` only"), "{r}");
}

// ---------- criterion 5 ----------

#[test]
fn save_works_only_with_the_write_flag_and_reopening_shows_the_agents_edits() {
    let dir = TempDir::new("save");
    let path = dir.file("agent.ecad");
    {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
        let m = c.refused("save", json!({"path": dir.file("other.ecad")}));
        assert!(m.contains("Save As stays"), "{m}");
        let saved = c.ok("save", json!({}));
        assert_eq!(saved["saved"], json!(true));
        assert_eq!(saved["unsaved"], json!(false));
    }
    {
        let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
        let mut c = Client::new(s);
        assert!(!c.s.writes());
        let h = c.ok("ecad_history", json!({}));
        let last = h["transactions"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["label"], json!("WIRE"), "{h}");
        assert_eq!(last["author"], json!(format!("agent {AGENT}")));
        assert_eq!(c.ok("ecad_project", json!({}))["conductors"]["count"], json!(1));
        c.exec("ecadinspect", json!({}));
        assert_eq!(c.refused("save", json!({})), policy::WRITES_OFF);
    }
    // An unsaved project in memory has no file: Save As stays with the person.
    let mut c = writer();
    let m = c.refused("save", json!({}));
    assert!(m.contains("Save As needs a file name"), "{m}");
}

// ---------- criterion 6 ----------

#[test]
fn hostile_input_never_panics_and_gets_json_rpc_errors() {
    let mut c = writer();
    let before = c.log();
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let too_many =
        Value::Array((0..=MAX_BATCH).map(|i| json!({"jsonrpc": "2.0", "id": i, "method": "ping"})).collect())
            .to_string();
    let lines: Vec<(&str, String, i64)> = vec![
        ("truncated", "{".into(), -32700),
        ("not json", "hello".into(), -32700),
        ("deep nesting", deep, -32700),
        ("empty batch", "[]".into(), -32600),
        ("batch too long", too_many, -32600),
        ("not an object", "42".into(), -32600),
        ("no method", r#"{"jsonrpc":"2.0","id":1}"#.into(), -32600),
        ("unknown method", r#"{"jsonrpc":"2.0","id":1,"method":"nope"}"#.into(), -32601),
        ("unknown tool", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nope"}}"#.into(), -32602),
        ("no tool name", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{}}"#.into(), -32602),
        ("script tool", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"script","arguments":{"text":"line 0,0 1,1"}}}"#.into(), -32602),
        ("open tool", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"open","arguments":{"path":"x.dxf"}}}"#.into(), -32602),
        ("arguments not an object", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecad_project","arguments":"x"}}"#.into(), -32602),
        ("negative limit", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecad_history","arguments":{"limit":-1}}}"#.into(), -32602),
        ("seq not a number", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecad_inspect","arguments":{"seq":"x"}}}"#.into(), -32602),
        ("execute without command", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"execute","arguments":{}}}"#.into(), -32602),
        ("command not a string", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"execute","arguments":{"command":7}}}"#.into(), -32602),
    ];
    for (what, line, code) in lines {
        let reply = c.s.handle_line(&line).unwrap_or_else(|| panic!("{what}: no reply"));
        let v: Value = serde_json::from_str(&reply).unwrap();
        let v = if v.is_array() { v[0].clone() } else { v };
        assert_eq!(v["error"]["code"], json!(code), "{what}: {v}");
        assert!(v["error"]["message"].as_str().is_some_and(|m| !m.is_empty()), "{what}: {v}");
    }
    // Hostile parameters are refused by the commands, as for a person.
    let too_long: Vec<[f64; 2]> = (0..10_001).map(|i| [f64::from(i), 0.0]).collect();
    for (cmd, params) in [
        ("wire", json!({"points": too_long})),
        ("wire", json!({"points": "x"})),
        ("cableslash", json!({"from": [0, 0]})),
        ("cableview", json!({"cable": "not hex", "view": "fan"})),
        ("connect", json!({"conductor": "ffffffffffffffffffffffffffffffffff", "end": "C"})),
        ("nosuchcommand", json!({})),
    ] {
        let r = c.call("execute", json!({"command": cmd, "params": params}));
        assert_eq!(r["isError"], json!(true), "{cmd}: {r}");
    }
    assert_eq!(c.log(), before, "nothing refused was recorded");
    // Extreme but finite coordinates: a result either way (the model decides), never a panic.
    let r = c.call("execute", json!({"command": "wire", "params": {"points": [[1e308, 0], [-1e308, 0]]}}));
    assert!(r["content"][0]["text"].as_str().is_some_and(|t| !t.is_empty()), "{r}");
    assert!(c.log().len() <= before.len() + 1);
    // Notifications and responses get no reply; the server keeps answering.
    assert!(c.s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{}}"#).is_none());
    assert!(c.s.handle_line(r#"{"jsonrpc":"2.0","id":9,"result":{}}"#).is_none());
    assert!(c.request("ping", json!({})).get("result").is_some());
}

#[test]
fn an_oversized_or_non_utf8_line_is_answered_and_skipped() {
    let mut c = writer();
    let mut input = Vec::new();
    input.extend(std::iter::repeat_n(b'x', MAX_REQUEST_BYTES + 10));
    input.push(b'\n');
    input.extend_from_slice(&[0xff, 0xfe, b'{', b'\n']);
    input.extend_from_slice(br#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#);
    input.push(b'\n');
    let mut out = Vec::new();
    c.s.serve(std::io::Cursor::new(input), &mut out).unwrap();
    let replies: Vec<Value> =
        String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 3, "{replies:?}");
    assert_eq!(replies[0]["error"]["code"], json!(-32600));
    assert!(replies[0]["error"]["message"].as_str().unwrap().contains("too large"));
    assert_eq!(replies[1]["error"]["code"], json!(-32700));
    assert_eq!(replies[2]["id"], json!(3));
    assert!(replies[2].get("result").is_some());
}

// ---------- criterion 7 ----------

fn spawn(args: &[&str], settings: &TempDir) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_ecad-mcp"))
        .args(args)
        .env(ecad_cadcraft::views::SETTINGS_ENV, &settings.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// Send `lines`, close stdin, and return the replies and whether the process exited cleanly.
fn session(args: &[&str], lines: &[Value]) -> (Vec<Value>, bool, String) {
    let settings = TempDir::new("stdio");
    let mut child = spawn(args, &settings);
    {
        let mut stdin = child.stdin.take().unwrap();
        for l in lines {
            writeln!(stdin, "{l}").unwrap();
        }
    }
    let replies: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
        .collect();
    let out = child.wait_with_output().unwrap();
    (replies, out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

#[test]
fn stdio_the_binary_serves_a_headless_session_and_exits_cleanly() {
    let lines = vec![
        rpc(
            1,
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "stdio-agent"}}),
        ),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        rpc(2, "tools/list", json!({})),
        rpc(
            3,
            "tools/call",
            json!({"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 0], [60, 0]]}}}),
        ),
        rpc(4, "tools/call", json!({"name": "ecad_history", "arguments": {}})),
        json!("garbage"),
    ];
    let (replies, ok, stderr) = session(&["--write"], &lines);
    assert!(ok, "exit status: {stderr}");
    assert!(stderr.contains("writes on"), "{stderr}");
    assert_eq!(replies.len(), 5, "{replies:?}");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], json!("ecad"));
    assert!(replies[1]["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "ecad_project"));
    assert_ne!(replies[2]["result"]["isError"], json!(true), "{}", replies[2]);
    let history: Value = serde_json::from_str(replies[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let last = history["transactions"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["author"], json!("agent stdio-agent"), "{history}");
    assert_eq!(replies[4]["error"]["code"], json!(-32600), "a bare string is not a request");

    // Without --write the same call is refused.
    let (replies, ok, stderr) = session(&[], &lines[..4]);
    assert!(ok, "{stderr}");
    assert!(stderr.contains("writes off"), "{stderr}");
    assert_eq!(replies[2]["result"]["isError"], json!(true));
    assert_eq!(replies[2]["result"]["content"][0]["text"], json!(policy::WRITES_OFF));
}

#[test]
fn stdio_usage_errors_exit_with_a_message() {
    let settings = TempDir::new("usage");
    for args in [&["--nope"][..], &["--agent"][..], &["--create"][..], &["a.ecad", "b.ecad"][..]] {
        let out = spawn(args, &settings).wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage: ecad-mcp"), "{args:?}");
    }
    let missing = settings.file("missing.ecad");
    let out = spawn(&[&missing], &settings).wait_with_output().unwrap();
    assert!(!out.status.success(), "a missing project is an error, not a new project");
    let out = Command::new(env!("CARGO_BIN_EXE_ecad-mcp")).arg("--help").output().unwrap();
    assert!(out.status.success() && String::from_utf8_lossy(&out.stdout).contains("--write"));
}
