//! Agent access to every generic command, as tables. Each row gives a command that
//! may edit (`Access::Edits`) parameters that change a small project. Sent to a server with writes
//! off, they are refused in the gate's words and record nothing; sent with writes on, they record
//! exactly one transaction, by the agent. The read commands record nothing with writes off.
//!
//! Every generic command that may edit has a row: a new command fails
//! [`every_generic_edit_command_has_a_row`] until it gets one. Session commands (the engine's
//! own, with their gate entries) keep their own contract tests.

use ecad_electrical::commands::run::Access;
use ecad_engine::agent::policy;
use serde_json::{Value, json};

use crate::common::{Client, writer};
use crate::mcp_contracts::reader;

const SEED: u64 = 0x0E_CAD0_0117;

/// Made-up rating table values (not from any standard).
const RATING_CSV: &str = "# made-up values for ECAD tests\ncapacity,E,pvc,3,1.5,21.0\ngrouping,E,4,0.71\nresistance,5,1.5,11.0\n";

/// Made-up PLC tags.
const PLC_TAGS: &str = "name,address,type,comment\nPump_Run,I0.0,Bool,Pump running\n";

/// The project every row starts from, and the IDs of what is in it.
struct Fx {
    page: String,
    folder: String,
    /// A coil: its device, symbol and connection points.
    coil: String,
    coil_rep: String,
    coil_points: Vec<String>,
    /// A contact of a device of its own.
    contact: String,
    /// A wire between two points, potential 24V.
    conductor: String,
    /// A line with free ends (no wire yet).
    route: String,
    /// A terminal strip of four blocks and their connection points.
    strip: String,
    strip_points: Vec<String>,
    /// Cable -W01 across three lines.
    cable: String,
    /// A PLC rack and a DI card in it.
    rack: String,
    card: String,
    /// A 24V continuation.
    continuation: String,
}

fn s(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_else(|| panic!("no {key} in {v}")).to_string()
}

fn points(v: &Value) -> Vec<String> {
    v["connection_points"].as_array().unwrap_or_else(|| panic!("no connection points in {v}")).iter().map(|p| s(p, "connection_point")).collect()
}

fn fixture() -> (Client, Fx) {
    let mut c = writer(SEED);
    let page = c.doc().page_order()[0].hex();
    let folder = s(&c.exec("newfolder", json!({"name": "Schematic"})), "folder");
    let coil = c.exec("place", json!({"symbol": "coil", "at": [30, 65]}));
    let contact = s(&c.exec("place", json!({"symbol": "no", "at": [65, 100]})), "component");
    let conductor = s(&c.wire_pp(json!({"points": [[200, 20], [260, 20]], "potential": "24V"})), "conductor");
    let route = s(&c.exec("wire", json!({"points": [[100, 40], [160, 40]]})), "route");
    for x in [100, 120, 140, 160] {
        c.wire_pp(json!({"points": [[x, 200], [x, 100]]}));
    }
    let strip = c.exec("terminalblocks", json!({"from": [90, 150], "to": [170, 150]}));
    for x in [320, 330, 340] {
        c.exec("wire", json!({"points": [[x, 100], [x, 70]]}));
    }
    let cable = s(&c.exec("cableslash", json!({"from": [315, 82], "to": [345, 88], "tag": "-W01"})), "cable");
    let rack = s(&c.exec("place", json!({"symbol": "plccpu", "at": [250, 250]})), "component");
    let card = s(&c.exec("place", json!({"symbol": "plcdi", "at": [290, 250], "rack": rack, "slot": 2})), "component");
    let continuation = s(&c.exec("place", json!({"symbol": "continuation", "at": [380, 150], "name": "24V"})), "aid");
    let fx = Fx {
        page,
        folder,
        coil: s(&coil, "component"),
        coil_rep: s(&coil, "representation"),
        coil_points: points(&coil),
        contact,
        conductor,
        route,
        strip: s(&strip, "strip"),
        strip_points: points(&strip),
        cable,
        rack,
        card,
        continuation,
    };
    (c, fx)
}

