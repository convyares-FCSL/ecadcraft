//! Brief 030 acceptance (design 030 §12): accessories placed with `accessory`, part data from the
//! strip and the project, the plan, BOM and Checks rows, renumbering, links made and edited with
//! `jumper`, and the update maps. Driven through the commands' JSON forms.

use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_model::{ComponentId, ConductorId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// A strip `-X01` of six drawn terminals (1…6, left to right) on six wires, offering `12+2PE`,
/// with fixture A's project parts (design 030 §12).
struct F {
    h: H,
    strip: ComponentId,
    terms: Vec<TerminalId>,
    wires: Vec<ConductorId>,
}

fn fixture(seed: u64) -> F {
    let mut h = H::new(seed);
    h.first_page();
    let mut wires = Vec::new();
    for i in 0..6 {
        let x = 50.0 + 40.0 * f64::from(i);
        let v = h.run("wire", json!({"points": pts(&[(x, 220.0), (x, 100.0)])})).unwrap();
        wires.push(id::<ConductorId>(&v, "conductor"));
    }
    let v = h.run("terminals", json!({"from": [30, 160], "to": [280, 160]})).unwrap();
    let strip: ComponentId = id(&v, "strip");
    let terms: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    assert_eq!(terms.len(), 6, "{v}");
    h.run("setprop", json!({"component": strip.hex(), "key": "terminals", "value": "12+2PE"})).unwrap();
    h.run(
        "projectprops",
        json!({"props": {
            "strip_end_stop_part_number": "ES-1",
            "strip_end_plate_part_number": "EP-1",
            "strip_partition_part_number": "TP-1",
            "strip_marker_part_number": "MK-1",
            "strip_marker_per_card": "10",
        }}),
    )
    .unwrap();
    F { h, strip, terms, wires }
}

impl F {
    fn acc(&mut self, params: Value) -> Value {
        let mut p = params;
        p["strip"] = json!(self.strip.hex());
        self.h.run("accessory", p).unwrap()
    }

    fn jumper(&mut self, params: Value) -> Value {
        self.h.run("jumper", params).unwrap()
    }

    fn plan(&mut self) -> Value {
        self.h.run("stripplan", json!({"strip": self.strip.hex()})).unwrap()["strips"][0].clone()
    }

    fn bom(&mut self) -> Vec<Value> {
        let v = self.h.run("bom", json!({})).unwrap();
        v["rows"].as_array().unwrap().iter().filter(|r| r.get("accessory").is_some()).cloned().collect()
    }

    fn bom_row(&mut self, accessory: &str) -> Option<Value> {
        self.bom().into_iter().find(|r| r["accessory"] == accessory)
    }

    fn prop(&mut self, key: &str) -> Option<String> {
        self.h.doc().components[&self.strip].properties.get(key).cloned()
    }

    fn same_as_built(&mut self) {
        let (kept, built) = self.h.cfg(|p| (p.outputs().clone(), Outputs::build(p.ledger.document())));
        assert!(kept.strips.same_as(&built.strips), "strips differ:\nkept {:#?}\nbuilt {:#?}", kept.strips.strips, built.strips.strips);
        assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location));
        assert_eq!(kept.terminal_table(None), built.terminal_table(None));
    }

    fn same_net(&mut self, a: usize, b: usize) -> bool {
        let doc = self.h.doc();
        let c = doc.connectivity();
        let (x, y) = (doc.ends_at(self.terms[a]), doc.ends_at(self.terms[b]));
        let (Some((wa, _)), Some((wb, _))) = (x.first(), y.first()) else { return false };
        c.net_of_conductor(*wa).is_some() && c.net_of_conductor(*wa) == c.net_of_conductor(*wb)
    }

    fn number(&mut self, i: usize) -> Option<String> {
        let doc = self.h.doc();
        let (w, _) = *doc.ends_at(self.terms[i]).first()?;
        doc.conductors[&w].properties.get("wire_number").cloned()
    }
}

