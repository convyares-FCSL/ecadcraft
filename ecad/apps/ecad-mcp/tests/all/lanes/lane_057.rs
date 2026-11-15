//! Lane 057 through MCP (brief 011 rule; design 057 §8.3): `rating` and `ratingtable {csv}` record
//! one transaction by the agent, undone by one `u`, and are refused with writes off;
//! `ratingtable {import}` and `{export}` are refused in every mode; `ratings` runs with writes off.
//!
//! The table rows here are synthetic, made up for the test; none is from a standard.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Document};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

const SYNTHETIC: &str = "# synthetic values for tests; not from any standard\ncapacity,E,pvc,3,1.5,20\ngrouping,E,3,0.90\n";

fn one_by_agent_then_undo(c: &mut Client, cmd: &str, params: Value) -> Value {
    let before: Document = c.doc();
    let n = c.log().len();
    let v = c.exec(cmd, params.clone());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{cmd} {params}: one transaction");
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())), "{cmd}");
    c.exec("u", json!({}));
    assert!(c.doc() == before, "{cmd} {params}: one undo restores the document");
    c.exec("redo", json!({}));
    v
}

#[test]
fn rating_and_ratingtable_are_one_agent_transaction_and_refused_with_writes_off() {
    let dir = TempDir::new("lane057");
    let path = dir.file("Ratings 057.ecad");
    {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let w = c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
        let conductor = w["conductor"].as_str().unwrap().to_string();
        one_by_agent_then_undo(&mut c, "rating", json!({"wire": conductor, "basis": "manual", "manual": "10 A", "bundling": "0.8"}));
        let t = one_by_agent_then_undo(&mut c, "ratingtable", json!({"name": "Test (synthetic)", "source": "made up", "csv": SYNTHETIC}));
        assert_eq!(t["rows"], json!(2), "{t}");
        let m = c.refused("execute", json!({"command": "ratingtable", "params": {"import": dir.file("x.csv")}}));
        assert!(m.contains("import"), "{m}");
        let m = c.refused("execute", json!({"command": "ratingtable", "params": {"export": dir.file("x.csv")}}));
        assert!(m.contains("export"), "{m}");
        let r = c.exec("ratings", json!({"wire": conductor}));
        assert_eq!(r["ratings"][0]["basis"], json!("manual"), "{r}");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
    }
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    let log = c.log();
    for (cmd, params) in [("rating", json!({"wire": "x", "load": "3"})), ("ratingtable", json!({"name": "T", "csv": SYNTHETIC}))] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    let r = c.exec("ratings", json!({}));
    assert_eq!(r["total"], json!(1), "{r}");
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
