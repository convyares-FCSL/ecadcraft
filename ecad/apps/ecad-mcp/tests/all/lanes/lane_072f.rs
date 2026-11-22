//! Lane 072 F, F1b (design 072 §6.2; brief 011): a continuation is a drawing aid. Placed through
//! MCP's `execute` it records `CreateAid` with its point owned by the aid; `setprop {aids}` and
//! `contcaption` record one `ModifyAid` each; `xrefs {aid}` lists its other end; all by the
//! agent, and every one refused with writes off.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{Aid, Author, Op};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

fn last_aid(c: &mut Client) -> (Author, Vec<Op>) {
    let log = c.log();
    let last = log.last().unwrap();
    (last.author.clone(), last.ops.clone())
}

#[test]
fn continuations_through_mcp_are_aids() {
    let mut c = Client::new(EcadMcp::in_memory("lane072f", 0x72f, Options { write: true, agent: None }).unwrap());
    let a = c.exec("place", json!({"symbol": "continuation", "at": [20, 20], "name": "24V"}));
    let a = a["aid"].as_str().unwrap().to_string();
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert!(ops.iter().any(|o| matches!(o, Op::CreateAid { aid: Aid::Continuation { .. }, .. })), "{ops:?}");
    assert!(ops.iter().any(|o| matches!(o, Op::AddConnectionPoint { terminal, .. } if terminal.owner.aid().is_some())), "{ops:?}");
    assert!(!ops.iter().any(|o| matches!(o, Op::CreateDevice { .. })), "no device: {ops:?}");
    let b = c.exec("place", json!({"symbol": "continuation", "at": [80, 20], "name": "24V"}))["aid"].as_str().unwrap().to_string();

    c.exec("setprop", json!({"aids": [a], "key": "name", "value": "0V"}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    match ops.as_slice() {
        [Op::ModifyAid { after, .. }] => assert_eq!(after.continuation_name().map(|n| n.as_str()), Some("0V")),
        other => panic!("one ModifyAid: {other:?}"),
    }
    c.exec("setprop", json!({"aids": [a], "key": "name", "value": "24V"}));

    c.exec("contcaption", json!({"continuations": [a, b], "caption": "TO PSU"}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::ModifyAid { after, .. } if after.continuation_caption().is_some())).count(), 2, "{ops:?}");

    let x = c.exec("xrefs", json!({"aid": a}));
    assert!(x.to_string().contains("continuation"), "{x}");

    let before = c.log();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"aids": [a], "key": "colour", "value": "red"}}));
    assert!(m.contains("name") && m.contains("caption"), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");

    let mut r = reader();
    r.refused("execute", json!({"command": "place", "params": {"symbol": "continuation", "at": [20, 20], "name": "24V"}}));
    r.refused("execute", json!({"command": "setprop", "params": {"aids": [a], "key": "name", "value": "0V"}}));
    r.refused("execute", json!({"command": "contcaption", "params": {"continuations": [a], "caption": "X"}}));
}

/// F1c: a location box is a drawing aid. Placed through `execute` it records `CreateAid` (no
/// device); `setprop {aids}` with its location records one `ModifyAid`; both by the agent. A key
/// a box does not have is refused with nothing recorded, and every one is refused with writes off.
#[test]
fn location_boxes_through_mcp_are_aids() {
    let mut c = Client::new(EcadMcp::in_memory("lane072f-box", 0x72fb, Options { write: true, agent: None }).unwrap());
    let v = c.exec("locationbox", json!({"location": "+CP1", "corners": [[10, 10], [80, 60]]}));
    let b = v["aid"].as_str().unwrap_or_else(|| panic!("the box answers its aid: {v}")).to_string();
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert!(ops.iter().any(|o| matches!(o, Op::CreateAid { aid: Aid::Box(x), .. } if x.location.is_some())), "{ops:?}");
    assert!(!ops.iter().any(|o| matches!(o, Op::CreateDevice { .. })), "no device: {ops:?}");

    c.exec("setprop", json!({"aids": [b], "key": "location", "value": "+CP1"}));
    let unchanged = c.log().len();
    c.exec("setprop", json!({"aids": [b], "key": "panel_temperature", "value": "40"}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(c.log().len(), unchanged + 1);
    match ops.as_slice() {
        [Op::ModifyAid { after: Aid::Box(x), .. }] => assert_eq!(x.panel_temperature, Some(ecad_model::geometry::units::Celsius(40.0))),
        other => panic!("one ModifyAid: {other:?}"),
    }
    let n = c.log().len();
    c.exec("setprop", json!({"aids": [b], "key": "location", "value": "+CP2"}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(c.log().len(), n + 1, "one transaction");
    let modified: Vec<&Op> = ops.iter().filter(|o| matches!(o, Op::ModifyAid { .. })).collect();
    assert_eq!(modified.len(), 1, "one ModifyAid: {ops:?}");
    assert!(matches!(modified[0], Op::ModifyAid { before: Aid::Box(x), after: Aid::Box(y), .. } if x.location != y.location), "{ops:?}");
    assert!(!ops.iter().any(|o| matches!(o, Op::SetProperty { .. } | Op::ModifyDevice { .. })), "{ops:?}");

    let before = c.log();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"aids": [b], "key": "name", "value": "24V"}}));
    assert!(m.contains("location box") && m.contains("panel_temperature"), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");

    let mut r = reader();
    r.refused("execute", json!({"command": "locationbox", "params": {"location": "+CP1", "corners": [[10, 10], [80, 60]]}}));
    r.refused("execute", json!({"command": "setprop", "params": {"aids": [b], "key": "location", "value": "+CP2"}}));
    r.refused("execute", json!({"command": "setprop", "params": {"aids": [b], "key": "panel_temperature", "value": "40"}}));
}

/// F1c-2: a box keeps its own location style. `setprop {aids, key: "style"}` records one
/// `ModifyAid` with the style's ID, by the agent, and null clears it; an unknown style is refused;
/// `setlocation` on a box names it as a box; every write is refused with writes off.
#[test]
fn a_location_boxs_own_style_through_mcp() {
    let mut c = Client::new(EcadMcp::in_memory("lane072f-box-style", 0x72fc, Options { write: true, agent: None }).unwrap());
    let v = c.exec("locationbox", json!({"location": "+CP1", "corners": [[10, 10], [80, 60]]}));
    let b = v["aid"].as_str().unwrap_or_else(|| panic!("the box answers its aid: {v}")).to_string();

    let n = c.log().len();
    c.exec("setprop", json!({"aids": [b], "key": "style", "value": "box"}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(c.log().len(), n + 1, "one transaction");
    match ops.as_slice() {
        [Op::ModifyAid { before: Aid::Box(x), after: Aid::Box(y), .. }] => {
            assert_eq!(x.style, None);
            assert_eq!(y.style, ecad_model::StyleRef::builtin("box"));
        }
        other => panic!("one ModifyAid: {other:?}"),
    }
    c.exec("setprop", json!({"aids": [b], "key": "style", "value": null}));
    let (_, ops) = last_aid(&mut c);
    assert!(matches!(ops.as_slice(), [Op::ModifyAid { after: Aid::Box(y), .. }] if y.style.is_none()), "cleared: {ops:?}");

    let before = c.log();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"aids": [b], "key": "style", "value": "No such style"}}));
    assert!(m.contains("No such style"), "{m}");
    let m = c.refused("execute", json!({"command": "setlocation", "params": {"component": b, "value": "+CP2"}}));
    assert!(m.contains(&format!("{b} is a location box; edit it with setprop {{aids}}")), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");

    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"aids": [b], "key": "style", "value": "box"}}));
}

