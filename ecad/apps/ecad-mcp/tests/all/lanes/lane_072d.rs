//! Lane 072 D (design 072 §4; brief 011): a wire's size, colours and rating inputs set through
//! MCP's `execute` record one `ModifyWire` by the agent; a value that does not read, and a key a
//! wire does not have, are refused and record nothing; `wire` point to point records its route
//! and the wire the rewrite made; `cableslash` across a loose line puts the core on the line;
//! with writes off each command is refused.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::units::Amps;
use ecad_model::wire::Basis;
use ecad_model::{Author, CorePosition, Op, RouteEnd, RouteId, Wire};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn last_modified(c: &mut Client) -> (Author, Wire) {
    let log = c.log();
    let last = log.last().unwrap();
    let after = last
        .ops
        .iter()
        .find_map(|o| match o {
            Op::ModifyWire { after, .. } => Some(after.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a ModifyWire: {:?}", last.ops));
    (last.author.clone(), after)
}

#[test]
fn a_wires_values_set_through_mcp_record_one_modify_wire() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d", 72, Options { write: true, agent: None }).unwrap());
    let v = c.wire_pp(json!({"points": [[0, 0], [50, 0]]}));
    let w = v["conductor"].as_str().unwrap().to_string();

    let before = c.log().len();
    c.exec("setprop", json!({"conductor": w, "key": "size", "value": "2.5 mm²"}));
    assert_eq!(c.log().len(), before + 1);
    let (author, after) = last_modified(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(after.size.map(|s| s.to_string()).as_deref(), Some("2.5"));

    c.exec("setprop", json!({"conductor": w, "key": "colours", "value": "GNYE"}));
    let (_, after) = last_modified(&mut c);
    assert_eq!(after.colours.map(|c| c.to_string()).as_deref(), Some("GN/YE"));

    c.exec("rating", json!({"wire": w, "basis": "manual", "manual": "16 A", "load": "4 A"}));
    let (author, after) = last_modified(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!((after.rating.basis, after.rating.manual, after.load), (Some(Basis::Manual), Some(Amps(16.0)), Some(Amps(4.0))));

    let before = c.log().len();
    for (key, value, words) in [("size", "thick", "wire size"), ("colours", "tartan", "colour list"), ("tag", "W1", "no property")] {
        let m = c.refused("execute", json!({"command": "setprop", "params": {"conductor": w, "key": key, "value": value}}));
        assert!(m.contains(words), "{key}: {m}");
    }
    assert_eq!(c.log().len(), before, "a refusal records nothing");

    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"conductor": w, "key": "size", "value": "2.5"}}));
}