/// A symbol of the project's own (the builder's JSON form), with `extra` fields.
fn float_switch(extra: Value) -> Value {
    let mut p = json!({
        "name": "Float switch", "key": "float1",
        "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
        "points": [{"key": "1", "label": "BN", "at": [0, 5]}, {"key": "2", "label": "BU", "at": [0, -5]}],
        "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle"}],
        "defaults": {"prefix": "B", "description": "Float switch"},
    });
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    p
}

/// The float switch made and placed twice: its definition and the first copy's symbol.
fn float_switch_placed(c: &mut Client) -> (String, String) {
    let def = s(&c.exec("symbolnew", float_switch(json!({}))), "definition");
    let rep = s(&c.exec("place", json!({"symbol": "float1", "at": [100, 250]})), "representation");
    c.exec("place", json!({"symbol": "float1", "at": [130, 250]}));
    (def, rep)
}

/// The text style made for a row; returns its own ID.
fn own_text_style(c: &mut Client) -> String {
    let v = c.exec("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}}));
    ["id", "style"].iter().filter_map(|k| v[*k].as_str()).find(|x| x.starts_with('#')).unwrap_or_else(|| panic!("no own style ID in {v}")).to_string()
}

type Params = fn(&mut Client, &Fx) -> Value;

// One row per generic command that may edit: parameters that change the fixture. A row may first
// make what its command needs (with writes on, before the transaction counted). The rows are in
// four tables, one test each, so that each test stays quick.

/// Wires, nets and cables.
const WIRES_AND_CABLES: &[(&str, Params)] = &[
    ("cablecontinue", |c, f| {
        c.exec("cablereference", json!({"cable": f.cable, "at": [330, 65]}));
        c.exec("newpage", json!({"title": "Field"}));
        json!({"cable": f.cable, "at": [130, 150], "toward": [130, 130]})
    }),
    ("cableedit", |_, f| json!({"cable": f.cable, "set": {"length": "3.5", "description": "Pump feed"}})),
    ("cablenew", |_, _| json!({"tag": "-W09", "cores": "3G"})),
    ("cablereference", |_, f| json!({"cable": f.cable, "at": [330, 65]})),
    // A cable drawn on a page is deleted there; one made in the cable manager is removed.
    ("cableremove", |c, _| json!({"cable": s(&c.exec("cablenew", json!({"tag": "-W09"})), "cable")})),
    ("category", |_, f| json!({"conductor": f.conductor, "value": "signal"})),
    ("connect", |_, f| json!({"route": f.route, "connection_point": f.coil_points[0], "end": "B"})),
    ("contcaption", |_, f| json!({"continuations": [f.continuation], "caption": "TO PSU"})),
    ("disconnect", |c, f| {
        c.exec("connect", json!({"route": f.route, "connection_point": f.coil_points[0], "end": "B"}));
        json!({"route": f.route, "end": "B"})
    }),
    ("potential", |_, f| json!({"conductor": f.conductor, "value": "L1"})),
    ("rating", |_, f| json!({"wire": f.conductor, "manual": "12 A"})),
    ("ratingtable", |_, _| json!({"name": "Synthetic", "source": "made up for tests", "csv": RATING_CSV})),
    ("renumberwires", |c, _| {
        c.exec("wiresettings", json!({"scheme": "page", "digits": 3}));
        json!({"scope": "all"})
    }),
    ("wirenumber", |_, f| json!({"conductor": f.conductor, "value": "24V-50"})),
    ("wiresettings", |_, _| json!({"label_rule": "each", "label_min_length": 10})),
];

/// Devices, their parts and symbols, and numbering.
const DEVICES: &[(&str, Params)] = &[
    ("article", |c, f| {
        c.exec("partline", json!({"component": f.coil, "add": {"part_number": "AB-1NO"}}));
        json!({"part_number": "AB-1NO", "article_number": "A-0110"})
    }),
    ("bomitem", |_, _| json!({"description": "DIN rail 35x7.5", "qty": "1.5", "unit": "m"})),
    ("devicedelete", |_, f| json!({"component": f.contact})),
    ("ecaddelete", |_, f| json!({"component": f.contact})),
    ("finalise", |c, _| {
        c.exec("numbering", json!({"provisional": true}));
        c.exec("place", json!({"symbol": "coil", "at": [60, 250]}));
        json!({})
    }),
    ("linkdevices", |c, f| {
        let other = s(&c.exec("place", json!({"symbol": "coil", "at": [60, 250]})), "component");
        json!({"kind": "mechanical_interlock", "devices": [f.coil, other]})
    }),
    ("mirrormove", |c, f| {
        c.exec("place", json!({"symbol": "no", "at": [200, 250], "device": f.coil}));
        json!({"component": f.coil, "at": [75, 45]})
    }),
    ("movetext", |_, f| json!({"representation": f.coil_rep, "kind": "tag", "to": [10, 20]})),
    ("numbering", |_, _| json!({"provisional": true})),
    ("partline", |_, f| json!({"component": f.coil, "add": {"part_number": "AB-1NO", "maker": "Generic"}})),
    ("renumber", |_, _| json!({"kind": "terminal_blocks", "start": 3})),
    ("setfunction", |_, f| json!({"component": f.coil, "value": "=F1"})),
    ("setlocation", |_, f| json!({"component": f.coil, "value": "+CP1"})),
    ("setprop", |_, f| json!({"component": f.coil, "key": "description", "value": "Pump contactor"})),
    ("symboledit", |c, _| {
        let (def, rep) = float_switch_placed(c);
        let moved = json!([{"key": "1", "label": "BN", "at": [0, 10]}, {"key": "2", "label": "BU", "at": [0, -5]}]);
        float_switch(json!({"definition": def, "points": moved, "update": "selected", "copies": [rep]}))
    }),
    ("symbolnew", |_, _| float_switch(json!({}))),
    ("symbolupdate", |c, _| {
        let (def, rep) = float_switch_placed(c);
        let moved = json!([{"key": "1", "label": "BN", "at": [0, 10]}, {"key": "2", "label": "BU", "at": [0, -5]}]);
        let v2 = c.exec("symboledit", float_switch(json!({"definition": def, "points": moved, "update": "selected", "copies": [rep]})));
        json!({"lineage": v2["lineage"]})
    }),
    ("tagsettings", |_, _| json!({"padding": 3})),
    ("unlinkdevices", |c, f| {
        let other = s(&c.exec("place", json!({"symbol": "coil", "at": [60, 250]})), "component");
        let link = c.exec("linkdevices", json!({"kind": "mechanical_interlock", "devices": [f.coil, other]}));
        json!({"link": s(&link, "link")})
    }),
    ("userfield", |_, _| json!({"name": "Row", "kind": "number", "unit": "mm"})),
];

/// PLC cards and terminal strips.
const PLC_AND_STRIPS: &[(&str, Params)] = &[
    ("accessory", |_, f| json!({"strip": f.strip, "at": "start", "items": ["end_stop"]})),
    ("jumper", |_, f| json!({"connection_points": [f.strip_points[0], f.strip_points[2]]})),
    ("plcaddress", |_, f| json!({"rack": f.rack, "style": "iec"})),
    ("plccardtype", |_, f| json!({"card": f.card, "type": "DQ"})),
    ("plcchannel", |_, f| json!({"card": f.card, "channel": 1, "text": "Door closed"})),
    ("plcimport", |_, _| json!({"text": PLC_TAGS})),
    ("plcmaketags", |c, f| {
        c.exec("plcchannel", json!({"card": f.card, "channel": 1, "text": "Door closed"}));
        json!({"card": f.card})
    }),
    ("plcslot", |_, f| json!({"card": f.card, "slot": 5})),
    ("terminalblocklabel", |_, f| json!({"connection_point": f.strip_points[0], "text": "1L"})),
    ("terminalblocks", |_, _| json!({"from": [90, 120], "to": [170, 120]})),
    ("toposition", |c, f| {
        let device = s(&c.exec("place", json!({"symbol": "fuseterminalblock", "at": [250, 180]})), "component");
        json!({"device": device, "strip": f.strip})
    }),
];

/// Folders, pages, designations, the project and its styles.
const PAGES_AND_STYLES: &[(&str, Params)] = &[
    ("conflicts", |c, f| {
        c.exec("setfunction", json!({"component": f.contact, "value": "=F2"}));
        c.exec("functionbox", json!({"function": "=F3", "corners": [[50, 85], [80, 115]]}));
        json!({"answer": "move"})
    }),
    ("deletefolder", |_, f| json!({"folder": f.folder})),
    ("deletestyle", |c, _| json!({"catalogue": "text", "id": own_text_style(c), "replace_with": "tag"})),
    ("designationadd", |_, _| json!({"aspect": "location", "code": "+CP2", "description": "Field"})),
    ("designationedit", |c, _| {
        c.exec("designationadd", json!({"aspect": "location", "code": "+CP1"}));
        json!({"aspect": "location", "code": "+CP1", "to": "+CP10"})
    }),
    ("designationmerge", |c, _| {
        c.exec("designationadd", json!({"aspect": "location", "code": "+CP1"}));
        c.exec("designationadd", json!({"aspect": "location", "code": "+CP2"}));
        json!({"aspect": "location", "from": "+CP1", "into": "+CP2"})
    }),
    ("designationremove", |c, _| {
        c.exec("designationadd", json!({"aspect": "function", "code": "=SPARE"}));
        json!({"aspect": "function", "code": "=SPARE"})
    }),
    ("folderprops", |_, f| json!({"folder": f.folder, "start": "10"})),
    ("functionbox", |_, _| json!({"function": "=F1", "corners": [[10, 40], [100, 120]]})),
    ("locationbox", |_, _| json!({"location": "+CP1", "corners": [[10, 10], [80, 60]]})),
    ("movefolder", |c, f| {
        let parent = s(&c.exec("newfolder", json!({"name": "Power"})), "folder");
        json!({"folder": f.folder, "parent": parent})
    }),
    ("movepage", |_, f| json!({"page": f.page, "folder": f.folder})),
    ("newfolder", |_, _| json!({"name": "Power"})),
    ("pageprops", |_, f| json!({"page": f.page, "sheet": "A2"})),
    ("projectprops", |_, _| json!({"location": "+CP1"})),
    ("promote", |c, _| {
        c.exec("standard", json!({"new": "Works standard"}));
        c.exec("setdefault", json!({"key": "symbol.coil.prefix", "level": "project", "value": "Q"}));
        json!({"key": "symbol.coil.prefix"})
    }),
    ("renamefolder", |_, f| json!({"folder": f.folder, "name": "Power"})),
    ("renamepage", |_, f| json!({"page": f.page, "title": "Supply"})),
    ("renumberpages", |c, f| {
        c.exec("pageprops", json!({"page": f.page, "number": "100"}));
        json!({})
    }),
    ("revision", |_, _| json!({"description": "Second issue", "drawn": "CVA"})),
    ("setdefault", |_, _| json!({"key": "symbol.coil.prefix", "level": "project", "value": "Q"})),
    ("setstyle", |_, _| json!({"catalogue": "text", "id": "tag", "fields": {"height": 3}})),
    ("standard", |_, _| json!({"new": "Works standard"})),
    ("textstyle", |_, _| json!({"kind": "tag", "height": 3})),
    ("titleblock", |c, _| {
        let own = c.exec("titleblock", json!({"new": {"from": "sheet", "sheet": "A3"}}));
        json!({"id": own["id"], "name": "Works frame"})
    }),
];

/// Every row.
fn rows() -> impl Iterator<Item = &'static (&'static str, Params)> {
    [WIRES_AND_CABLES, DEVICES, PLC_AND_STRIPS, PAGES_AND_STYLES].into_iter().flatten()
}