fn kinds(plan: &Value) -> Vec<String> {
    plan["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| match r["kind"].as_str().unwrap() {
            "accessory" => format!("{}@{}", r["accessory"].as_str().unwrap(), r["gap"].as_str().unwrap()),
            _ => r["label"].as_str().unwrap().to_string(),
        })
        .collect()
}

/// A–D: nothing placed shows nothing; ends, a partition and markers each one transaction, in
/// position order, on the BOM per part; `u` removes them.
#[test]
fn placed_accessories_in_the_plan_and_on_the_bom() {
    let mut f = fixture(0x30_01);
    let plan = f.plan();
    assert!(kinds(&plan).iter().all(|k| !k.contains('@')), "A: no gap rows: {plan}");
    assert!(f.bom().is_empty(), "A: no accessory rows");
    let off = f.h.run("stripplan", json!({"strip": f.strip.hex(), "accessories": false})).unwrap();
    assert!(off["strips"][0]["rows"][0].get("kind").is_none(), "028's rows exactly");
    // B
    let head = f.h.head();
    f.acc(json!({"at": "start", "items": ["end_stop"]}));
    f.acc(json!({"at": "end", "items": ["end_plate", "end_stop"]}));
    assert_eq!(f.h.head(), head + 2, "two transactions");
    let k = kinds(&f.plan());
    assert_eq!(k.first().map(String::as_str), Some("end_stop@start"));
    assert_eq!(k.get(k.len() - 2..).unwrap(), ["end_plate@end", "end_stop@end"]);
    assert_eq!(f.bom_row("end_stop").unwrap()["qty"], 2);
    assert_eq!(f.bom_row("end_stop").unwrap()["part_number"], "ES-1");
    assert_eq!(f.bom_row("end_plate").unwrap()["qty"], 1);
    f.same_as_built();
    f.h.cmdline("u");
    assert_eq!(f.prop("acc.end"), None, "u removes the last");
    assert_eq!(f.bom_row("end_stop").unwrap()["qty"], 1);
    // C
    let v = f.acc(json!({"at": "4", "add": ["partition"]}));
    assert_eq!(v["message"], "Partition after -X01:4.", "{v}");
    let k = kinds(&f.plan());
    let at4 = k.iter().position(|x| x == "4").unwrap();
    assert_eq!(k[at4 + 1], "partition@4");
    assert_eq!(f.bom_row("partition").unwrap()["part_number"], "TP-1");
    f.acc(json!({"at": "4", "items": null}));
    assert_eq!(f.prop("acc.after.4"), None);
    // D: markers on 1…12 in one transaction; cards of 10.
    let head = f.h.head();
    let at: Vec<String> = (1..=12).map(|n| n.to_string()).collect();
    let v = f.acc(json!({"at": at, "on": true, "items": ["marker"]}));
    assert_eq!(f.h.head(), head + 1, "one transaction");
    assert_eq!(v["message"], "Markers on -X01:1\u{2026}12.", "{v}");
    let plan = f.plan();
    let marked = plan["rows"].as_array().unwrap().iter().filter(|r| r["marker"] == true).count();
    assert_eq!(marked, 12);
    assert_eq!(plan["accessories"]["marker"], 12);
    let m = f.bom_row("marker").unwrap();
    assert_eq!((m["qty"].clone(), m["description"].clone()), (json!(2), json!("Marker card \u{d7}10")), "{m}");
    f.h.run("projectprops", json!({"props": {"strip_marker_part_number": null}})).unwrap();
    assert!(f.bom_row("marker").is_none(), "markers only with a part number");
    let checks = f.h.run("checks", json!({"kinds": ["no-accessory-part"]})).unwrap();
    assert!(checks.to_string().contains("12 strip accessories have no part number"), "{checks}");
    f.same_as_built();
}

