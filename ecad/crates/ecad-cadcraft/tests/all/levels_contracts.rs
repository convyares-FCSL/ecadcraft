//! Brief 036 (design 036 §13): multi-level terminals placed by fence and on wires, labels per
//! level, links per level, internal bridges, fuse positions breaking the net, `termlabel`,
//! `toposition`, the plan, the BOM per type and the update maps. Driven through the commands'
//! JSON forms on a small fixture of loose vertical wires.

use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_model::{ComponentId, ConductorId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// The kept models equal a fresh build (the update map left nothing behind).
fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    assert!(kept.strips.same_as(&built.strips), "strips differ:\nkept {:#?}\nbuilt {:#?}", kept.strips.strips, built.strips.strips);
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location));
}

/// Three loose vertical wires at x = 10, 20, 30 between `y0` and `y0 + 40`.
fn wires(h: &mut H, y0: f64) -> Vec<ConductorId> {
    (1..=3)
        .map(|i| {
            let x = 10.0 * f64::from(i);
            id(&h.run("wire", json!({"points": pts(&[(x, y0), (x, y0 + 40.0)])})).unwrap(), "conductor")
        })
        .collect()
}

/// A fence across the wires at height `y`.
fn fence(h: &mut H, strip: Option<ComponentId>, ty: &str, level: &str, y: f64) -> Value {
    let mut p = json!({"type": ty, "level": level, "from": [0, y], "to": [40, y]});
    if let Some(c) = strip {
        p["strip"] = json!(c.hex());
    }
    h.run("terminals", p).unwrap()
}

fn terminals(v: &Value) -> Vec<TerminalId> {
    v["terminals"].as_array().unwrap().iter().map(|t| id(t, "terminal")).collect()
}

fn net(h: &mut H, t: TerminalId) -> Option<ecad_model::NetId> {
    h.doc().connectivity().net_of_terminal(t)
}

fn plan(h: &mut H, c: ComponentId) -> Value {
    h.run("stripplan", json!({"strip": c.hex()})).unwrap()["strips"][0].clone()
}