/// F2b: a BOM item is a loose part (design 072 §6.4). `bomitem` through `execute` records one
/// `CreateLoosePart` (no device), `{item, set}` one `ModifyLoosePart` and `{item, delete}` one
/// `DeleteLoosePart`, all by the agent; `bom` names the item by its own id. A description longer
/// than 200 characters is refused in words, never cut, with nothing recorded; every write is
/// refused with writes off.
#[test]
fn bom_items_through_mcp_are_loose_parts() {
    let mut c = Client::new(EcadMcp::in_memory("lane072f-loose", 0x72f2, Options { write: true, agent: None }).unwrap());
    let v = c.exec("bomitem", json!({"description": "DIN rail 35x7.5", "qty": "1.5", "unit": "m", "location": "+CP1"}));
    let item = v["item"].as_str().unwrap_or_else(|| panic!("the item answers its id: {v}")).to_string();
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    let made = ops.iter().find_map(|o| match o {
        Op::CreateLoosePart { id, part } => Some((id.hex(), part.clone())),
        _ => None,
    });
    let (id, part) = made.unwrap_or_else(|| panic!("one CreateLoosePart: {ops:?}"));
    assert_eq!(id, item);
    assert_eq!(
        (part.description.as_str(), part.qty.to_string(), part.unit),
        ("DIN rail 35x7.5", "1.5".to_string(), ecad_model::objects::loose_part::ItemUnit::Metres)
    );
    assert!(part.location.is_some(), "the typed location is an entry of the list");
    assert!(!ops.iter().any(|o| matches!(o, Op::CreateDevice { .. })), "no device: {ops:?}");

    c.exec("bomitem", json!({"item": item, "set": {"part_number": "DR-35", "qty": 2}}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    match ops.as_slice() {
        [Op::ModifyLoosePart { after, .. }] => {
            assert_eq!((after.part_number.as_ref().map(|t| t.as_str()), after.qty.to_string()), (Some("DR-35"), "2".to_string()))
        }
        other => panic!("one ModifyLoosePart: {other:?}"),
    }
    let rows = c.exec("bom", json!({}))["rows"].as_array().unwrap().clone();
    let rail = rows.iter().find(|r| r["part_number"] == "DR-35").unwrap_or_else(|| panic!("the item is on the BOM: {rows:?}"));
    assert_eq!(rail["entries"], json!([{"item": item}]));

    let before = c.log();
    let m = c.refused("execute", json!({"command": "bomitem", "params": {"description": "x".repeat(201)}}));
    assert!(m.contains("description") && m.contains("200"), "{m}");
    let m = c.refused("execute", json!({"command": "bomitem", "params": {"item": item, "set": {"description": "y".repeat(201)}}}));
    assert!(m.contains("200"), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");
    c.exec("bomitem", json!({"description": "z".repeat(200)}));
    assert_eq!(c.log().len(), before.len() + 1, "200 characters are taken");

    c.exec("bomitem", json!({"item": item, "delete": true}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert!(matches!(ops.as_slice(), [Op::DeleteLoosePart { .. }]), "{ops:?}");

    let mut r = reader();
    r.refused("execute", json!({"command": "bomitem", "params": {"description": "DIN rail"}}));
    r.refused("execute", json!({"command": "bomitem", "params": {"item": item, "delete": true}}));
}