/// E (FP4, FP7): renumbering moves `acc.after.*`, `acc.on.*` and rewrites links, in the same
/// transaction; the preview lists them; `u` restores all.
#[test]
fn renumbering_moves_accessory_keys_and_links() {
    let mut f = fixture(0x30_02);
    f.acc(json!({"at": "4", "add": ["partition"]}));
    f.acc(json!({"at": "2", "on": true, "items": ["marker"]}));
    f.jumper(json!({"strip": f.strip.hex(), "positions": "1-4"}));
    let pv = f.h.run("renumber", json!({"kind": "terminals", "scope": "all", "start": 3, "preview": true})).unwrap();
    let lines = pv["accessories"].to_string();
    assert!(lines.contains("partition after 4 \u{2192} after 6"), "{pv}");
    assert!(lines.contains("link 1-4 \u{2192} 3-6"), "{pv}");
    let head = f.h.head();
    f.h.run("renumber", json!({"kind": "terminals", "scope": "all", "start": 3})).unwrap();
    assert_eq!(f.h.head(), head + 1, "one transaction");
    assert_eq!(f.prop("acc.after.6").as_deref(), Some("partition"));
    assert_eq!(f.prop("acc.after.4"), None);
    assert_eq!(f.prop("acc.on.4").as_deref(), Some("marker"));
    assert_eq!(f.prop("link.1").as_deref(), Some("3-6"));
    assert!(f.same_net(0, 3) && !f.same_net(0, 4), "the link joins the same terminals");
    f.same_as_built();
    f.h.cmdline("u");
    assert_eq!(f.prop("acc.after.4").as_deref(), Some("partition"));
    assert_eq!(f.prop("link.1").as_deref(), Some("1-4"));
}

/// F, P: orphans and link checks have working fixes.
#[test]
fn checks_rows_have_working_fixes() {
    let mut f = fixture(0x30_03);
    f.acc(json!({"at": "11", "add": ["partition"]}));
    f.h.run("setprop", json!({"component": f.strip.hex(), "key": "terminals", "value": "8"})).unwrap();
    let v = f.h.run("checks", json!({"kinds": ["accessory-orphan"]})).unwrap();
    assert!(v.to_string().contains("-X01: partition after 11, but -X01 has no position 11"), "{v}");
    let fixes = f.h.cfg(|p| {
        let o = p.outputs().clone();
        let fix = o.checks().into_iter().find(|c| c.kind == ecad_cadcraft::checks::Kind::AccessoryOrphan).and_then(|c| c.fix).unwrap();
        ecad_cadcraft::outputs::accessory_fix_commands(&o, fix)
    });
    assert_eq!(fixes.len(), 2);
    assert_eq!(fixes[0].0, "Move to end");
    let head = f.h.head();
    f.h.run(fixes[0].1, fixes[0].2.clone()).unwrap();
    assert_eq!(f.h.head(), head + 1, "Move to end is one transaction");
    assert_eq!(f.prop("acc.after.11"), None);
    assert_eq!(f.prop("acc.end").as_deref(), Some("partition"));
    // P: a link across a partition; split it there.
    f.acc(json!({"at": "4", "add": ["partition"]}));
    f.jumper(json!({"strip": f.strip.hex(), "positions": "3-5"}));
    let v = f.h.run("checks", json!({"kinds": ["link-across-partition"]})).unwrap();
    assert!(v.to_string().contains("-X01: link 3-5 crosses the partition after 4"), "{v}");
    let fixes = f.h.cfg(|p| {
        let o = p.outputs().clone();
        let fix = o.checks().into_iter().find(|c| c.kind == ecad_cadcraft::checks::Kind::LinkAcrossPartition).and_then(|c| c.fix).unwrap();
        ecad_cadcraft::outputs::accessory_fix_commands(&o, fix)
    });
    assert_eq!(fixes[0].0, "Split link at 4");
    f.jumper(json!({"link": {"strip": f.strip.hex(), "n": 1}, "positions": "3-6"}));
    let head = f.h.head();
    f.h.run(fixes[0].1, fixes[0].2.clone()).unwrap();
    assert_eq!(f.h.head(), head + 1, "Split is one transaction");
    assert_eq!(f.prop("link.1").as_deref(), Some("3-4"));
    assert_eq!(f.prop("link.2").as_deref(), Some("5-6"));
    // A link naming missing positions: drop them.
    f.h.run("setprop", json!({"component": f.strip.hex(), "key": "link.3", "value": "7-15"})).unwrap();
    let v = f.h.run("checks", json!({"kinds": ["link-orphan"]})).unwrap();
    assert!(v.to_string().contains("names 15, which -X01 does not have"), "{v}");
    f.jumper(json!({"link": {"strip": f.strip.hex(), "n": 3}, "drop_missing": true}));
    assert_eq!(f.prop("link.3").as_deref(), Some("7-8"));
    f.same_as_built();
}

