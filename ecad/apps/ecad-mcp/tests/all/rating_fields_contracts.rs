//! One rating field through MCP: `deviceprops` lists a device's `rating_fields`; `setprop
//! rating.<word>` records one transaction by `Author::Agent(<client>)`, a value of another type is
//! refused with the field's label and records nothing, and with writes off it is refused in the
//! gate's words.

use ecad_engine::agent::policy;
use ecad_mcp::{EcadMcp, Options};
use ecad_model::Author;
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, TempDir};

#[test]
fn a_rating_field_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("d4 rating fields");
    let path = dir.file("Fields.ecad");
    let motor = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let m = c.exec("place", json!({"symbol": "motor3", "at": [100, 100]}))["component"].clone();
        let fields = c.exec("deviceprops", json!({"component": m}))["rating_fields"].clone();
        assert_eq!(fields[2]["key"], json!("rating.current"));
        assert_eq!((fields[2]["unit"].clone(), fields[2]["example"].clone()), (json!("A"), json!("16 A")));
        let n = c.log().len();
        c.exec("setprop", json!({"component": m, "key": "rating.current", "value": "15.2"}));
        let log = c.log();
        assert_eq!(log.len(), n + 1, "one transaction");
        assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())));
        assert_eq!(c.exec("deviceprops", json!({"component": m}))["rating_fields"][2]["value"], json!("15.2 A"));
        let e = c.refused("execute", json!({"command": "setprop", "params": {"component": m, "key": "rating.current", "value": "400 V"}}));
        assert!(e.contains("Rated current takes a current"), "{e}");
        assert_eq!(c.log().len(), n + 1, "a refusal records nothing");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        m
    };
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    let log = c.log();
    let e = c.refused("execute", json!({"command": "setprop", "params": {"component": motor, "key": "rating.current", "value": "16 A"}}));
    assert_eq!(e, policy::WRITES_OFF);
    assert_eq!(c.log(), log, "nothing recorded with writes off");
    assert_eq!(c.exec("deviceprops", json!({"component": motor}))["rating_fields"][2]["value"], json!("15.2 A"), "saved and read back");
}

/// A datasheet field typed by the agent is one transaction, saved with the project and read back.
#[test]
fn a_datasheet_field_by_the_agent_is_saved_and_read_back() {
    let dir = TempDir::new("d4 indicator fields");
    let path = dir.file("Lamp.ecad");
    let lamp = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let l = c.exec("place", json!({"symbol": "lamp", "at": [100, 100]}))["component"].clone();
        let n = c.log().len();
        c.exec("setprop", json!({"component": l, "key": "rating.ip_rating", "value": "IPX4"}));
        let log = c.log();
        assert_eq!(log.len(), n + 1, "one transaction");
        assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())));
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        l
    };
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    let fields = c.exec("deviceprops", json!({"component": lamp}))["rating_fields"].clone();
    let ip = fields.as_array().and_then(|f| f.iter().find(|f| f["key"] == json!("rating.ip_rating"))).cloned().unwrap_or_default();
    assert_eq!((ip["value"].clone(), ip["source"].clone(), ip["type"].clone()), (json!("IPX4"), json!("datasheet"), json!("ip_code")));
}
