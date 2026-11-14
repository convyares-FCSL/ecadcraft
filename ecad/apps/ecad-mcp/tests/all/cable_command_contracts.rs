//! Brief 011's MCP contract rule applied to the commands briefs 009 and 010 added or changed:
//! `cablecore`, `cablefan`, `cableref`, `cablecontinue`, `cablegoto`, `cablerefs`, `renamepage` and
//! `movepage`. Every test drives the server through JSON-RPC lines, in-process.
//!
//! - Each is listed by `list_commands` with its parameter docs.
//! - Each change through `execute` records exactly the JSON form's transaction (the same rows run
//!   directly through `Session::execute`, as `cable_reference_contracts.rs`'s JSON-form table does),
//!   authored `agent <client name>`, and `u` undoes it in one step.
//! - Refusals record nothing and leave the document as it was.
//! - With writes off the changing commands are refused; `cablerefs` and `cablegoto` run, record
//!   nothing, and `cablegoto` only switches the open page.

use std::path::PathBuf;

use cadcraft_engine::Session;
use ecad_cadcraft::{Persistence, Project, ProjectHandle};
use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::derive::cable_references;
use ecad_model::{Author, Document, PageId, Transaction};
use serde_json::{Value, json};

const AGENT: &str = "cable-agent";
const SEED: u64 = 0x0E_CAD0_0910;

/// The commands of briefs 009 and 010.
const COMMANDS: &[&str] = &["cablecore", "cablefan", "cableref", "cablecontinue", "cablegoto", "cablerefs", "renamepage", "movepage"];

struct Client {
    s: EcadMcp,
    next: u64,
}

impl Client {
    fn new(s: EcadMcp) -> Self {
        let mut c = Client { s, next: 1 };
        let r = c.request("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": AGENT, "version": "1"}}));
        assert!(r.get("result").is_some(), "{r}");
        assert!(c.s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        c.s.with_session(|s, p| p.configure(s, |p| p.settings = ecad_cadcraft::views::Settings::none())).unwrap().unwrap();
        c
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

    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        assert!(r.get("error").is_none(), "{tool} {r}");
        r["result"].clone()
    }

    fn ok(&mut self, tool: &str, args: Value) -> Value {
        let r = self.call(tool, args.clone());
        assert_ne!(r["isError"], json!(true), "{tool} {args}: {r}");
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    fn refused(&mut self, tool: &str, args: Value) -> String {
        let r = self.call(tool, args.clone());
        assert_eq!(r["isError"], json!(true), "{tool} {args} should be refused: {r}");
        r["content"][0]["text"].as_str().unwrap().to_string()
    }

    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.ok("execute", json!({"command": command, "params": params}))
    }

    fn log(&mut self) -> Vec<Transaction> {
        self.s.with_session(|s, p| p.read(s, |p, _| p.ledger.full_log().unwrap()).unwrap()).unwrap()
    }

    fn doc(&mut self) -> Document {
        self.s.with_session(|s, p| p.read(s, |p, _| p.ledger.document().clone()).unwrap()).unwrap()
    }

    /// The active document's uid and the uid of `page`'s open document, if it is open.
    fn active_and(&mut self, page: PageId) -> (Option<u64>, Option<u64>) {
        self.s
            .with_session(|s, p| {
                let active = s.state().ok().map(|st| st.uid);
                (active, p.read(s, |p, _| p.pages.get(&page).and_then(|v| v.uid)).unwrap())
            })
            .unwrap()
    }
}

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("cables", SEED, Options { write: true, agent: None }).unwrap())
}

/// The same project driven directly through the JSON forms, as a person's typed commands are.
struct Direct {
    s: Session,
    p: ProjectHandle,
}

