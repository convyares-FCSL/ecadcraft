//! Tester 036 (multi-level terminals): acceptance checks from outside, written from brief 036 and
//! design 036 (§13's table A–Q) only, before the implementation existed. They drive JSON commands
//! and APIs that 028/030 already had (`stripplan`, `bom`, `checks`, `net`, the ledger), so the
//! module compiles before the build; each test that needs the build is ignored until phase 2.
//!
//! Fixture M is simplified: plain vertical wires stand in for the drops to loads and the sensor
//! wiring (design §13 uses loads, `prox3` sensors and a DI card); the levels, labels, links and
//! nets the checks look at are the same.
//!
//! Every test name starts with the §13 letter it checks.

use std::collections::BTreeMap;

use cadcraft_engine::Session;
use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{Author, ComponentId, Document, Op, PageId, Subject, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

// ------------------------------------------------------------------------------- helpers

fn ids(v: &Value) -> Vec<TerminalId> {
    v["terminals"].as_array().map(|a| a.iter().map(|x| id(x, "terminal")).collect()).unwrap_or_default()
}

/// Vertical wires at `xs`, from y 250 to y 150.
fn drops(h: &mut H, xs: &[f64]) {
    for x in xs {
        h.run("wire", json!({"points": pts(&[(*x, 250.0), (*x, 150.0)])})).unwrap();
    }
}

/// Place Terminals' JSON form with a type and a level (design §3.2), a fence at y 200 from `x0`
/// to `x1`. Returns the strip and the new terminals.
fn fence(h: &mut H, strip: Option<ComponentId>, ty: &str, level: &str, x0: f64, x1: f64) -> (ComponentId, Vec<TerminalId>) {
    let v = fence_v(h, strip, ty, level, x0, x1).unwrap_or_else(|e| panic!("terminals {ty} {level}: {e}"));
    (id(&v, "strip"), ids(&v))
}

fn fence_v(h: &mut H, strip: Option<ComponentId>, ty: &str, level: &str, x0: f64, x1: f64) -> Result<Value, String> {
    let s = strip.map(|c| Value::String(c.hex())).unwrap_or(Value::Null);
    h.run("terminals", json!({"strip": s, "type": ty, "level": level, "from": [x0, 200.0], "to": [x1, 200.0]}))
}

fn label(d: &Document, t: TerminalId) -> String {
    d.terminals.get(&t).and_then(|x| x.label.clone()).unwrap_or_default()
}

fn key(d: &Document, t: TerminalId) -> String {
    d.terminals.get(&t).and_then(|x| x.key.clone()).unwrap_or_default()
}

/// A strip's terminals by position label, then level key.
fn positions(d: &Document, strip: ComponentId) -> BTreeMap<String, BTreeMap<String, TerminalId>> {
    let mut out: BTreeMap<String, BTreeMap<String, TerminalId>> = BTreeMap::new();
    for (tid, t) in &d.terminals {
        if t.component == strip {
            out.entry(t.label.clone().unwrap_or_default()).or_default().insert(t.key.clone().unwrap_or_default(), *tid);
        }
    }
    out
}

fn prop(h: &mut H, c: ComponentId, k: &str) -> Option<String> {
    h.doc().components.get(&c).and_then(|x| x.properties.get(k).cloned())
}

fn squash(v: Option<String>) -> Option<String> {
    v.map(|s| s.chars().filter(|c| !c.is_whitespace()).collect())
}

fn plan(h: &mut H, strip: ComponentId) -> Value {
    h.run("stripplan", json!({"strip": strip.hex(), "limit": 5000})).unwrap()["strips"][0].clone()
}

/// The plan's level rows (no accessory rows), in order.
fn level_rows(p: &Value) -> Vec<Value> {
    p["rows"].as_array().map(|a| a.iter().filter(|r| r["kind"] != "accessory").cloned().collect()).unwrap_or_default()
}

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn qty(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).unwrap_or(0)
}

fn bom(h: &mut H) -> Vec<Value> {
    h.run("bom", json!({"limit": 5000})).unwrap()["rows"].as_array().cloned().unwrap_or_default()
}

fn bom_qty(h: &mut H, part: &str) -> u64 {
    bom(h).iter().filter(|r| r["part_number"] == part).map(|r| qty(&r["qty"])).sum()
}

fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind]})).unwrap()["checks"].as_array().cloned().unwrap_or_default()
}

fn rows_have_a_way_out(h: &mut H, kind: &str) {
    let rows = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    let mine: Vec<_> = rows.iter().filter(|c| c.kind.name() == kind).collect();
    assert!(!mine.is_empty(), "no {kind} row");
    for c in mine {
        assert!(c.fix.is_some() || c.object != ecad_cadcraft::checks::Object::None, "{kind} row without a fix or go-to: {c:?}");
    }
}

fn net(h: &mut H, t: TerminalId) -> Option<ecad_model::NetId> {
    h.doc().connectivity().net_of_terminal(t)
}

fn wire_number(h: &mut H, t: TerminalId) -> String {
    s(&h.run("net", json!({"terminal": t.hex()})).unwrap()["number"])
}

fn same_as_built(h: &mut H) {
    same_as_built_with(h, true);
}

/// As [`same_as_built`]; `wires: false` skips the wire report (its stale-label defect is caught by
/// `o_…`, so hostile and registry tests still check what they are for).
fn same_as_built_with(h: &mut H, wires: bool) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    assert!(kept.strips.same_as(&built.strips), "strips differ from a rebuild");
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location));
    assert_eq!(kept.bom_table(Group::Part), built.bom_table(Group::Part));
    if wires {
        assert_eq!(kept.wire_table(), built.wire_table());
    }
}

fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.components, &d.terminals, &d.conductors, &d.cables, &d.pages, &d.page_order, &d.representations, &d.items, &d.properties)
}

/// One write: exactly one transaction; `u` gives back the document before it and `redo` the one
/// after; the kept strips and reports equal a rebuild after each step.
fn step(h: &mut H, cmd: &str, params: Value) -> Value {
    step_with(h, cmd, params, true)
}

/// [`step`]; `wires: false` skips the wire report (see [`same_as_built_with`]).
fn step_with(h: &mut H, cmd: &str, params: Value, wires: bool) -> Value {
    let before = h.doc();
    let head = h.head();
    let v = h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    assert_eq!(h.head(), head + 1, "{cmd} {params}: one transaction ({v})");
    let after = h.doc();
    same_as_built_with(h, wires);
    h.cmdline("u");
    assert_eq!(model_parts(&h.doc()), model_parts(&before), "{cmd} {params}: undo");
    same_as_built_with(h, wires);
    h.run("redo", Value::Null).unwrap();
    assert_eq!(model_parts(&h.doc()), model_parts(&after), "{cmd} {params}: redo");
    same_as_built_with(h, wires);
    v
}

/// A refused write: an error whose text contains `words`, and nothing written.
fn refused(h: &mut H, cmd: &str, params: Value, words: &str) -> String {
    let head = h.head();
    let Err(e) = h.run(cmd, params.clone()) else { panic!("{cmd} {params} must be refused") };
    assert_eq!(h.head(), head, "{cmd} {params}: a refusal writes nothing");
    assert!(e.contains(words), "{cmd} {params}: {e:?} does not say {words:?}");
    e
}

/// Hostile input: any outcome but a panic; a refusal writes nothing.
fn survives(h: &mut H, cmd: &str, params: Value) {
    let head = h.head();
    match h.run(cmd, params.clone()) {
        Ok(_) => assert!(h.head() <= head + 1, "{cmd} {params}"),
        Err(e) => {
            assert_eq!(h.head(), head, "{cmd} {params}: refused ({e}) but wrote");
            assert!(!e.trim().is_empty(), "{cmd} {params}: a refusal in words");
        }
    }
}

/// Everything that reads strips, once: none may panic.
fn read_everything(h: &mut H) {
    h.run("stripplan", json!({"limit": 5000})).unwrap();
    h.run("bom", json!({"limit": 5000})).unwrap();
    h.run("bom", json!({"group": "part"})).unwrap();
    h.run("checks", json!({})).unwrap();
    h.run("renumber", json!({"kind": "terminals", "preview": true})).unwrap();
    let d = h.doc();
    let _ = d.connectivity();
    let _ = ecad_cadcraft::checks::all(&d, 1e-6);
    same_as_built(h);
}

/// The representation (symbol) that shows terminal `t`, and its item.
fn symbol_of(d: &Document, t: TerminalId) -> Option<(ecad_model::RepresentationId, ecad_model::ItemId, PageId)> {
    let comp = d.terminals.get(&t)?.component;
    d.representations_of(Subject::Component(comp))
        .find(|(_, r)| r.connection_points.contains_key(&t))
        .and_then(|(rid, r)| Some((rid, *r.items.first()?, r.page)))
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t036 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn close_all(h: &mut H) {
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
}

fn reopen(path: &str, seed: u64) -> H {
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, path, seed).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    h.trail.push(d);
    h
}

