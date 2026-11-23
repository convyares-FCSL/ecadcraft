//! A PLC module's field set through MCP's `setprop` records
//! one `ModifyDevice` with the typed field, as the agent, and is refused with writes off.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::objects::plc::ModuleFunction;
use ecad_model::{Author, DeviceType, Op};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

#[test]
fn setprop_on_a_module_records_modify_device() {
    let mut c = Client::new(EcadMcp::in_memory("lane072c", 72, Options { write: true, agent: None }).unwrap());
    c.exec("place", json!({"symbol": "plccpu", "at": [40, 40]}));
    let card = c.exec("place", json!({"symbol": "plcdi", "at": [140, 40]}));
    let id = card["component"].as_str().unwrap().to_string();
    let before = c.log().len();
    c.exec("setprop", json!({"component": id, "key": "plc.ch.2.text", "value": "Door closed"}));
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    let [Op::ModifyDevice { after, .. }] = last.ops.as_slice() else { panic!("one ModifyDevice: {:?}", last.ops) };
    let DeviceType::PlcModule(m) = &after.device_type else { panic!("a module: {:?}", after.device_type) };
    assert!(matches!(m.function, ModuleFunction::DigitalIn(_)));
    assert_eq!(m.channel(2).and_then(|x| x.text.as_ref()).map(|t| t.as_str()), Some("Door closed"));

    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"component": id, "key": "plc.ch.2.text", "value": "x"}}));
}