impl Direct {
    fn new() -> Self {
        ecad_cadcraft::install();
        let mut s = Session::empty();
        let p = Project::create_with(&mut s, "cables", SEED, Persistence::File).unwrap();
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
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("ecad-mcp-cables-{tag}-{}-{nanos}", std::process::id()));
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

// ---------------------------------------------------------------- the rows

/// The page titled `title` (the first page is renamed `Supply`, later `Source`).
fn page_titled(doc: &Document, titles: &[&str]) -> Option<PageId> {
    doc.page_order.iter().copied().find(|p| doc.pages.get(p).is_some_and(|pg| titles.contains(&pg.title.as_str())))
}

fn supply(doc: &Document) -> PageId {
    page_titled(doc, &["Supply", "Source"]).unwrap_or(doc.page_order[0])
}

fn field(doc: &Document) -> PageId {
    page_titled(doc, &["Field"]).expect("the Field page")
}

/// Fills a row's placeholders from the document as it is when the row runs: `<supply>`, `<field>`,
/// `<c1>` (the one cable), `<w0>` (its first member) and `<ref supply>` / `<ref field>`.
fn fill(doc: &Document, v: &Value) -> Value {
    match v {
        Value::String(s) => {
            let reference = |page: PageId| cable_references(doc).into_iter().find(|r| r.page == page).expect("a reference").reference.hex();
            let c1 = || *doc.cables.keys().next().expect("a cable");
            match s.as_str() {
                "<supply>" => Value::String(supply(doc).hex()),
                "<field>" => Value::String(field(doc).hex()),
                "<c1>" => Value::String(c1().hex()),
                "<w0>" => Value::String(doc.cables[&c1()].members[0].conductor.hex()),
                "<ref supply>" => Value::String(reference(supply(doc))),
                "<ref field>" => Value::String(reference(field(doc))),
                _ => v.clone(),
            }
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| fill(doc, x)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), fill(doc, x))).collect()),
        other => other.clone(),
    }
}

