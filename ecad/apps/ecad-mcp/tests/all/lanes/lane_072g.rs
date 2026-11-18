//! Lane 072 G (design 072 §8, F36; brief 011): `setstyle` on a wire type through MCP records the
//! typed record (size, colours, insulation) as the agent, and is refused with writes off.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::style::wire::Insulation;
use ecad_model::{Author, Op, StyleCatalogue};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

#[test]
fn setstyle_records_a_typed_wire_type() {
    let mut c = Client::new(EcadMcp::in_memory("lane072g", 72, Options { write: true, agent: None }).unwrap());
    let before = c.log().len();
    let params = json!({"catalogue": "wire", "id": "pe", "fields": {"size": "6 mm²", "colours": "green/yellow", "insulation": "XLPE"}});
    c.exec("setstyle", params.clone());
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    let [Op::SetStyle { catalogue: StyleCatalogue::Wire, after: Some(r), .. }] = last.ops.as_slice() else { panic!("{:?}", last.ops) };
    let w = r.body.wire().expect("a wire type body");
    assert_eq!(w.size.map(|s| s.to_string()).as_deref(), Some("6"));
    assert_eq!(w.colours.as_ref().map(ToString::to_string).as_deref(), Some("GN/YE"));
    assert_eq!(w.insulation, Some(Insulation::Xlpe));

    let mut r = reader();
    r.refused("execute", json!({"command": "setstyle", "params": params}));
}

#[test]
fn a_value_the_field_does_not_take_is_refused() {
    let mut c = Client::new(EcadMcp::in_memory("lane072g-bad", 72, Options { write: true, agent: None }).unwrap());
    let before = c.log().len();
    for fields in [json!({"insulation": "paper"}), json!({"size": "thick"}), json!({"colours": "tartan"})] {
        c.refused("execute", json!({"command": "setstyle", "params": {"catalogue": "wire", "id": "pe", "fields": fields}}));
    }
    assert_eq!(c.log().len(), before, "nothing recorded");
}

/// Design 072 F37: a cable type takes a core colour scheme; recorded typed, as the agent.
#[test]
fn setstyle_records_a_cable_type_with_a_core_scheme() {
    use ecad_model::style::cable::CoreScheme;
    let mut c = Client::new(EcadMcp::in_memory("lane072g-cable", 72, Options { write: true, agent: None }).unwrap());
    let params =
        json!({"catalogue": "cable", "new": true, "name": "Power 5G", "fields": {"cores": "5G", "size": "2.5", "core_scheme": "harmonised"}});
    c.exec("setstyle", params.clone());
    let log = c.log();
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    let [Op::SetStyle { catalogue: StyleCatalogue::Cable, after: Some(r), .. }] = last.ops.as_slice() else { panic!("{:?}", last.ops) };
    let t = r.body.cable().expect("a cable type body");
    assert_eq!(t.core_scheme, Some(CoreScheme::Harmonised));
    assert_eq!(t.cores.as_ref().map(|c| c.as_str()), Some("5G"));
    c.refused(
        "execute",
        json!({"command": "setstyle", "params": {"catalogue": "cable", "new": true, "name": "Bad", "fields": {"core_scheme": "rainbow"}}}),
    );
    let mut r = reader();
    r.refused("execute", json!({"command": "setstyle", "params": params}));
}
