//! Brief 011, the tester's acceptance tests (written from the brief, independent of the worker's
//! `mcp_contracts.rs`). They drive `ecad-mcp` only through JSON-RPC lines, in-process or over the
//! built binary's real stdin and stdout, and look at the project's ledger from outside.
//!
//! Criteria: 2 (the transaction cap must leave the project as it was), 3 (every catalogued command
//! refused with writes off, notifications included), 4 (raw CADCraft edits on an ECAD page give the
//! same result as the JSON form), 5 (saving and reopening in a folder with spaces and non-ASCII
//! characters, the file untouched with writes off), 6 (hostile lines), 7 (the binary on this
//! platform: Windows paths with spaces, CRLF, hostile lines, clean exit).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use cadcraft_engine::Session;
use ecad_cadcraft::{Persistence, Project, ProjectHandle};
use ecad_mcp::{EcadMcp, MAX_REQUEST_BYTES, Options, policy};
use ecad_model::{Author, Document, Transaction};
use serde_json::{Value, json};

const AGENT: &str = "tester-agent";
const SEED: u64 = 0x7E57_0011;

struct Client {
    s: EcadMcp,
    next: u64,
}

impl Client {
    fn new(s: EcadMcp) -> Self {
        let mut c = Client { s, next: 1 };
        let r = c.request("initialize", json!({"protocolVersion": "2025-06-18", "clientInfo": {"name": AGENT}}));
        assert!(r.get("result").is_some(), "{r}");
        c.s.with_session(|s, p| p.configure(s, |p| p.settings = ecad_cadcraft::views::Settings::none())).unwrap().unwrap();
        c
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        let v: Value = serde_json::from_str(&self.s.handle_line(&line).expect("a reply")).unwrap();
        assert_eq!(v["id"], json!(id));
        v
    }
    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        assert!(r.get("error").is_none(), "{tool}: {r}");
        r["result"].clone()
    }
    fn exec_raw(&mut self, command: &str, params: Value) -> Value {
        self.call("execute", json!({"command": command, "params": params}))
    }
    fn exec(&mut self, command: &str, params: Value) -> Value {
        let r = self.exec_raw(command, params.clone());
        assert_ne!(r["isError"], json!(true), "{command} {params}: {r}");
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }
    fn log(&mut self) -> Vec<Transaction> {
        self.s.with_session(|s, p| p.read(s, |p, _| p.ledger.full_log().unwrap()).unwrap()).unwrap()
    }
    fn doc(&mut self) -> Document {
        self.s.with_session(|s, p| p.read(s, |p, _| p.ledger.document().clone()).unwrap()).unwrap()
    }
    /// The handles of the entities on the active page.
    fn handles(&mut self) -> Vec<String> {
        let r = self.exec("entities", json!({}));
        r["entities"].as_array().map_or_else(Vec::new, |a| a.iter().filter_map(|e| e["handle"].as_str().map(str::to_string)).collect())
    }
}

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("tester", SEED, Options { write: true, agent: None }).unwrap())
}
fn reader() -> Client {
    Client::new(EcadMcp::in_memory("tester", SEED, Options { write: false, agent: None }).unwrap())
}

/// The same project driven through the JSON forms directly, as a person's typed commands are.
struct Direct {
    s: Session,
    p: ProjectHandle,
}

impl Direct {
    fn new() -> Self {
        ecad_cadcraft::install();
        let mut s = Session::empty();
        let p = Project::create_with(&mut s, "tester", SEED, Persistence::File).unwrap();
        p.configure(&mut s, |p| p.settings = ecad_cadcraft::views::Settings::none()).unwrap();
        Direct { s, p }
    }
    fn log(&mut self) -> Vec<Transaction> {
        self.p.read(&mut self.s, |p, _| p.ledger.full_log().unwrap()).unwrap()
    }
    fn doc(&mut self) -> Document {
        self.p.read(&mut self.s, |p, _| p.ledger.document().clone()).unwrap()
    }
}

struct TempDir(PathBuf);

impl TempDir {
    /// A folder whose name has spaces and non-ASCII characters, as Windows users' folders do.
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("ecad mcp tester {tag} Ünïcode {}-{nanos}", std::process::id()));
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

fn three_wires(c: &mut Client) {
    for y in [0, 5, 10] {
        c.exec("wire", json!({"points": [[0, y], [60, y]]}));
    }
}

// ---------- criterion 2 / scope 4: caps on transactions per call ----------