/// (command, params, a substring of the result, transactions it records). Fixture R of
/// `cable_reference_contracts.rs` without its components, plus brief 009's core labels and join.
fn rows() -> Vec<(&'static str, Value, &'static str, usize)> {
    vec![
        ("renamepage", json!({"page": "<supply>", "title": "Supply"}), r#""changed":true"#, 1),
        ("wire", json!({"points": [[0, 0], [60, 0]]}), "conductor", 1),
        ("wire", json!({"points": [[0, 5], [60, 5]]}), "conductor", 1),
        ("wire", json!({"points": [[0, 10], [60, 10]]}), "conductor", 1),
        ("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"}), "cable", 1),
        // Brief 009: core labels, one or all; the same label again records nothing.
        ("cablecore", json!({"cable": "<c1>", "cores": ["L1", "L2", null]}), "", 1),
        ("cablecore", json!({"conductor": "<w0>", "core": "N"}), r#""changed":true"#, 1),
        ("cablecore", json!({"conductor": "<w0>", "core": "N"}), r#""changed":false"#, 0),
        // Brief 009: the fan join, as a fraction; the same value again records nothing.
        ("cablefan", json!({"cable": "<c1>", "end": "A", "at": 0.25}), r#""changed":true"#, 1),
        ("cablefan", json!({"cable": "<c1>", "end": "A", "at": 0.25}), r#""changed":false"#, 0),
        ("cablefan", json!({"cable": "<c1>", "end": "A", "at": null}), "", 1),
        // Brief 010: a reference on Supply (an orphan until continued), then the continuation.
        ("cablerefs", json!({}), r#"{"references":[]}"#, 0),
        ("cableref", json!({"cable": "<c1>", "at": [65, 5]}), r#""orphan":true"#, 1),
        ("cablerefs", json!({"cable": "<c1>"}), r#""text":"?""#, 0),
        ("newpage", json!({"title": "Field"}), "page", 1),
        ("openpage", json!({"page": "<field>"}), "document", 0),
        ("cablecontinue", json!({"cable": "<c1>", "at": [20, 5], "toward": [40, 5]}), r#""text":"1/F2""#, 1),
        ("cablerefs", json!({}), r#""text":"2/F1""#, 0),
        ("renamepage", json!({"page": "<supply>", "title": "  Supply  "}), r#""changed":false"#, 0),
        ("renamepage", json!({"page": "<supply>", "title": "Source"}), r#""changed":true"#, 1),
        ("movepage", json!({"page": "<field>", "index": 2}), r#""changed":false"#, 0),
        ("movepage", json!({"page": "<field>", "index": 1}), r#""index":1"#, 1),
        ("cablerefs", json!({"cable": "<c1>"}), r#""text":"2/F1""#, 0),
        ("cablegoto", json!({"reference": "<ref field>"}), r#"Page 1 \"Source\", at the other end of cable -C01"#, 0),
    ]
}

// ---------------------------------------------------------------- listed

#[test]
fn the_cable_and_page_commands_are_listed_with_their_parameter_docs() {
    let mut c = writer();
    let catalog = c.ok("list_commands", json!({}));
    let catalog = catalog.as_array().unwrap();
    let specs = ecad_cadcraft::commands::specs();
    for id in COMMANDS {
        let spec = specs.iter().find(|s| s.id == *id).unwrap_or_else(|| panic!("{id} is an ECAD command"));
        let info = catalog.iter().find(|i| i["id"] == *id).unwrap_or_else(|| panic!("{id} not listed"));
        assert_eq!(info["params"].as_str(), Some(spec.params), "{id}");
        assert!(spec.params.starts_with('{'), "{id} has parameter docs: {:?}", spec.params);
    }
    let pages = c.ok("list_commands", json!({"filter": "page"}));
    let ids: Vec<&str> = pages.as_array().unwrap().iter().filter_map(|i| i["id"].as_str()).collect();
    assert!(ids.contains(&"renamepage") && ids.contains(&"movepage"), "{ids:?}");
}

// ---------------------------------------------------------------- the transactions

#[test]
fn each_change_through_mcp_records_the_json_forms_transaction_as_the_agent_and_u_undoes_it() {
    // Directly, as a person's JSON forms.
    let mut d = Direct::new();
    for (cmd, params, want, _) in rows() {
        let doc = d.doc();
        let params = fill(&doc, &params);
        let v = d.s.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
        assert!(v.to_string().contains(want), "{cmd} {params}: {v}");
    }
    let direct = d.log();

    // Through MCP.
    let mut c = writer();
    let mut docs = vec![c.doc()];
    for (cmd, params, want, added) in rows() {
        let doc = c.doc();
        let params = fill(&doc, &params);
        let before = c.log();
        let r = c.exec(cmd, params.clone());
        assert!(r.to_string().contains(want), "{cmd} {params}: {r}");
        let after = c.log();
        assert_eq!(after.len(), before.len() + added, "{cmd} {params}: transactions");
        if added == 0 {
            assert!(r.get("ecadTransactions").is_none(), "{cmd} {params}: {r}");
            assert_eq!(c.doc(), doc, "{cmd} {params} changed nothing");
            continue;
        }
        let txs = r["ecadTransactions"].as_array().unwrap_or_else(|| panic!("{cmd}: {r}"));
        assert_eq!(txs.len(), 1, "{cmd}: {r}");
        assert_eq!(txs[0]["author"], json!(format!("agent {AGENT}")), "{cmd}");
        assert_eq!(txs[0]["seq"], json!(after.last().unwrap().seq), "{cmd}");
        docs.push(c.doc());
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
    // Every changing command of the two briefs took part in the comparison.
    let labels: Vec<&str> = agent.iter().map(|t| t.label.as_str()).collect();
    for label in ["Core labels", "Core label", "Cable join", "Cable reference", "Continue cable", "Rename page", "Move page"] {
        assert!(labels.contains(&label), "{label} missing from {labels:?}");
    }

    // `u` undoes each change in one step, newest first, as the agent.
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
}

// ---------------------------------------------------------------- refusals

/// Builds the continued fixture through MCP: C1 referenced on Supply and continued on Field.
fn continued(c: &mut Client) {
    for (cmd, params, _, _) in rows().into_iter().take(17) {
        let doc = c.doc();
        c.exec(cmd, fill(&doc, &params));
    }
    assert_eq!(cable_references(&c.doc()).len(), 2, "two references");
}

#[test]
fn refusals_through_mcp_say_why_and_change_nothing() {
    let mut c = writer();
    continued(&mut c);
    let doc = c.doc();
    c.exec("openpage", json!({"page": supply(&doc).hex()}));
    let log = c.log();
    let rows: Vec<(&str, Value, &str)> = vec![
        ("cablecore", json!({"cable": "<c1>", "cores": ["a"]}), "`cores` must list one label (or null) per member"),
        ("cablecore", json!({"conductor": "zz", "core": "a"}), "`conductor` (hex id) is required"),
        ("cablefan", json!({"cable": "<c1>", "end": "A", "at": 1.5}), "`at` as a fraction must be from 0 to 1"),
        ("cablefan", json!({"cable": "<c1>", "at": [1e300, 0]}), "Point out of range."),
        ("cableref", json!({"cable": "<c1>", "at": [70, 0]}), "Cable -C01 already has a reference on this page."),
        ("cableref", json!({"cable": "<c1>", "at": [f64::MAX, 0]}), "is not a finite [x,y] point"),
        ("cablecontinue", json!({"cable": "<c1>", "at": [0, 0], "toward": [0, 0]}), "`toward` must differ from `at`"),
        ("cablecontinue", json!({"cable": "<c1>", "at": [80, 5], "toward": [90, 5]}), "Cable -C01 already continues to another page"),
        ("cablegoto", json!({}), "`reference` (hex id) is required"),
        ("cablegoto", json!({"reference": "<w0>"}), "no such reference"),
        ("cablerefs", json!({"cable": "nope"}), "`cable` (hex id) is required"),
        ("renamepage", json!({"page": "<supply>", "title": "   "}), "`title` must be a non-empty string"),
        ("renamepage", json!({"page": "<supply>", "title": "x".repeat(10_000)}), "`title` must be a non-empty string"),
        ("movepage", json!({"page": "<supply>", "index": 0}), "`index` must be a whole number from 1 to 2"),
        ("movepage", json!({"page": "<supply>", "index": 1e20}), "`index` must be a whole number from 1 to 2"),
    ];
    for (cmd, params, want) in rows {
        let params = fill(&doc, &params);
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert!(m.contains(want), "{cmd} {params}: {m}");
        assert_eq!(c.log(), log, "{cmd} {params}: nothing recorded");
        assert_eq!(c.doc(), doc, "{cmd} {params}: nothing changed");
    }
}

// ---------------------------------------------------------------- writes off

#[test]
fn with_writes_off_the_changes_are_refused_and_cablerefs_and_cablegoto_only_read() {
    let dir = TempDir::new("readonly");
    let path = dir.file("cables.ecad");
    {
        let mut w = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        continued(&mut w);
        w.ok("save", json!({}));
    }
    let bytes = std::fs::read(&path).unwrap();
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    let doc = c.doc();

    // Every changing command of the two briefs is refused before it reaches the engine.
    for (cmd, params) in [
        ("cablecore", json!({"cable": "<c1>", "cores": ["L1", "L2", "L3"]})),
        ("cablefan", json!({"cable": "<c1>", "end": "A", "at": 0.5})),
        ("cableref", json!({"cable": "<c1>", "at": [70, 0]})),
        ("cablecontinue", json!({"cable": "<c1>", "at": [0, 30], "toward": [10, 30]})),
        ("renamepage", json!({"page": "<supply>", "title": "Other"})),
        ("movepage", json!({"page": "<field>", "index": 1})),
    ] {
        let params = fill(&doc, &params);
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
        assert_eq!(c.log(), log, "{cmd} moved the head");
    }

    // cablerefs reads.
    let refs = c.exec("cablerefs", json!({}));
    let texts: Vec<&str> = refs["references"].as_array().unwrap().iter().filter_map(|r| r["text"].as_str()).collect();
    assert_eq!(texts.len(), 2, "{refs}");
    assert!(texts.contains(&"2/F1") && texts.contains(&"1/F2"), "{refs}");

    // cablegoto from Supply's reference opens Field, selects the other end, and records nothing.
    let (supply, field) = (supply(&doc), field(&doc));
    c.exec("openpage", json!({"page": supply.hex()}));
    let (active, on_supply) = c.active_and(supply);
    assert_eq!(active, on_supply, "Supply is open");
    let from = fill(&doc, &json!("<ref supply>"));
    let v = c.exec("cablegoto", json!({"reference": from}));
    assert_eq!(v["page"], json!(field.hex()), "{v}");
    assert_eq!(v["reference"], fill(&doc, &json!("<ref field>")), "{v}");
    assert_eq!(v["message"], json!("Page 2 \"Field\", at the other end of cable -C01 (2/F1)."), "{v}");
    assert!(v.get("ecadTransactions").is_none(), "{v}");
    let (active, on_field) = c.active_and(field);
    assert!(on_field.is_some() && active == on_field, "the open page switched to Field");
    // And back.
    let back = fill(&doc, &json!("<ref field>"));
    let v = c.exec("cablegoto", json!({"reference": back}));
    assert_eq!(v["page"], json!(supply.hex()), "{v}");
    let (active, on_supply) = c.active_and(supply);
    assert_eq!(active, on_supply, "and back to Supply");

    // Nothing changed: the history, the document, the unsaved state and the file.
    assert_eq!(c.log(), log, "cablerefs and cablegoto recorded nothing");
    assert_eq!(c.doc(), doc);
    assert_eq!(c.ok("ecad_project", json!({}))["unsaved"], json!(false));
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "the file is untouched");
}