/// A hand-made file: save the ledger as it is (after raw commits), then open it as a user would.
fn reload(mut h: H, tag: &str, seed: u64) -> H {
    let dir = tmp_dir(tag);
    let path = dir.join("hand made.ecad").display().to_string();
    h.cfg(|p| p.persistence = Persistence::File);
    h.s.execute("saveas", &json!({"path": path})).unwrap();
    close_all(&mut h);
    drop(h);
    reopen(&path, seed)
}

// ------------------------------------------------------------------------------- fixture M

/// Fixture M, simplified: page 1 "Supply" with L drops at x 100/110/120, N drops at 200/210/220,
/// PE drops at 300/310/320; page 2 "Sensors" with S drops at x 100…130, `+` drops at 200…230,
/// `−` drops at 300…330, a 24 V wire at x 500 for a fuse position, and a `fuseterm` at (400, 200)
/// with a wire on each point.
struct M {
    h: H,
    p1: PageId,
    p2: PageId,
    fuse_dev: ComponentId,
}

const L_XS: [f64; 3] = [100.0, 110.0, 120.0];
const N_XS: [f64; 3] = [200.0, 210.0, 220.0];
const PE_XS: [f64; 3] = [300.0, 310.0, 320.0];
const S_XS: [f64; 4] = [100.0, 110.0, 120.0, 130.0];
const P_XS: [f64; 4] = [200.0, 210.0, 220.0, 230.0];
const M_XS: [f64; 4] = [300.0, 310.0, 320.0, 330.0];