/// `undo {"count": n}` records n undo transactions in one call. The cap refuses it; the project
/// must then be as it was before the call, not n further steps back.
#[test]
fn an_undo_of_several_steps_in_one_call_is_refused_and_leaves_the_project_as_it_was() {
    let mut c = writer();
    three_wires(&mut c);
    let before = c.doc();
    let r = c.exec_raw("undo", json!({"count": 3}));
    assert_eq!(r["isError"], json!(true), "more than one transaction in one call is refused: {r}");
    let labels: Vec<String> = c.log().iter().map(|t| format!("{} {:?}", t.label, t.kind)).collect();
    assert_eq!(c.doc().conductors.len(), 3, "the refused call must not leave wires undone: {r}\nlog: {labels:?}");
    assert_eq!(c.doc(), before, "the project is as before the refused call");
}

#[test]
fn a_redo_of_several_steps_in_one_call_is_refused_and_leaves_the_project_as_it_was() {
    let mut c = writer();
    three_wires(&mut c);
    c.exec("u", json!({}));
    c.exec("u", json!({}));
    let before = c.doc();
    assert_eq!(before.conductors.len(), 1);
    let r = c.exec_raw("redo", json!({"count": 2}));
    assert_eq!(r["isError"], json!(true), "{r}");
    assert_eq!(c.doc(), before, "the project is as before the refused call");
}

/// One `u` per call is one step, and is the agent's (criterion 2, from outside).
#[test]
fn u_and_redo_are_one_step_each_and_the_agents() {
    let mut c = writer();
    three_wires(&mut c);
    let all_three: Document = c.doc();
    let r = c.exec("u", json!({}));
    assert_eq!(r["ecadTransactions"].as_array().map(Vec::len), Some(1), "{r}");
    assert_eq!(c.doc().conductors.len(), 2);
    let r = c.exec("redo", json!({}));
    assert_eq!(r["ecadTransactions"].as_array().map(Vec::len), Some(1), "{r}");
    assert_eq!(c.doc(), all_three);
    for t in c.log().iter().skip(1) {
        assert_eq!(t.author, Author::Agent(AGENT.into()), "seq {}", t.seq);
    }
}

/// Fix round 1: UNDO, REDO and MREDO asking for more than one step are refused before they run,
/// with a message that says nothing was changed, and the head does not move.
#[test]
fn multi_step_undo_and_redo_are_refused_before_they_run() {
    let mut c = writer();
    three_wires(&mut c);
    c.exec("u", json!({}));
    let log = c.log();
    let doc = c.doc();
    for (cmd, p) in [
        ("undo", json!({"count": 2})),
        ("undo", json!({"count": 1000})),
        ("undo", json!({"count": 1.5})),
        ("undo", json!({"count": "3"})),
        ("undo", json!({"count": 1e300})),
        ("undo", json!({"mode": "back"})),
        ("undo", json!({"mode": "BACK", "confirm": true})),
        ("UNDO", json!({"count": 2})),
        ("redo", json!({"count": 2})),
        ("redo", json!({"count": 18446744073709551615u64})),
        ("mredo", json!({"count": 2})),
        ("mredo", json!({"all": true})),
        ("mredo", json!({"all": "yes"})),
    ] {
        let r = c.exec_raw(cmd, p.clone());
        assert_eq!(r["isError"], json!(true), "{cmd} {p}: {r}");
        let m = r["content"][0]["text"].as_str().unwrap_or_default();
        assert!(m.starts_with("Refused:") && m.contains("Nothing was changed"), "{cmd} {p}: {m}");
        assert!(!m.contains("taken back"), "refused before it ran, not after: {cmd} {p}: {m}");
        assert_eq!(c.log(), log, "{cmd} {p}: the head moved");
        assert_eq!(c.doc(), doc, "{cmd} {p}");
    }
    // Spellings the gate does not catch reach ECAD's own refusal of non-count UNDO modes, or run
    // as one step: never more than one transaction.
    for (cmd, p) in [("undo", json!({"mode": " back "})), ("undo", json!({"mode": "begin"})), ("mredo", json!({"all": false}))] {
        let before = c.log().len();
        let _ = c.exec_raw(cmd, p.clone());
        assert!(c.log().len() <= before + 1, "{cmd} {p}");
    }
}

/// Run one batch line; return the replies in id order.
fn batch(c: &mut Client, calls: &[(&str, Value)]) -> Vec<Value> {
    let msgs: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(i, (cmd, p))| {
            json!({"jsonrpc": "2.0", "id": 100 + i, "method": "tools/call",
                "params": {"name": "execute", "arguments": {"command": cmd, "params": p}}})
        })
        .collect();
    let reply = c.s.handle_line(&Value::Array(msgs).to_string()).expect("replies");
    let v: Value = serde_json::from_str(&reply).unwrap();
    let mut out = v.as_array().cloned().unwrap_or_default();
    out.sort_by_key(|r| r["id"].as_u64());
    out
}

fn is_err(r: &Value) -> bool {
    r.get("error").is_some() || r["result"]["isError"] == json!(true)
}