/// The generic commands with access `access`.
fn generic(access: Access) -> Vec<&'static str> {
    ecad_engine::commands::typed_commands().into_iter().filter(|t| t.access == Some(access)).map(|t| t.id).collect()
}

#[test]
fn every_generic_edit_command_has_a_row() {
    let rows: Vec<&str> = rows().map(|(id, _)| *id).collect();
    let missing: Vec<&str> = generic(Access::Edits).into_iter().filter(|id| !rows.contains(id)).collect();
    assert!(missing.is_empty(), "generic commands without a row: {missing:?}");
    let stale: Vec<&str> = rows.iter().copied().filter(|id| !generic(Access::Edits).contains(id)).collect();
    assert!(stale.is_empty(), "rows for commands that are not generic edits: {stale:?}");
}

/// Each row's parameters are refused in the gate's words with writes off and record nothing; with
/// writes on, on a fresh fixture, they record exactly one transaction, by the agent.
fn check(rows: &[(&str, Params)]) {
    let mut r = reader();
    let read_log = r.log();
    let mut failed = Vec::new();
    for (id, params) in rows {
        let (mut c, f) = fixture();
        let Ok(p) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| params(&mut c, &f))) else {
            failed.push(format!("{id}: the row's own setup failed"));
            continue;
        };
        // The gate decides from the command and its parameters alone, so the reader's own project
        // (without the fixture's objects) shows the refusal.
        let m = r.call("execute", json!({"command": id, "params": p}));
        let text = m["content"][0]["text"].as_str().unwrap_or_default();
        if m["isError"] != json!(true) || text != policy::WRITES_OFF {
            failed.push(format!("{id} {p}: with writes off: {m}"));
        }
        let before = c.log().len();
        let v = c.call("execute", json!({"command": id, "params": p}));
        if v["isError"] == json!(true) {
            failed.push(format!("{id} {p}: refused with writes on: {v}"));
            continue;
        }
        let log = c.log();
        let ours: Vec<_> = log.get(before..).unwrap_or_default().iter().map(|t| (t.label.clone(), t.author.clone())).collect();
        if ours.len() != 1 || ours[0].1 != ecad_model::Author::Agent(crate::common::AGENT.into()) {
            failed.push(format!("{id} {p}: one transaction by the agent, got {ours:?}"));
        }
    }
    assert_eq!(r.log(), read_log, "nothing was recorded with writes off");
    assert!(failed.is_empty(), "{} of {} rows failed:\n{}", failed.len(), rows.len(), failed.join("\n"));
}