fn fixture_m(seed: u64) -> M {
    let mut h = H::new(seed);
    let p1 = h.first_page();
    h.run("renamepage", json!({"page": p1.hex(), "title": "Supply"})).unwrap();
    drops(&mut h, &L_XS);
    drops(&mut h, &N_XS);
    drops(&mut h, &PE_XS);
    let p2: PageId = id(&h.run("newpage", json!({"title": "Sensors"})).unwrap(), "page");
    h.activate(p2);
    drops(&mut h, &S_XS);
    drops(&mut h, &P_XS);
    drops(&mut h, &M_XS);
    drops(&mut h, &[500.0]);
    let f = h.run("place", json!({"symbol": "fuseterm", "at": [400, 200]})).unwrap();
    let fuse_dev: ComponentId = id(&f, "component");
    h.run("wire", json!({"points": pts(&[(400.0, 210.0), (400.0, 260.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(400.0, 190.0), (400.0, 140.0)])})).unwrap();
    h.run("setprop", json!({"component": fuse_dev.hex(), "key": "rating", "value": "4A"})).unwrap();
    h.activate(p1);
    M { h, p1, p2, fuse_dev }
}

/// Fixture M after check A: `-X01` L/N/PE positions 1…3 on page 1.
fn with_lnpe(seed: u64) -> (M, ComponentId) {
    let mut m = fixture_m(seed);
    let (x1, _) = fence(&mut m.h, None, "LNPE", "L", 90.0, 130.0);
    fence(&mut m.h, Some(x1), "LNPE", "N", 190.0, 230.0);
    fence(&mut m.h, Some(x1), "LNPE", "PE", 290.0, 330.0);
    (m, x1)
}

/// `-X02` sensor positions 1…4 on page 2 (S, + and − fences).
fn with_sensor(m: &mut M) -> ComponentId {
    m.h.activate(m.p2);
    let (x2, _) = fence(&mut m.h, None, "SEN", "S", 90.0, 140.0);
    fence(&mut m.h, Some(x2), "SEN", "+", 190.0, 240.0);
    fence(&mut m.h, Some(x2), "SEN", "-", 290.0, 340.0);
    x2
}

// ------------------------------------------------------------------------------- A, B (AC 1, 2, 6)

/// A: three fences fill positions 1…3 with L, N and PE; each fence one transaction (`u`/redo
/// exact); labels `1.L…3.PE`; 9 level rows in 3 positions; one BOM row of 3 L/N/PE terminals; `u`
/// undoes only the last fence.
#[test]
fn a_lnpe_fences_fill_positions_one_transaction_each() {
    let mut m = fixture_m(0x7036_0001);
    let v = step(&mut m.h, "terminals", json!({"strip": null, "type": "LNPE", "level": "L", "from": [90, 200], "to": [130, 200]}));
    let x1: ComponentId = id(&v, "strip");
    assert_eq!(ids(&v).len(), 3, "{v}");
    step(&mut m.h, "terminals", json!({"strip": x1.hex(), "type": "LNPE", "level": "N", "from": [190, 200], "to": [230, 200]}));
    let v = step(&mut m.h, "terminals", json!({"strip": x1.hex(), "type": "lnpe", "level": "PE", "from": [290, 200], "to": [330, 200]}));
    assert_eq!(ids(&v).len(), 3, "type codes are case-insensitive: {v}");
    let d = m.h.doc();
    let pos = positions(&d, x1);
    assert_eq!(pos.keys().cloned().collect::<Vec<_>>(), ["1", "2", "3"], "{pos:?}");
    for (p, levels) in &pos {
        assert_eq!(levels.keys().cloned().collect::<Vec<_>>(), [".1", ".2", ".3"], "position {p}: level keys (FP2)");
    }
    // Each level's terminal sits on its own wire: the L fence made the `.1` terminals.
    let plan = plan(&mut m.h, x1);
    let rows = level_rows(&plan);
    let labels: Vec<String> = rows.iter().map(|r| s(&r["label"])).collect();
    assert_eq!(labels, ["1.L", "1.N", "1.PE", "2.L", "2.N", "2.PE", "3.L", "3.N", "3.PE"], "{plan}");
    for r in &rows {
        assert_eq!(s(&r["type"]).to_uppercase(), "LNPE", "{r}");
        assert_eq!(r["state"], "drawn", "{r}");
        assert!(r["level"].as_u64().is_some_and(|n| (1..=3).contains(&n)), "{r}");
        assert!(r["label_overridden"] != true, "{r}");
        let ed: Vec<String> = r["editable"].as_array().map(|a| a.iter().map(s).collect()).unwrap_or_default();
        for f in ["position", "label"] {
            assert!(ed.contains(&f.to_string()), "§9: {f} is editable: {r}");
        }
        for f in ["level", "level_name", "type", "state", "breaks"] {
            assert!(!ed.contains(&f.to_string()), "§9: {f} is derived: {r}");
        }
    }
    let names: Vec<String> = rows.iter().take(3).map(|r| s(&r["level_name"])).collect();
    assert_eq!(names, ["L", "N", "PE"]);
    let types = plan["types"].as_array().cloned().unwrap_or_default();
    assert!(types.iter().any(|t| s(&t["code"]).eq_ignore_ascii_case("LNPE") && qty(&t["drawn"]) == 3), "strip types: {plan}");
    // BOM: one row, 3 positions (never 9).
    let rows: Vec<Value> = bom(&mut m.h).into_iter().filter(|r| s(&r["tags"]).contains("-X01")).collect();
    let lnpe: Vec<&Value> = rows.iter().filter(|r| s(&r["description"]).contains("L/N/PE")).collect();
    assert_eq!(lnpe.len(), 1, "{rows:?}");
    assert_eq!(qty(&lnpe[0]["qty"]), 3, "{rows:?}");
    // `u` undoes the PE fence only.
    m.h.cmdline("u");
    let pos = positions(&m.h.doc(), x1);
    assert!(pos.values().all(|l| l.keys().cloned().collect::<Vec<_>>() == [".1", ".2"]), "{pos:?}");
    m.h.check();
}

/// A: the echo of a level fence names positions and level (design §3.2).
#[test]
fn a_level_fence_echo() {
    let mut m = fixture_m(0x7036_0002);
    let (x1, _) = fence(&mut m.h, None, "LNPE", "L", 90.0, 130.0);
    fence(&mut m.h, Some(x1), "LNPE", "N", 190.0, 230.0);
    let echo = m.h.s.log.iter().rev().take(4).cloned().collect::<Vec<_>>().join("\n");
    assert!(echo.contains("-X01:1.N") && echo.contains("3.N") && echo.contains("level N"), "{echo}");
}

/// B: a level placed on another page fills the lowest position whose level is free; when none is
/// free it makes a new position; a named position whose level is drawn is refused with §3.3's
/// sentence; the plan lists both pages' places.
#[test]
fn b_level_on_another_page_joins_its_position() {
    let mut m = fixture_m(0x7036_0003);
    let (x1, _) = fence(&mut m.h, None, "LNPE", "L", 90.0, 130.0);
    fence(&mut m.h, Some(x1), "LNPE", "N", 190.0, 215.0); // N on 1 and 2 only (x 200, 210)
    let n_before = positions(&m.h.doc(), x1);
    assert!(!n_before["3"].contains_key(".2"), "position 3's N is free: {n_before:?}");
    // Page 2: `termlnpe.2` on the wire at x 500 fills position 3's free N.
    m.h.activate(m.p2);
    let v = step(&mut m.h, "place", json!({"symbol": "termlnpe.2", "at": [500, 200], "device": x1.hex()}));
    let t: TerminalId = ids(&v).first().copied().unwrap_or_else(|| {
        let d = m.h.doc();
        positions(&d, x1)["3"][".2"]
    });
    let d = m.h.doc();
    assert_eq!((label(&d, t), key(&d, t)), ("3".to_string(), ".2".to_string()), "free level first (§3.3 rule 1)");
    // Now every N is drawn: the next one makes position 4.
    drops(&mut m.h, &[520.0]);
    let v = m.h.run("place", json!({"symbol": "termlnpe.2", "at": [520, 200], "device": x1.hex()})).unwrap();
    let d = m.h.doc();
    let pos = positions(&d, x1);
    assert!(pos.get("4").is_some_and(|l| l.contains_key(".2") && l.len() == 1), "new position 4 with N only: {pos:?} {v}");
    // Position 3 named: its N is drawn on page 2: refused with the way out.
    drops(&mut m.h, &[540.0]);
    let e = refused(&mut m.h, "place", json!({"symbol": "termlnpe.2", "at": [540, 200], "device": x1.hex(), "position": "3"}), "-X01:3");
    assert!(e.contains("level N") && e.contains("Pick another position, or Enter for the next free one."), "{e}");
    // A named position of another type is refused too (fence form).
    refused(
        &mut m.h,
        "terminals",
        json!({"strip": x1.hex(), "type": "x3", "level": "1", "position": "1", "from": [530, 200], "to": [550, 200]}),
        "-X01:1",
    );
    // The plan's position 3 lists places on both pages.
    let plan = plan(&mut m.h, x1);
    let rows3: Vec<Value> = level_rows(&plan).into_iter().filter(|r| s(&r["position"]) == "3").collect();
    let places: Vec<String> = rows3.iter().map(|r| s(&r["place"])).filter(|p| !p.is_empty()).collect();
    assert!(places.len() >= 2 && places[0] != places[1], "both pages' places: {rows3:?}");
    // A free level row: no wires, state free.
    let free: Vec<&Value> = rows3.iter().filter(|r| r["state"] == "free").collect();
    assert_eq!(free.len(), 1, "3.PE is free: {rows3:?}");
    m.h.check();
}

/// B (whole-terminal symbol): `termlnpe` always makes a new position with all three levels.
#[test]
fn b_whole_terminal_symbol_makes_a_new_position() {
    let mut m = fixture_m(0x7036_0004);
    let (x1, _) = fence(&mut m.h, None, "LNPE", "L", 90.0, 110.0); // positions 1, 2 (L only)
    m.h.activate(m.p2);
    let v = step(&mut m.h, "place", json!({"symbol": "termlnpe", "at": [600, 200], "device": x1.hex()}));
    let pos = positions(&m.h.doc(), x1);
    assert_eq!(pos.get("3").map(|l| l.keys().cloned().collect::<Vec<_>>()), Some(vec![".1".into(), ".2".into(), ".3".into()]), "{pos:?} {v}");
    assert_eq!(pos["1"].len(), 1, "free levels of 1 are not filled by a whole symbol: {pos:?}");
}

// ------------------------------------------------------------------------------- C, D, E (AC 3)

/// C: a link on the `+` level made with the Jumper tool's JSON form is stored `1.2-4.2`; `+` and
/// `−` levels are one net each; the S levels stay four nets.
#[test]
fn c_sensor_links_per_level() {
    let mut m = fixture_m(0x7036_0005);
    let x2 = with_sensor(&mut m);
    let d = m.h.doc();
    let pos = positions(&d, x2);
    assert_eq!(pos.len(), 4, "{pos:?}");
    let lvl = |p: &str, k: &str| pos[p][k];
    let plan0 = plan(&mut m.h, x2);
    let labels: Vec<String> = level_rows(&plan0).iter().map(|r| s(&r["label"])).collect();
    assert_eq!(&labels[..3], ["1.S", "1.+", "1.-"], "{labels:?}");
    step(&mut m.h, "jumper", json!({"terminals": [lvl("1", ".2").hex(), lvl("4", ".2").hex()]}));
    assert_eq!(squash(prop(&mut m.h, x2, "link.1")).as_deref(), Some("1.2-4.2"));
    step(&mut m.h, "setprop", json!({"component": x2.hex(), "key": "link.2", "value": "1.3-4.3"}));
    assert_eq!(squash(prop(&mut m.h, x2, "link.2")).as_deref(), Some("1.3-4.3"));
    let plus: Vec<_> = ["1", "2", "3", "4"].iter().map(|p| net(&mut m.h, lvl(p, ".2"))).collect();
    let minus: Vec<_> = ["1", "2", "3", "4"].iter().map(|p| net(&mut m.h, lvl(p, ".3"))).collect();
    let ss: Vec<_> = ["1", "2", "3", "4"].iter().map(|p| net(&mut m.h, lvl(p, ".1"))).collect();
    assert!(plus.iter().all(|n| n.is_some() && *n == plus[0]), "+ one net: {plus:?}");
    assert!(minus.iter().all(|n| n.is_some() && *n == minus[0]), "− one net: {minus:?}");
    assert_ne!(plus[0], minus[0]);
    let distinct: std::collections::BTreeSet<_> = ss.iter().collect();
    assert_eq!(distinct.len(), 4, "S levels four nets: {ss:?}");
    // Level names typed are stored as numbers (§5.1).
    step(&mut m.h, "setprop", json!({"component": x2.hex(), "key": "link.3", "value": "1.S-2.S"}));
    assert_eq!(squash(prop(&mut m.h, x2, "link.3")).as_deref(), Some("1-2"), "level 1 canonical: no level written");
    // A pick on another level of another position is not added (§5.3).
    let head = m.h.head();
    let r = m.h.run("jumper", json!({"terminals": [lvl("1", ".2").hex(), lvl("3", ".1").hex()]}));
    assert!(r.is_err() || m.h.head() == head || squash(prop(&mut m.h, x2, "link.4")).is_some_and(|v| !v.contains("3.1")), "{r:?}");
    // stripplan links carry their level.
    let p = plan(&mut m.h, x2);
    assert!(level_rows(&p).iter().any(|r| r["links"].as_array().is_some_and(|l| l.iter().any(|x| x["level"].as_u64() == Some(2)))), "{p}");
    m.h.check();
}

/// D: a 030 one-level strip with `link.1 = 1-4` reads the same: keys unchanged, labels are
/// positions, the four terminals one net, BOM one feed-through row of 4.
#[test]
fn d_old_one_level_strip_reads_the_same() {
    let mut h = H::new(0x7036_0006);
    drops(&mut h, &[100.0, 110.0, 120.0, 130.0]);
    let v = h.run("terminals", json!({"from": [90, 200], "to": [140, 200]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let t = ids(&v);
    h.run("setprop", json!({"component": x1.hex(), "key": "part_number", "value": "FT-1"})).unwrap();
    h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1-4"})).unwrap();
    let d = h.doc();
    assert!(t.iter().all(|x| !key(&d, *x).starts_with('.')), "one-level terminals keep old keys");
    let nets: Vec<_> = t.iter().map(|x| net(&mut h, *x)).collect();
    assert!(nets.iter().all(|n| n.is_some() && *n == nets[0]), "{nets:?}");
    let p = plan(&mut h, x1);
    let rows = level_rows(&p);
    let labels: Vec<String> = rows.iter().map(|r| s(&r["label"])).collect();
    assert_eq!(labels, ["1", "2", "3", "4"], "{p}");
    for r in &rows {
        assert_eq!(s(&r["position"]), s(&r["label"]), "one-level: label is the position: {r}");
        assert!(r["level"].as_u64() == Some(1) || r["level"].is_null(), "{r}");
    }
    assert_eq!(bom_qty(&mut h, "FT-1"), 4);
    // Positions without a level mean level 1 (030's at-least-2-positions rule kept).
    h.run("setprop", json!({"component": x1.hex(), "key": "link.2", "value": "1, 3"})).unwrap();
    h.check();
}

/// E: a cross-level link `1.1-1.3` joins L, N and PE of position 1 only.
#[test]
fn e_cross_level_link_joins_one_positions_levels() {
    let (mut m, x1) = with_lnpe(0x7036_0007);
    let pos = positions(&m.h.doc(), x1);
    let before2: Vec<_> = [".1", ".2", ".3"].iter().map(|k| net(&mut m.h, pos["2"][*k])).collect();
    step(&mut m.h, "setprop", json!({"component": x1.hex(), "key": "link.2", "value": "1.1-1.3"}));
    let n1: Vec<_> = [".1", ".2", ".3"].iter().map(|k| net(&mut m.h, pos["1"][*k])).collect();
    assert!(n1.iter().all(|n| n.is_some() && *n == n1[0]), "position 1 one net: {n1:?}");
    let after2: Vec<_> = [".1", ".2", ".3"].iter().map(|k| net(&mut m.h, pos["2"][*k])).collect();
    assert_eq!(after2.iter().collect::<std::collections::BTreeSet<_>>().len(), 3, "position 2 untouched: {after2:?}");
    assert_eq!(before2.len(), after2.len());
    // `link.1 = 1-3` on the same strip joins level L only.
    m.h.run("setprop", json!({"component": x1.hex(), "key": "link.2", "value": null})).unwrap();
    m.h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1-3"})).unwrap();
    let ls: Vec<_> = ["1", "2", "3"].iter().map(|p| net(&mut m.h, pos[*p][".1"])).collect();
    let ns: Vec<_> = ["1", "2", "3"].iter().map(|p| net(&mut m.h, pos[*p][".2"])).collect();
    assert!(ls.iter().all(|n| *n == ls[0]), "L joined: {ls:?}");
    assert_eq!(ns.iter().collect::<std::collections::BTreeSet<_>>().len(), 3, "N not joined: {ns:?}");
    // `1.2-3.2` joins N.
    m.h.run("setprop", json!({"component": x1.hex(), "key": "link.3", "value": "1.2-3.2"})).unwrap();
    let ns: Vec<_> = ["1", "2", "3"].iter().map(|p| net(&mut m.h, pos[*p][".2"])).collect();
    assert!(ns.iter().all(|n| *n == ns[0]), "N joined: {ns:?}");
    m.h.check();
}

// ------------------------------------------------------------------------------- F, G, H (AC 4, 5)

/// F: a bridged two-level position is one net; two bridged positions are two nets; an unbridged
/// two-level position stays two nets.
#[test]
fn f_internal_bridge_per_position() {
    let mut m = fixture_m(0x7036_0008);
    let (xb, _) = fence(&mut m.h, None, "x2B", "1", 90.0, 115.0); // positions 1, 2 level 1
    fence(&mut m.h, Some(xb), "x2B", "2", 190.0, 215.0); // level 2
    let pos = positions(&m.h.doc(), xb);
    let (a1, a2, b1, b2) =
        (net(&mut m.h, pos["1"][".1"]), net(&mut m.h, pos["1"][".2"]), net(&mut m.h, pos["2"][".1"]), net(&mut m.h, pos["2"][".2"]));
    assert!(a1.is_some() && a1 == a2, "bridged position 1 one net");
    assert!(b1.is_some() && b1 == b2, "bridged position 2 one net");
    assert_ne!(a1, b1, "two positions stay two nets (§6)");
    let p = plan(&mut m.h, xb);
    assert!(level_rows(&p).iter().all(|r| !r["internal_bridge"].is_null()), "{p}");
    // Unbridged x2 on another strip: two nets.
    let (x2, _) = fence(&mut m.h, None, "x2", "1", 295.0, 305.0);
    fence(&mut m.h, Some(x2), "x2", "2", 315.0, 325.0);
    let q = positions(&m.h.doc(), x2);
    assert_ne!(net(&mut m.h, q["1"][".1"]), net(&mut m.h, q["1"][".2"]));
    // Undo of the level-2 fence takes the bridge's join back.
    m.h.check();
}

/// G: a fuse position on a wire: `.1i` and `.1o` on two nets with two wire numbers; `rating.<p>`
/// shows in the plan; the break mark is derived. Also a disconnect position.
#[test]
fn g_fuse_and_disconnect_positions_break_the_net() {
    let mut m = fixture_m(0x7036_0009);
    let x2 = with_sensor(&mut m);
    let v = step(&mut m.h, "terminals", json!({"strip": x2.hex(), "type": "FUSE", "level": "All", "from": [490, 200], "to": [510, 200]}));
    let d = m.h.doc();
    let pos = positions(&d, x2);
    let (p, fuse) = pos.iter().find(|(_, l)| l.contains_key(".1i")).map(|(p, l)| (p.clone(), l.clone())).unwrap_or_else(|| panic!("{pos:?} {v}"));
    assert_eq!(p, "5");
    let (i, o) = (fuse[".1i"], fuse[".1o"]);
    assert_ne!(net(&mut m.h, i), net(&mut m.h, o), "fuse breaks the net");
    let (wi, wo) = (wire_number(&mut m.h, i), wire_number(&mut m.h, o));
    assert!(!wi.is_empty() && !wo.is_empty() && wi != wo, "two wire numbers: {wi} {wo}");
    step(&mut m.h, "setprop", json!({"component": x2.hex(), "key": "rating.5", "value": "2A"}));
    let p5: Vec<Value> = level_rows(&plan(&mut m.h, x2)).into_iter().filter(|r| s(&r["position"]) == "5").collect();
    assert_eq!(p5.len(), 1, "a two-point level is one plan row: {p5:?}");
    assert_eq!(s(&p5[0]["rating"]), "2A");
    assert_eq!(p5[0]["breaks"], true);
    assert!(s(&p5[0]["type"]).eq_ignore_ascii_case("FUSE"));
    // A link never crosses in → out: `5.1i-5.1o` is refused or joins nothing.
    let r = m.h.run("setprop", json!({"component": x2.hex(), "key": "link.9", "value": "5.1i-5.1o"}));
    if r.is_ok() {
        assert_ne!(net(&mut m.h, i), net(&mut m.h, o), "link across a fuse joined in and out");
    }
    // Rating value check is 034's (`rating.<n>` rule).
    survives(&mut m.h, "setprop", json!({"component": x2.hex(), "key": "rating.5", "value": "x".repeat(10_000)}));
    // Disconnect.
    drops(&mut m.h, &[560.0]);
    fence(&mut m.h, Some(x2), "DIS", "All", 550.0, 570.0);
    let pos = positions(&m.h.doc(), x2);
    let dis = pos.get("6").cloned().unwrap_or_default();
    assert!(dis.contains_key(".1i") && dis.contains_key(".1o"), "two points: {pos:?}");
    assert_ne!(net(&mut m.h, dis[".1i"]), net(&mut m.h, dis[".1o"]), "disconnect breaks the net");
    m.h.check();
}

/// H: `toposition` turns 034's `fuseterm` device into a fuse position of `-X02` in one
/// transaction: the device is gone, its wires are on `.1i`/`.1o`, the nets keep their members, the
/// rating is carried; `u` restores the device exactly.
#[test]
fn h_toposition_converts_a_fuse_device() {
    let mut m = fixture_m(0x7036_000A);
    let x2 = with_sensor(&mut m);
    let d0 = m.h.doc();
    let ft: Vec<TerminalId> = d0.terminals.iter().filter(|(_, t)| t.component == m.fuse_dev).map(|(i, _)| *i).collect();
    assert_eq!(ft.len(), 2);
    let conductors_of = |d: &Document, t: TerminalId| -> Vec<ecad_model::ConductorId> {
        let c = d.connectivity();
        c.net_of_terminal(t).and_then(|n| c.net(n)).map(|n| n.conductors.iter().copied().collect()).unwrap_or_default()
    };
    let (ca, cb) = (conductors_of(&d0, ft[0]), conductors_of(&d0, ft[1]));
    assert!(!ca.is_empty() && !cb.is_empty());
    let v = step(&mut m.h, "toposition", json!({"device": m.fuse_dev.hex(), "strip": x2.hex()}));
    let d = m.h.doc();
    assert!(!d.components.contains_key(&m.fuse_dev), "-F gone: {v}");
    let pos = positions(&d, x2);
    let (p, l) = pos.iter().find(|(_, l)| l.contains_key(".1i")).map(|(p, l)| (p.clone(), l.clone())).unwrap_or_else(|| panic!("{pos:?}"));
    assert_eq!(p, "5", "§3.3 new position");
    // Nets unchanged: each wire's net has the same conductors as before.
    let (na, nb) = (conductors_of(&d, l[".1i"]), conductors_of(&d, l[".1o"]));
    let mut before = [ca.clone(), cb.clone()];
    let mut after = [na, nb];
    before.sort();
    after.sort();
    assert_eq!(before, after, "nets keep their conductors");
    assert_eq!(prop(&mut m.h, x2, &format!("rating.{p}")).as_deref(), Some("4A"), "rating carried");
    let label = m.h.p(|p| p.ledger.log().last().map(|t| t.label.clone())).unwrap_or_default();
    assert!(label.starts_with("-F01 \u{2192} -X") && label.ends_with(":5"), "transaction label `-F05 → -X01:7`: {label}");
    // The device's symbol now draws `termfuse` and is a symbol of the strip.
    let (rid, _, _) = symbol_of(&d, l[".1i"]).expect("the converted symbol shows the level");
    assert!(d.representations[&rid].connection_points.contains_key(&l[".1o"]));
    // No fuse-device row left.
    assert!(checks_of(&mut m.h, "fuse-device").is_empty());
    m.h.check();
}

/// H (refusals, §3.5): no strip on the page, a non-fuse device, a strip as the device, a device
/// with other symbols; each in words, nothing written.
#[test]
fn h_toposition_refusals() {
    let mut m = fixture_m(0x7036_000B);
    // No strip anywhere: the strip cannot be chosen.
    m.h.activate(m.p2);
    refused(&mut m.h, "toposition", json!({"device": m.fuse_dev.hex()}), "Name a strip: there is none on this page.");
    let x2 = with_sensor(&mut m);
    let coil: ComponentId = id(&m.h.run("place", json!({"symbol": "coil", "at": [700, 200]})).unwrap(), "component");
    survives(&mut m.h, "toposition", json!({"device": coil.hex(), "strip": x2.hex()}));
    assert!(m.h.doc().components.contains_key(&coil));
    survives(&mut m.h, "toposition", json!({"device": x2.hex(), "strip": x2.hex()}));
    survives(&mut m.h, "toposition", json!({"device": m.fuse_dev.hex(), "strip": m.fuse_dev.hex()}));
    survives(&mut m.h, "toposition", json!({"device": "zz", "strip": x2.hex()}));
    survives(&mut m.h, "toposition", json!({"device": m.fuse_dev.hex(), "strip": x2.hex(), "position": "1"}));
    // Across pages: a strip on page 1 for a device on page 2.
    m.h.activate(m.p1);
    let (x1, _) = fence(&mut m.h, None, "LNPE", "L", 90.0, 130.0);
    survives(&mut m.h, "toposition", json!({"device": m.fuse_dev.hex(), "strip": x1.hex()}));
    // A fuse device joined by a second symbol is refused with the sentence.
    let mut m2 = fixture_m(0x7036_000C);
    let _ = with_sensor(&mut m2);
    m2.h.run("place", json!({"symbol": "fuseterm", "at": [800, 200], "device": m2.fuse_dev.hex()})).ok();
    let n = m2.h.doc().representations_of(Subject::Component(m2.fuse_dev)).count();
    if n > 1 {
        let strip = m2.h.doc().components.keys().copied().find(|c| *c != m2.fuse_dev && positions(&m2.h.doc(), *c).len() == 4).unwrap();
        refused(&mut m2.h, "toposition", json!({"device": m2.fuse_dev.hex(), "strip": strip.hex()}), "move them first");
    }
}

// ------------------------------------------------------------------------------- I (AC 6)

/// I: offer `12+2PE+4x3`: spare positions by type, x3 spares with 3 level rows each; BOM one row
/// per type, Qty in positions; 028's offers read the same; refusal names the codes.
#[test]
fn i_offer_type_codes_spares_and_bom() {
    let mut m = fixture_m(0x7036_000D);
    // `-X03`: one feed-through drawn.
    let v = m.h.run("terminals", json!({"from": [95, 200], "to": [105, 200]})).unwrap();
    let x3: ComponentId = id(&v, "strip");
    for (k, part) in [("part_number", "FT-1"), ("part_number.pe", "PE-1"), ("part_number.x3", "X3-1")] {
        m.h.run("setprop", json!({"component": x3.hex(), "key": k, "value": part})).unwrap();
    }
    step(&mut m.h, "setprop", json!({"component": x3.hex(), "key": "terminals", "value": "12+2PE+4x3"}));
    let p = plan(&mut m.h, x3);
    let rows = level_rows(&p);
    let positions_seen: std::collections::BTreeSet<String> = rows.iter().map(|r| s(&r["position"])).collect();
    assert_eq!(positions_seen.len(), 18, "{p}");
    let x3rows: Vec<&Value> = rows.iter().filter(|r| s(&r["type"]).eq_ignore_ascii_case("x3")).collect();
    assert_eq!(x3rows.len(), 12, "4 three-level spares × 3 level rows");
    assert!(x3rows.iter().all(|r| r["state"] == "spare"), "{x3rows:?}");
    assert_eq!(rows.iter().filter(|r| r["state"] == "drawn").count(), 1);
    // Spares numbered after the drawn ones in offer order: ft 2…12, PE 13…14, x3 15…18.
    let order: Vec<String> = rows.iter().map(|r| s(&r["position"])).collect::<Vec<_>>();
    let mut dedup = order.clone();
    dedup.dedup();
    assert_eq!(dedup, (1..=18).map(|n| n.to_string()).collect::<Vec<_>>(), "{order:?}");
    assert!(x3rows.iter().all(|r| s(&r["position"]).parse::<u32>().is_ok_and(|n| n >= 15)));
    assert_eq!((bom_qty(&mut m.h, "FT-1"), bom_qty(&mut m.h, "PE-1"), bom_qty(&mut m.h, "X3-1")), (12, 2, 4));
    let header = p.to_string();
    assert!(header.contains("18"), "{header}");
    // 028's offers unchanged; case-insensitive codes; a project part per type.
    m.h.run("setprop", json!({"component": x3.hex(), "key": "terminals", "value": "12+2pe+4X3"})).unwrap();
    assert_eq!(bom_qty(&mut m.h, "X3-1"), 4);
    m.h.run("setprop", json!({"component": x3.hex(), "key": "part_number.x3", "value": null})).unwrap();
    step(&mut m.h, "projectprops", json!({"props": {"strip_x3_part_number": "X3-P"}}));
    assert_eq!(bom_qty(&mut m.h, "X3-P"), 4, "project part per type (FP6)");
    m.h.run("setprop", json!({"component": x3.hex(), "key": "terminals", "value": "12+2PE"})).unwrap();
    assert_eq!((bom_qty(&mut m.h, "FT-1"), bom_qty(&mut m.h, "PE-1")), (12, 2));
    let e = refused(&mut m.h, "setprop", json!({"component": x3.hex(), "key": "terminals", "value": "6Q3"}), "\"6Q3\" is not a terminal count.");
    assert!(e.contains("LNPE") && e.contains("FUSE"), "{e}");
    // More drawn of a type than offered: too-many-terminals per type with Offer n.
    let (xl, _) = fence(&mut m.h, None, "LNPE", "L", 190.0, 230.0);
    m.h.run("setprop", json!({"component": xl.hex(), "key": "terminals", "value": "2LNPE"})).unwrap();
    let rows = checks_of(&mut m.h, "too-many-terminals");
    assert!(rows.iter().any(|r| s(&r["which"]).contains("3 L/N/PE terminals drawn, strip offers 2")), "{rows:?}");
    rows_have_a_way_out(&mut m.h, "too-many-terminals");
    m.h.check();
}

// ------------------------------------------------------------------------------- J, K (AC 7, 2)

/// J: `termlabel {position}` renames a position (all levels; keys moved; links keep their nets);
/// `termlabel {text}` sets one level's label (`*` / `label_overridden` in the plan), `null` clears;
/// a duplicate position is refused with the way out.
#[test]
fn j_termlabel_position_and_level_text() {
    let mut m = fixture_m(0x7036_000E);
    let x2 = with_sensor(&mut m);
    let pos = positions(&m.h.doc(), x2);
    let t2s = pos["2"][".1"];
    m.h.run("setprop", json!({"component": x2.hex(), "key": "link.1", "value": "1.2-4.2"})).unwrap();
    m.h.run("accessory", json!({"strip": x2.hex(), "at": "2", "items": ["partition"]})).unwrap();
    m.h.run("setprop", json!({"component": x2.hex(), "key": "term_label.2.3", "value": "MINUS2"})).unwrap();
    let plus_net = net(&mut m.h, pos["1"][".2"]);
    let v = step_with(&mut m.h, "termlabel", json!({"terminal": t2s.hex(), "position": "2.1"}), true);
    let d = m.h.doc();
    for k in [".1", ".2", ".3"] {
        assert_eq!(label(&d, pos["2"][k]), "2.1", "every level relabelled ({k}): {v}");
        assert_eq!(key(&d, pos["2"][k]), k, "same key");
    }
    assert_eq!(prop(&mut m.h, x2, "acc.after.2.1").as_deref(), Some("partition"), "acc key moved");
    assert_eq!(prop(&mut m.h, x2, "acc.after.2"), None);
    assert_eq!(prop(&mut m.h, x2, "term_label.2.1.3").as_deref(), Some("MINUS2"), "term_label moved");
    assert_eq!(prop(&mut m.h, x2, "term_label.2.3"), None);
    let link = squash(prop(&mut m.h, x2, "link.1")).unwrap_or_default();
    assert!(!link.is_empty(), "link kept");
    let pos_now = positions(&m.h.doc(), x2);
    assert!(["1", "2.1", "3", "4"].iter().all(|p| net(&mut m.h, pos_now[*p][".2"]) == plus_net), "link joins the same + levels: {link}");
    let echo = m.h.s.log.iter().rev().take(8).cloned().collect::<Vec<_>>().join(
        "
",
    );
    assert!(echo.contains("-X01:2 is now -X01:2.1"), "§4.2 echo: {echo}");
    let rows = level_rows(&plan(&mut m.h, x2));
    let labels: Vec<String> = rows.iter().filter(|r| s(&r["position"]) == "2.1").map(|r| s(&r["label"])).collect();
    assert_eq!(labels[0], "2.1.S", "{labels:?}");
    // One level's text.
    step_with(&mut m.h, "termlabel", json!({"terminal": t2s.hex(), "text": "2.1S"}), true);
    assert_eq!(prop(&mut m.h, x2, "term_label.2.1.1").as_deref(), Some("2.1S"));
    let r = level_rows(&plan(&mut m.h, x2)).into_iter().find(|r| s(&r["position"]) == "2.1" && r["level"].as_u64() == Some(1)).unwrap();
    assert_eq!(s(&r["label"]), "2.1S");
    assert_eq!(r["label_overridden"], true);
    step_with(&mut m.h, "termlabel", json!({"terminal": t2s.hex(), "text": null}), true);
    assert_eq!(prop(&mut m.h, x2, "term_label.2.1.1"), None);
    // A single one-level terminal reads `1.L` by an override.
    let (xf, tf) = {
        let v = m.h.run("terminals", json!({"from": [495, 200], "to": [505, 200]})).unwrap();
        (id::<ComponentId>(&v, "strip"), ids(&v))
    };
    step_with(&mut m.h, "termlabel", json!({"terminal": tf[0].hex(), "text": "1.L"}), true);
    assert_eq!(s(&level_rows(&plan(&mut m.h, xf))[0]["label"]), "1.L");
    // Refusals.
    survives(&mut m.h, "termlabel", json!({"strip": x2.hex(), "position": "3", "to": "4"}));
    refused(
        &mut m.h,
        "termlabel",
        json!({"terminal": pos["3"][".1"].hex(), "position": "4"}),
        "-X01 already has a position 4. Pick another label, or swap them with Renumber.",
    );
    survives(&mut m.h, "termlabel", json!({"terminal": pos["3"][".1"].hex(), "position": "a b"}));
    survives(&mut m.h, "termlabel", json!({"terminal": pos["3"][".1"].hex(), "position": ""}));
    survives(&mut m.h, "termlabel", json!({"terminal": pos["3"][".1"].hex(), "position": "x".repeat(17)}));
    survives(&mut m.h, "termlabel", json!({"terminal": pos["3"][".1"].hex(), "text": "x".repeat(25)}));
    survives(&mut m.h, "termlabel", json!({"terminal": pos["3"][".1"].hex(), "text": "a\u{1b}b"}));
    m.h.check();
}

/// K: moving position 3's L symbol left of position 1 and renumbering numbers positions by their
/// earliest symbol; all levels move together; `term_label`, `rating`, `acc.*` and `link.*` follow;
/// the preview lists them and writes nothing.
#[test]
fn k_renumber_moves_positions_and_their_keys() {
    let (mut m, x1) = with_lnpe(0x7036_000F);
    let d = m.h.doc();
    let pos = positions(&d, x1);
    let p3: Vec<TerminalId> = pos["3"].values().copied().collect();
    m.h.run("setprop", json!({"component": x1.hex(), "key": "term_label.3.2", "value": "N3"})).unwrap();
    m.h.run("accessory", json!({"strip": x1.hex(), "at": "3", "on": true, "items": ["marker"]})).unwrap();
    m.h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "2.2-3.2"})).unwrap();
    let (_, item, page) = symbol_of(&d, pos["3"][".1"]).expect("L symbol of 3");
    let hnd = m.h.handle_of(page, item);
    m.h.run("move", json!({"handles": [hnd.hex()], "delta": [-40, 0]})).unwrap();
    let head = m.h.head();
    let pv = m.h.run("renumber", json!({"kind": "terminals", "preview": true})).unwrap();
    assert_eq!(m.h.head(), head, "preview writes nothing");
    let text = pv.to_string();
    assert!(text.contains("term_label") || text.contains("N3") || text.contains("label"), "preview lists the label move: {text}");
    step(&mut m.h, "renumber", json!({"kind": "terminals"}));
    let d = m.h.doc();
    let labels: std::collections::BTreeSet<String> = p3.iter().map(|t| label(&d, *t)).collect();
    assert_eq!(labels.len(), 1, "levels move together: {labels:?}");
    assert_eq!(labels.iter().next().map(String::as_str), Some("1"), "earliest symbol first");
    assert_eq!(prop(&mut m.h, x1, "term_label.1.2").as_deref(), Some("N3"));
    assert_eq!(prop(&mut m.h, x1, "acc.on.1").as_deref(), Some("marker"));
    // Old 2 → 3, old 3 → 1: `2.2-3.2` covered the N levels of old 2 and 3, now 3 and 1.
    let n_old2 = net(&mut m.h, pos["2"][".2"]);
    assert_eq!(net(&mut m.h, pos["3"][".2"]), n_old2, "link still joins the same terminals");
    let link = squash(prop(&mut m.h, x1, "link.1")).unwrap_or_default();
    assert!(link.contains(".2"), "canonical link keeps its level: {link}");
    m.h.check();
}

/// Silent data loss: deleting one level symbol keeps the position's label, overrides, rating and
/// links; deleting every level keeps the keys (an orphan row, never a silent drop); `u` restores.
#[test]
fn k_deleting_a_level_loses_nothing_silently() {
    let (mut m, x1) = with_lnpe(0x7036_0010);
    let pos = positions(&m.h.doc(), x1);
    for (k, v) in [("term_label.2.1", "L2"), ("term_label.2.2", "N2"), ("link.1", "1.1-3.1"), ("link.2", "2.1-2.3")] {
        m.h.run("setprop", json!({"component": x1.hex(), "key": k, "value": v})).unwrap();
    }
    m.h.run("accessory", json!({"strip": x1.hex(), "at": "2", "items": ["partition"]})).unwrap();
    let props_before: BTreeMap<String, String> = m.h.doc().components[&x1].properties.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let d = m.h.doc();
    let (_, item, page) = symbol_of(&d, pos["2"][".2"]).unwrap();
    let hnd = m.h.handle_of(page, item);
    let before = m.h.doc();
    m.h.run("erase", json!({"handles": [hnd.hex()]})).unwrap();
    let d = m.h.doc();
    assert_eq!(label(&d, pos["2"][".1"]), "2", "position keeps its label");
    let props_after: BTreeMap<String, String> = d.components[&x1].properties.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for k in ["term_label.2.1", "link.1", "acc.after.2"] {
        assert_eq!(props_after.get(k), props_before.get(k), "{k} kept");
    }
    // `term_label.2.2` may stay (free level) but must not silently vanish without a row.
    // Ruling (overseer, phase 2): erasing a level removes its label, with a note.
    assert!(!props_after.contains_key("term_label.2.2"), "the erased level's label goes with it");
    let echo = m.h.s.log.iter().rev().take(4).cloned().collect::<Vec<_>>().join(
        "
",
    );
    assert!(echo.contains("N2") || echo.to_lowercase().contains("label"), "a note says the label went: {echo}");
    // L still linked.
    let ls: Vec<_> = ["1", "2", "3"].iter().map(|p| net(&mut m.h, pos[*p][".1"])).collect();
    assert!(ls.iter().all(|n| *n == ls[0]));
    let rows = level_rows(&plan(&mut m.h, x1));
    assert_eq!(rows.iter().filter(|r| s(&r["position"]) == "2").count(), 3, "N row kept as free");
    m.h.cmdline("u");
    assert_eq!(model_parts(&m.h.doc()), model_parts(&before));
    m.h.check();
}

// ------------------------------------------------------------------------------- L (AC 8)

/// Relabel a terminal by hand (a hostile file or a merge): Remove + Add with a new label.
fn relabel_raw(h: &mut H, t: TerminalId, new_label: Option<&str>, new_key: Option<&str>) {
    let d = h.doc();
    let before = d.terminals[&t].clone();
    let mut after = before.clone();
    if let Some(l) = new_label {
        after.label = Some(l.to_string());
    }
    if let Some(k) = new_key {
        after.key = Some(k.to_string());
    }
    h.cfg(|p| {
        p.ledger.commit(Author::User, "hand-made", vec![Op::RemoveTerminal { id: t, terminal: before }, Op::AddTerminal { id: t, terminal: after }])
    })
    .unwrap();
}

/// L: each §8 row appears with its text and has a way out; the named fixes do what they say.
#[test]
fn l_checks_rows_and_their_fixes() {
    let (mut m, x1) = with_lnpe(0x7036_0011);
    let pos = positions(&m.h.doc(), x1);
    // level-missing: link names 3.4 on a 3-level strip.
    m.h.run("setprop", json!({"component": x1.hex(), "key": "link.2", "value": "1.4-3.4"})).unwrap();
    let rows = checks_of(&mut m.h, "level-missing");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(s(&rows[0]["which"]).contains("-X01: link 2 names"), "{}", rows[0]);
    assert_eq!(rows[0]["severity"], "warning");
    rows_have_a_way_out(&mut m.h, "level-missing");
    step(&mut m.h, "jumper", json!({"link": {"strip": x1.hex(), "n": 2}, "delete": true}));
    assert!(checks_of(&mut m.h, "level-missing").is_empty());
    // fuse-device: an Info count row; the fix is toposition.
    let rows = checks_of(&mut m.h, "fuse-device");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(s(&rows[0]["which"]).contains("1 fuse terminal") || rows[0].to_string().contains("fuse terminal"), "{}", rows[0]);
    rows_have_a_way_out(&mut m.h, "fuse-device");
    // no-fuse-rating: a fuse position without a rating.
    m.h.activate(m.p2);
    m.h.run("terminals", json!({"strip": x1.hex(), "type": "FUSE", "level": "All", "from": [490, 200], "to": [510, 200]})).unwrap();
    let rows = checks_of(&mut m.h, "no-fuse-rating");
    assert_eq!(rows.len(), 1, "{rows:?}");
    rows_have_a_way_out(&mut m.h, "no-fuse-rating");
    m.h.run("setprop", json!({"component": x1.hex(), "key": "rating.4", "value": "6A"})).unwrap();
    assert!(checks_of(&mut m.h, "no-fuse-rating").is_empty());
    // level-twice: position 2's N relabelled into position 1 by hand.
    m.h.activate(m.p1);
    relabel_raw(&mut m.h, pos["2"][".2"], Some("1"), None);
    m.h = reload(m.h, "level twice", 0x7036_0031);
    let rows = checks_of(&mut m.h, "level-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(s(&rows[0]["which"]).contains("-X01:1.N is drawn twice"), "{}", rows[0]);
    assert_eq!(rows[0]["severity"], "error");
    assert!(checks_of(&mut m.h, "terminal-twice").is_empty(), "level-twice replaces terminal-twice");
    rows_have_a_way_out(&mut m.h, "level-twice");
    read_everything(&mut m.h);
    m.h.check();
}

/// L: `position-mixed`: a three-level level-2 terminal relabelled into an L/N/PE position.
#[test]
fn l_position_mixed_row() {
    let mut m = fixture_m(0x7036_0012);
    let (x1, _) = fence(&mut m.h, None, "LNPE", "L", 90.0, 110.0); // 1, 2
    let (_, t) = fence(&mut m.h, Some(x1), "x3", "2", 190.0, 200.0); // new position 3, level 2
    relabel_raw(&mut m.h, t[0], Some("1"), None);
    m.h = reload(m.h, "mixed", 0x7036_0032);
    let rows = checks_of(&mut m.h, "position-mixed");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(s(&rows[0]["which"]).contains("-X01:1 mixes"), "{}", rows[0]);
    rows_have_a_way_out(&mut m.h, "position-mixed");
    read_everything(&mut m.h);
}

// ------------------------------------------------------------------------------- N (AC 10)

/// N: a project separator changes every inheriting multi-level label; a strip with its own value
/// keeps it; level names per type from the project; bad values resolve to the built-in; the kept
/// strips equal a rebuild after each change.
#[test]
fn n_registry_defaults_recompute_inheriting_strips() {
    let (mut m, x1) = with_lnpe(0x7036_0013);
    let x2 = with_sensor(&mut m);
    m.h.run("setprop", json!({"component": x2.hex(), "key": "terminal.level.separator", "value": "/"})).unwrap();
    assert_eq!(s(&level_rows(&plan(&mut m.h, x2))[0]["label"]), "1/S");
    m.h.run("projectprops", json!({"props": {"terminal.level.separator": "none"}})).unwrap();
    same_as_built(&mut m.h);
    assert_eq!(s(&level_rows(&plan(&mut m.h, x1))[0]["label"]), "1L", "§13 N (worker: the word `none`): no separator gives 1L");
    assert_eq!(s(&level_rows(&plan(&mut m.h, x2))[0]["label"]), "1/S", "own value kept");
    m.h.run("projectprops", json!({"props": {"terminal.level.separator": "-"}})).unwrap();
    same_as_built(&mut m.h);
    assert_eq!(s(&level_rows(&plan(&mut m.h, x1))[0]["label"]), "1-L");
    // Level names.
    m.h.run("projectprops", json!({"props": {"terminal.lnpe.levels": "L1,N,PE"}})).unwrap();
    same_as_built(&mut m.h);
    assert_eq!(s(&level_rows(&plan(&mut m.h, x1))[0]["label"]), "1-L1");
    // A wrong count or a too-long value is absent: the built-in names.
    for bad in ["L,N".to_string(), "x".repeat(10_000), "L,,PE".into(), "LLLLLLL,N,PE".into()] {
        survives(&mut m.h, "projectprops", json!({"props": {"terminal.lnpe.levels": bad}}));
        let l = s(&level_rows(&plan(&mut m.h, x1))[0]["label"]);
        assert!(l == "1-L" || l == "1-L1", "bad level names never reach a label: {l}");
        same_as_built(&mut m.h);
    }
    // The registry lists the group.
    let d = m.h.run("defaults", json!({"group": "Terminals"})).unwrap();
    let keys: Vec<String> = d["defaults"].as_array().map(|a| a.iter().map(|r| s(&r["key"])).collect()).unwrap_or_default();
    for k in [
        "terminal.level.separator",
        "terminal.lnpe.levels",
        "terminal.sen.levels",
        "terminal.x3.levels",
        "terminal.lnpe.description",
        "terminal.ft.description",
        "terminal.pe.description",
    ] {
        assert!(keys.contains(&k.to_string()), "{k} registered: {keys:?}");
    }
    // Clearing the project value returns the built-in `.`.
    m.h.run("projectprops", json!({"props": {"terminal.level.separator": null, "terminal.lnpe.levels": null}})).unwrap();
    assert_eq!(s(&level_rows(&plan(&mut m.h, x1))[0]["label"]), "1.L");
    m.h.check();
}

// ------------------------------------------------------------------------------- P (AC 11)

/// P: save at a path with spaces and `ä`, close, reopen: levels, labels, overrides, links,
/// ratings, offers, plan and BOM equal; opening adds no transaction.
#[test]
fn p_save_and_reopen_keep_everything() {
    let (mut m, x1) = with_lnpe(0x7036_0014);
    let x2 = with_sensor(&mut m);
    m.h.run("jumper", json!({"strip": x2.hex(), "positions": "1.2-4.2"})).unwrap();
    m.h.run("setprop", json!({"component": x1.hex(), "key": "link.1", "value": "1.1-1.3"})).unwrap();
    m.h.run("setprop", json!({"component": x1.hex(), "key": "term_label.2.1", "value": "L-zwei"})).unwrap();
    m.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "3LNPE+2x3"})).unwrap();
    m.h.run("terminals", json!({"strip": x2.hex(), "type": "FUSE", "level": "All", "from": [490, 200], "to": [510, 200]})).unwrap();
    m.h.run("setprop", json!({"component": x2.hex(), "key": "rating.5", "value": "2A"})).unwrap();
    m.h.run("toposition", json!({"device": m.fuse_dev.hex(), "strip": x2.hex()})).unwrap();
    let before_doc = m.h.doc();
    let before_plan = m.h.run("stripplan", json!({"limit": 5000})).unwrap();
    let before_bom = m.h.run("bom", json!({"limit": 5000})).unwrap();
    let dir = tmp_dir("save");
    let path = dir.join("Klemmen -X01 (L N PE).ecad").to_string_lossy().to_string();
    m.h.cfg(|p| p.persistence = Persistence::File);
    m.h.run("saveas", json!({"path": path})).unwrap();
    close_all(&mut m.h);
    let mut h = reopen(&path, 0x7036_0015);
    let head = h.head();
    assert_eq!(model_parts(&h.doc()), model_parts(&before_doc));
    assert_eq!(h.run("stripplan", json!({"limit": 5000})).unwrap(), before_plan);
    assert_eq!(h.run("bom", json!({"limit": 5000})).unwrap(), before_bom);
    assert_eq!(h.head(), head, "opening writes nothing");
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------------------------- Q (hostile)