/// Fix round 1: a batch line is one call. A second transaction in the line, whatever its kind
/// (edit, undo, redo), is refused and taken back by the opposite step, so the design is as after
/// the first; the person's earlier transactions are never touched.
#[test]
fn a_batch_over_the_cap_is_taken_back_by_kind_and_spares_the_persons_work() {
    let mut c = writer();
    // The person's own wire, recorded as the person's.
    c.s.with_session(|s, p| {
        p.configure(s, |p| p.author = Author::User).unwrap();
        s.execute("wire", &json!({"points": [[0, -20], [60, -20]]})).unwrap();
        p.configure(s, |p| p.author = Author::Agent(AGENT.into())).unwrap();
    })
    .unwrap();
    let person: Vec<Transaction> = c.log();
    assert_eq!(person.last().unwrap().author, Author::User);
    let only_person = c.doc();
    let wire = |y: i32| ("wire", json!({"points": [[0, y], [60, y]]}));

    // Edit then edit: the second edit is undone.
    let r = batch(&mut c, &[wire(0), wire(5)]);
    assert!(!is_err(&r[0]), "{}", r[0]);
    assert!(is_err(&r[1]), "{}", r[1]);
    let msg = r[1]["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(msg.contains("taken back") && msg.contains("as before"), "a truthful message: {msg}");
    let with_w0 = c.doc();
    assert_eq!(with_w0.conductors.len(), only_person.conductors.len() + 1, "{msg}");

    // Undo then undo: the second undo is redone.
    let r = batch(&mut c, &[("u", json!({})), ("u", json!({}))]);
    assert!(!is_err(&r[0]) && is_err(&r[1]), "{r:?}");
    assert_eq!(c.doc(), only_person, "one undo stands, the second was taken back by a redo");

    // Redo then edit: the edit is undone, the redo stands.
    let r = batch(&mut c, &[("redo", json!({})), wire(10)]);
    assert!(!is_err(&r[0]) && is_err(&r[1]), "{r:?}");
    assert_eq!(c.doc(), with_w0);

    // Undo then redo: the redo is undone, the undo stands.
    let r = batch(&mut c, &[("u", json!({})), ("redo", json!({}))]);
    assert!(!is_err(&r[0]) && is_err(&r[1]), "{r:?}");
    assert_eq!(c.doc(), only_person);

    // A long batch of edits: only the first stands; every later one is refused and taken back.
    let calls: Vec<(&str, Value)> = (0..10).map(|i| wire(30 + i)).collect();
    let r = batch(&mut c, &calls);
    assert!(!is_err(&r[0]));
    assert!(r[1..].iter().all(is_err), "{r:?}");
    assert_eq!(c.doc().conductors.len(), only_person.conductors.len() + 1);

    // A batch of reads and refusals records nothing beyond its one write.
    let before = c.log().len();
    let r = batch(&mut c, &[("ecadinspect", json!({})), ("nosuch", json!({})), wire(50), ("entities", json!({}))]);
    assert!(!is_err(&r[0]) && is_err(&r[1]) && !is_err(&r[2]) && !is_err(&r[3]), "{r:?}");
    assert_eq!(c.log().len(), before + 1);

    // The person's transactions are untouched, still the person's, and their wire survives.
    let log = c.log();
    assert_eq!(&log[..person.len()], &person[..], "earlier history is append-only and unchanged");
    let doc = c.doc();
    for id in only_person.conductors.keys() {
        assert!(doc.conductors.contains_key(id), "the person's wire survives every rollback");
    }
    // Each new line starts a fresh call: a single write after a refused batch is accepted.
    let r = c.exec_raw("wire", json!({"points": [[0, 70], [60, 70]]}));
    assert_ne!(r["isError"], json!(true), "{r}");
}

// ---------- criterion 3: writes off ----------

#[test]
fn with_writes_off_every_catalogued_command_but_the_read_ones_is_refused_and_nothing_moves() {
    let mut c = reader();
    let log = c.log();
    let doc = c.doc();
    let never_msg = |id: &str| policy::never(id);
    let mut checked = 0;
    for spec in cadcraft_engine::command_specs() {
        let id = spec.id;
        // Brief 019: `pageprops {}` and `projectprops {}` only read.
        if policy::READ_COMMANDS.contains(&id) || policy::reads_only(id, &json!({})) {
            continue;
        }
        let r = c.exec_raw(id, json!({}));
        assert_eq!(r["isError"], json!(true), "{id} must be refused with writes off: {r}");
        let m = r["content"][0]["text"].as_str().unwrap_or_default().to_string();
        assert!(m == policy::WRITES_OFF || m == never_msg(id), "{id}: unexpected message {m}");
        checked += 1;
    }
    assert!(checked > 100, "the whole catalogue was walked ({checked})");
    assert_eq!(c.log(), log, "the head never moved");
    assert_eq!(c.doc(), doc);
    // The read commands themselves record nothing, even with odd parameters.
    for id in policy::READ_COMMANDS {
        for params in [json!({}), json!({"page": "ffff", "index": 99, "name": "x"})] {
            let _ = c.exec_raw(id, params);
        }
    }
    assert_eq!(c.log(), log, "read commands recorded nothing");
}

/// A JSON-RPC notification (no `id`) gets no reply, so a refusal there must still leave the
/// project unchanged: it goes through the same gate.
#[test]
fn with_writes_off_notifications_cannot_change_or_save_the_project() {
    let dir = TempDir::new("notify");
    let path = dir.file("held.ecad");
    {
        let mut w = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        w.exec("wire", json!({"points": [[0, 0], [10, 0]]}));
        assert_eq!(w.call("save", json!({}))["isError"], json!(null));
    }
    let bytes = std::fs::read(&path).unwrap();
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    let log = c.log();
    for (tool, args) in [
        ("execute", json!({"command": "wire", "params": {"points": [[0, 5], [10, 5]]}})),
        ("execute", json!({"command": "u", "params": {}})),
        ("execute", json!({"command": "qsave", "params": {}})),
        ("save", json!({})),
        ("command_line", json!({"text": "line 0,0 5,5 "})),
        ("script", json!({"text": "erase all\n\nqsave\n"})),
        ("open", json!({"path": path})),
        ("new_drawing", json!({})),
        ("cancel", json!({})),
    ] {
        let line = json!({"jsonrpc": "2.0", "method": "tools/call", "params": {"name": tool, "arguments": args}});
        assert!(c.s.handle_line(&line.to_string()).is_none(), "{tool}: a notification gets no reply");
        assert_eq!(c.log(), log, "{tool} as a notification changed the project");
    }
    drop(c);
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "the file is untouched");
}

