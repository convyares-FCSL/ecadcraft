//! Brief 024 (design 024 §6.4, check G): through `execute`, `setprop` location on one and on
//! several devices, `locationbox` and `newpage {generated}` record one transaction as the agent and
//! are refused with writes off; `bom`, `wirelist`, `cablelist` and `printbook` (no `path`) read
//! with writes off and record nothing; `devices` shows `location_from`; a `path` is refused.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::Author;
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("mcp-024", 0x24_0011, Options { write: true, agent: None }).unwrap())
}

#[test]
fn locations_boxes_and_generated_pages_record_as_the_agent_and_reports_read_with_writes_off() {
    let mut c = writer();
    let k = c.exec("place", json!({"symbol": "coil", "at": [100, 100]}));
    let q = c.exec("place", json!({"symbol": "coil", "at": [150, 100]}));
    let (k, q) = (k["component"].as_str().unwrap().to_string(), q["component"].as_str().unwrap().to_string());
    let steps = vec![
        ("setprop", json!({"component": k, "key": "location", "value": "+CP2"})),
        ("setprop", json!({"components": [k, q], "key": "location", "value": "+CP3"})),
        ("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [200, 130]]})),
        ("locationbox", json!({"location": "+JB2", "points": [[0, 200], [60, 200], [60, 260], [0, 260]]})),
        ("newpage", json!({"generated": "bom-location"})),
    ];
    for (cmd, params) in steps {
        let before = c.log().len();
        c.exec(cmd, params);
        let log = c.log();
        assert_eq!(log.len(), before + 1, "{cmd}: one transaction");
        assert_eq!(log.last().unwrap().author, Author::Agent(AGENT.into()), "{cmd}");
    }
    let v = c.exec("devices", json!({}));
    assert!(v["devices"].as_array().unwrap().iter().all(|d| d["location_from"] == "device"), "{v}");
    // Reading records nothing.
    let before = c.log().len();
    let bom = c.exec("bom", json!({}));
    assert_eq!(bom["rows"].as_array().unwrap().len(), 2, "two coils with no part number: one row each: {bom}");
    c.exec("wirelist", json!({}));
    c.exec("cablelist", json!({}));
    let pdf = c.exec("printbook", json!({}));
    assert!(pdf["files"][0]["data"].as_str().unwrap().starts_with("JVBER"), "a PDF");
    assert_eq!(c.log().len(), before, "reports and printing record nothing");
    // A file path is refused in every mode.
    let m = c.refused("execute", json!({"command": "bom", "params": {"path": "x.csv"}}));
    assert!(m.contains("path"), "{m}");
    let m = c.refused("execute", json!({"command": "printbook", "params": {"path": "x.pdf"}}));
    assert!(m.contains("path"), "{m}");
}

#[test]
fn with_writes_off_location_writes_are_refused_and_reports_read() {
    let mut r = reader();
    let before = r.log();
    for (cmd, params) in [
        ("setprop", json!({"component": "00", "key": "location", "value": "+CP1"})),
        ("locationbox", json!({"location": "+JB1", "corners": [[0, 0], [50, 50]]})),
        ("newpage", json!({"generated": "cover"})),
    ] {
        r.refused("execute", json!({"command": cmd, "params": params}));
    }
    assert_eq!(r.log(), before);
    for cmd in ["bom", "wirelist", "cablelist", "printbook"] {
        r.exec(cmd, json!({}));
    }
    assert_eq!(r.log(), before);
}
