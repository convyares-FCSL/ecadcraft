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