#[test]
fn with_writes_on_render_cannot_write_a_file() {
    let dir = TempDir::new("render");
    let png = dir.file("out.png");
    let mut c = writer();
    c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
    let r = c.request("tools/call", json!({"name": "render", "arguments": {"path": png}}));
    let refused = r.get("error").is_some() || r["result"]["isError"] == json!(true);
    assert!(refused, "render with a path is refused: {r}");
    assert!(!std::path::Path::new(&png).exists(), "no file was written");
    // Without a path it returns the image.
    let r = c.call("render", json!({"width": 64, "height": 64}));
    assert_eq!(r["content"][0]["type"], json!("image"), "{r}");
}

// ---------- criterion 4: raw CADCraft edits ----------

/// Raw CADCraft edits on an ECAD page through MCP do exactly what the JSON form does for a person:
/// the same transaction or the same refusal.
#[test]
fn raw_cadcraft_edits_through_mcp_match_the_json_forms() {
    let mut d = Direct::new();
    let mut c = writer();
    let steps: Vec<(&str, Value)> = vec![
        ("wire", json!({"points": [[0, 0], [60, 0]]})),
        ("wire", json!({"points": [[0, 5], [60, 5]]})),
        ("line", json!({"points": [[0, 15], [60, 15]]})),
        ("cableslash", json!({"from": [28, -3], "to": [32, 18]})),
    ];
    for (cmd, p) in &steps {
        d.s.execute(cmd, p).unwrap();
        c.exec(cmd, p.clone());
    }
    let handles = c.handles();
    assert!(handles.len() >= 4, "{handles:?}");
    // Each raw edit, on each object of the page, through both routes.
    for h in &handles {
        for (cmd, p) in [
            ("move", json!({"handles": [h], "from": [0, 0], "to": [1, 1]})),
            ("erase", json!({"handles": [h]})),
            ("explode", json!({"handles": [h]})),
        ] {
            let grew_from = c.log().len();
            let direct = d.s.execute(cmd, &p).map_err(|e| e.to_string());
            let mcp = c.exec_raw(cmd, p.clone());
            let mcp_err = mcp["isError"] == json!(true);
            assert_eq!(direct.is_err(), mcp_err, "{cmd} {h}: direct {direct:?}, mcp {mcp}");
            if let Err(e) = &direct {
                assert_eq!(mcp["content"][0]["text"].as_str(), Some(e.as_str()), "{cmd} {h}: the person's refusal");
            }
            let (dl, ml) = (d.log(), c.log());
            assert_eq!(dl.len(), ml.len(), "{cmd} {h}");
            if let (Some(a), Some(b)) = (dl.last(), ml.last()) {
                assert_eq!((&a.label, &a.ops), (&b.label, &b.ops), "{cmd} {h}: the same transaction");
            }
            assert_eq!(d.doc(), c.doc(), "{cmd} {h}");
            // Back to the same state on both sides when something was recorded.
            if ml.len() > grew_from {
                let _ = d.s.execute("u", &json!({}));
                c.exec("u", json!({}));
                assert_eq!(d.doc(), c.doc(), "{cmd} {h} undone");
            }
        }
    }
    for t in c.log().iter().skip(1) {
        assert_eq!(t.author, Author::Agent(AGENT.into()));
    }
}

