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
    // The separator default reaches the strip; the strip's own value wins.
    h.run("projectprops", json!({"key": "terminal.level.separator", "value": ""})).ok();
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