/// Brief 011's contract for lane 072 D (design 072 §4): `wire` from one connection point to
/// another through `execute` records one transaction by the agent holding the route drawn
/// (`AddRoute`) and the wire the commit's rewrite made (`CreateWire`); with writes off it is
/// refused and records nothing.
#[test]
fn wire_point_to_point_through_mcp_records_the_route_and_its_wire_as_the_agent() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d-wire", 721, Options { write: true, agent: None }).unwrap());
    let [a, b] = c.points_at(&[(0.0, 0.0), (60.0, 0.0)])[..] else { panic!("two points") };
    let before = c.log().len();
    let v = c.exec("wire", json!({"points": [[0, 0], [30, 0], [60, 0]]}));
    let log = c.log();
    assert_eq!(log.len(), before + 1, "one transaction: {v}");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    let route = t
        .ops
        .iter()
        .find_map(|o| match o {
            Op::AddRoute { id, route } => Some((*id, route.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("an AddRoute: {:?}", t.ops));
    assert_eq!(route.1.ends, [RouteEnd::Point(a), RouteEnd::Point(b)], "both ends on the connection points");
    assert_eq!(v["route"], json!(route.0.hex()));
    let (w, wire) = t
        .ops
        .iter()
        .find_map(|o| match o {
            Op::CreateWire { id, wire } => Some((*id, wire.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a CreateWire: {:?}", t.ops));
    assert_eq!(wire.ends, [a, b], "the wire runs between the two points");
    assert_eq!(v["conductor"], json!(w.hex()), "{v}");
    assert!(c.doc().wires.contains_key(&w));

    let mut r = reader();
    let before = r.log();
    let m = r.refused("execute", json!({"command": "wire", "params": {"points": [[0, 0], [60, 0]]}}));
    assert_eq!(m, ecad_mcp::policy::WRITES_OFF);
    assert_eq!(r.log(), before, "nothing recorded with writes off");
}

/// Lane 072 D (owner ruling, 2026-11-18): `cableslash` across a loose line through `execute` puts
/// the core on the line (`CorePosition::Route`, no wire yet) in one transaction by the agent; the
/// result names the line under `lines` and no conductor; with writes off it is refused.
#[test]
fn cableslash_across_a_loose_line_puts_the_core_on_the_line() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d-slash", 722, Options { write: true, agent: None }).unwrap());
    let [a] = c.points_at(&[(0.0, 0.0)])[..] else { panic!("one point") };
    let line = c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
    assert_eq!(line["conductor"], Value::Null, "a line with a free end carries no wire: {line}");
    let route = RouteId::parse(line["route"].as_str().unwrap()).unwrap();

    let before = c.log().len();
    let v = c.exec("cableslash", json!({"from": [30, -5], "to": [30, 5], "tag": "-W72"}));
    let log = c.log();
    assert_eq!(log.len(), before + 1, "one transaction: {v}");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    // The new cable carries its one core.
    let member = t
        .ops
        .iter()
        .find_map(|o| match o {
            Op::CreateCable { cable, .. } => cable.cores.first().cloned(),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a CreateCable with a core: {:?}", t.ops));
    assert_eq!(member.position, CorePosition::Route { route, landed: Some(a) }, "the core sits on the line, landed at its point");
    assert_eq!(v["lines"], json!([route.hex()]), "{v}");
    assert_eq!(v["conductors"], json!([]), "{v}");
    let cable = c.doc().cables_tagged("-W72")[0];
    assert_eq!(c.doc().cables[&cable].cores.len(), 1);

    let mut r = reader();
    let before = r.log();
    let m = r.refused("execute", json!({"command": "cableslash", "params": {"from": [30, -5], "to": [30, 5]}}));
    assert_eq!(m, ecad_mcp::policy::WRITES_OFF);
    assert_eq!(r.log(), before, "nothing recorded with writes off");
}

/// Brief 011's contract for design 072 F40: `wiresettings {number_by}` through `execute` records
/// one transaction by the agent holding the settings change and the renumbered wires; each wire
/// then shows its own number; a word it does not know is refused; with writes off it is refused.
#[test]
fn number_by_through_mcp_renumbers_in_the_settings_edit_as_the_agent() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d-number-by", 722, Options { write: true, agent: None }).unwrap());
    let _ = c.points_at(&[(0.0, 0.0), (30.0, 0.0), (60.0, 0.0)]);
    c.exec("wire", json!({"points": [[0, 0], [30, 0]]}));
    c.exec("wire", json!({"points": [[30, 0], [60, 0]]}));
    let numbers = |c: &mut Client| -> Vec<Option<String>> {
        let d = c.doc();
        let mut v: Vec<_> = d.wires.values().map(|w| (w.net, w.net_order, w.number_text().map(str::to_string))).collect();
        v.sort();
        v.into_iter().map(|x| x.2).collect()
    };
    let per_net = numbers(&mut c);
    assert_eq!(per_net.len(), 2);
    assert!(per_net[0].is_some() && per_net[0] == per_net[1], "{per_net:?}");

    let before = c.log().len();
    let r = c.exec("wiresettings", json!({"number_by": "conductor"}));
    assert_eq!(r["number_by"], json!("conductor"), "{r}");
    let log = c.log();
    assert_eq!(log.len(), before + 1, "one transaction");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    assert!(t.ops.iter().any(|o| matches!(o, Op::SetProjectSettings { .. })), "{:?}", t.ops);
    assert!(t.ops.iter().any(|o| matches!(o, Op::ModifyWire { .. })), "the numbers change in the same edit: {:?}", t.ops);
    let per_wire = numbers(&mut c);
    assert_eq!(per_wire[0], per_net[0], "the first wire in net order keeps the number");
    assert!(per_wire[1].is_some() && per_wire[1] != per_wire[0], "{per_wire:?}");

    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "wiresettings", "params": {"number_by": "wire"}}));
    assert!(m.contains("number_by") && m.contains("conductor"), "{m}");
    assert_eq!(c.log().len(), before, "a refusal records nothing");

    let mut r = reader();
    let before = r.log();
    let m = r.refused("execute", json!({"command": "wiresettings", "params": {"number_by": "conductor"}}));
    assert_eq!(m, ecad_mcp::policy::WRITES_OFF);
    assert_eq!(r.log(), before, "nothing recorded with writes off");
}

/// The `ModifyCable` of the last transaction, with its author.
fn last_cable(c: &mut Client) -> (Author, ecad_model::Cable) {
    let log = c.log();
    let last = log.last().unwrap();
    let after = last
        .ops
        .iter()
        .find_map(|o| match o {
            Op::ModifyCable { after, .. } => Some(after.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a ModifyCable: {:?}", last.ops));
    assert!(!last.ops.iter().any(|o| matches!(o, Op::SetProperty { object: ecad_model::ObjectRef::Cable(_), .. })), "{:?}", last.ops);
    (last.author.clone(), after)
}

/// Brief 011's contract for lane 072 D c3 (design 072 §4.5): a cable is written whole. `cableedit`,
/// `cablecore`, `rating` and `setprop` on a cable (its offer, a core's Not used, its shield, its
/// design current, its tag) through `execute` each record one transaction by the agent holding one
/// `ModifyCable` and no `SetProperty`; the offer stores the cores it offers; with writes off each
/// is refused.
#[test]
fn a_cables_values_set_through_mcp_record_one_modify_cable() {
    use ecad_model::{CorePosition, Purpose};
    let mut c = Client::new(EcadMcp::in_memory("lane072d-cable", 723, Options { write: true, agent: None }).unwrap());
    for x in [0, 10] {
        c.exec("wire", json!({"points": [[x, 0], [x, 50]]}));
    }
    let k = c.exec("cablenew", json!({"cores": "3G"}))["cable"].as_str().unwrap().to_string();

    let step = |c: &mut Client, cmd: &str, params: Value| -> ecad_model::Cable {
        let before = c.log().len();
        c.exec(cmd, params.clone());
        assert_eq!(c.log().len(), before + 1, "{cmd} {params}: one transaction");
        let (author, after) = last_cable(c);
        assert_eq!(author, Author::Agent(AGENT.into()), "{cmd}");
        after
    };
    let after = step(&mut c, "cableedit", json!({"cable": k, "set": {"length": 12}}));
    assert_eq!(after.properties.get("length").map(String::as_str), Some("12"));
    // The offer stores the cores it offers, on no line yet.
    let after = step(&mut c, "setprop", json!({"cable": k, "key": "cores", "value": "4G"}));
    assert_eq!(after.cores.iter().filter(|m| m.position == CorePosition::Unplaced).count(), 4, "{:?}", after.cores);
    let after = step(&mut c, "setprop", json!({"cable": k, "key": "unused.GNYE", "value": "yes"}));
    assert!(after.cores.iter().any(|m| m.label_str() == Some("GNYE") && m.purpose == Purpose::Spare), "{:?}", after.cores);
    let after = step(&mut c, "setprop", json!({"cable": k, "key": "shield", "value": "overall"}));
    assert!(after.shield.is_some() && !after.properties.contains_key("shield"));
    let after = step(&mut c, "setprop", json!({"cable": k, "key": "load", "value": "4 A"}));
    assert_eq!((after.load, after.properties.contains_key("load")), (Some(Amps(4.0)), false));
    let after = step(&mut c, "rating", json!({"cable": k, "basis": "manual", "manual": "20 A"}));
    assert_eq!((after.rating.basis, after.rating.manual), (Some(Basis::Manual), Some(Amps(20.0))));
    let after = step(&mut c, "setprop", json!({"cable": k, "key": "tag", "value": "-W50"}));
    assert_eq!(after.properties.get("tag").map(String::as_str), Some("-W50"));
    // A slash places two stored cores; `cablecore` relabels one, still one ModifyCable.
    c.exec("cableslash", json!({"from": [-5, 25], "to": [15, 25], "cable": k}));
    let after = step(&mut c, "cablecore", json!({"cable": k, "cores": ["2", "1", "3", "GNYE"]}));
    let on: Vec<_> = after.cores.iter().filter(|m| m.position != CorePosition::Unplaced).filter_map(|m| m.label_str()).collect();
    assert_eq!(on, ["2", "1"], "{:?}", after.cores);

    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"cable": k, "key": "shield", "value": "tinfoil"}}));
    assert!(m.contains("not a shield"), "{m}");
    assert_eq!(c.log().len(), before, "a refusal records nothing");

    let mut r = reader();
    for (cmd, params) in [
        ("cableedit", json!({"cable": "-W01", "set": {"length": 5}})),
        ("cablecore", json!({"cable": k, "cores": ["1"]})),
        ("setprop", json!({"cable": k, "key": "cores", "value": "5G"})),
        ("setprop", json!({"cable": k, "key": "unused.1", "value": "yes"})),
    ] {
        let before = r.log();
        let m = r.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, ecad_mcp::policy::WRITES_OFF, "{cmd}");
        assert_eq!(r.log(), before, "{cmd}: nothing recorded with writes off");
    }
}

/// Lane 072 D step d (overseer ruling, 2026-11-18): `rating {protected_by}` through `execute`
/// records one `ModifyWire` by the agent holding the device itself; deleting that device clears it
/// in the same transaction and says so, and undo brings it back; a name that is no device is
/// refused and records nothing; with writes off it is refused.
#[test]
fn protected_by_through_mcp_is_the_device_and_deleting_it_clears_it() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d-protect", 723, Options { write: true, agent: None }).unwrap());
    let v = c.wire_pp(json!({"points": [[0, 0], [50, 0]]}));
    let w = v["conductor"].as_str().unwrap().to_string();
    let q = c.exec("place", json!({"symbol": "mcb1", "at": [200, 100]}))["component"].as_str().unwrap().to_string();
    c.exec("setprop", json!({"component": q, "key": "rating", "value": "C16"}));
    let q_id = c.doc().devices.keys().copied().find(|d| d.hex() == q).unwrap();
    let tag = c.doc().devices[&q_id].tag().unwrap_or_default().to_string();

    c.exec("rating", json!({"wire": w, "protected_by": tag}));
    let (author, after) = last_modified(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(after.protected_by, Some(ecad_model::wire::ProtectedBy { device: q_id, position: None }));
    assert!(!after.display.contains_key("protected_by"), "no free-text copy: {:?}", after.display);

    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "rating", "params": {"wire": w, "protected_by": "-Q99"}}));
    assert!(m.contains("-Q99"), "{m}");
    assert_eq!(c.log().len(), before, "a refusal records nothing");

    let r = c.exec("ecaddelete", json!({"component": q}));
    assert!(r["message"].as_str().is_some_and(|m| m.contains("protected_by cleared on 1 wire")), "{r}");
    let last = c.log().last().cloned().unwrap();
    assert!(last.ops.iter().any(|o| matches!(o, Op::DeleteDevice { .. })), "{:?}", last.ops);
    let wid = c.doc().wires.keys().copied().find(|x| x.hex() == w).unwrap();
    assert_eq!(c.doc().wires[&wid].protected_by, None, "cleared in the same transaction");
    c.exec("undo", json!({}));
    assert_eq!(c.doc().wires[&wid].protected_by.map(|p| p.device), Some(q_id), "undo brings it back");

    let mut r = reader();
    r.refused("execute", json!({"command": "rating", "params": {"wire": w, "protected_by": tag}}));
}