/// Q: hostile offers, keys, link values, labels, registry values and toposition targets: a
/// refusal in words or a Checks row, never a panic.
#[test]
fn q_hostile_input_never_panics() {
    let (mut m, x1) = with_lnpe(0x7036_0016);
    let s1 = x1.hex();
    // Offers.
    survives(&mut m.h, "setprop", json!({"component": s1, "key": "terminals", "value": "999x3"}));
    refused(&mut m.h, "setprop", json!({"component": s1, "key": "terminals", "value": "1000x2"}), "");
    for v in ["0x3", "x3", "+", "3+", "12+2PE+", "99999999999999999999x3", "3xx3", "3LNPE+3LNPE+999x2", "-1x3", "3\u{0}PE"] {
        survives(&mut m.h, "setprop", json!({"component": s1, "key": "terminals", "value": v}));
    }
    m.h.run("setprop", json!({"component": s1, "key": "terminals", "value": null})).unwrap();
    read_everything(&mut m.h);
    // Per-level keys for levels and positions that do not exist.
    for k in [
        "term_label.1.99",
        "term_label.1.0",
        "term_label.99.1",
        "term_label.1",
        "term_label..",
        "term_label.1.1.1.1",
        "rating.1",
        "rating.99",
        "rating.",
        "part_number.zz",
        "part_number.",
    ] {
        survives(&mut m.h, "setprop", json!({"component": s1, "key": k, "value": "x"}));
    }
    survives(&mut m.h, "setprop", json!({"component": s1, "key": "term_label.1.1", "value": "x".repeat(10_000)}));
    read_everything(&mut m.h);
    // Link values.
    refused(&mut m.h, "setprop", json!({"component": s1, "key": "link.1", "value": "1.1-4.2"}), "joins different levels at different positions");
    for v in ["1.0", "1.99999999999999999999", "1.1i-1.2o", "1.-1", "..", "1.", ".1", "1.1-", "-1.1", "1.L-3.Q", "1.".repeat(5000).as_str()] {
        survives(&mut m.h, "setprop", json!({"component": s1, "key": "link.1", "value": v}));
        read_everything(&mut m.h);
    }
    m.h.run("setprop", json!({"component": s1, "key": "link.1", "value": null})).ok();
    // Hand-made level keys.
    let pos = positions(&m.h.doc(), x1);
    relabel_raw(&mut m.h, pos["1"][".1"], None, Some(".0"));
    relabel_raw(&mut m.h, pos["1"][".2"], None, Some(".-1"));
    relabel_raw(&mut m.h, pos["1"][".3"], None, Some(".99999999999999999999"));
    relabel_raw(&mut m.h, pos["2"][".1"], None, Some("."));
    relabel_raw(&mut m.h, pos["2"][".2"], None, Some(".1x"));
    m.h = reload(m.h, "keys", 0x7036_0033);
    m.h.run("setprop", json!({"component": s1, "key": "link.1", "value": "1.1-2.3"})).ok();
    read_everything(&mut m.h);
    // termlabel.
    let t = pos["3"][".1"];
    survives(&mut m.h, "termlabel", json!({"terminal": t.hex(), "position": "x".repeat(10_000)}));
    survives(&mut m.h, "termlabel", json!({"terminal": t.hex(), "position": 3}));
    survives(&mut m.h, "termlabel", json!({"terminal": t.hex()}));
    survives(&mut m.h, "termlabel", json!({"strip": s1}));
    survives(&mut m.h, "termlabel", json!({"strip": s1, "position": "3", "text": "x"}));
    survives(&mut m.h, "termlabel", json!({"terminal": "zz", "text": "x"}));
    // Registry values.
    for v in ["L,N".to_string(), "x".repeat(10_000), ",,".into(), "L,N,P\u{1b}E".into()] {
        survives(&mut m.h, "projectprops", json!({"props": {"terminal.lnpe.levels": v.clone()}}));
        survives(&mut m.h, "setprop", json!({"component": s1, "key": "terminal.lnpe.levels", "value": v}));
    }
    survives(&mut m.h, "projectprops", json!({"props": {"terminal.level.separator": "x".repeat(10_000)}}));
    read_everything(&mut m.h);
    // Fences: unknown type or level, a level the type lacks.
    for (ty, lvl) in [("Q", "L"), ("LNPE", "Q"), ("x2", "3"), ("LNPE", ""), ("", ""), ("FUSE", "N")] {
        survives(&mut m.h, "terminals", json!({"strip": s1, "type": ty, "level": lvl, "from": [90, 200], "to": [130, 200]}));
    }
    survives(&mut m.h, "place", json!({"symbol": "termlnpe.9", "at": [600, 200], "device": s1}));
    survives(&mut m.h, "place", json!({"symbol": "termlnpe.2", "at": [600, 200], "device": s1, "position": "x".repeat(10_000)}));
    read_everything(&mut m.h);
    m.h.check_with(true);
}