#[test]
fn lnpe_fences_fill_positions_and_label_each_level() {
    let mut h = H::new(0x0360_0001);
    wires(&mut h, 0.0);
    wires(&mut h, 100.0);
    wires(&mut h, 200.0);
    let head = h.head();
    let l = fence(&mut h, None, "LNPE", "L", 20.0);
    assert_eq!(h.head(), head + 1, "one transaction per fence");
    let x1: ComponentId = id(&l, "strip");
    assert_eq!(l["message"], "Placed -X01:1.L\u{2026}3.L on 3 wires (positions 1\u{2013}3, level L).", "{l}");
    let n = fence(&mut h, Some(x1), "LNPE", "N", 120.0);
    let pe = fence(&mut h, Some(x1), "LNPE", "PE", 220.0);
    let positions = |v: &Value| v["terminals"].as_array().unwrap().iter().map(|t| t["position"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(positions(&n), ["1", "2", "3"], "the N row fills positions 1-3");
    assert_eq!(positions(&pe), ["1", "2", "3"]);
    let s = plan(&mut h, x1);
    let rows = s["rows"].as_array().unwrap();
    let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["1.L", "1.N", "1.PE", "2.L", "2.N", "2.PE", "3.L", "3.N", "3.PE"]);
    assert_eq!(rows[0]["type"], "lnpe");
    assert_eq!(rows[0]["state"], "drawn");
    assert_eq!(s["drawn"], 3, "three positions");
    let bom = h.run("bom", json!({})).unwrap();
    let x_rows: Vec<&Value> = bom["rows"].as_array().unwrap().iter().filter(|r| r["tags"] == "-X01").collect();
    assert_eq!(x_rows.len(), 1, "{bom}");
    assert_eq!((x_rows[0]["qty"].as_u64(), x_rows[0]["description"].as_str()), (Some(3), Some("L/N/PE terminal")));
    // Hover and words show the shown label.
    let t = terminals(&n)[1];
    assert_eq!(h.p(|p| ecad_cadcraft::terminal_cmds::terminal_words(p.ledger.document(), t)), "-X01:2.N");
    same_as_built(&mut h);
    // `u` undoes the last fence only.
    h.run("undo", json!({})).unwrap();
    assert_eq!(plan(&mut h, x1)["rows"].as_array().unwrap().iter().filter(|r| r["state"] == "free").count(), 3, "PE levels free again");
    same_as_built(&mut h);
}

#[test]
fn links_join_one_level_and_cross_level_links_join_one_position() {
    let mut h = H::new(0x0360_0002);
    wires(&mut h, 0.0);
    wires(&mut h, 100.0);
    wires(&mut h, 200.0);
    let l = fence(&mut h, None, "LNPE", "L", 20.0);
    let x1: ComponentId = id(&l, "strip");
    let n = fence(&mut h, Some(x1), "LNPE", "N", 120.0);
    let pe = fence(&mut h, Some(x1), "LNPE", "PE", 220.0);
    let (lt, nt, pt) = (terminals(&l), terminals(&n), terminals(&pe));
    assert_ne!(net(&mut h, lt[0]), net(&mut h, lt[2]));
    h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1-3"})).unwrap();
    assert_eq!(net(&mut h, lt[0]), net(&mut h, lt[2]), "level L joined");
    assert_ne!(net(&mut h, nt[0]), net(&mut h, nt[2]), "level N not");
    h.run("setprop", json!({"component": x1.hex(), "key": "link.2", "value": "1.2-3.2"})).unwrap();
    assert_eq!(net(&mut h, nt[0]), net(&mut h, nt[2]), "level N joined");
    assert_ne!(net(&mut h, nt[0]), net(&mut h, lt[0]));
    h.run("setprop", json!({"component": x1.hex(), "key": "link.3", "value": "1.1-1.3"})).unwrap();
    assert_eq!(net(&mut h, lt[0]), net(&mut h, pt[0]), "cross-level link");
    assert_eq!(net(&mut h, lt[0]), net(&mut h, nt[0]));
    let e = h.run("setprop", json!({"component": x1.hex(), "key": "link.4", "value": "1.1-3.2"})).unwrap_err();
    assert!(e.contains("joins different levels at different positions"), "{e}");
    let s = plan(&mut h, x1);
    let links = s["links"].as_array().unwrap();
    assert_eq!(links.iter().map(|l| l["level"].clone()).collect::<Vec<_>>(), [json!(1), json!(2), Value::Null]);
    same_as_built(&mut h);
    h.check();
}

#[test]
fn bridged_two_level_positions_and_fuse_positions_break_the_net() {
    let mut h = H::new(0x0360_0003);
    wires(&mut h, 0.0);
    wires(&mut h, 100.0);
    let a = fence(&mut h, None, "x2B", "1", 20.0);
    let x1: ComponentId = id(&a, "strip");
    let b = fence(&mut h, Some(x1), "x2B", "2", 120.0);
    let (at, bt) = (terminals(&a), terminals(&b));
    assert_eq!(net(&mut h, at[0]), net(&mut h, bt[0]), "a bridged position's levels are one net");
    assert_ne!(net(&mut h, at[0]), net(&mut h, at[1]), "two positions stay two nets");
    same_as_built(&mut h);
    // A fuse position on a wire: in and out on two nets.
    let mut h = H::new(0x0360_0004);
    let w: ConductorId = id(&h.run("wire", json!({"points": pts(&[(10.0, 0.0), (10.0, 80.0)])})).unwrap(), "conductor");
    let f = h.run("terminals", json!({"type": "FUSE", "from": [0, 40], "to": [20, 40]})).unwrap();
    let x1: ComponentId = id(&f, "strip");
    let t_in: TerminalId = id(&f["terminals"][0], "terminal");
    let new: ConductorId = id(&f["terminals"][0], "new");
    let doc = h.doc();
    let conn = doc.connectivity();
    assert_ne!(conn.net_of_conductor(w), conn.net_of_conductor(new), "the fuse breaks the net");
    let tout = doc.terminals_of(x1).find(|(_, x)| x.key.as_deref() == Some(".1o")).map(|(t, _)| t).unwrap();
    assert!(doc.ends_at(t_in).len() == 1 && doc.ends_at(tout).len() == 1, "one half on each point");
    h.run("setprop", json!({"component": x1.hex(), "key": "rating.1", "value": "2A"})).unwrap();
    let s = plan(&mut h, x1);
    assert_eq!((s["rows"][0]["rating"].as_str(), s["rows"][0]["breaks"].as_bool()), (Some("2A"), Some(true)), "{s}");
    same_as_built(&mut h);
    h.check();
}

#[test]
fn termlabel_relabels_a_position_and_sets_a_level_label() {
    let mut h = H::new(0x0360_0005);
    wires(&mut h, 0.0);
    wires(&mut h, 100.0);
    let s1 = fence(&mut h, None, "SEN", "S", 20.0);
    let x1: ComponentId = id(&s1, "strip");
    fence(&mut h, Some(x1), "SEN", "+", 120.0);
    h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1.2-3.2"})).unwrap();
    h.run("setprop", json!({"component": x1.hex(), "key": "acc.on.2", "value": "marker"})).unwrap();
    let t2 = terminals(&s1)[1];
    let v = h.run("termlabel", json!({"terminal": t2.hex(), "position": "2.1"})).unwrap();
    assert_eq!(v["message"], "-X01:2 is now -X01:2.1 (2 levels).", "{v}");
    let doc = h.doc();
    let props = &doc.components[&x1].properties;
    assert_eq!(props.get("acc.on.2.1").map(String::as_str), Some("marker"), "accessories follow");
    assert_eq!(props.get("link.1").map(String::as_str), Some("1.2-3.2"), "the link names the same positions in plan order");
    let e = h.run("termlabel", json!({"terminal": t2.hex(), "position": "3"})).unwrap_err();
    assert!(e.contains("already has a position 3"), "{e}");
    h.run("termlabel", json!({"terminal": t2.hex(), "text": "2.1S"})).unwrap();
    assert_eq!(h.p(|p| ecad_cadcraft::interact::terminal_name(p.ledger.document(), t2)), "2.1S");
    let s = plan(&mut h, x1);
    let row = s["rows"].as_array().unwrap().iter().find(|r| r["terminal"] == json!(t2.hex())).cloned().unwrap();
    assert_eq!((row["label"].as_str(), row["label_overridden"].as_bool(), row["position"].as_str()), (Some("2.1S"), Some(true), Some("2.1")));
    // The separator default reaches the strip (design 036 §13 N): `""` on the project is none.
    let t1 = terminals(&s1)[0];
    h.run("projectprops", json!({"props": {"terminal.level.separator": ""}})).unwrap();
    assert_eq!(h.doc().properties.get("terminal.level.separator").map(String::as_str), Some("none"));
    assert_eq!(h.p(|p| ecad_cadcraft::interact::terminal_name(p.ledger.document(), t1)), "1S");
    same_as_built(&mut h);
    // The strip's own value wins.
    h.run("setprop", json!({"component": x1.hex(), "key": "terminal.level.separator", "value": "/"})).unwrap();
    assert_eq!(h.p(|p| ecad_cadcraft::interact::terminal_name(p.ledger.document(), t1)), "1/S");
    same_as_built(&mut h);
    h.check();
}

#[test]
fn toposition_turns_a_fuse_terminal_device_into_a_fuse_position() {
    let mut h = H::new(0x0360_0006);
    wires(&mut h, 0.0);
    let ft = fence(&mut h, None, "Feedthrough", "1", 20.0);
    let x1: ComponentId = id(&ft, "strip");
    let f5: ComponentId = id(&h.run("place", json!({"symbol": "fuseterm", "at": [100, 100]})).unwrap(), "component");
    let w1: ConductorId = id(&h.run("wire", json!({"points": pts(&[(100.0, 110.0), (100.0, 140.0)])})).unwrap(), "conductor");
    let w2: ConductorId = id(&h.run("wire", json!({"points": pts(&[(100.0, 90.0), (100.0, 60.0)])})).unwrap(), "conductor");
    h.run("setprop", json!({"component": f5.hex(), "key": "rating", "value": "4A"})).unwrap();
    let nets_before = {
        let d = h.doc();
        let c = d.connectivity();
        (c.net_of_conductor(w1).is_some(), c.net_of_conductor(w1) == c.net_of_conductor(w2))
    };
    let e = h.run("toposition", json!({"device": x1.hex()})).unwrap_err();
    assert!(e.contains("is a terminal strip"), "{e}");
    let head = h.head();
    let v = h.run("toposition", json!({"device": f5.hex(), "strip": x1.hex()})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(v["position"], "4", "{v}");
    let doc = h.doc();
    assert!(!doc.components.contains_key(&f5), "-F05 gone");
    assert_eq!(doc.components[&x1].properties.get("rating.4").map(String::as_str), Some("4A"));
    let c = doc.connectivity();
    assert_eq!((c.net_of_conductor(w1).is_some(), c.net_of_conductor(w1) == c.net_of_conductor(w2)), nets_before, "nets unchanged");
    let ends: Vec<_> = doc.terminals_of(x1).filter(|(_, x)| x.label.as_deref() == Some("4")).map(|(t, _)| doc.ends_at(t).len()).collect();
    assert_eq!(ends, [1, 1], "each wire on its point");
    same_as_built(&mut h);
    h.check();
    h.run("undo", json!({})).unwrap();
    assert!(h.doc().components.contains_key(&f5), "u restores -F05");
}

#[test]
fn offers_by_type_give_spare_positions_and_bom_rows() {
    let mut h = H::new(0x0360_0007);
    wires(&mut h, 0.0);
    let v = fence(&mut h, None, "Feedthrough", "1", 20.0);
    let x1: ComponentId = id(&v, "strip");
    h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "12+2PE+4x3"})).unwrap();
    let s = plan(&mut h, x1);
    assert_eq!(s["offered"], "12+2PE+4x3");
    let rows = s["rows"].as_array().unwrap();
    assert_eq!(rows.iter().filter(|r| r["state"] == "spare" && r["type"] == "x3").count(), 12, "4 spare three-level positions, 3 rows each");
    let bom = h.run("bom", json!({})).unwrap();
    let qty: Vec<u64> = bom["rows"].as_array().unwrap().iter().filter(|r| r["tags"] == "-X01").map(|r| r["qty"].as_u64().unwrap()).collect();
    assert_eq!(qty, [12, 2, 4]);
    same_as_built(&mut h);
}

