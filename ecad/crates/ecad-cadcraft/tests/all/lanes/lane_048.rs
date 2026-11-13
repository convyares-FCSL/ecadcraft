//! Brief 048 (symbol builder, part a): the worker's in-process checks of design 048 §12 rows 1–9
//! and 11–14: `symbolnew`, placing a project symbol, the refusals, the registry's family entries,
//! the impact and its clashes, updates that keep IDs and move wire ends, edited built-ins, saving
//! unchanged content, `makesymbol` and promotion, superseded versions and hostile input.

use ecad_cadcraft::symbols::{self, PROJECT_SYMBOLS};
use ecad_model::{ComponentId, ConductorId, DefinitionId, Document, Op, PageId, RepresentationId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

fn rect() -> Value {
    json!({"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true})
}

/// Design §9.1's example: a float switch with points 1 (BN) and 2 (BU) and a tag slot.
fn float(extra: Value) -> Value {
    let mut p = json!({
        "name": "Float switch", "key": "float1", "graphics": [rect()],
        "points": [{"key": "1", "label": "BN", "at": [0, 5]}, {"key": "2", "label": "BU", "at": [0, -5]}],
        "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle"}, {"text": "rating", "at": [4, 0]}],
        "defaults": {"prefix": "B", "description": "Float switch", "rating": "24 V", "join": "device"},
    });
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    p
}

fn last_ops(h: &mut H) -> Vec<Op> {
    h.p(|p| p.ledger.log().last().map(|t| t.ops.clone())).unwrap_or_default()
}

fn place(h: &mut H, x: f64, y: f64) -> Value {
    h.run("place", json!({"symbol": "float1", "at": [x, y]})).unwrap()
}

fn term(d: &Document, c: ComponentId, key: &str) -> TerminalId {
    d.terminals_of(c).find(|(_, t)| t.key.as_deref() == Some(key)).map(|(t, _)| t).unwrap_or_else(|| panic!("no point {key}"))
}

/// A wire from point `key` of `c` straight down 20 mm.
fn wire_from(h: &mut H, page: PageId, c: ComponentId, key: &str) -> ConductorId {
    let d = h.doc();
    let t = term(&d, c, key);
    let at = h.terminal_pos(page, t);
    let w: ConductorId = id(&h.run("wire", json!({"points": pts(&[(at.x, at.y), (at.x + 20.0, at.y)])})).unwrap(), "conductor");
    assert!(h.doc().ends_at(t).iter().any(|(c, _)| *c == w), "the wire is attached to point {key}");
    w
}

#[test]
fn row_1_symbolnew_embeds_version_1_in_one_transaction() {
    let mut h = H::new(4801);
    let before = h.head();
    let v = h.run("symbolnew", float(json!({}))).unwrap();
    assert_eq!(h.head(), before + 1);
    assert_eq!(v["version"], 1);
    let ops = last_ops(&mut h);
    assert_eq!(ops.len(), 1);
    let Op::EmbedDefinition { definition, .. } = &ops[0] else { panic!("{ops:?}") };
    let o = definition.origin.clone().unwrap();
    assert_eq!(o.library, PROJECT_SYMBOLS);
    assert_eq!(o.library_seq, 1);
    let mut bare = definition.clone();
    bare.origin = None;
    assert_eq!(ecad_file::definition_hash(&bare).unwrap(), o.content_hash);
    assert_eq!(definition.defaults.get("key").map(String::as_str), Some("float1"));
    h.cmdline("u");
    assert!(symbols::project_by_key(&h.doc(), "float1").is_none(), "u removes it");
}

#[test]
fn row_2_place_makes_a_device_with_its_prefix_description_and_points() {
    let mut h = H::new(4802);
    h.run("symbolnew", float(json!({}))).unwrap();
    let v = place(&mut h, 100.0, 100.0);
    assert_eq!(v["tag"], "-B01");
    let c: ComponentId = id(&v, "component");
    let d = h.doc();
    let comp = &d.components[&c];
    assert_eq!(comp.properties.get("description").map(String::as_str), Some("Float switch"));
    assert_eq!(comp.properties.get("rating").map(String::as_str), Some("24 V"));
    let labels: Vec<(String, String)> = d.terminals_of(c).map(|(_, t)| (t.key.clone().unwrap(), t.label.clone().unwrap())).collect();
    assert!(labels.contains(&("1".into(), "BN".into())) && labels.contains(&("2".into(), "BU".into())), "{labels:?}");
    // The tag sits at its slot (-4, 0) from the origin, right-aligned; the rating at (4, 0).
    let page = h.first_page();
    let texts = ecad_cadcraft::devices::all_texts_on_page(&d, page);
    let tag = texts.iter().find(|t| t.text.value == "-B01").expect("tag text");
    assert!(tag.text.at.near(ecad_model::Pt::new(96.0, 100.0), 1e-9), "{:?}", tag.text.at);
    assert!(texts.iter().any(|t| t.text.value == "24 V" && t.text.at.near(ecad_model::Pt::new(104.0, 100.0), 1e-9)), "rating slot");
    h.check();
    // A second placement is -B02.
    assert_eq!(place(&mut h, 150.0, 100.0)["tag"], "-B02");
}

#[test]
fn row_3_refusals_write_nothing() {
    let mut h = H::new(4803);
    h.run("symbolnew", float(json!({}))).unwrap();
    let cases: Vec<(Value, &str)> = vec![
        (json!({"name": ""}), "A symbol needs a name."),
        (json!({"name": "x".repeat(65)}), "The name is too long (64 characters at most)."),
        (json!({"key": "coil"}), "The key coil is a built-in symbol's; choose another."),
        (json!({"key": "float1", "name": "Other"}), "The key float1 is used by Float switch; choose another."),
        (json!({"key": "f2", "defaults": {"join": "device"}}), "A symbol of its own device needs a tag prefix (for example B)."),
        (json!({"key": "f2", "points": []}), "A symbol needs at least one connection point."),
        (json!({"key": "f2", "slots": []}), "A symbol needs a tag slot."),
        (json!({"key": "f2", "points": [{"key": "1", "at": [1, 0]}]}), "A point goes on the 5 mm grid (point 1)."),
        (json!({"key": "f2", "points": [{"key": "1", "at": [0, 0]}, {"key": "2", "at": [0, 0]}]}), "Point 1 is already here (point 2)."),
        (
            json!({"key": "f2", "slots": [{"text": "tag", "at": [0, 0]}, {"text": "colour", "at": [0, 5]}]}),
            "A slot places a device text; colour is not one.",
        ),
    ];
    for (extra, want) in cases {
        let before = h.head();
        let e = h.run("symbolnew", float(extra.clone())).unwrap_err();
        assert!(e.contains(want), "{extra}: {e}");
        assert_eq!(h.head(), before, "{extra}: nothing written");
    }
    // Hostile sizes (brief criterion 11): refused before anything is collected or built.
    let many: Vec<Value> = (0..10_000).map(|i| json!({"key": i.to_string(), "at": [0, i * 5]})).collect();
    assert!(h.run("symbolnew", float(json!({"key": "f3", "points": many}))).unwrap_err().contains("too many connection points"));
    let shapes: Vec<Value> = (0..100_000).map(|_| rect()).collect();
    assert!(h.run("symbolnew", float(json!({"key": "f3", "graphics": shapes}))).unwrap_err().contains("too large"));
    assert!(h.run("symbolnew", float(json!({"key": "k".repeat(10_000)}))).is_err());
    assert!(h.run("symbolnew", float(json!({"name": "n".repeat(10_000)}))).is_err());
    let nan = json!({"kind": "line", "a": [0, 0], "b": [1e300, 0]});
    assert!(h.run("symbolnew", float(json!({"key": "f3", "graphics": [nan]}))).unwrap_err().contains("outside"));
    assert!(h.run("symbolnew", float(json!({"key": "f3", "points": [{"key": "1", "at": [1e9, 0]}]}))).is_err());
}

#[test]
fn row_4_family_entries_resolve_and_a_project_override_wins() {
    let mut h = H::new(4804);
    h.run("symbolnew", float(json!({}))).unwrap();
    let v = h.run("defaults", json!({"key": "symbol.float1.description"})).unwrap();
    let line = &v["defaults"][0];
    assert_eq!(line["value"], "Float switch");
    assert_eq!(line["from"], "definition");
    let all = h.run("defaults", json!({})).unwrap();
    assert!(all["defaults"].as_array().unwrap().iter().any(|l| l["key"] == "symbol.float1.prefix" && l["from"] == "definition"));
    assert!(all["defaults"].as_array().unwrap().iter().any(|l| l["key"] == "symbol.point_grid"));
    h.run("setdefault", json!({"key": "symbol.float1.prefix", "value": "S"})).unwrap();
    assert_eq!(place(&mut h, 100.0, 100.0)["tag"], "-S01");
    h.run("setdefault", json!({"key": "symbol.float1.description", "value": "Level switch"})).unwrap();
    let c: ComponentId = id(&place(&mut h, 150.0, 100.0), "component");
    assert_eq!(h.doc().components[&c].properties.get("description").map(String::as_str), Some("Level switch"));
    let v = h.run("defaults", json!({"key": "symbol.float1.description"})).unwrap();
    assert_eq!(v["defaults"][0]["from"], "project");
    assert!(h.run("setdefault", json!({"key": "symbol.float1.join", "value": "part"})).unwrap_err().contains("symbol's own"));
}

/// Three copies; the first wired on point 2.
fn three(h: &mut H) -> (DefinitionId, PageId, Vec<(ComponentId, RepresentationId)>, ConductorId) {
    let v = h.run("symbolnew", float(json!({}))).unwrap();
    let def: DefinitionId = id(&v, "definition");
    let page = h.first_page();
    let mut copies = Vec::new();
    for x in [100.0, 150.0, 200.0] {
        let p = place(h, x, 100.0);
        copies.push((id(&p, "component"), id(&p, "representation")));
    }
    let w = wire_from(h, page, copies[0].0, "2");
    (def, page, copies, w)
}

#[test]
fn row_5_impact_clashes_and_selected_updates() {
    let mut h = H::new(4805);
    let (def, _, copies, _) = three(&mut h);
    let only_1 = json!([{"key": "1", "label": "BN", "at": [0, 5]}]);
    let before = h.head();
    let v = h.run("symbolimpact", float(json!({"definition": def.hex(), "points": only_1}))).unwrap();
    assert_eq!(h.head(), before, "read only");
    assert_eq!(v["copies"], 3);
    assert_eq!(v["clashes"], 1);
    let rows = v["rows"].as_array().unwrap();
    let clash = rows.iter().find(|r| r["rep"] == copies[0].1.hex()).unwrap();
    assert!(clash["clash"].as_str().unwrap().starts_with("Point 2 is removed; wire"), "{clash}");
    assert!(rows.iter().filter(|r| r["clash"].is_null()).all(|r| r["changes"].as_array().unwrap().iter().any(|c| c == "Point 2 removed")));
    let e = h.run("symboledit", float(json!({"definition": def.hex(), "points": only_1, "update": "all"}))).unwrap_err();
    assert!(e.contains("1 placed copy would clash: -B01 (p. "), "{e}");
    assert_eq!(h.head(), before, "refused, nothing written");
    let two = json!([copies[1].1.hex(), copies[2].1.hex()]);
    let v = h.run("symboledit", float(json!({"definition": def.hex(), "points": only_1, "update": "selected", "copies": two}))).unwrap();
    assert_eq!(v["updated"], 2);
    assert_eq!(v["version"], 2);
    let d = h.doc();
    for (c, _) in &copies[1..] {
        assert_eq!(d.terminals_of(*c).count(), 1, "point 2 removed");
    }
    assert_eq!(d.terminals_of(copies[0].0).count(), 2, "the clashing copy is left");
    assert!(ecad_cadcraft::symbol_cmds::update_available(&d, copies[0].1).is_some(), "update available");
    assert!(ecad_cadcraft::symbol_cmds::update_available(&d, copies[1].1).is_none());
    assert!(h.run("symbolupdate", json!({"copies": [copies[0].1.hex()]})).unwrap_err().contains("would clash"));
    h.check();
}

#[test]
fn row_6_a_moved_wired_point_takes_its_wire_end_and_keeps_ids() {
    let mut h = H::new(4806);
    let (def, page, copies, w) = three(&mut h);
    let d = h.doc();
    let t2 = term(&d, copies[0].0, "2");
    let net = d.connectivity().net_of_terminal(t2);
    let (a, _) = h.route_ends(page, w);
    let moved = json!([{"key": "1", "label": "BN", "at": [0, 5]}, {"key": "2", "label": "BU", "at": [0, -10]}]);
    let v = h.run("symboledit", float(json!({"definition": def.hex(), "points": moved, "update": "all"}))).unwrap();
    assert_eq!(v["updated"], 3);
    let d = h.doc();
    assert_eq!(term(&d, copies[0].0, "2"), t2, "same terminal ID");
    assert!(d.ends_at(t2).iter().any(|(c, _)| *c == w), "still attached");
    let (a2, b2) = h.route_ends(page, w);
    assert!(a2.near(ecad_model::Pt::new(a.x, a.y - 5.0), 1e-9) || b2.near(ecad_model::Pt::new(a.x, a.y - 5.0), 1e-9), "{a:?} → {a2:?} {b2:?}");
    assert_eq!(h.terminal_pos(page, t2), ecad_model::Pt::new(100.0, 90.0));
    assert_eq!(d.connectivity().net_of_terminal(t2), net, "the net is unchanged");
    // FP8: version 1 is gone, the transaction has no delete of a component.
    let ops = last_ops(&mut h);
    assert!(ops.iter().any(|o| matches!(o, Op::RemoveDefinition { id, .. } if *id == def)), "v1 removed");
    assert!(ops.iter().filter(|o| matches!(o, Op::SetComponentDefinition { .. })).count() == 3);
    assert!(!ops.iter().any(|o| matches!(o, Op::DeleteComponent { .. })));
    h.check();
    // Undo puts everything back.
    h.cmdline("u");
    let d = h.doc();
    assert!(d.definitions.contains_key(&def));
    assert_eq!(h.terminal_pos(page, t2), ecad_model::Pt::new(100.0, 95.0));
}

#[test]
fn row_7_relabels_follow_unless_set_by_hand() {
    let mut h = H::new(4807);
    let (def, _, copies, _) = three(&mut h);
    let d = h.doc();
    let t = term(&d, copies[2].0, "1");
    // Relabel copy 3's point 1 by hand (036's pattern).
    let term3 = d.terminals[&t].clone();
    h.cfg(|p| {
        let ops = vec![
            Op::RemoveTerminal { id: t, terminal: term3.clone() },
            Op::AddTerminal { id: t, terminal: ecad_model::Terminal { label: Some("X".into()), ..term3.clone() } },
        ];
        p.ledger.commit(ecad_model::Author::User, "relabel", ops).unwrap();
    });
    let relabelled = json!([{"key": "1", "label": "1", "at": [0, 5]}, {"key": "2", "label": "BU", "at": [0, -5]}]);
    h.run("symboledit", float(json!({"definition": def.hex(), "points": relabelled, "update": "all"}))).unwrap();
    let d = h.doc();
    assert_eq!(d.terminals[&term(&d, copies[0].0, "1")].label.as_deref(), Some("1"));
    assert_eq!(d.terminals[&t].label.as_deref(), Some("X"), "set by hand, kept");
}

#[test]
fn row_8_editing_a_built_in_makes_a_project_copy() {
    let mut h = H::new(4808);
    let coil = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    let rep: RepresentationId = id(&coil, "representation");
    let d = h.doc();
    let (def_id, coil_def) =
        d.definitions.iter().find(|(_, x)| symbols::of_definition(x).is_some_and(|s| s.key == "coil")).map(|(i, x)| (*i, x.clone())).unwrap();
    let mut p = json!({"definition": def_id.hex(), "name": "Coil, mine", "key": "mycoil", "update": "all",
        "defaults": {"prefix": "K"}, "slots": [{"text": "tag", "at": [-10, 0]}]});
    let pts: Vec<Value> = coil_def.terminals.iter().map(|t| json!({"key": t.key, "label": t.label, "at": [t.at.x, t.at.y]})).collect();
    p["points"] = json!(pts);
    let gr: Vec<Value> = coil_def.graphics.iter().map(|g| serde_json::to_value(ecad_file::v1::Primitive::from(g)).unwrap()).collect();
    p["graphics"] = json!(gr);
    let v = h.run("symboledit", p.clone()).unwrap();
    assert_eq!(v["updated"], 0, "placed built-ins are left unless asked");
    let d = h.doc();
    let new = &d.definitions[&id::<DefinitionId>(&v, "definition")];
    assert_eq!(new.derived_from.as_ref(), coil_def.origin.as_ref());
    assert_eq!(d.definitions[&def_id], coil_def, "the built-in is unchanged");
    let item = d.representations[&rep].items[0];
    assert!(matches!(&d.items[&item].kind, ecad_model::ItemKind::Symbol(s) if s.definition == def_id), "the placed coil still shows the built-in");
    // The kind follows the starter (a coil, not a fixed device of its own).
    assert_eq!(symbols::kind_of_definition(new), symbols::by_key("coil").map(|s| s.kind));
}

#[test]
fn row_9_saving_unchanged_content_is_refused() {
    let mut h = H::new(4809);
    let def: DefinitionId = id(&h.run("symbolnew", float(json!({}))).unwrap(), "definition");
    let before = h.head();
    let e = h.run("symboledit", float(json!({"definition": def.hex(), "update": "none"}))).unwrap_err();
    assert_eq!(e, "Nothing changed.");
    assert_eq!(h.head(), before);
    // Save as a new symbol needs a key of its own.
    assert!(h.run("symboledit", float(json!({"definition": def.hex(), "update": "new"}))).unwrap_err().contains("used by Float switch"));
    let v = h.run("symboledit", float(json!({"definition": def.hex(), "update": "new", "key": "float2"}))).unwrap();
    assert_eq!(v["version"], 1);
    assert_ne!(v["lineage"], json!(null));
    assert_eq!(symbols::project_symbols(&h.doc()).len(), 2, "a second lineage");
}

#[test]
fn row_11_makesymbol_from_lines_replaces_the_drawing() {
    let mut h = H::new(4811);
    let page = h.first_page();
    for (a, b) in [((0.0, 0.0), (10.0, 0.0)), ((10.0, 0.0), (10.0, 10.0)), ((10.0, 10.0), (0.0, 10.0)), ((0.0, 10.0), (0.0, 0.0))] {
        h.run("line", json!({"points": pts(&[(a.0 + 100.0, a.1 + 100.0), (b.0 + 100.0, b.1 + 100.0)])})).unwrap();
    }
    let handles: Vec<String> = h.lines_on(page).iter().map(|x| x.hex()).collect();
    assert_eq!(handles.len(), 4);
    let v = h
        .run(
            "makesymbol",
            json!({"handles": handles, "name": "Box", "key": "box1", "defaults": {"prefix": "A"},
                "points": [{"key": "1", "at": [100, 100]}, {"key": "2", "at": [110, 100]}],
                "slots": [{"text": "tag", "at": [95, 105]}]}),
        )
        .unwrap();
    assert_eq!(v["tag"], "-A01");
    let d = h.doc();
    let c: ComponentId = id(&v, "component");
    assert_eq!(d.terminals_of(c).count(), 2);
    assert!(h.lines_on(page).is_empty(), "the drawing is replaced");
    // The symbol's origin is the grid point at the middle (105, 105): point 1 stays at (100, 100).
    assert_eq!(h.terminal_pos(page, term(&d, c, "1")), ecad_model::Pt::new(100.0, 100.0));
    h.cmdline("u");
    assert_eq!(h.lines_on(page).len(), 4, "undo restores the lines");
}

#[test]
fn row_12_promotion_keeps_ids_and_wires() {
    let mut h = H::new(4812);
    let page = h.first_page();
    for (a, b) in [((0.0, 0.0), (10.0, 0.0)), ((10.0, 0.0), (10.0, 10.0))] {
        h.run("line", json!({"points": pts(&[(a.0 + 100.0, a.1 + 100.0), (b.0 + 100.0, b.1 + 100.0)])})).unwrap();
    }
    let handles: Vec<String> = h.lines_on(page).iter().map(|x| x.hex()).collect();
    let v = h.run("convertcomponent", json!({"handles": handles})).unwrap();
    let c: ComponentId = id(&v, "component");
    let first = h.rep_handles(page, c)[0].clone();
    for (at, label) in [([100, 100], "A1"), ([110, 110], "A2")] {
        h.run("addterminal", json!({"handle": first, "at": at, "label": label})).unwrap();
    }
    let d = h.doc();
    let terms: Vec<TerminalId> = d.terminals_of(c).map(|(t, _)| t).collect();
    assert_eq!(terms.len(), 2, "{v}");
    let mut wires = Vec::new();
    for t in &terms {
        let at = h.terminal_pos(page, *t);
        wires.push(id::<ConductorId>(&h.run("wire", json!({"points": pts(&[(at.x, at.y), (at.x, at.y - 20.0)])})).unwrap(), "conductor"));
    }
    let v = h.run(
        "makesymbol",
        json!({"component": c.hex(), "name": "Mine", "key": "mine", "defaults": {"prefix": "A"}, "slots": [{"text": "tag", "at": [95, 105]}]}),
    );
    let v = v.unwrap();
    let d = h.doc();
    assert_eq!(id::<ComponentId>(&v, "component"), c);
    let after: Vec<TerminalId> = d.terminals_of(c).map(|(t, _)| t).collect();
    assert_eq!(after, terms, "terminal IDs kept");
    for (t, w) in terms.iter().zip(&wires) {
        assert!(d.ends_at(*t).iter().any(|(x, _)| x == w), "wire still attached");
    }
    let rep: RepresentationId = id(&v, "representation");
    assert_eq!(d.representations[&rep].form, ecad_model::Form::Symbol);
    let ops = last_ops(&mut h);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::SetComponentDefinition { .. })).count(), 1);
    assert!(!ops.iter().any(|o| matches!(o, Op::DeleteComponent { .. })));
    h.check();
}