/// Every command ECAD refuses on its pages gets, through MCP, exactly the refusal a person's JSON
/// form gets (whichever check refuses it first), and records nothing.
#[test]
fn every_command_ecad_refuses_gets_the_persons_refusal_through_mcp() {
    let mut d = Direct::new();
    let mut c = writer();
    c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
    d.s.execute("wire", &json!({"points": [[0, 0], [60, 0]]})).unwrap();
    let before = c.log();
    for cmd in ecad_cadcraft::project::REFUSED_COMMANDS {
        for params in [json!({}), json!({"name": "x", "handles": ["1"]})] {
            let person = d.s.execute(cmd, &params).map_err(|e| e.to_string());
            let r = c.exec_raw(cmd, params.clone());
            assert_eq!(r["isError"], json!(true), "{cmd} {params}: {r}");
            assert_eq!(r["content"][0]["text"].as_str(), person.as_ref().err().map(String::as_str), "{cmd} {params}");
            assert_eq!(c.log(), before, "{cmd} recorded nothing");
        }
    }
}

// ---------- criterion 5: save and reopen ----------

#[test]
fn saving_in_a_folder_with_spaces_and_reopening_without_ecad_mcp_shows_the_agent() {
    let dir = TempDir::new("save");
    let path = dir.file("agent project.ecad");
    {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        three_wires(&mut c);
        c.exec("cableslash", json!({"from": [28, -3], "to": [32, 18]}));
        c.exec("u", json!({}));
        c.exec("redo", json!({}));
        let r = c.call("save", json!({}));
        assert_ne!(r["isError"], json!(true), "{r}");
    }
    // Reopened by the trial's own code (no MCP involved), read-only.
    ecad_cadcraft::install();
    let mut s = Session::empty();
    let (p, _) = ProjectHandle::open_file(&mut s, &path, 1).unwrap();
    let log = p.read(&mut s, |p, _| p.ledger.full_log().unwrap()).unwrap();
    let labels: Vec<&str> = log.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(log.len(), 7, "create, 3 wires, slash, undo, redo: {labels:?}");
    assert_eq!(log[0].author, Author::User, "the person created it (--create)");
    for t in &log[1..] {
        assert_eq!(t.author, Author::Agent(AGENT.into()), "seq {} {}", t.seq, t.label);
    }
    let doc = p.read(&mut s, |p, _| p.ledger.document().clone()).unwrap();
    assert_eq!(doc.conductors.len(), 3);
    assert_eq!(doc.cables.len(), 1);
}

