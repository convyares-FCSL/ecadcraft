//! Lane 072 B (design 072 §2.3, brief 011): `place` through MCP records the device with the type
//! its symbol declares, as the agent, and is refused with writes off.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::device::DeviceGroup;
use ecad_model::{Author, Op};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

#[test]
fn place_records_the_device_with_its_symbol_s_type() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b", 72, Options { write: true, agent: None }).unwrap());
    let before = c.log().len();
    c.exec("place", json!({"symbol": "mcb1", "at": [50, 50]}));
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    let types: Vec<DeviceGroup> = last
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::CreateDevice { device, .. } => Some(device.device_type.group()),
            _ => None,
        })
        .collect();
    assert_eq!(types, [DeviceGroup::Protection], "{:?}", last.ops);

    let mut r = reader();
    r.refused("execute", json!({"command": "place", "params": {"symbol": "mcb1", "at": [50, 50]}}));
}

/// Step 2b (design 072 §2.1): `setprop` of a device's description or maker through MCP records one
/// `ModifyDevice` as the agent, and is refused with writes off.
#[test]
fn setprop_of_a_text_field_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-text", 73, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "coil", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    for (key, value) in [("description", "Main pump"), ("maker", "Siemens"), ("part_number", "3RT2016")] {
        let before = c.log().len();
        c.exec("setprop", json!({"component": id, "key": key, "value": value}));
        let log = c.log();
        assert_eq!(log.len(), before + 1, "{key}");
        let last = log.last().unwrap();
        assert_eq!(last.author, Author::Agent(AGENT.into()));
        match last.ops.as_slice() {
            [Op::ModifyDevice { after, .. }] => assert_eq!(after.get(key), Some(value), "{key}"),
            other => panic!("{key}: {other:?}"),
        }
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "description", "value": "x"}}));
}

/// Step 2c (design 072 §6.3): `setlocation` through MCP records one `ModifyDevice` (with the new
/// list entry) as the agent, the location fixed, and is refused with writes off.
#[test]
fn setlocation_records_modify_device_with_fixed() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-loc", 74, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "coil", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    c.exec("setlocation", json!({"component": id, "value": "+CP1"}));
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyProject { .. }, Op::ModifyDevice { after, .. }] => assert!(after.location.is_some() && after.location_fixed, "{after:?}"),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setlocation", "params": {"component": id, "value": "+CP2"}}));
}

/// Step 3, Connector: `setprop` of a connector's pin count through MCP records one `ModifyDevice`
/// as the agent, and is refused with writes off.
#[test]
fn setprop_of_a_connector_field_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-conn", 75, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "socket4", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    c.exec("setprop", json!({"component": id, "key": "pins", "value": "6"}));
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyDevice { after, .. }] => assert_eq!(after.value("pins").as_deref(), Some("6")),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "pins", "value": "5"}}));
}

/// Step 3, Protection: `setprop rating` through MCP records one `ModifyDevice` with the typed
/// rating as the agent, answers what it read, and is refused with writes off.
#[test]
fn setprop_of_a_rating_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-rating", 76, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "mcb1", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    let r = c.exec("setprop", json!({"component": id, "key": "rating", "value": "C10"}));
    assert_eq!(r["message"], json!("Read: curve C, 10 A."), "{r}");
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyDevice { after, .. }] => assert_eq!(after.value("rating").as_deref(), Some("curve C, 10 A")),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "rating", "value": "B6"}}));
}

/// Step 3, Switching: `setprop rating` on a coil through MCP records one `ModifyDevice` with the
/// typed coil voltage and AC-3 rating as the agent, answers what it read, and is refused with
/// writes off.
#[test]
fn setprop_of_a_switching_rating_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-switching", 77, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "coil", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    let r = c.exec("setprop", json!({"component": id, "key": "rating", "value": "230 V AC, AC-3 12 A"}));
    assert_eq!(r["message"], json!("Read: coil 230 V AC, AC-3 12 A."), "{r}");
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyDevice { after, .. }] => assert_eq!(after.value("rating").as_deref(), Some("coil 230 V AC, AC-3 12 A")),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "rating", "value": "24 V DC"}}));
}

/// Step 3, Sensor: `setprop rating` on a PNP sensor through MCP records one `ModifyDevice` with
/// the typed supply as the agent, answers what it read, and is refused with writes off.
#[test]
fn setprop_of_a_sensor_rating_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-sensor", 78, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "prox3", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    let r = c.exec("setprop", json!({"component": id, "key": "rating", "value": "24 V DC"}));
    assert_eq!(r["message"], json!("Read: 24 V DC."), "{r}");
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyDevice { after, .. }] => assert_eq!(after.value("rating").as_deref(), Some("24 V DC")),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "rating", "value": "24 V DC"}}));
}

/// Step 3, TerminalStrip: `setprop rating.<p>` on a strip's fuse position through MCP records one
/// `ModifyDevice` with the typed fuse as the agent, answers what it read, and is refused with
/// writes off.
#[test]
fn setprop_of_a_strip_fuse_position_rating_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-strip", 79, Options { write: true, agent: None }).unwrap());
    for i in 1..=2 {
        let x = 10 * i;
        c.exec("wire", json!({"points": [[x, 0], [x, 40]]}));
    }
    let f = c.exec("terminals", json!({"type": "FUSE", "level": "1", "from": [0, 20], "to": [30, 20]}));
    let id = f["strip"].as_str().unwrap().to_string();
    let before = c.log().len();
    let r = c.exec("setprop", json!({"component": id, "key": "rating.1", "value": "4A"}));
    assert_eq!(r["message"], json!("Read: 4 A."), "{r}");
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyDevice { after, .. }] => assert_eq!(after.value("rating.1").as_deref(), Some("4 A")),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "rating.1", "value": "4A"}}));
}

/// Step 3, Motor: `setprop rating` on a motor through MCP records one `ModifyDevice` with the
/// typed nameplate as the agent, answers what it read, and is refused with writes off.
#[test]
fn setprop_of_a_motor_rating_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-motor", 79, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "motor3", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    let r = c.exec("setprop", json!({"component": id, "key": "rating", "value": "4 kW 8.2 A DOL"}));
    assert_eq!(r["message"], json!("Read: 4 kW, 8.2 A, DOL."), "{r}");
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyDevice { after, .. }] => assert_eq!(after.value("rating").as_deref(), Some("4 kW, 8.2 A, DOL")),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "rating", "value": "4 kW"}}));
}

/// Step 3, Load: `setprop rating` on a heater through MCP records one `ModifyDevice` with the
/// typed power as the agent, answers what it read, and is refused with writes off.
#[test]
fn setprop_of_a_load_rating_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072b-load", 80, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "heater", "at": [50, 50]}));
    let id = placed["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    let r = c.exec("setprop", json!({"component": id, "key": "rating", "value": "2,5 kW"}));
    assert_eq!(r["message"], json!("Read: 2.5 kW."), "{r}");
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    match last.ops.as_slice() {
        [Op::ModifyDevice { after, .. }] => assert_eq!(after.value("rating").as_deref(), Some("2.5 kW")),
        other => panic!("{other:?}"),
    }
    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "rating", "value": "2 kW"}}));
}
