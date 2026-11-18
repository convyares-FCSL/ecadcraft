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