/// Brief 011's contract for lane 072 D e2/e4 (design 072 §4.5, §4.6): `setprop wire.category`
/// records one `ModifyWire` writing `wire_type`, and `cableedit` / `setprop cable_type` one
/// `ModifyCable` writing `cable_type`, each by the agent; with writes off both are refused.
#[test]
fn wire_and_cable_types_set_through_mcp_record_typed_fields_as_the_agent() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d-types", 724, Options { write: true, agent: None }).unwrap());
    let v = c.wire_pp(json!({"points": [[0, 0], [50, 0]]}));
    let w = v["conductor"].as_str().unwrap().to_string();
    let before = c.log().len();
    c.exec("setprop", json!({"conductor": w, "key": "wire.category", "value": "dc24"}));
    assert_eq!(c.log().len(), before + 1);
    let (author, after) = last_modified(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(after.wire_type.as_ref().map(ecad_model::StyleRef::text).as_deref(), Some("dc24"));
    assert!(!after.display.contains_key("wire.category"));

    let ty = c.exec("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth MCP", "fields": {"cores": "3G", "size": "1.5"}}));
    let t = ty["id"].as_str().unwrap().to_string();
    let k = c.exec("cablenew", json!({}))["cable"].as_str().unwrap().to_string();
    for (cmd, params, want) in [
        ("cableedit", json!({"cable": k, "set": {"cable_type": "Synth MCP"}}), Some(t.clone())),
        ("setprop", json!({"cable": k, "key": "cable_type", "value": null}), None),
        ("setprop", json!({"cable": k, "key": "cable_type", "value": t}), Some(t.clone())),
    ] {
        let before = c.log().len();
        c.exec(cmd, params);
        let log = c.log();
        assert_eq!(log.len(), before + 1, "{cmd}");
        let last = log.last().unwrap();
        assert_eq!(last.author, Author::Agent(AGENT.into()));
        let after = last
            .ops
            .iter()
            .find_map(|o| match o {
                Op::ModifyCable { after, .. } => Some(after.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("a ModifyCable: {:?}", last.ops));
        assert!(!after.properties.contains_key("cable_type"), "{cmd}: never a property");
        assert_eq!(after.cable_type.as_ref().map(ecad_model::StyleRef::text), want, "{cmd}");
    }

    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"conductor": w, "key": "wire.category", "value": "signal"}}));
    r.refused("execute", json!({"command": "cableedit", "params": {"cable": k, "set": {"cable_type": null}}}));
}

/// Lane 072 D step F3: the junction is gone for agents too. `place {"symbol": "junction"}`
/// through `execute` is refused and records nothing; no listed ECAD command offers a junction (a T is
/// a line drawn onto a line, its dot drawn with the page).
#[test]
fn place_junction_through_mcp_is_refused_and_no_command_offers_one() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d-junction", 724, Options { write: true, agent: None }).unwrap());
    let before = c.log();
    let m = c.refused("execute", json!({"command": "place", "params": {"symbol": "junction", "at": [10, 10]}}));
    assert!(m.contains("No symbol \"junction\""), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");
    // ECAD's own commands (CADCraft's geometry commands may use the word for line ends).
    let ecad: Vec<&str> = ecad_engine::commands::specs().iter().map(|s| s.id).collect();
    let catalog = c.ok("list_commands", json!({}));
    for info in catalog.as_array().unwrap().iter().filter(|i| i["id"].as_str().is_some_and(|id| ecad.contains(&id))) {
        let text = info.to_string().to_lowercase();
        assert!(!text.contains("junction"), "a listed command still offers a junction: {info}");
    }
}