#[test]
fn wire_and_cable_commands_are_refused_with_writes_off_and_one_transaction_by_the_agent() {
    check(WIRES_AND_CABLES);
}

#[test]
fn device_commands_are_refused_with_writes_off_and_one_transaction_by_the_agent() {
    check(DEVICES);
}

#[test]
fn plc_and_strip_commands_are_refused_with_writes_off_and_one_transaction_by_the_agent() {
    check(PLC_AND_STRIPS);
}

#[test]
fn page_and_style_commands_are_refused_with_writes_off_and_one_transaction_by_the_agent() {
    check(PAGES_AND_STYLES);
}

#[test]
fn the_generic_read_commands_record_nothing_with_writes_off() {
    let (mut c, f) = fixture();
    let before = c.log();
    let mut r = reader();
    let read_log = r.log();
    for id in generic(Access::Reads) {
        assert!(policy::is_read_command(id), "{id} declares Reads, so the gate lets it through");
        for p in [json!({}), json!({"component": f.coil, "page": f.page, "cable": f.cable, "card": f.card})] {
            r.call("execute", json!({"command": id, "params": p}));
            c.call("execute", json!({"command": id, "params": p}));
        }
    }
    assert_eq!(r.log(), read_log, "the reader recorded nothing");
    assert_eq!(c.log(), before, "read commands record nothing with writes on either");
}