#[test]
fn without_write_the_save_tool_leaves_the_file_byte_for_byte() {
    let dir = TempDir::new("nosave");
    let path = dir.file("p.ecad");
    {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
        c.call("save", json!({}));
    }
    let bytes = std::fs::read(&path).unwrap();
    {
        let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
        let mut c = Client::new(s);
        for args in [json!({}), json!({"path": null}), json!({"path": dir.file("q.ecad")})] {
            let r = c.call("save", args.clone());
            assert_eq!(r["isError"], json!(true), "{args}: {r}");
        }
        for cmd in ["qsave", "save", "saveas", "close", "new", "open"] {
            let r = c.exec_raw(cmd, json!({}));
            assert_eq!(r["isError"], json!(true), "{cmd}: {r}");
        }
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(!std::path::Path::new(&dir.file("q.ecad")).exists());
}

// ---------- criterion 6: hostile input ----------

#[test]
fn hostile_lines_get_json_rpc_errors_and_never_panic() {
    let mut c = writer();
    let log = c.log();
    let nest = |n: usize| format!("{}1{}", "[".repeat(n), "]".repeat(n));
    let cases: Vec<(&str, String, Option<i64>)> = vec![
        ("NaN literal", r#"{"jsonrpc":"2.0","id":1,"method":"ping","x":NaN}"#.into(), Some(-32700)),
        ("Infinity literal", r#"{"jsonrpc":"2.0","id":1,"method":"ping","x":Infinity}"#.into(), Some(-32700)),
        ("number out of range", r#"{"jsonrpc":"2.0","id":1,"method":"ping","x":1e400}"#.into(), Some(-32700)),
        ("trailing garbage", r#"{"jsonrpc":"2.0","id":1,"method":"ping"} x"#.into(), Some(-32700)),
        ("BOM", "\u{feff}{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}".into(), None),
        ("NUL", "\0".into(), Some(-32700)),
        ("method not a string", r#"{"jsonrpc":"2.0","id":1,"method":7}"#.into(), Some(-32600)),
        ("params an array", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":[1,2]}"#.into(), Some(-32602)),
        ("name not a string", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":5}}"#.into(), Some(-32602)),
        ("arguments an array", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"execute","arguments":[]}}"#.into(), Some(-32602)),
        ("params not an object", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"execute","arguments":{"command":"wire","params":"x"}}}"#.into(), None),
        ("limit huge", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecad_history","arguments":{"limit":18446744073709551615,"offset":18446744073709551615}}}"#.into(), None),
        ("limit too big for u64", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecad_project","arguments":{"limit":1e30}}}"#.into(), Some(-32602)),
        ("limit a float", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecad_project","arguments":{"limit":1.5}}}"#.into(), Some(-32602)),
        ("seq huge", r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecad_inspect","arguments":{"seq":18446744073709551615}}}"#.into(), None),
        ("id an object", r#"{"jsonrpc":"2.0","id":{"a":[1]},"method":"ping"}"#.into(), None),
        ("nesting just inside the parser limit", format!(r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"execute","arguments":{{"command":"wire","params":{{"points":{}}}}}}}}}"#, nest(100)), None),
        ("resources/read unknown", r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"file:///C:/Windows/win.ini"}}"#.into(), Some(-32002)),
        ("batch of garbage", r#"[1,"x",null,{}]"#.into(), Some(-32600)),
        ("huge hostile client name", format!(r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"clientInfo":{{"name":"{}\u0007\u0000"}}}}}}"#, "\u{202e}x".repeat(5000)), None),
        ("client name not a string", r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":[1]}}}"#.into(), None),
    ];
    for (what, line, code) in cases {
        let reply = c.s.handle_line(&line).unwrap_or_else(|| panic!("{what}: no reply"));
        let v: Value = serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{what}: reply not JSON ({e}): {reply}"));
        let v = if v.is_array() { v[0].clone() } else { v };
        assert_eq!(v["jsonrpc"], json!("2.0"), "{what}: {v}");
        match code {
            Some(code) => assert_eq!(v["error"]["code"], json!(code), "{what}: {v}"),
            None => assert!(v.get("result").is_some() || v.get("error").is_some(), "{what}: {v}"),
        }
    }
    // The hostile client name was cut to a safe author name.
    let name = c.s.agent();
    assert!(name.chars().count() <= ecad_mcp::MAX_AGENT_NAME && !name.chars().any(char::is_control), "{name:?}");
    // Extreme numbers in parameters: refused or recorded, never a panic, never more than one step.
    for p in [
        json!({"points": [[1e308, 1e308], [-1e308, -1e308]]}),
        json!({"points": [[f64::MAX, 0], [f64::MIN, 0]]}),
        json!({"points": [[5e-324, 0], [0, 5e-324]]}),
        json!({"points": [[0, 0], [0, 0]]}),
        json!({"points": [[0, 0]]}),
        json!({"points": []}),
        json!({"points": [[0, 0, 0, 0], [1]]}),
        json!({"points": [["1", "2"], [null, true]]}),
        json!({"points": [[18446744073709551615u64, 0], [-9223372036854775808i64, 0]]}),
    ] {
        let before = c.log().len();
        let r = c.exec_raw("wire", p.clone());
        assert!(r["content"][0]["text"].is_string(), "{p}: {r}");
        assert!(c.log().len() <= before + 1, "{p}");
    }
    for p in [json!({"from": [1e308, 0], "to": [-1e308, 0]}), json!({"from": [0, 0], "to": [0, 0]})] {
        let r = c.exec_raw("cableslash", p.clone());
        assert!(r["content"][0]["text"].is_string(), "{p}: {r}");
    }
    let _ = log;
    // Still serving.
    assert!(c.request("ping", json!({})).get("result").is_some());
}

/// The stdio framing: CRLF, blank lines, whitespace-only lines, a line just under and just over the
/// cap, and an unterminated last line.
#[test]
fn the_framing_takes_crlf_blank_lines_and_the_size_cap() {
    let mut c = writer();
    let ping = |id: u64| format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
    let mut input: Vec<u8> = Vec::new();
    input.extend_from_slice(format!("{}\r\n", ping(1)).as_bytes());
    input.extend_from_slice(b"\r\n\n   \t \r\n");
    // Just under the cap: a ping padded with spaces, ending in CRLF.
    let pad = MAX_REQUEST_BYTES - ping(2).len() - 2;
    input.extend_from_slice(ping(2).as_bytes());
    input.extend(std::iter::repeat_n(b' ', pad));
    input.extend_from_slice(b"\r\n");
    // Just over.
    input.extend_from_slice(ping(3).as_bytes());
    input.extend(std::iter::repeat_n(b' ', MAX_REQUEST_BYTES));
    input.extend_from_slice(b"\n");
    // Invalid UTF-8 inside a JSON string.
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"p\xc3\x28ing\"}\r\n");
    // The last line has no newline.
    input.extend_from_slice(ping(5).as_bytes());
    let mut out = Vec::new();
    c.s.serve(std::io::Cursor::new(input), &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    let replies: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 5, "{text}");
    assert_eq!(replies[0]["id"], json!(1));
    assert!(replies[0].get("result").is_some(), "CRLF is accepted: {}", replies[0]);
    assert_eq!(replies[1]["id"], json!(2), "a line just under the cap is served: {}", replies[1]);
    assert_eq!(replies[2]["error"]["code"], json!(-32600), "{}", replies[2]);
    assert_eq!(replies[3]["error"]["code"], json!(-32700), "{}", replies[3]);
    assert_eq!(replies[4]["id"], json!(5), "an unterminated last line is served: {}", replies[4]);
    assert!(!text.contains('\r'), "replies are LF-terminated");
}

// ---------- criterion 7: the binary on this platform ----------

fn rpc(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

/// Run the binary with `args`, feed `input` (raw bytes), close stdin; return the reply lines, the
/// exit code and stderr.
fn run_binary(args: &[&str], input: &[u8], settings: &TempDir) -> (Vec<Value>, Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ecad-mcp"))
        .args(args)
        .env(ecad_cadcraft::views::SETTINGS_ENV, &settings.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let replies: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|l| {
            let l = l.unwrap();
            serde_json::from_str(&l).unwrap_or_else(|e| panic!("stdout is not JSON-RPC ({e}): {l}"))
        })
        .collect();
    writer.join().unwrap();
    let out = child.wait_with_output().unwrap();
    (replies, out.status.code(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn scripted_session(path: &str) -> Vec<u8> {
    let mut s = Vec::new();
    let mut line = |l: &str| {
        s.extend_from_slice(l.as_bytes());
        s.extend_from_slice(b"\r\n");
    };
    line(&rpc(1, "initialize", json!({"protocolVersion": "2025-06-18", "clientInfo": {"name": "win-agent"}})));
    line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    line(&rpc(2, "tools/list", json!({})));
    line(&rpc(3, "tools/call", json!({"name": "ecad_project", "arguments": {}})));
    line(&rpc(4, "tools/call", json!({"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 0], [60, 0]]}}})));
    line(&rpc(5, "tools/call", json!({"name": "save", "arguments": {}})));
    line(&rpc(6, "tools/call", json!({"name": "execute", "arguments": {"command": "saveas", "params": {"path": path}}})));
    // Hostile lines.
    line("");
    line("{");
    line(r#"{"jsonrpc":"2.0","id":7,"method":"nope"}"#);
    line(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"nope"}}"#);
    line(&format!("{}{}", "[".repeat(10_000), "]".repeat(10_000)));
    s.extend_from_slice(b"\xff\xfe\x00garbage\r\n");
    s.extend(std::iter::repeat_n(b'x', MAX_REQUEST_BYTES + 100));
    s.extend_from_slice(b"\r\n");
    let mut line = |l: &str| {
        s.extend_from_slice(l.as_bytes());
        s.extend_from_slice(b"\r\n");
    };
    line(&rpc(9, "tools/call", json!({"name": "ecad_history", "arguments": {}})));
    s
}

#[test]
fn stdio_the_binary_on_a_path_with_spaces_with_and_without_write() {
    let dir = TempDir::new("stdio");
    let path = dir.file("agent project.ecad");
    let other = dir.file("other copy.ecad");

    // Create the project (the person's step), then serve it without --write.
    let (_, code, err) = run_binary(&["--create", &path], b"", &dir);
    assert_eq!(code, Some(0), "{err}");
    assert!(std::path::Path::new(&path).exists(), "{err}");
    let bytes = std::fs::read(&path).unwrap();

    let input = scripted_session(&other);
    let (r, code, err) = run_binary(&[&path], &input, &dir);
    assert_eq!(code, Some(0), "exit status: {err}");
    assert!(!err.contains("panicked"), "{err}");
    assert!(err.contains("writes off"), "{err}");
    let by_id = |r: &[Value], id: u64| r.iter().find(|v| v["id"] == json!(id)).cloned().unwrap_or(Value::Null);
    assert_eq!(by_id(&r, 1)["result"]["serverInfo"]["name"], json!("ecad"));
    let names: Vec<String> = by_id(&r, 2)["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert!(names.iter().any(|n| n == "ecad_project"), "{names:?}");
    assert_ne!(by_id(&r, 3)["result"]["isError"], json!(true));
    assert_eq!(by_id(&r, 4)["result"]["content"][0]["text"], json!(policy::WRITES_OFF));
    assert_eq!(by_id(&r, 5)["result"]["content"][0]["text"], json!(policy::WRITES_OFF));
    assert_eq!(by_id(&r, 6)["result"]["isError"], json!(true));
    assert_eq!(by_id(&r, 7)["error"]["code"], json!(-32601));
    assert_eq!(by_id(&r, 8)["error"]["code"], json!(-32602));
    assert!(by_id(&r, 9)["result"].is_object(), "still serving after the hostile lines");
    let nulls = r.iter().filter(|v| v["id"].is_null()).count();
    assert_eq!(nulls, 4, "parse error, deep nesting, non-UTF-8 and the oversized line: {r:?}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "the file is untouched without --write");
    assert!(!std::path::Path::new(&other).exists());

    // With --write: the wire is recorded and saved; Save As is still refused.
    let (r, code, err) = run_binary(&["--write", &path], &input, &dir);
    assert_eq!(code, Some(0), "exit status: {err}");
    assert!(!err.contains("panicked"), "{err}");
    assert!(err.contains("writes on"), "{err}");
    assert_ne!(by_id(&r, 4)["result"]["isError"], json!(true), "{:?}", by_id(&r, 4));
    assert_ne!(by_id(&r, 5)["result"]["isError"], json!(true), "{:?}", by_id(&r, 5));
    assert_eq!(by_id(&r, 6)["result"]["isError"], json!(true));
    assert!(!std::path::Path::new(&other).exists(), "Save As wrote nothing");

    // Reopened by a third run, read-only: the agent's wire with its author.
    let mut input = Vec::new();
    input.extend_from_slice(rpc(1, "tools/call", json!({"name": "ecad_history", "arguments": {}})).as_bytes());
    input.extend_from_slice(b"\r\n");
    let (r, code, err) = run_binary(&[&path], &input, &dir);
    assert_eq!(code, Some(0), "{err}");
    let h: Value = serde_json::from_str(r[0]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let last = h["transactions"].as_array().unwrap().last().unwrap().clone();
    assert!(last["label"].as_str().is_some_and(|l| l.eq_ignore_ascii_case("wire")), "{h}");
    assert_eq!(last["author"], json!("agent win-agent"), "{h}");
}

#[test]
fn stdio_a_backslash_path_and_a_second_server_on_a_held_file() {
    let dir = TempDir::new("held");
    // Backslashes on Windows; the platform separator elsewhere.
    let path = dir.file("held.ecad");
    let (_, code, err) = run_binary(&["--create", &path], b"", &dir);
    assert_eq!(code, Some(0), "{err}");
    // --create on an existing file is refused, and the file is kept.
    let bytes = std::fs::read(&path).unwrap();
    let history_before = history_of(&path);
    let (_, code, err) = run_binary(&["--create", &path], b"", &dir);
    assert_ne!(code, Some(0), "{err}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    // A first server holds the writer lock while a second one opens the file.
    let mut first = Command::new(env!("CARGO_BIN_EXE_ecad-mcp"))
        .args(["--write", &path])
        .env(ecad_cadcraft::views::SETTINGS_ENV, &dir.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut fin = first.stdin.take().unwrap();
    let mut fout = BufReader::new(first.stdout.take().unwrap());
    writeln!(fin, "{}", rpc(1, "ping", json!({}))).unwrap();
    let mut l = String::new();
    fout.read_line(&mut l).unwrap();
    assert!(l.contains("\"id\":1"), "{l}");

    let input = format!(
        "{}\n{}\n",
        rpc(1, "tools/call", json!({"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 0], [1, 0]]}}})),
        rpc(2, "tools/call", json!({"name": "save", "arguments": {}}))
    );
    let (r, code, err) = run_binary(&["--write", &path], input.as_bytes(), &dir);
    assert_eq!(code, Some(0), "{err}");
    assert!(err.contains("writes off"), "the second server is read-only: {err}");
    for v in &r {
        assert_eq!(v["result"]["isError"], json!(true), "{v}");
        assert!(v["result"]["content"][0]["text"].as_str().unwrap().contains("read-only"), "{v}");
    }
    drop(fin);
    let out = first.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The first server opened the file for writing (redb updates its own bookkeeping on open and
    // close, brief 021), so compare what the file holds: the same history, nothing saved.
    let _ = bytes;
    assert_eq!(history_of(&path), history_before, "nobody saved");
}

/// The whole history a version 2 file holds, as `ecadinspect export-jsonl` writes it.
fn history_of(path: &str) -> Vec<u8> {
    let store = ecad_file::v2::Store::open(std::path::Path::new(path), false).unwrap();
    let mut out = Vec::new();
    ecad_file::v2::inspect::export_jsonl(&store, false, &mut out).unwrap();
    out
}