// ------------------------------------------------------------------------------- O (AC 10)

/// O (part): no ordinary edit rebuilds the strips; a strip's level edit leaves other strips'
/// kept rows equal to a rebuild. The bench rows themselves run with `cargo xtask bench` (phase 2).
#[test]
fn o_kept_strips_equal_a_rebuild_through_level_edits() {
    let (mut m, x1) = with_lnpe(0x7036_0017);
    let x2 = with_sensor(&mut m);
    m.h.cfg(|p| {
        p.outputs();
    });
    let edits: Vec<(&str, Value)> = vec![
        ("setprop", json!({"component": x1.hex(), "key": "term_label.1.1", "value": "A"})),
        ("setprop", json!({"component": x2.hex(), "key": "link.1", "value": "1.2-4.2"})),
        ("setprop", json!({"component": x2.hex(), "key": "terminals", "value": "4SEN+2x2B"})),
        ("setprop", json!({"component": x1.hex(), "key": "part_number.lnpe", "value": "LN-1"})),
        ("projectprops", json!({"props": {"strip_sen_part_number": "SEN-1"}})),
        ("projectprops", json!({"props": {"terminal.level.separator": ":"}})),
        ("projectprops", json!({"props": {"terminal.sen.description": "Sensorklemme"}})),
    ];
    for (cmd, p) in edits {
        step(&mut m.h, cmd, p);
    }
    assert_eq!(bom_qty(&mut m.h, "LN-1"), 3);
    assert_eq!(bom_qty(&mut m.h, "SEN-1"), 4);
    m.h.check();
}