/// Review M1 (risk 2.1): the planner's empty-points branch takes the first terminal per definition
/// key (FP8c). A 028 strip whose joined terminal's key collides with the first symbol's key (`1`:
/// joined terminals got `key = label`) is numbered by each terminal's own symbol; a strip without
/// a collision numbers exactly as before (drawing order).
#[test]
fn renumber_keeps_028_strips_and_numbers_colliding_keys_by_their_own_symbol() {
    let mut h = H::new(0x0360_0008);
    for x in [10.0, 20.0, 30.0] {
        h.run("wire", json!({"points": pts(&[(x, 0.0), (x, 40.0)])})).unwrap();
    }
    // Right to left: the first (main) symbol is at x = 30, the joined ones at 20 and 10.
    let v = h.run("terminals", json!({"from": [40, 20], "to": [0, 20]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let ts = terminals(&v);
    let labels = |h: &mut H| ts.iter().map(|t| h.doc().terminals[t].label.clone().unwrap_or_default()).collect::<Vec<_>>();
    assert_eq!(labels(&mut h), ["1", "2", "3"]);
    // No collision: renumbering numbers in drawing order (left to right), as 028 did.
    h.run("renumber", json!({"kind": "terminals", "scope": "selection", "strips": [x1.hex()]})).unwrap();
    assert_eq!(labels(&mut h), ["3", "2", "1"]);
    h.run("undo", json!({})).unwrap();
    // A collision: the joined terminal at x = 20 keyed `1`, as the first symbol's.
    h.cfg(|p| {
        let doc = p.ledger.document();
        let before = doc.terminals[&ts[1]].clone();
        let mut after = before.clone();
        after.key = Some("1".into());
        let ops = vec![ecad_model::Op::RemoveTerminal { id: ts[1], terminal: before }, ecad_model::Op::AddTerminal { id: ts[1], terminal: after }];
        p.ledger.commit(ecad_model::Author::User, "collide", ops).unwrap();
    });
    // The page is not reprojected after that direct commit, so the session runs the command
    // without the harness's page check (as release_contracts does).
    h.s.execute("renumber", &json!({"kind": "terminals", "scope": "selection", "strips": [x1.hex()]})).unwrap();
    assert_eq!(labels(&mut h), ["3", "2", "1"], "each terminal numbered at its own symbol");
}

/// Review M1 (no silent data loss): a level's own label and a fuse position's rating go with the
/// last terminal of their level or position (and come back with `u`); a two-point split reports a
/// wire number its out half does not keep; `toposition` lists every property it does not carry.
#[test]
fn nothing_is_lost_silently() {
    let mut h = H::new(0x0360_0009);
    let page = h.first_page();
    wires(&mut h, 0.0);
    let s1 = fence(&mut h, None, "SEN", "S", 20.0);
    let x1: ComponentId = id(&s1, "strip");
    let t2 = terminals(&s1)[1];
    h.run("termlabel", json!({"terminal": t2.hex(), "text": "2S"})).unwrap();
    // Erase position 2's S symbol: its label key goes with it.
    let rep = h.doc().representations_showing(t2).next().map(|(r, _)| r).unwrap();
    let item = h.doc().representations[&rep].items[0];
    let handle = h.handle_of(page, item);
    h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    assert_eq!(h.doc().components[&x1].properties.get("term_label.2.1"), None, "removed with its level");
    h.run("undo", json!({})).unwrap();
    assert_eq!(h.doc().components[&x1].properties.get("term_label.2.1").map(String::as_str), Some("2S"), "u brings it back");
    same_as_built(&mut h);
    // A fixed wire number on a wire a fuse position splits: the out half's loss is reported.
    let w: ConductorId = id(&h.run("wire", json!({"points": pts(&[(100.0, 0.0), (100.0, 80.0)])})).unwrap(), "conductor");
    h.run("wirenumber", json!({"conductor": w.hex(), "value": "24V-07"})).unwrap();
    let f = h.run("terminals", json!({"type": "FUSE", "from": [90, 40], "to": [110, 40]})).unwrap();
    assert_eq!(f["dropped_wire_numbers"][0]["number"], "24V-07", "{f}");
    assert!(f["message"].as_str().unwrap().contains("no longer carry wire number 24V-07"), "{f}");
    // toposition lists what goes with the device.
    let dev: ComponentId = id(&h.run("place", json!({"symbol": "fuseterm", "at": [200, 100]})).unwrap(), "component");
    h.run("setprop", json!({"component": dev.hex(), "key": "part_number", "value": "FT-4"})).unwrap();
    let v = h.run("toposition", json!({"device": dev.hex(), "strip": x1.hex()})).unwrap();
    let lost: Vec<&str> = v["lost"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(lost.contains(&"part_number") && lost.contains(&"tag"), "{v}");
    h.check();
}

/// Review M1: relabelling a position moves every position-keyed property (one shared list) and
/// rewrites the links naming it, in one transaction.
#[test]
fn relabel_moves_every_position_keyed_property() {
    let mut h = H::new(0x0360_000a);
    wires(&mut h, 0.0);
    let f = fence(&mut h, None, "FUSE", "1", 20.0);
    let x1: ComponentId = id(&f, "strip");
    let t2 = terminals(&f)[1];
    let suffix = |prefix: &str, p: &str| if prefix == ecad_cadcraft::terminal_types::LABEL_PREFIX { format!("{p}.1") } else { p.to_string() };
    let value = |prefix: &str| {
        if prefix == ecad_cadcraft::accessories::AFTER_PREFIX {
            "partition"
        } else if prefix == ecad_cadcraft::accessories::ON_PREFIX {
            "marker"
        } else {
            "2A"
        }
    };
    for prefix in ecad_cadcraft::terminal_types::POSITION_KEYED {
        h.run("setprop", json!({"component": x1.hex(), "key": format!("{prefix}{}", suffix(prefix, "2")), "value": value(prefix)})).unwrap();
    }
    h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1-2"})).unwrap();
    let head = h.head();
    h.run("termlabel", json!({"terminal": t2.hex(), "position": "7"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let props = h.doc().components[&x1].properties.clone();
    for prefix in ecad_cadcraft::terminal_types::POSITION_KEYED {
        assert_eq!(props.get(&format!("{prefix}{}", suffix(prefix, "2"))), None, "{prefix} left on 2");
        assert_eq!(props.get(&format!("{prefix}{}", suffix(prefix, "7"))).map(String::as_str), Some(value(prefix)), "{prefix} moved to 7");
    }
    assert_eq!(props.get("link.1").map(String::as_str), Some("1, 7"));
    same_as_built(&mut h);
}

/// Review M1 (update maps, design 036 §11): an edit unrelated to strips recomputes none; the
/// project separator recomputes only the strips with a multi-level position that inherit it.
#[test]
fn update_map_counters() {
    let mut h = H::new(0x0360_000b);
    wires(&mut h, 0.0);
    wires(&mut h, 100.0);
    let a = fence(&mut h, None, "LNPE", "L", 20.0);
    let _plain = fence(&mut h, None, "Feedthrough", "1", 120.0);
    let x1: ComponentId = id(&a, "strip");
    let work = |h: &mut H| h.cfg(|p| p.outputs().strips.last_work);
    h.run("wire", json!({"points": pts(&[(300.0, 0.0), (300.0, 40.0)])})).unwrap();
    assert_eq!(work(&mut h), 0, "a wire elsewhere recomputes no strip");
    h.run("projectprops", json!({"props": {"terminal.level.separator": "/"}})).unwrap();
    assert_eq!(work(&mut h), 1, "only the L/N/PE strip inherits the separator");
    h.run("setprop", json!({"component": x1.hex(), "key": "terminal.level.separator", "value": "-"})).unwrap();
    h.run("projectprops", json!({"props": {"terminal.level.separator": ":"}})).unwrap();
    assert_eq!(work(&mut h), 0, "a strip with its own separator no longer inherits it");
    same_as_built(&mut h);
}

/// Review 036, 1: a link on several levels, and a cross-level link holding a position range, keep
/// every span's level, point and both range ends through `termlabel` and through `renumber`.
#[test]
fn relabel_and_renumber_keep_mixed_and_cross_level_links() {
    for via_renumber in [false, true] {
        let mut h = H::new(0x0360_000c);
        wires(&mut h, 0.0);
        wires(&mut h, 100.0);
        // An x2 strip: positions 1-3, both levels drawn; one more wire for position 4.
        let a = fence(&mut h, None, "x2", "1", 20.0);
        let x1: ComponentId = id(&a, "strip");
        fence(&mut h, Some(x1), "x2", "2", 120.0);
        h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1.2, 3.1"})).unwrap();
        h.run("setprop", json!({"component": x1.hex(), "key": "link.2", "value": "1.1-1.2, 2-3"})).unwrap();
        let (l1, l2) = if via_renumber {
            // Renumber from 7: positions 1, 2, 3 become 7, 8, 9.
            h.run("renumber", json!({"kind": "terminals", "scope": "selection", "strips": [x1.hex()], "start": 7})).unwrap();
            ("7.2, 9", "7.1-7.2, 8-9")
        } else {
            h.run("termlabel", json!({"strip": x1.hex(), "at": "1", "position": "9"})).unwrap();
            ("9.2, 3", "9.1-9.2, 2-3")
        };
        let props = h.doc().components[&x1].properties.clone();
        assert_eq!(props.get("link.1").map(String::as_str), Some(l1), "renumber {via_renumber}: levels kept per span");
        assert_eq!(props.get("link.2").map(String::as_str), Some(l2), "renumber {via_renumber}: the range keeps both ends");
        same_as_built(&mut h);
    }
}

/// Review 036, 2: a link on the out points stays on the out points; the out cells share one wire
/// number and the in cells keep theirs.
#[test]
fn a_link_on_out_points_keeps_its_point() {
    let mut h = H::new(0x0360_000d);
    for x in [10.0, 20.0, 30.0] {
        h.run("wire", json!({"points": pts(&[(x, 0.0), (x, 80.0)])})).unwrap();
    }
    let f = h.run("terminals", json!({"type": "FUSE", "from": [0, 40], "to": [40, 40]})).unwrap();
    let x1: ComponentId = id(&f, "strip");
    h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1.1o-3.1o"})).unwrap();
    assert_eq!(h.doc().components[&x1].properties.get("link.1").map(String::as_str), Some("1.1o-3.1o"));
    let s = plan(&mut h, x1);
    let rows = s["rows"].as_array().unwrap();
    let wire = |r: &Value, side: &str| r[side][0]["wire"].as_str().map(str::to_string);
    let outs: std::collections::BTreeSet<_> = rows.iter().map(|r| wire(r, "outside")).collect();
    let ins: std::collections::BTreeSet<_> = rows.iter().map(|r| wire(r, "inside")).collect();
    assert_eq!(outs.len(), 1, "the out points are one net: {s}");
    assert_eq!(ins.len(), 3, "the in points keep three nets: {s}");
    same_as_built(&mut h);
}

/// Review 036, 3: a strip whose only multi-level positions are spares follows a project separator
/// change (it inherits the key), and only the inheriting strips are recomputed.
#[test]
fn spare_only_multi_level_strips_inherit_the_separator() {
    let mut h = H::new(0x0360_000e);
    wires(&mut h, 0.0);
    let v = fence(&mut h, None, "Feedthrough", "1", 20.0);
    let x1: ComponentId = id(&v, "strip");
    h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "3+2x3"})).unwrap();
    let labels = |h: &mut H| plan(h, x1)["rows"].as_array().unwrap().iter().map(|r| r["label"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert!(labels(&mut h).contains(&"4.1".to_string()));
    h.run("projectprops", json!({"props": {"terminal.level.separator": "-"}})).unwrap();
    assert_eq!(h.cfg(|p| p.outputs().strips.last_work), 1, "the spare-only strip recomputed");
    assert!(labels(&mut h).contains(&"4-1".to_string()), "{:?}", labels(&mut h));
    same_as_built(&mut h);
}

/// Open point 6 (§5.1, Q5): a typed `2.1` is position 2, level 1; when a position labelled `2.1`
/// exists, the echo says so and how to write that position.
#[test]
fn a_typed_dotted_end_says_which_position_it_is() {
    let mut h = H::new(0x0360_000f);
    wires(&mut h, 0.0);
    let v = fence(&mut h, None, "Feedthrough", "1", 20.0);
    let x1: ComponentId = id(&v, "strip");
    let t3 = terminals(&v)[2];
    h.run("termlabel", json!({"terminal": t3.hex(), "position": "2.1"})).unwrap();
    let r = h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1, 2.1"})).unwrap();
    assert_eq!(r["message"], "Link -X01 2.1 (position 2, level 1); position 2.1 is written 2.1.1.", "{r}");
    let r = h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1, 2"})).unwrap();
    assert!(r.get("message").is_none(), "{r}");
}