/// L, N, O: links join nets with one wire number in one transaction; extend, shorten, split and
/// delete are one transaction each; bars per link.
#[test]
fn links_make_one_net_and_count_bars() {
    let mut f = fixture(0x30_04);
    let head = f.h.head();
    let v = f.jumper(json!({"terminals": [f.terms[0].hex(), f.terms[3].hex()]}));
    assert_eq!(f.h.head(), head + 1, "one transaction");
    assert_eq!(v["positions"], "1-4", "{v}");
    assert_eq!(f.prop("link.1").as_deref(), Some("1-4"));
    assert!(f.same_net(0, 3) && f.same_net(1, 2) && !f.same_net(0, 4));
    assert_eq!(f.number(0), f.number(3), "one wire number");
    let tx = f.h.p(|p| p.ledger.log().last().map(|t| t.ops.len()).unwrap_or(0));
    assert!(tx > 1, "the numbering pass's ops are in the link's transaction");
    let skip = f.jumper(json!({"strip": f.strip.hex(), "positions": "7, 9, 11", "skip": true}));
    assert_eq!(skip["link"], 2);
    let plan = f.plan();
    let links = plan["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    // N: extend, shorten (an inner position makes a skip), split, delete.
    let n1 = json!({"strip": f.strip.hex(), "n": 1});
    for (params, want) in [
        (json!({"link": n1, "add": "5-8"}), Some("1-8")),
        (json!({"link": n1, "remove": "5"}), Some("1-4, 6-8")),
        (json!({"link": n1, "split_at": "2"}), Some("1-2")),
    ] {
        let head = f.h.head();
        f.jumper(params);
        assert_eq!(f.h.head(), head + 1);
        assert_eq!(f.prop("link.1").as_deref(), want);
    }
    assert_eq!(f.prop("link.3").as_deref(), Some("3-4, 6-8"), "the split's second part");
    f.h.cmdline("u");
    assert_eq!(f.prop("link.1").as_deref(), Some("1-4, 6-8"));
    assert_eq!(f.prop("link.3"), None);
    // O: bars. Link 1-12 is 12 positions: 2 bars of 10; jumper.bar 4 → 3.
    f.h.run("setprop", json!({"component": f.strip.hex(), "key": "link.1", "value": "1-12"})).unwrap();
    let bars = |f: &mut F| f.plan()["links"].as_array().unwrap().iter().find(|l| l["link"] == 1).unwrap()["bars"].clone();
    assert_eq!(bars(&mut f), 2);
    f.h.run("setprop", json!({"component": f.strip.hex(), "key": "jumper.bar", "value": "4"})).unwrap();
    assert_eq!(bars(&mut f), 3);
    f.h.run("setprop", json!({"component": f.strip.hex(), "key": "link.2.part_number", "value": "JB-10-BU"})).unwrap();
    let rows: Vec<Value> = f.bom().into_iter().filter(|r| r["accessory"] == "jumper").collect();
    assert_eq!(rows.len(), 2, "a link's own part gives its own row: {rows:?}");
    let delete = f.jumper(json!({"link": n1, "delete": true}));
    assert_eq!(delete["message"], "Deleted link -X01 1-12.");
    assert!(!f.same_net(0, 3));
    f.same_as_built();
}

/// G: update maps. A strip property recomputes only that strip; a project default only the
/// strips inheriting it; a wire edit elsewhere none.
#[test]
fn accessory_edits_recompute_only_what_they_touch() {
    let mut f = fixture(0x30_05);
    // A second strip with its own end stop part, and nothing placed.
    let mut xs = Vec::new();
    for i in 0..2 {
        let x = 350.0 + 40.0 * f64::from(i);
        f.h.run("wire", json!({"points": pts(&[(x, 220.0), (x, 100.0)])})).unwrap();
        xs.push(x);
    }
    let v = f.h.run("terminals", json!({"from": [330, 160], "to": [420, 160]})).unwrap();
    let x2: ComponentId = id(&v, "strip");
    f.h.run("setprop", json!({"component": x2.hex(), "key": "end_stop.part_number", "value": "OWN"})).unwrap();
    f.h.run("accessory", json!({"strip": x2.hex(), "at": "start", "items": ["end_stop"]})).unwrap();
    f.acc(json!({"at": "start", "items": ["end_stop"]}));
    f.h.cfg(|p| {
        p.outputs();
    });
    let work = |f: &mut F| f.h.cfg(|p| p.outputs().strips.last_work);
    f.h.run("setprop", json!({"component": f.strip.hex(), "key": "acc.after.2", "value": "partition"})).unwrap();
    assert_eq!(work(&mut f), 1, "one strip");
    f.h.run("projectprops", json!({"props": {"strip_end_stop_part_number": "ES-2"}})).unwrap();
    assert_eq!(work(&mut f), 1, "only the strip inheriting it");
    assert_eq!(f.bom_row("end_stop").unwrap()["part_number"], "ES-2");
    f.h.run("projectprops", json!({"props": {"strip_partition_maker": "M"}})).unwrap();
    assert_eq!(work(&mut f), 1);
    f.h.run("wire", json!({"points": pts(&[(500.0, 50.0), (550.0, 50.0)])})).unwrap();
    assert_eq!(work(&mut f), 0, "an unrelated wire changes no strip");
    f.same_as_built();
}

/// K: hostile values are refused in words or listed by Checks, never a panic.
#[test]
fn hostile_accessory_and_link_values_are_refused_in_words() {
    let mut f = fixture(0x30_06);
    let s = f.strip.hex();
    let refused = |f: &mut F, cmd: &str, p: Value| {
        let e = f.h.run(cmd, p.clone()).expect_err(&format!("{cmd} {p} should be refused"));
        assert!(!e.is_empty() && e.chars().next().is_some_and(|c| c.is_uppercase() || c == '"' || c == '`' || c == '-'), "{e}");
    };
    refused(&mut f, "setprop", json!({"component": s, "key": format!("acc.after.{}", "9".repeat(10_000)), "value": "partition"}));
    refused(&mut f, "setprop", json!({"component": s, "key": "acc.start", "value": "end_stop,".repeat(1000)}));
    refused(&mut f, "setprop", json!({"component": s, "key": "acc.start", "value": "marker"}));
    refused(&mut f, "setprop", json!({"component": s, "key": "acc.on.4", "value": "partition"}));
    refused(&mut f, "setprop", json!({"component": s, "key": "marker.per_card", "value": "0"}));
    refused(&mut f, "setprop", json!({"component": s, "key": "jumper.bar", "value": "0"}));
    let at: Vec<String> = (0..5000).map(|n| n.to_string()).collect();
    refused(&mut f, "accessory", json!({"strip": s, "at": at, "on": true, "items": ["marker"]}));
    refused(&mut f, "accessory", json!({"strip": s, "at": "15", "add": ["partition"]}));
    refused(&mut f, "setprop", json!({"component": s, "key": "link.0", "value": "1-4"}));
    refused(&mut f, "setprop", json!({"component": s, "key": "link.1000", "value": "1-4"}));
    refused(&mut f, "setprop", json!({"component": s, "key": "link.1", "value": "4"}));
    refused(&mut f, "jumper", json!({"strip": s, "positions": "4"}));
    // A link past the strip is kept and listed by Checks.
    f.h.run("setprop", json!({"component": s, "key": "link.1", "value": "1-99999"})).unwrap();
    let v = f.h.run("checks", json!({"kinds": ["link-orphan"]})).unwrap();
    assert!(v.to_string().contains("99999"), "{v}");
    // Keys on a device that is not a strip.
    let w = f.wires[0];
    let doc = f.h.doc();
    let far = doc.conductors[&w].ends.iter().flatten().find_map(|t| {
        let c = doc.terminals[t].component;
        (c != f.strip).then_some(c)
    });
    if let Some(c) = far {
        refused(&mut f, "setprop", json!({"component": c.hex(), "key": "acc.start", "value": "end_stop"}));
    }
    let coil = f.h.run("place", json!({"symbol": "coil", "at": [500, 200]})).unwrap();
    let k1: ComponentId = id(&coil, "component");
    refused(&mut f, "setprop", json!({"component": k1.hex(), "key": "link.1", "value": "1-2"}));
    refused(&mut f, "accessory", json!({"strip": k1.hex(), "at": "start", "items": ["end_stop"]}));
    f.same_as_built();
}

/// M: the link line is drawn from the link on the page: through the terminals shown, with a
/// stub where the link goes on to positions not shown here; a link edit redraws it.
#[test]
fn link_lines_are_drawn_from_the_link() {
    let mut f = fixture(0x30_07);
    let page = f.h.first_page();
    f.jumper(json!({"strip": f.strip.hex(), "positions": "1-4"}));
    let parts = |f: &mut F| f.h.p(|p| ecad_cadcraft::link_lines::parts_on_page(p.ledger.document(), page));
    let v = parts(&mut f);
    assert_eq!(v.len(), 3, "1-2, 2-3, 3-4 and no stub: {v:?}");
    assert!(v.iter().all(|s| (s.a.y - s.b.y).abs() < 1e-9), "a horizontal row");
    // 5-8: 5 and 6 are drawn here, 7 and 8 are spare: a stub past 6, in lane 0 (no overlap).
    f.jumper(json!({"strip": f.strip.hex(), "positions": "5-8"}));
    let v = parts(&mut f);
    let two: Vec<_> = v.iter().filter(|s| s.n == 2).collect();
    assert_eq!(two.len(), 2, "5-6 and the stub: {two:?}");
    let x6 = f.h.terminal_pos(page, f.terms[5]).x;
    assert!(two.iter().any(|s| (s.b.x - (x6 + ecad_cadcraft::link_lines::STUB_MM)).abs() < 1e-6), "{two:?}");
    // A skip link over 1-4 runs in lane 1, offset up.
    f.jumper(json!({"strip": f.strip.hex(), "positions": "2, 4", "skip": true}));
    let v = parts(&mut f);
    let y = f.h.terminal_pos(page, f.terms[1]).y;
    assert!(v.iter().filter(|s| s.n == 3).all(|s| (s.a.y - (y + ecad_cadcraft::link_lines::LANE_MM)).abs() < 1e-6), "{v:?}");
    // The page as projected shows them, and verify mode agrees.
    f.h.check();
    let lines = f.h.p(|p| p.pages.get(&page).map(|v| v.map.clone()));
    assert!(lines.is_some());
}