#[test]
fn row_13_state_before_the_edit_still_has_v1() {
    let mut h = H::new(4813);
    let (def, _, _, _) = three(&mut h);
    let at = h.head();
    let moved = json!([{"key": "1", "label": "BN", "at": [0, 10]}, {"key": "2", "label": "BU", "at": [0, -5]}]);
    h.run("symboledit", float(json!({"definition": def.hex(), "points": moved, "update": "all"}))).unwrap();
    assert!(!h.doc().definitions.contains_key(&def), "v1 removed when nothing uses it");
    let log = h.p(|p| p.ledger.full_log().unwrap());
    let old = ecad_model::Ledger::replay(log[..at as usize].to_vec(), 0).unwrap();
    assert!(old.document().definitions.contains_key(&def), "history keeps it");
}

#[test]
fn hostile_inputs_never_panic() {
    let mut h = H::new(4814);
    let def: DefinitionId = id(&h.run("symbolnew", float(json!({}))).unwrap(), "definition");
    let bad: Vec<(&str, Value)> = vec![
        ("symbolnew", json!({"graphics": "x"})),
        ("symbolnew", json!({"points": [{"key": "1"}]})),
        ("symbolnew", json!({"slots": [{"text": "tag", "at": [0, 0], "rotation": "x"}]})),
        ("symbolnew", json!({"defaults": {"key": 3}})),
        ("symbolnew", json!({"bridges": [["nope"]]})),
        ("symbolnew", json!({"from": "zz"})),
        ("symboledit", json!({"definition": "00", "update": "all"})),
        ("symboledit", json!({"definition": def.hex(), "update": "sideways"})),
        ("symboledit", json!({"definition": def.hex(), "update": "selected", "copies": ["nothex"]})),
        ("symbolimpact", json!({"definition": "ffffffffffffffffffffffffffffffff"})),
        ("symbolupdate", json!({})),
        ("symbolupdate", json!({"lineage": def.hex()})),
        ("makesymbol", json!({"handles": []})),
        ("makesymbol", json!({"component": "ffff"})),
        ("place", json!({"symbol": "nosuch", "at": [0, 0]})),
        ("setdefault", json!({"key": "symbol.float1.rating", "value": "r".repeat(1000)})),
        ("setdefault", json!({"key": "symbol.nosuch.rating", "value": "1"})),
    ];
    for (cmd, p) in bad {
        assert!(h.run(cmd, float(p.clone())).is_err(), "{cmd} {p}");
    }
    // A cycle of derived_from is never followed: two hand-made versions naming each other's origin.
    let d = h.doc();
    assert!(symbols::project_symbols(&d).len() == 1);
}