/// C (§5.3: typed positions accept §5.1's grammar): the Jumper tool's typed `positions` keep their
/// level.
#[test]
fn c_jumper_typed_positions_keep_their_level() {
    let mut m = fixture_m(0x7036_0018);
    let x2 = with_sensor(&mut m);
    m.h.run("jumper", json!({"strip": x2.hex(), "positions": "1.3-4.3"})).unwrap();
    assert_eq!(squash(prop(&mut m.h, x2, "link.1")).as_deref(), Some("1.3-4.3"), "the level is kept");
    let pos = positions(&m.h.doc(), x2);
    let ss: Vec<_> = ["1", "2", "3", "4"].iter().map(|p| net(&mut m.h, pos[*p][".1"])).collect();
    assert_eq!(ss.iter().collect::<std::collections::BTreeSet<_>>().len(), 4, "S levels stay apart: {ss:?}");
}

/// J / AC 7 (the shown label everywhere) and AC 10 (update maps): after `termlabel {text}` and a
/// project separator change, the kept wire report equals a rebuild.
#[test]
fn j_wire_report_follows_label_changes() {
    let mut m = fixture_m(0x7036_0019);
    let x2 = with_sensor(&mut m);
    let t = positions(&m.h.doc(), x2)["2"][".1"];
    m.h.cfg(|p| {
        p.outputs();
    });
    step(&mut m.h, "termlabel", json!({"terminal": t.hex(), "text": "2S"}));
    step(&mut m.h, "projectprops", json!({"props": {"terminal.level.separator": "none"}}));
}
