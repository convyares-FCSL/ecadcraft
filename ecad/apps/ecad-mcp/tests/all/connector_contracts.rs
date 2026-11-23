//! Connector devices through MCP: `connector` as a
//! row and as pins, `place` joining a half, and `pinlabel` each record one transaction by
//! `Author::Agent(<client>)`; the refusals record nothing; with writes off each is refused.

use ecad_mcp::policy;
use ecad_model::{DeviceId, Op};
use serde_json::{Value, json};

use crate::common::{assert_one_by_agent, writer};

const WRITER_SEED: u64 = 44;

fn component(v: &Value) -> DeviceId {
    DeviceId::from(u128::from_str_radix(v["component"].as_str().unwrap(), 16).unwrap())
}

#[test]
fn connector_row_and_pins_by_the_agent() {
    let mut c = writer(WRITER_SEED);
    let n = c.log().len();
    let v = c.exec("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, 50]}));
    let ops = assert_one_by_agent(&mut c, n, "write").ops;
    let x = component(&v);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::CreateDevice { .. })).count(), 1);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::EmbedDefinition { .. })).count(), 2);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::AddConnectionPoint { .. })).count(), 8);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::LinkRepresentation { rep, .. } if !rep.connection_points.is_empty())).count(), 2);
    assert_eq!(v["connection_points"].as_array().map(Vec::len), Some(8));

    // Pins across wires: one transaction with the splits.
    for x in [100.0, 120.0] {
        c.wire_pp(json!({"points": [[x, 250.0], [x, 150.0]]}));
    }
    let n = c.log().len();
    let v = c.exec("connector", json!({"halves": "both", "family": "m12", "pins": 4, "layout": "pins", "line": [[90, 200], [130, 200]]}));
    let ops = assert_one_by_agent(&mut c, n, "write").ops;
    assert_eq!(v["split"].as_array().map(Vec::len), Some(2));
    // Design 072: each wire split by a pin is two new wires (the pair it joined is gone).
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::CreateWire { .. })).count(), 4, "{ops:?}");
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::DeleteWire { .. })).count(), 2, "{ops:?}");

    // `place {symbol: pinpair, device}` joins and takes the next free pin.
    let n = c.log().len();
    let v = c.exec("place", json!({"symbol": "pinpair", "device": x.hex(), "at": [300, 300]}));
    assert_one_by_agent(&mut c, n, "write");
    assert_eq!(component(&v), x);
    assert_eq!(v["connection_points"][0]["pin"], "5", "pins 1-4 are drawn on both halves, so a pin pair takes a new pin");
}

#[test]
fn pinlabel_by_the_agent_and_refusals_record_nothing() {
    let mut c = writer(WRITER_SEED);
    let v = c.exec("connector", json!({"halves": "socket", "family": "plain", "pins": 3, "layout": "row", "at": [50, 50]}));
    let x = component(&v);
    let n = c.log().len();
    let r = c.exec("pinlabel", json!({"device": x.hex(), "pin": "3", "to": "PE"}));
    assert_one_by_agent(&mut c, n, "write");
    assert_eq!(r["to"], "PE");
    // §6.1's refusals leave the head where it was.
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": "connector", "params": {"halves": "socket", "device": x.hex(), "layout": "row", "at": [0, 0]}}));
    assert!(m.contains("already has a socket half"), "{m}");
    let m = c.refused("execute", json!({"command": "connector", "params": {"family": "dsub", "pins": 4, "layout": "row", "at": [0, 0]}}));
    assert!(m.contains("A D-sub has 9, 15 or 25 pins"), "{m}");
    let m = c.refused("execute", json!({"command": "pinlabel", "params": {"device": x.hex(), "pin": "1", "to": "PE"}}));
    assert!(m.contains("already has a pin PE"), "{m}");
    assert_eq!(c.log().len(), n, "refusals record nothing");
}

#[test]
fn writes_off_refuses_connector_commands() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    for (cmd, params) in [
        ("connector", json!({"layout": "row", "at": [0, 0]})),
        ("pinlabel", json!({"device": "-X01", "pin": "1", "to": "2"})),
        ("place", json!({"symbol": "socket4", "at": [0, 0]})),
    ] {
        let m = r.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}
