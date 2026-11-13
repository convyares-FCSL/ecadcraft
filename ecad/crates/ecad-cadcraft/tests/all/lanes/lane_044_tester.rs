//! Tester 044 (connector devices): acceptance checks from outside, written from brief 044 and
//! design 044 (§12's table, rows 1–10 and 12–14, and the overseer's FP1–FP6 conditions) only,
//! before the implementation existed. The checks drive JSON commands (`connector`, `pinlabel`,
//! `place`, `setprop`, `setdefault`, `bom`, `wirelist`, `xrefs`, `refgoto`, `checks`, `stripplan`)
//! and read the document, so they depend on as little of the new code's Rust API as possible.
//!
//! Fixture C (design §12), simplified: page 100 with three vertical wires (the sensor's 24 V, 0 V
//! and signal) at x 100/110/120 from y 250 to y 150; page 101 with a location box `+FIELD`;
//! page 102 with a location box `+CP1`. The supply and the sensor `-B01` are left out: the checks
//! look only at the wires, the pins and the nets.
//!
//! Each test name starts with the §12 row (`r01`…`r14`) or the focus item it checks.

use std::collections::BTreeMap;

use cadcraft_engine::Session;
use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{Author, ComponentId, ConductorId, DefinitionId, Document, Op, PageId, Pt, RepresentationId, Subject, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

// ------------------------------------------------------------------------------- helpers

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

/// The connector a `connector` result names (design §4.3: "the connector's ID and tag").
fn conn_of(v: &Value) -> ComponentId {
    for k in ["component", "connector", "device"] {
        if v[k].is_string() {
            return id(v, k);
        }
    }
    panic!("no connector ID in {v}")
}

/// A connector's terminals by (side key, pin label).
fn pins(d: &Document, c: ComponentId) -> BTreeMap<(String, String), TerminalId> {
    d.terminals_of(c).map(|(t, x)| ((x.key.clone().unwrap_or_default(), x.label.clone().unwrap_or_default()), t)).collect()
}

fn pin(d: &Document, c: ComponentId, side: &str, label: &str) -> TerminalId {
    pins(d, c).get(&(side.to_string(), label.to_string())).copied().unwrap_or_else(|| panic!("{side}:{label} missing: {:?}", pins(d, c)))
}

/// The pin labels drawn on one side, in natural order.
fn side_labels(d: &Document, c: ComponentId, side: &str) -> Vec<String> {
    let mut v: Vec<String> = pins(d, c).keys().filter(|(k, _)| k == side).map(|(_, l)| l.clone()).collect();
    v.sort_by_key(|l| (l.parse::<u64>().unwrap_or(u64::MAX), l.clone()));
    v
}

fn tag(d: &Document, c: ComponentId) -> String {
    ecad_cadcraft::devices::tag_words(d, c)
}

fn name(h: &mut H, t: TerminalId) -> String {
    let d = h.doc();
    ecad_cadcraft::connectors::pin_name(&d, t).unwrap_or_else(|| ecad_cadcraft::interact::terminal_name(&d, t))
}

fn net(h: &mut H, t: TerminalId) -> Option<ecad_model::NetId> {
    h.doc().connectivity().net_of_terminal(t)
}

fn wire_number(h: &mut H, t: TerminalId) -> String {
    s(&h.run("net", json!({"terminal": t.hex()})).unwrap()["number"])
}

fn newpage(h: &mut H, number: &str, title: &str) -> PageId {
    let p: PageId = id(&h.run("newpage", json!({"title": title, "number": number})).unwrap(), "page");
    h.activate(p);
    p
}

/// Vertical wires at `xs`, from y 250 to y 150, on the active page.
fn drops(h: &mut H, xs: &[f64]) -> Vec<ConductorId> {
    xs.iter().map(|x| id(&h.run("wire", json!({"points": pts(&[(*x, 250.0), (*x, 150.0)])})).unwrap(), "conductor")).collect()
}

/// Where `t` is shown on `page`, if it is.
fn shown_at(d: &Document, page: PageId, t: TerminalId) -> Option<Pt> {
    ecad_model::derive::terminal_positions(d, page).into_iter().find(|(x, _, _)| *x == t).map(|(_, _, p)| p)
}

/// A wire from pin `t` (on `page`) 20 mm away from its half's body: up from a socket pin, down
/// from a plug pin, unless the mate on the same page says the halves are swapped.
fn wire_out(h: &mut H, page: PageId, c: ComponentId, t: TerminalId) -> ConductorId {
    h.activate(page);
    let d = h.doc();
    let at = shown_at(&d, page, t).unwrap_or_else(|| panic!("{} not on that page", name(h, t)));
    let term = &d.terminals[&t];
    let side = term.key.clone().unwrap_or_default();
    let other = if side == ".s" { ".p" } else { ".s" };
    let mate = pins(&d, c).get(&(other.to_string(), term.label.clone().unwrap_or_default())).and_then(|m| shown_at(&d, page, *m));
    let dir = match mate {
        Some(m) if (at.y - m.y).abs() > 1e-6 => (at.y - m.y).signum(),
        _ if side == ".s" => 1.0,
        _ => -1.0,
    };
    let v = h.run("wire", json!({"points": pts(&[(at.x, at.y), (at.x, at.y + 20.0 * dir)])})).unwrap();
    let w: ConductorId = id(&v, "conductor");
    assert!(h.doc().conductors[&w].ends.contains(&Some(t)), "the wire lands on {}", name(h, t));
    w
}

fn last_ops(h: &mut H) -> Vec<Op> {
    h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default())
}

fn bom(h: &mut H) -> Vec<Value> {
    h.run("bom", json!({"limit": 5000})).unwrap()["rows"].as_array().cloned().unwrap_or_default()
}

/// BOM rows whose tag column names exactly `t` (a half tag, or a device tag).
fn bom_rows(h: &mut H, t: &str) -> Vec<Value> {
    bom(h).into_iter().filter(|r| s(&r["tags"]).split([',', ' ']).any(|x| x.trim() == t)).collect()
}

fn wirelist(h: &mut H) -> Vec<Value> {
    h.run("wirelist", json!({"limit": 5000})).unwrap()["rows"].as_array().cloned().unwrap_or_default()
}

fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind]})).unwrap()["checks"].as_array().cloned().unwrap_or_default()
}

/// Every row of `kind` has a go-to (an object) and, where `fix` says so, a fix.
/// A Checks row's words: its kind's title (`text`) and what it names (`which`).
fn row_words(r: &Value) -> String {
    format!("{} {}", s(&r["text"]), s(&r["which"]))
}

fn rows_have(h: &mut H, kind: &str, fix: bool) {
    let rows = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    let mine: Vec<_> = rows.iter().filter(|c| c.kind.name() == kind).collect();
    assert!(!mine.is_empty(), "no {kind} row");
    for c in mine {
        assert!(c.object != ecad_cadcraft::checks::Object::None, "{kind} row without a go-to: {c:?}");
        if fix {
            assert!(c.fix.is_some(), "{kind} row without its fix: {c:?}");
        }
    }
}

fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location), "kept BOM differs from a rebuild");
    assert_eq!(kept.bom_table(Group::Part), built.bom_table(Group::Part), "kept BOM by part differs from a rebuild");
    assert_eq!(kept.wire_table(), built.wire_table(), "kept wire list differs from a rebuild");
    assert!(kept.strips.same_as(&built.strips), "kept strips differ from a rebuild");
}

fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.components, &d.terminals, &d.conductors, &d.cables, &d.pages, &d.page_order, &d.representations, &d.items, &d.properties, &d.definitions)
}

/// One write: exactly one transaction; `u` gives back the document before it and `redo` the one
/// after; the kept reports equal a rebuild after each step.
fn step(h: &mut H, cmd: &str, params: Value) -> Value {
    let before = h.doc();
    let head = h.head();
    let v = h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    assert_eq!(h.head(), head + 1, "{cmd} {params}: one transaction ({v})");
    let after = h.doc();
    same_as_built(h);
    h.cmdline("u");
    assert_eq!(model_parts(&h.doc()), model_parts(&before), "{cmd} {params}: undo");
    same_as_built(h);
    h.run("redo", Value::Null).unwrap();
    assert_eq!(model_parts(&h.doc()), model_parts(&after), "{cmd} {params}: redo");
    same_as_built(h);
    v
}

/// A refused write: an error whose text contains each of `words`, and nothing written.
fn refused(h: &mut H, cmd: &str, params: Value, words: &[&str]) -> String {
    let head = h.head();
    let Err(e) = h.run(cmd, params.clone()) else { panic!("{cmd} {params} must be refused") };
    assert_eq!(h.head(), head, "{cmd} {params}: a refusal writes nothing");
    for w in words {
        assert!(e.contains(w), "{cmd} {params}: {e:?} does not say {w:?}");
    }
    e
}

/// Hostile input: any outcome but a panic; a refusal writes nothing and is in words.
fn survives(h: &mut H, cmd: &str, params: Value) {
    let head = h.head();
    match h.run(cmd, params.clone()) {
        Ok(_) => assert!(h.head() <= head + 1, "{cmd}"),
        Err(e) => {
            assert_eq!(h.head(), head, "{cmd}: refused ({e}) but wrote");
            assert!(!e.trim().is_empty(), "{cmd}: a refusal in words");
        }
    }
}

/// Everything that reads connectors, once: none may panic, and the kept views equal a rebuild.
fn read_everything(h: &mut H) {
    h.run("bom", json!({"limit": 5000})).unwrap();
    h.run("bom", json!({"group": "part"})).unwrap();
    h.run("wirelist", json!({"limit": 5000})).unwrap();
    h.run("stripplan", json!({"limit": 5000})).unwrap();
    h.run("xrefs", json!({"limit": 2000})).unwrap();
    h.run("checks", json!({})).unwrap();
    h.run("devices", json!({})).unwrap();
    h.run("renumber", json!({"kind": "terminals", "preview": true})).unwrap();
    let d = h.doc();
    let _ = d.connectivity();
    let _ = ecad_cadcraft::checks::all(&d, 1e-6);
    for t in d.terminals.keys() {
        let _ = ecad_cadcraft::interact::terminal_name(&d, *t);
        let _ = ecad_cadcraft::terminal_types::shown_label(&d, *t);
    }
    same_as_built(h);
}

/// The symbols (representations) of `c` on `page`.
fn reps_on(d: &Document, c: ComponentId, page: PageId) -> Vec<RepresentationId> {
    d.representations_of(Subject::Component(c)).filter(|(_, r)| r.page == page).map(|(rid, _)| rid).collect()
}

/// The representation showing `t`.
fn rep_of(d: &Document, t: TerminalId) -> RepresentationId {
    let c = d.terminals[&t].component;
    d.representations_of(Subject::Component(c)).find(|(_, r)| r.connection_points.contains_key(&t)).map(|(rid, _)| rid).expect("shown")
}

fn page_no(d: &Document, p: PageId) -> String {
    ecad_model::book::page_number(d, p)
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t044 {tag} Steckverbinder \u{e4} {}", ecad_store::disk::unique()));
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

/// Save the ledger as it is (also after raw commits) and open it as a user would.
fn reload(mut h: H, tag: &str, seed: u64) -> (H, String) {
    let dir = tmp_dir(tag);
    let path = dir.join("Stecker -X01 (S P).ecad").display().to_string();
    h.cfg(|p| p.persistence = Persistence::File);
    h.s.execute("saveas", &json!({"path": path})).unwrap();
    close_all(&mut h);
    drop(h);
    (reopen(&path, seed), path)
}

// ------------------------------------------------------------------------------- fixture C

struct C {
    h: H,
    p100: PageId,
    p101: PageId,
    p102: PageId,
    /// The three wires on page 100 (24 V, 0 V, signal).
    w: Vec<ConductorId>,
}

const XS: [f64; 3] = [100.0, 110.0, 120.0];

fn fixture_c(seed: u64) -> C {
    let mut h = H::new(seed);
    let p100 = newpage(&mut h, "100", "Sensor");
    let w = drops(&mut h, &XS);
    let p101 = newpage(&mut h, "101", "Field");
    h.run("locationbox", json!({"location": "+FIELD", "corners": [[5, 5], [400, 280]], "page": p101.hex()})).unwrap();
    let p102 = newpage(&mut h, "102", "Cabinet");
    h.run("locationbox", json!({"location": "+CP1", "corners": [[5, 5], [400, 280]], "page": p102.hex()})).unwrap();
    h.activate(p101);
    C { h, p100, p101, p102, w }
}

/// Row 1's `-X01`: a plain 4-pin pair as rows on page 101.
fn with_x01(c: &mut C) -> ComponentId {
    c.h.activate(c.p101);
    let v = c.h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [100, 100], "page": c.p101.hex()}));
    conn_of(&v.unwrap_or_else(|e| panic!("connector row: {e}")))
}

/// Row 3's `-X02`: M12 4-pin pin pairs across page 100's three wires.
fn with_x02(c: &mut C) -> (ComponentId, Value) {
    c.h.activate(c.p100);
    let v = c
        .h
        .run("connector", json!({"halves": "both", "family": "m12", "pins": 4, "names": "numbers", "layout": "pins", "line": [[90, 200], [130, 200]], "page": c.p100.hex()}))
        .unwrap_or_else(|e| panic!("connector pins: {e}"));
    (conn_of(&v), v)
}

// ------------------------------------------------------------------------------- row 1 (AC 1)

/// Row 1: one device `-X01`, halves `-X01.S:1…4` and `-X01.P:1…4` (keys `.s`/`.p`, labels 1–4);
/// one transaction with one `EmbedDefinition` each for `socket4` and `plug4` at version 2 with
/// the bridge `[".s", ".p"]`; explicit connection points; `u` removes it all, redo puts it back.
#[test]
fn r01_row_pair_is_one_device_with_two_halves_one_transaction() {
    let mut c = fixture_c(0x7044_0001);
    let before_defs = c.h.doc().definitions.len();
    let v =
        step(&mut c.h, "connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [100, 100], "page": c.p101.hex()}));
    let x = conn_of(&v);
    let d = c.h.doc();
    assert_eq!(tag(&d, x), "-X01", "{v}");
    assert_eq!(side_labels(&d, x, ".s"), ["1", "2", "3", "4"]);
    assert_eq!(side_labels(&d, x, ".p"), ["1", "2", "3", "4"]);
    assert_eq!(d.terminals_of(x).count(), 8, "one connection point per pin per half");
    // No other X device: one connector, not two half devices.
    let xs: Vec<ComponentId> = d.components.keys().copied().filter(|k| tag(&d, *k).starts_with("-X")).collect();
    assert_eq!(xs, [x], "one device for both halves");
    for t in pins(&d, x).values() {
        let n = name(&mut c.h, *t);
        let term = &d.terminals[t];
        let side = if term.key.as_deref() == Some(".s") { "S" } else { "P" };
        assert_eq!(n, format!("-X01.{side}:{}", term.label.clone().unwrap_or_default()), "pin name (§5.1)");
    }
    // The ops of §4.6.
    let ops = last_ops(&mut c.h);
    let embeds: Vec<&ecad_model::Definition> =
        ops.iter().filter_map(|o| if let Op::EmbedDefinition { definition, .. } = o { Some(definition) } else { None }).collect();
    assert_eq!(embeds.len(), 2, "one EmbedDefinition per new half definition: {ops:?}");
    for (k, def) in ["socket4", "plug4"].map(|k| (k, ecad_cadcraft::symbols::by_key(k).expect(k))) {
        let built = def.definition().expect(k);
        let o = built.origin.as_ref().expect("starter origin");
        assert_eq!(o.library_seq, 2, "{k}: version 2 (FP3)");
        assert_eq!(built.bridges, vec![vec![".s".to_string(), ".p".to_string()]], "{k}: bridge (FP2)");
        assert!(
            embeds.iter().any(|e| e.origin.as_ref().is_some_and(|eo| eo.definition == o.definition && eo.library_seq == 2)),
            "{k} embedded at v2"
        );
    }
    assert_eq!(c.h.doc().definitions.len(), before_defs + 2);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::CreateComponent { .. })).count(), 1, "one CreateComponent");
    let adds: Vec<(Option<String>, Option<String>)> = ops
        .iter()
        .filter_map(|o| if let Op::AddTerminal { terminal, .. } = o { Some((terminal.key.clone(), terminal.label.clone())) } else { None })
        .collect();
    assert_eq!(adds.len(), 8, "{adds:?}");
    assert!(adds.iter().all(|(k, _)| matches!(k.as_deref(), Some(".s" | ".p"))), "{adds:?}");
    let links: Vec<&ecad_model::Representation> =
        ops.iter().filter_map(|o| if let Op::LinkRepresentation { rep, .. } = o { Some(rep) } else { None }).collect();
    assert_eq!(links.len(), 2, "one symbol per half");
    assert!(links.iter().all(|r| r.connection_points.len() == 4), "explicit connection points, also for the first half: {links:?}");
    // Echo (§4.2).
    let echo = c.h.s.log.iter().rev().take(4).cloned().collect::<Vec<_>>().join("\n");
    assert!(echo.contains("Placed -X01") && echo.contains("-X01.S:1") && echo.contains("-X01.P:1"), "{echo}");
    c.h.check();
}

/// Row 1, `swap` and `rotation`: the pair still has both halves with the same pins.
#[test]
fn r01_row_swap_and_rotation_keep_the_halves() {
    let mut c = fixture_c(0x7044_0002);
    for (i, (rot, swap)) in [(90, false), (0, true), (270, true)].into_iter().enumerate() {
        let v = step(
            &mut c.h,
            "connector",
            json!({"halves": "both", "family": "plain", "pins": 3, "layout": "row", "at": [60 + 80 * i, 150], "rotation": rot, "swap": swap}),
        );
        let x = conn_of(&v);
        let d = c.h.doc();
        assert_eq!(side_labels(&d, x, ".s"), ["1", "2", "3"], "rot {rot} swap {swap}");
        assert_eq!(side_labels(&d, x, ".p"), ["1", "2", "3"], "rot {rot} swap {swap}");
    }
}

/// Row 1, one half: `halves: socket` gives one half and no plug pins; adding the plug later
/// (`connector {device}`) joins the same device.
#[test]
fn r01_one_half_then_the_other_on_another_page() {
    let mut c = fixture_c(0x7044_0003);
    let x = conn_of(&step(
        &mut c.h,
        "connector",
        json!({"halves": "socket", "family": "dsub", "pins": 9, "layout": "row", "at": [100, 100], "page": c.p101.hex()}),
    ));
    let d = c.h.doc();
    assert_eq!(side_labels(&d, x, ".s").len(), 9);
    assert!(side_labels(&d, x, ".p").is_empty());
    c.h.activate(c.p102);
    let v = step(&mut c.h, "connector", json!({"device": x.hex(), "halves": "plug", "layout": "row", "at": [100, 100], "page": c.p102.hex()}));
    assert_eq!(conn_of(&v), x, "joins the named connector");
    let d = c.h.doc();
    assert_eq!(side_labels(&d, x, ".p").len(), 9, "family and size taken from the connector");
    assert_eq!(reps_on(&d, x, c.p101).len(), 1);
    assert_eq!(reps_on(&d, x, c.p102).len(), 1);
}

// ------------------------------------------------------------------------------- row 2 (AC 2)

/// Row 2: a wire to `-X01.S:3` and one to `-X01.P:3` are one net and one wire number; `.S:3` and
/// `.S:4` are two nets; pins of one half never join each other.
#[test]
fn r02_mated_pins_are_one_net_and_one_wire_number() {
    let mut c = fixture_c(0x7044_0010);
    let x = with_x01(&mut c);
    let d = c.h.doc();
    let (s3, p3, s4, p4) = (pin(&d, x, ".s", "3"), pin(&d, x, ".p", "3"), pin(&d, x, ".s", "4"), pin(&d, x, ".p", "4"));
    for t in [s3, p3, s4, p4] {
        wire_out(&mut c.h, c.p101, x, t);
    }
    assert!(net(&mut c.h, s3).is_some());
    assert_eq!(net(&mut c.h, s3), net(&mut c.h, p3), "S:3 and P:3 mate");
    assert_eq!(net(&mut c.h, s4), net(&mut c.h, p4), "S:4 and P:4 mate");
    assert_ne!(net(&mut c.h, s3), net(&mut c.h, s4), "pins 3 and 4 are two nets");
    assert_eq!(wire_number(&mut c.h, s3), wire_number(&mut c.h, p3), "one wire number");
    assert_ne!(wire_number(&mut c.h, s3), wire_number(&mut c.h, s4));
    same_as_built(&mut c.h);
    c.h.check();
}

/// Row 2 across pages: socket on 101, plug on 102; wires on both pages are one net.
#[test]
fn r02_mating_joins_nets_across_pages() {
    let mut c = fixture_c(0x7044_0011);
    let x = conn_of(
        &c.h.run("connector", json!({"halves": "socket", "family": "plain", "pins": 4, "layout": "row", "at": [100, 100], "page": c.p101.hex()}))
            .unwrap(),
    );
    c.h.activate(c.p102);
    c.h.run("connector", json!({"device": x.hex(), "halves": "plug", "layout": "row", "at": [100, 100], "page": c.p102.hex()})).unwrap();
    let d = c.h.doc();
    let (s2, p2, s1) = (pin(&d, x, ".s", "2"), pin(&d, x, ".p", "2"), pin(&d, x, ".s", "1"));
    wire_out(&mut c.h, c.p101, x, s2);
    wire_out(&mut c.h, c.p101, x, s1);
    wire_out(&mut c.h, c.p102, x, p2);
    assert_eq!(net(&mut c.h, s2), net(&mut c.h, p2), "mated across pages");
    assert_ne!(net(&mut c.h, s1), net(&mut c.h, p2));
    assert_eq!(wire_number(&mut c.h, s2), wire_number(&mut c.h, p2));
    // Deleting the plug half's symbol unmates: two nets again (036 FP8a's update rows).
    let d = c.h.doc();
    let rep = rep_of(&d, p2);
    let item = d.representations[&rep].items[0];
    let hnd = c.h.handle_of(c.p102, item);
    c.h.activate(c.p102);
    c.h.run("erase", json!({"handles": [hnd.hex()]})).unwrap();
    let still = c.h.doc().terminals.contains_key(&p2);
    if still {
        assert_ne!(net(&mut c.h, s2), net(&mut c.h, p2), "an undrawn plug pin joins nothing");
    }
    same_as_built(&mut c.h);
    c.h.cmdline("u");
    assert_eq!(net(&mut c.h, s2), net(&mut c.h, p2), "undo mates them again");
    same_as_built(&mut c.h);
}

/// Row 2: two connectors never join each other, even with the same pin names.
#[test]
fn r02_halves_of_different_connectors_never_join() {
    let mut c = fixture_c(0x7044_0012);
    let x1 = with_x01(&mut c);
    let x3 = conn_of(
        &c.h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [250, 100], "page": c.p101.hex()}))
            .unwrap(),
    );
    let d = c.h.doc();
    let (a, b) = (pin(&d, x1, ".s", "1"), pin(&d, x3, ".p", "1"));
    wire_out(&mut c.h, c.p101, x1, a);
    wire_out(&mut c.h, c.p101, x3, b);
    assert_ne!(net(&mut c.h, a), net(&mut c.h, b));
}

// ------------------------------------------------------------------------------- row 3 (AC 3)

/// Row 3: pin pairs across three wires: `-X02` with pins 1–3 mated, `pins = 4`; each wire split,
/// both parts keep the wire's number and are one net; three wires, three nets; one transaction.
#[test]
fn r03_pin_pairs_across_wires_split_and_keep_the_number() {
    let mut c = fixture_c(0x7044_0020);
    with_x01(&mut c);
    c.h.activate(c.p100);
    let before_numbers: Vec<String> = c.w.clone().iter().map(|w| s(&c.h.run("net", json!({"conductor": w.hex()})).unwrap()["number"])).collect();
    let v = step(
        &mut c.h,
        "connector",
        json!({"halves": "both", "family": "m12", "pins": 4, "names": "numbers", "layout": "pins", "line": [[90, 200], [130, 200]], "page": c.p100.hex()}),
    );
    let x = conn_of(&v);
    let d = c.h.doc();
    assert_eq!(tag(&d, x), "-X02", "{v}");
    assert_eq!(side_labels(&d, x, ".s"), ["1", "2", "3"], "{v}");
    assert_eq!(side_labels(&d, x, ".p"), ["1", "2", "3"], "{v}");
    assert_eq!(d.components[&x].properties.get("pins").map(String::as_str), Some("4"), "pins = 4 (FP4)");
    assert_eq!(reps_on(&d, x, c.p100).len(), 3, "one pin pair per wire");
    let mut nets = Vec::new();
    for (i, label) in ["1", "2", "3"].iter().enumerate() {
        let (sp, pp) = (pin(&d, x, ".s", label), pin(&d, x, ".p", label));
        let on_s: Vec<&ConductorId> = d.conductors.iter().filter(|(_, k)| k.ends.contains(&Some(sp))).map(|(w, _)| w).collect();
        let on_p: Vec<&ConductorId> = d.conductors.iter().filter(|(_, k)| k.ends.contains(&Some(pp))).map(|(w, _)| w).collect();
        assert_eq!((on_s.len(), on_p.len()), (1, 1), "pin {label}: the wire is split onto both points");
        let n = net(&mut c.h, sp);
        assert!(n.is_some() && n == net(&mut c.h, pp), "pin {label}: one net through the mate");
        assert_eq!(wire_number(&mut c.h, sp), before_numbers[i], "pin {label}: socket side keeps the number");
        assert_eq!(wire_number(&mut c.h, pp), before_numbers[i], "pin {label}: plug side is the same net, no new number");
        nets.push(n);
    }
    nets.dedup();
    assert_eq!(nets.len(), 3, "three wires, three nets");
    let echo = c.h.s.log.iter().rev().take(4).cloned().collect::<Vec<_>>().join("\n");
    assert!(echo.contains("-X02") && echo.contains("3 wires"), "{echo}");
    c.h.check();
}

/// Row 3 / §4.4 (owner Q3): socket pins across three wires on one page and plug pins across
/// three wires on another pair up 1–3 with 1–3; pins fill like 036's levels (mate first).
#[test]
fn r03_pins_fill_mate_first_across_pages() {
    let mut c = fixture_c(0x7044_0021);
    let v =
        c.h.run(
            "connector",
            json!({"halves": "socket", "family": "plain", "pins": 6, "layout": "pins", "line": [[90, 200], [130, 200]], "page": c.p100.hex()}),
        )
        .unwrap();
    let x = conn_of(&v);
    assert_eq!(side_labels(&c.h.doc(), x, ".s"), ["1", "2", "3"]);
    assert!(side_labels(&c.h.doc(), x, ".p").is_empty());
    c.h.activate(c.p102);
    drops(&mut c.h, &XS);
    step(
        &mut c.h,
        "connector",
        json!({"device": x.hex(), "halves": "plug", "layout": "pins", "line": [[90, 200], [130, 200]], "page": c.p102.hex()}),
    );
    let d = c.h.doc();
    assert_eq!(side_labels(&d, x, ".p"), ["1", "2", "3"], "plug pins take the socket's drawn pins first");
    for l in ["1", "2", "3"] {
        assert_eq!(net(&mut c.h, pin(&d, x, ".s", l)), net(&mut c.h, pin(&d, x, ".p", l)), "pin {l} mated across pages");
    }
    // A plug pin by `place` now takes the next free pin (4), a pin pair the next drawn on neither.
    c.h.activate(c.p101);
    let v = step(&mut c.h, "place", json!({"symbol": "pinp", "device": x.hex(), "at": [300, 100]}));
    let d = c.h.doc();
    assert_eq!(side_labels(&d, x, ".p"), ["1", "2", "3", "4"], "next free pin: {v}");
    step(&mut c.h, "place", json!({"symbol": "pinpair", "device": x.hex(), "at": [320, 100]}));
    let d = c.h.doc();
    assert_eq!(side_labels(&d, x, ".s"), ["1", "2", "3", "5"], "pin pair takes the lowest drawn on neither side");
    assert_eq!(side_labels(&d, x, ".p"), ["1", "2", "3", "4", "5"]);
    // A named pin through `place` (the prompt's `Pin <n>`).
    step(&mut c.h, "place", json!({"symbol": "pins", "device": x.hex(), "pin": "4", "at": [340, 100]}));
    assert!(side_labels(&c.h.doc(), x, ".s").contains(&"4".to_string()));
}

/// Row 3, single wire: `wire: id` places one pin pair on that wire.
#[test]
fn r03_pins_on_one_wire() {
    let mut c = fixture_c(0x7044_0022);
    c.h.activate(c.p100);
    let v = step(
        &mut c.h,
        "connector",
        json!({"halves": "both", "family": "plain", "pins": 2, "layout": "pins", "wire": c.w[1].hex(), "page": c.p100.hex()}),
    );
    let x = conn_of(&v);
    let d = c.h.doc();
    assert_eq!(side_labels(&d, x, ".s"), ["1"]);
    assert_eq!(side_labels(&d, x, ".p"), ["1"]);
}

/// Row 3, a one-side pin on a wire: the wire lands on its one point, the cut-off part is left
/// open (§4.5), so the two parts are two nets.
#[test]
fn r03_one_side_pin_on_a_wire_leaves_the_far_part_open() {
    let mut c = fixture_c(0x7044_0023);
    c.h.activate(c.p100);
    let v =
        c.h.run("connector", json!({"halves": "socket", "family": "plain", "pins": 2, "layout": "pins", "wire": c.w[0].hex(), "page": c.p100.hex()}))
            .unwrap();
    let x = conn_of(&v);
    let d = c.h.doc();
    let sp = pin(&d, x, ".s", "1");
    let on: Vec<&ecad_model::Conductor> = d.conductors.values().filter(|k| k.ends.contains(&Some(sp))).collect();
    assert_eq!(on.len(), 1, "one point, one wire");
}

// ------------------------------------------------------------------------------- row 4 (AC 4)

/// Row 4: the plug half again as a row on `-X02`, a socket pin 2 that is drawn, and a half joined
/// to a non-connector: each refused with §6.1's words; head unchanged.
#[test]
fn r04_refusals_name_a_way_forward() {
    let mut c = fixture_c(0x7044_0030);
    let (x2, _) = with_x02(&mut c);
    let t2 = tag(&c.h.doc(), x2);
    let has_plug = format!("{t2} already has a plug half");
    c.h.activate(c.p101);
    refused(
        &mut c.h,
        "connector",
        json!({"device": x2.hex(), "halves": "plug", "layout": "row", "at": [100, 100], "page": c.p101.hex()}),
        &[has_plug.as_str(), "Enter for a new connector."],
    );
    refused(&mut c.h, "place", json!({"symbol": "plug4", "device": x2.hex(), "at": [100, 100]}), &[has_plug.as_str()]);
    refused(
        &mut c.h,
        "place",
        json!({"symbol": "pins", "device": x2.hex(), "pin": "2", "at": [150, 100]}),
        &[format!("{t2}.S:2 is drawn on").as_str(), "Pick another pin, or Enter for the next free one."],
    );
    let k: ComponentId = id(&c.h.run("place", json!({"symbol": "coil", "at": [300, 200]})).unwrap(), "component");
    let kt = tag(&c.h.doc(), k);
    for sym in ["socket4", "pinpair", "pins"] {
        refused(
            &mut c.h,
            "place",
            json!({"symbol": sym, "device": k.hex(), "at": [200, 100]}),
            &[format!("{kt} is not a connector.").as_str(), "Pick a connector, or Enter for a new one."],
        );
    }
    refused(&mut c.h, "connector", json!({"device": k.hex(), "halves": "socket", "layout": "row", "at": [200, 100]}), &["is not a connector"]);
    c.h.check();
}

/// §6.1's other refusals: a row size the type does not have, a name list of the wrong count, a
/// bad name, letters past 26, more than 64 pins, no `at`/`line`/`wire`.
#[test]
fn r04_argument_refusals() {
    let mut c = fixture_c(0x7044_0031);
    let at = json!([100, 100]);
    refused(
        &mut c.h,
        "connector",
        json!({"family": "dsub", "pins": 10, "layout": "row", "at": at}),
        &["A D-sub has 9, 15 or 25 pins.", "draw pin by pin with Pins"],
    );
    refused(&mut c.h, "connector", json!({"pins": 4, "names": ["L", "N", "PE"], "layout": "row", "at": at}), &["Give 4 pin names; got 3."]);
    refused(&mut c.h, "connector", json!({"pins": 2, "names": ["P E", "N"], "layout": "row", "at": at}), &["cannot contain spaces", "\"P E\""]);
    for bad in ["A,B", "A:B", "TOOLONGNM", ""] {
        survives(&mut c.h, "connector", json!({"pins": 2, "names": [bad, "Z"], "layout": "pins", "line": [[90, 200], [130, 200]]}));
        assert!(c.h.doc().terminals.values().all(|t| t.label.as_deref() != Some(bad)), "{bad:?} is not a pin name");
    }
    refused(&mut c.h, "connector", json!({"pins": 2, "names": ["A", "A"], "layout": "row", "at": at}), &[]);
    refused(&mut c.h, "connector", json!({"pins": 27, "names": "letters", "layout": "pins", "line": [[90, 200], [130, 200]]}), &[]);
    refused(&mut c.h, "connector", json!({"pins": 65, "layout": "pins", "line": [[90, 200], [130, 200]]}), &[]);
    refused(
        &mut c.h,
        "connector",
        json!({"family": "plain", "pins": 4, "layout": "row"}),
        &["connector needs at for a row, or line or wire for pins."],
    );
    refused(
        &mut c.h,
        "connector",
        json!({"family": "plain", "pins": 4, "layout": "pins"}),
        &["connector needs at for a row, or line or wire for pins."],
    );
}

/// §4.4 step 3: past the names of a list, a pin is refused with the way out; with numbers it is
/// a new pin and a `connector-over` row.
#[test]
fn r04_pin_past_the_names_is_refused() {
    let mut c = fixture_c(0x7044_0032);
    let x =
        conn_of(&c.h.run("connector", json!({"halves": "socket", "pins": 3, "names": ["L", "N", "PE"], "layout": "row", "at": [100, 100]})).unwrap());
    assert_eq!(side_labels(&c.h.doc(), x, ".s").len(), 3);
    refused(
        &mut c.h,
        "place",
        json!({"symbol": "pins", "device": x.hex(), "at": [200, 100]}),
        &["-X01 has 3 named pins (L, N, PE), all drawn on the socket half.", "Add a name with pinlabel, or set pins."],
    );
}

// ------------------------------------------------------------------------------- row 5 (AC 4)

/// Row 5 and owner Q10: a whole half of another family or size is placed (not refused) and gives
/// a `connector-family` / `connector-size` warning with a go-to.
#[test]
fn r05_family_or_size_mismatch_is_a_warning_not_a_refusal() {
    let mut c = fixture_c(0x7044_0040);
    // The design's literal row: -X02's pin pairs deleted, then a plain plug6 on it.
    let (x2, _) = with_x02(&mut c);
    let d = c.h.doc();
    let handles: Vec<String> = reps_on(&d, x2, c.p100).iter().map(|r| c.h.handle_of(c.p100, d.representations[r].items[0]).hex()).collect();
    c.h.activate(c.p100);
    c.h.run("erase", json!({"handles": handles})).unwrap();
    if c.h.doc().components.contains_key(&x2) {
        c.h.activate(c.p101);
        step(&mut c.h, "place", json!({"symbol": "plug6", "device": x2.hex(), "at": [100, 100]}));
        let rows: Vec<Value> = ["connector-family", "connector-size"].iter().flat_map(|k| checks_of(&mut c.h, k)).collect();
        assert!(
            !rows.is_empty() || !c.h.doc().components[&x2].properties.contains_key("pins"),
            "an M12 4-pin connector with a plain plug6: {rows:?}"
        );
    }
    // Family: a D-sub socket with an M12 plug.
    let x3 = conn_of(
        &c.h.run("connector", json!({"halves": "socket", "family": "dsub", "pins": 9, "layout": "row", "at": [60, 200], "page": c.p101.hex()}))
            .unwrap(),
    );
    step(&mut c.h, "place", json!({"symbol": "m12p4", "device": x3.hex(), "at": [200, 200]}));
    let rows = checks_of(&mut c.h, "connector-family");
    let t3 = tag(&c.h.doc(), x3);
    let row = rows.iter().find(|r| row_words(r).contains(&format!("{t3}'s halves differ"))).unwrap_or_else(|| panic!("{rows:?}"));
    assert_eq!(s(&row["severity"]).to_lowercase(), "warning", "{row}");
    assert!(s(&row["text"]).contains("D-sub socket") && s(&row["text"]).contains("M12 plug"), "{row}");
    rows_have(&mut c.h, "connector-family", false);
    // Size: a plain socket4 with a plain plug6.
    let x4 = conn_of(
        &c.h.run("connector", json!({"halves": "socket", "family": "plain", "pins": 4, "layout": "row", "at": [60, 100], "page": c.p102.hex()}))
            .unwrap(),
    );
    c.h.activate(c.p102);
    step(&mut c.h, "place", json!({"symbol": "plug6", "device": x4.hex(), "at": [200, 100]}));
    let t4 = tag(&c.h.doc(), x4);
    let rows = checks_of(&mut c.h, "connector-size");
    let row = rows.iter().find(|r| row_words(r).contains(&format!("{t4}.S has 4 pins, {t4}.P has 6"))).unwrap_or_else(|| panic!("{rows:?}"));
    assert_eq!(s(&row["severity"]).to_lowercase(), "warning", "{row}");
    rows_have(&mut c.h, "connector-size", false);
    // Pins 5 and 6 of the plug have no mate: one count row (both halves have pins).
    let lone = checks_of(&mut c.h, "connector-lone-pin");
    assert!(lone.iter().any(|r| row_words(r).contains(&format!("{t4}: 2 pins have no mate (P:5, P:6)"))), "{lone:?}");
    rows_have(&mut c.h, "connector-lone-pin", true);
}

/// §6.3 `connector-over` and `connector-lone-pin`: a plug pin past `pins` (numbers) is a new pin
/// with an Info row and its fix (`Set pins to 5`); a lone half raises nothing.
#[test]
fn r05_over_and_lone_pin_rows_and_quiet_lone_halves() {
    let mut c = fixture_c(0x7044_0041);
    let x = with_x01(&mut c);
    let lone_half = conn_of(&c.h.run("connector", json!({"halves": "socket", "pins": 4, "layout": "row", "at": [250, 100]})).unwrap());
    for k in ["connector-pin-twice", "connector-family", "connector-size", "connector-lone-pin", "connector-over"] {
        assert!(checks_of(&mut c.h, k).is_empty(), "{k}: a mated pair and a lone half raise nothing");
    }
    let _ = lone_half;
    step(&mut c.h, "place", json!({"symbol": "pinp", "device": x.hex(), "at": [300, 200]}));
    assert_eq!(side_labels(&c.h.doc(), x, ".p"), ["1", "2", "3", "4", "5"], "numbers: a new pin 5");
    let over = checks_of(&mut c.h, "connector-over");
    assert!(over.iter().any(|r| row_words(r).contains("-X01 has pin 5 beyond its 4 pins") && s(&r["severity"]).to_lowercase() == "info"), "{over:?}");
    rows_have(&mut c.h, "connector-over", true);
    let lone = checks_of(&mut c.h, "connector-lone-pin");
    assert!(lone.iter().any(|r| row_words(r).contains("-X01: 1 pin") && row_words(r).contains("P:5")), "{lone:?}");
    // The fix: `setprop pins 5` clears connector-over.
    step(&mut c.h, "setprop", json!({"component": x.hex(), "key": "pins", "value": "5"}));
    assert!(checks_of(&mut c.h, "connector-over").is_empty());
}

/// §6.3 `connector-pin-twice` on a hand-made file: S:4 relabelled `3` by raw ops gives an Error
/// row with the go-to and the fix **Move to next free**; nothing panics.
#[test]
fn r05_pin_twice_in_a_hand_made_file() {
    let mut c = fixture_c(0x7044_0042);
    let x = with_x01(&mut c);
    let d = c.h.doc();
    let t = pin(&d, x, ".s", "4");
    let before = d.terminals[&t].clone();
    let mut after = before.clone();
    after.label = Some("3".into());
    c.h.cfg(|p| {
        p.ledger.commit(Author::User, "hand-made", vec![Op::RemoveTerminal { id: t, terminal: before }, Op::AddTerminal { id: t, terminal: after }])
    })
    .unwrap();
    let (mut h, _) = reload(c.h, "twice", 0x7044_0043);
    let rows = h.run("checks", json!({"kinds": ["connector-pin-twice"]})).unwrap()["checks"].as_array().cloned().unwrap_or_default();
    assert!(rows.iter().any(|r| row_words(r).contains("-X01.S:3 is drawn twice") && s(&r["severity"]).to_lowercase() == "error"), "{rows:?}");
    rows_have(&mut h, "connector-pin-twice", true);
    read_everything(&mut h);
}

// ------------------------------------------------------------------------------- row 6 (AC 5)

/// Row 6: hover, the wire list and the terminal plan's targets name a pin `-X01.S:3`; a typed
/// half tag finds the connector; `-X01:3` is never a name of a pin on both halves.
#[test]
fn r06_pin_names_in_hover_wire_list_and_targets() {
    let mut c = fixture_c(0x7044_0050);
    let x = with_x01(&mut c);
    let d = c.h.doc();
    let (s3, p3) = (pin(&d, x, ".s", "3"), pin(&d, x, ".p", "3"));
    let ws = wire_out(&mut c.h, c.p101, x, s3);
    wire_out(&mut c.h, c.p101, x, p3);
    assert_eq!(name(&mut c.h, s3), "-X01.S:3");
    assert_eq!(name(&mut c.h, p3), "-X01.P:3");
    // Hover over the pin (interact::hover_terminal) names it.
    let at = shown_at(&c.h.doc(), c.p101, s3).unwrap();
    let uid = c.h.p(|p| p.pages[&c.p101].uid).unwrap();
    let hover = c.h.p(|p| ecad_cadcraft::interact::hover_terminal(p, uid, cadcraft_engine::geom::Vec2::new(at.x, at.y), 1.0));
    let (ht, lines) = hover.expect("a pin under the cursor");
    assert_eq!(ht, s3);
    let mut bad: Vec<String> = Vec::new();
    if !lines.iter().any(|l| l.contains("-X01.S:3")) {
        bad.push(format!("hover: {lines:?}"));
    }
    // Wire list From/To.
    let rows = wirelist(&mut c.h);
    let text: String = rows.iter().map(Value::to_string).collect();
    if !(text.contains("-X01.S:3") && text.contains("-X01.P:3")) {
        bad.push(format!("wire list: {rows:?}"));
    }
    // A terminal strip on the socket wire: its plan's target names the half pin (046's targets).
    let d = c.h.doc();
    assert!(d.conductors[&ws].ends.contains(&Some(s3)));
    let at = shown_at(&d, c.p101, s3).unwrap();
    c.h.activate(c.p101);
    let v = c.h.run("terminals", json!({"strip": null, "from": [at.x - 5.0, at.y + 10.0], "to": [at.x + 5.0, at.y + 10.0]})).unwrap();
    let strip: ComponentId = id(&v, "strip");
    let plan = c.h.run("stripplan", json!({"strip": strip.hex(), "limit": 5000})).unwrap();
    if !plan.to_string().contains("-X01.S:3") {
        bad.push(format!("strip targets name the half pin: {plan}"));
    }
    // Devices: typing the half tag finds the connector.
    for typed in ["-X01.S", "-X01.P", "-X01"] {
        let v = c.h.run("devices", json!({"tag": typed})).unwrap();
        let hits: Vec<String> = v["devices"].as_array().cloned().unwrap_or_default().iter().map(|r| s(&r["component"])).collect();
        if !hits.contains(&x.hex()) {
            bad.push(format!("devices {typed}: {v}"));
        }
    }
    // Every shown name of a connector pin carries its side.
    for t in pins(&c.h.doc(), x).values() {
        let n = name(&mut c.h, *t);
        assert!(n.starts_with("-X01.S:") || n.starts_with("-X01.P:"), "{n}");
    }
    assert!(
        bad.is_empty(),
        "{}",
        bad.join(
            "
"
        )
    );
}

/// §5.1 typed: `place {tag: "-X01.P"}` adds a plug half to `-X01`; a device cannot take a half
/// tag of a connector (`-X01.S is the socket half of -X01. Pick another tag.`).
#[test]
fn r06_typed_half_tags_and_the_tag_guard() {
    let mut c = fixture_c(0x7044_0051);
    let x = conn_of(&c.h.run("connector", json!({"halves": "socket", "pins": 4, "layout": "row", "at": [100, 100]})).unwrap());
    let k: ComponentId = id(&c.h.run("place", json!({"symbol": "coil", "at": [300, 200]})).unwrap(), "component");
    refused(
        &mut c.h,
        "setprop",
        json!({"component": k.hex(), "key": "tag", "value": "-X01.S"}),
        &["-X01.S is the socket half of -X01. Pick another tag."],
    );
    let v = step(&mut c.h, "place", json!({"symbol": "plug4", "tag": "-X01.P", "at": [100, 60]}));
    assert_eq!(id::<ComponentId>(&v, "component"), x, "typed half tag joins the connector: {v}");
    assert_eq!(side_labels(&c.h.doc(), x, ".p").len(), 4);
}

/// FP6 (finding F1): a connector pin is never read as a strip level: its shown label is the pin
/// name, never a derived level label; no strip plan lists it; no level check names it; renumber
/// of terminals leaves it alone.
#[test]
fn fp6_a_connector_pin_never_shows_a_strip_level_label() {
    let mut c = fixture_c(0x7044_0052);
    let x1 = with_x01(&mut c);
    let (x2, _) = with_x02(&mut c);
    let x5 = conn_of(
        &c.h.run(
            "connector",
            json!({"halves": "both", "pins": 3, "names": ["L", "N", "PE"], "layout": "row", "at": [250, 100], "page": c.p101.hex()}),
        )
        .unwrap(),
    );
    let d = c.h.doc();
    for x in [x1, x2, x5] {
        let tg = tag(&d, x);
        for ((k, l), t) in pins(&d, x) {
            let side = if k == ".s" { "S" } else { "P" };
            let want = format!("{tg}.{side}:{l}");
            assert_eq!(ecad_cadcraft::terminal_types::shown_label(&d, t).as_deref(), Some(want.as_str()), "shown_label of {want}");
            assert_eq!(ecad_cadcraft::interact::terminal_name(&d, t), want);
        }
        assert!(!ecad_cadcraft::strips::is_strip(&d, x), "{tg} is not a strip");
    }
    let plan = c.h.run("stripplan", json!({"limit": 5000})).unwrap();
    let strips = plan["strips"].as_array().cloned().unwrap_or_default();
    assert!(strips.is_empty(), "no strip plan for connectors: {plan}");
    for k in ["level-missing", "level-twice", "position-mixed", "terminal-twice", "too-many-terminals"] {
        let rows = checks_of(&mut c.h, k);
        assert!(rows.is_empty(), "{k} on a connector: {rows:?}");
    }
    let before = c.h.doc();
    let pv = c.h.run("renumber", json!({"kind": "terminals", "preview": true})).unwrap();
    assert!(!pv.to_string().contains(&x1.hex()), "renumber terminals leaves connectors alone: {pv}");
    assert_eq!(model_parts(&c.h.doc()), model_parts(&before), "a preview writes nothing");
    // The page texts show pin labels and half tags, never `3.1`-style level labels.
    let texts = c.h.run("pagetexts", json!({"page": c.p101.hex()})).unwrap().to_string();
    assert!(texts.contains("-X01.S") && texts.contains("-X01.P"), "half tags on the page: {texts}");
    for bad in ["1.1", "3.2", "PE.1", "1.L"] {
        assert!(!texts.contains(&format!("\"{bad}\"")), "a level label {bad} on the page: {texts}");
    }
}

// ------------------------------------------------------------------------------- row 7 (AC 5, 8)

/// Row 7 and AC 8: project side names `F`/`M` show `-X01.F`/`-X01.M` everywhere at once (names,
/// BOM, wire list, page texts); a connector-level side name beats the project's; the separator;
/// reset gives the built-in back; a side name equal to the other's resolves to the next level.
#[test]
fn r07_side_names_and_separator_from_the_registry() {
    let mut c = fixture_c(0x7044_0060);
    let x1 = with_x01(&mut c);
    let (x2, _) = with_x02(&mut c);
    let d = c.h.doc();
    let (s3, p3) = (pin(&d, x1, ".s", "3"), pin(&d, x1, ".p", "3"));
    wire_out(&mut c.h, c.p101, x1, s3);
    wire_out(&mut c.h, c.p101, x1, p3);
    step(&mut c.h, "setdefault", json!({"key": "connector.side.socket", "level": "project", "value": "F"}));
    step(&mut c.h, "setdefault", json!({"key": "connector.side.plug", "level": "project", "value": "M"}));
    assert_eq!(name(&mut c.h, s3), "-X01.F:3");
    assert_eq!(name(&mut c.h, p3), "-X01.M:3");
    let d = c.h.doc();
    assert_eq!(name(&mut c.h, pin(&d, x2, ".s", "1")), "-X02.F:1", "every connector that inherits");
    assert_eq!(bom_rows(&mut c.h, "-X01.F").len(), 1);
    assert_eq!(bom_rows(&mut c.h, "-X01.M").len(), 1);
    assert!(bom_rows(&mut c.h, "-X01.S").is_empty());
    let wl: String = wirelist(&mut c.h).iter().map(Value::to_string).collect();
    assert!(wl.contains("-X01.F:3") && wl.contains("-X01.M:3") && !wl.contains("-X01.S:3"), "{wl}");
    let texts = c.h.run("pagetexts", json!({"page": c.p101.hex()})).unwrap().to_string();
    assert!(texts.contains("-X01.F") && !texts.contains("-X01.S"), "{texts}");
    // A connector's own side name.
    step(&mut c.h, "setdefault", json!({"key": "connector.side.socket", "level": "object", "component": x2.hex(), "value": "B"}));
    let d = c.h.doc();
    assert_eq!(name(&mut c.h, pin(&d, x2, ".s", "1")), "-X02.B:1");
    assert_eq!(name(&mut c.h, s3), "-X01.F:3", "the other connector keeps the project's");
    // The separator.
    step(&mut c.h, "setdefault", json!({"key": "connector.half.separator", "level": "project", "value": "-"}));
    assert_eq!(name(&mut c.h, s3), "-X01-F:3");
    // A plug name equal to the socket's does not parse: refused, or resolved to the next level.
    let head = c.h.head();
    let _ = c.h.run("setdefault", json!({"key": "connector.side.plug", "level": "project", "value": "F"}));
    assert_ne!(
        name(&mut c.h, s3).rsplit_once(':').map(|x| x.0.to_string()),
        name(&mut c.h, p3).rsplit_once(':').map(|x| x.0.to_string()),
        "halves stay apart"
    );
    if c.h.head() > head {
        c.h.cmdline("u");
    }
    // Reset to the built-in.
    for k in ["connector.side.socket", "connector.side.plug", "connector.half.separator"] {
        c.h.run("setdefault", json!({"key": k, "level": "project", "reset": true})).unwrap();
    }
    assert_eq!(name(&mut c.h, s3), "-X01.S:3");
    same_as_built(&mut c.h);
    // Bad values never panic and resolve to something usable.
    for (k, v) in [
        ("connector.half.separator", "abc"),
        ("connector.half.separator", ":"),
        ("connector.side.socket", "TOOLONG"),
        ("connector.side.socket", "S P"),
        ("connector.side.plug", ""),
    ] {
        survives(&mut c.h, "setdefault", json!({"key": k, "level": "project", "value": v}));
        let n = name(&mut c.h, s3);
        assert!(n.starts_with("-X01") && n.ends_with(":3"), "{k}={v:?}: {n}");
    }
}

/// AC 8: the registry lists the `connector` area's keys; the description defaults reach the BOM
/// rows that have no own description; the maker's first answers come from the registry.
#[test]
fn r07_registry_area_and_description_defaults() {
    let mut c = fixture_c(0x7044_0061);
    let v = c.h.run("defaults", json!({})).unwrap();
    let keys: Vec<String> = v["defaults"].as_array().map(|a| a.iter().map(|r| s(&r["key"])).collect()).unwrap_or_default();
    for k in [
        "connector.half.separator",
        "connector.side.socket",
        "connector.side.plug",
        "connector.description",
        "connector.plain.socket.description",
        "connector.dsub.plug.description",
        "connector.m12.socket.description",
        "connector.maker.halves",
        "connector.maker.family",
        "connector.maker.pins",
        "connector.maker.names",
        "connector.maker.layout",
        "symbol.pinpair.prefix",
    ] {
        assert!(keys.iter().any(|x| x == k), "registry lists {k}: {keys:?}");
    }
    let x = with_x01(&mut c);
    let rows = bom_rows(&mut c.h, "-X01.S");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(s(&rows[0]["description"]), "Socket, 4-pin");
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.P")[0]["description"]), "Plug, 4-pin");
    step(&mut c.h, "setdefault", json!({"key": "connector.plain.socket.description", "level": "project", "value": "Buchse {n}-polig"}));
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.S")[0]["description"]), "Buchse 4-polig");
    step(&mut c.h, "setprop", json!({"component": x.hex(), "key": "half.s.description", "value": "Own socket"}));
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.S")[0]["description"]), "Own socket", "own description beats the registry");
    // The maker's answers: a project default of 6 pins, plug only.
    step(&mut c.h, "setdefault", json!({"key": "connector.maker.pins", "level": "project", "value": 6}));
    step(&mut c.h, "setdefault", json!({"key": "connector.maker.halves", "level": "project", "value": "plug"}));
    let y = conn_of(&c.h.run("connector", json!({"family": "plain", "layout": "row", "at": [250, 100]})).unwrap());
    let d = c.h.doc();
    assert_eq!(side_labels(&d, y, ".p").len(), 6, "registry pins and halves");
    assert!(side_labels(&d, y, ".s").is_empty());
    let d = c.h.doc();
    assert_eq!(d.components[&y].properties.get("description").map(String::as_str), Some("Connector"), "connector.description");
}

// ------------------------------------------------------------------------------- row 8 (AC 6)

/// Row 8: one BOM row per drawn half with that half's description, maker, part and location;
/// a lone half one row; a bulkhead one row; the connector's own fields unused while it has a half.
#[test]
fn r08_bom_one_row_per_drawn_half() {
    let mut c = fixture_c(0x7044_0070);
    let x1 = with_x01(&mut c);
    let (_x2, _) = with_x02(&mut c);
    step(&mut c.h, "setprop", json!({"component": x1.hex(), "key": "half.s.part_number", "value": "SKT-4"}));
    step(&mut c.h, "setprop", json!({"component": x1.hex(), "key": "half.s.maker", "value": "Acme"}));
    step(&mut c.h, "setprop", json!({"component": x1.hex(), "key": "part_number", "value": "WHOLE"}));
    let sr = bom_rows(&mut c.h, "-X01.S");
    let pr = bom_rows(&mut c.h, "-X01.P");
    assert_eq!((sr.len(), pr.len()), (1, 1), "{sr:?} {pr:?}");
    assert_eq!((s(&sr[0]["part_number"]), s(&sr[0]["maker"]), s(&sr[0]["location"])), ("SKT-4".into(), "Acme".into(), "+FIELD".into()));
    assert_eq!(qty(&sr[0]["qty"]), 1);
    assert!(s(&pr[0]["part_number"]).trim_matches('?').is_empty(), "plug part empty (amber): {pr:?}");
    assert!(!bom(&mut c.h).iter().any(|r| s(&r["part_number"]) == "WHOLE"), "the connector's own part is not used while it has a half");
    assert!(bom_rows(&mut c.h, "-X01").is_empty(), "no whole-device row");
    assert_eq!(bom_rows(&mut c.h, "-X02.S").len(), 1);
    assert_eq!(bom_rows(&mut c.h, "-X02.P").len(), 1);
    assert_eq!(s(&bom_rows(&mut c.h, "-X02.S")[0]["description"]), "Socket, 4-pin", "pin-by-pin half counts `pins` (no family is stored)");
    // A lone half: one row.
    let lone = conn_of(
        &c.h.run("connector", json!({"halves": "plug", "family": "dsub", "pins": 15, "layout": "row", "at": [100, 100], "page": c.p102.hex()}))
            .unwrap(),
    );
    let lt = tag(&c.h.doc(), lone);
    assert_eq!(bom_rows(&mut c.h, &format!("{lt}.P")).len(), 1);
    assert!(bom_rows(&mut c.h, &format!("{lt}.S")).is_empty());
    assert_eq!(s(&bom_rows(&mut c.h, &format!("{lt}.P"))[0]["location"]), "+CP1");
    assert_eq!(s(&bom_rows(&mut c.h, &format!("{lt}.P"))[0]["description"]), "D-sub plug, 15-way");
    // A bulkhead: one row, its own tag, as 034.
    let b = conn_of(&c.h.run("connector", json!({"family": "rj45", "at": [250, 100], "page": c.p102.hex()})).unwrap());
    let bt = tag(&c.h.doc(), b);
    assert_eq!(bom_rows(&mut c.h, &bt).len(), 1, "bulkhead one row");
    assert!(bom_rows(&mut c.h, &format!("{bt}.S")).is_empty());
    same_as_built(&mut c.h);
}

/// §8.2: halves in different locations: socket in `+FIELD`, plug in `+CP1`, with nothing typed;
/// `half.p.location` beats the box; `location` on the connector beats both boxes.
#[test]
fn r08_two_locations_for_one_connector() {
    let mut c = fixture_c(0x7044_0071);
    let x = conn_of(&c.h.run("connector", json!({"halves": "socket", "pins": 4, "layout": "row", "at": [100, 100], "page": c.p101.hex()})).unwrap());
    c.h.activate(c.p102);
    c.h.run("connector", json!({"device": x.hex(), "halves": "plug", "layout": "row", "at": [100, 100], "page": c.p102.hex()})).unwrap();
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.S")[0]["location"]), "+FIELD");
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.P")[0]["location"]), "+CP1");
    step(&mut c.h, "setprop", json!({"component": x.hex(), "key": "location", "value": "+JB1"}));
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.S")[0]["location"]), "+JB1");
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.P")[0]["location"]), "+JB1");
    step(&mut c.h, "setprop", json!({"component": x.hex(), "key": "half.p.location", "value": "+CP2"}));
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.P")[0]["location"]), "+CP2");
    assert_eq!(s(&bom_rows(&mut c.h, "-X01.S")[0]["location"]), "+JB1");
    // Grouped by part: the half rows group like device rows.
    let part = c.h.run("bom", json!({"group": "part"})).unwrap();
    assert!(part.to_string().contains("-X01.S") && part.to_string().contains("-X01.P"), "{part}");
}

// ------------------------------------------------------------------------------- row 9 (AC 7)

/// Row 9: socket on 101, plug on 102: each half's reference names the other's page (`/102.…`,
/// `/101.…`); Go to Reference jumps to the other half; hover says which half it mates with; a pin
/// pair's mate on its own page shows nothing extra; a lone half has none.
#[test]
fn r09_cross_references_between_the_halves() {
    let mut c = fixture_c(0x7044_0080);
    let x = conn_of(&c.h.run("connector", json!({"halves": "socket", "pins": 4, "layout": "row", "at": [100, 100], "page": c.p101.hex()})).unwrap());
    c.h.activate(c.p102);
    c.h.run("connector", json!({"device": x.hex(), "halves": "plug", "layout": "row", "at": [100, 100], "page": c.p102.hex()})).unwrap();
    let d = c.h.doc();
    let (n101, n102) = (page_no(&d, c.p101), page_no(&d, c.p102));
    let srep = reps_on(&d, x, c.p101)[0];
    let prep = reps_on(&d, x, c.p102)[0];
    let v = c.h.run("xrefs", json!({"component": x.hex()})).unwrap();
    let refs = v["references"].as_array().cloned().unwrap_or_default();
    let from_s = refs.iter().find(|r| s(&r["source"]) == srep.hex()).unwrap_or_else(|| panic!("socket half reference: {v}"));
    assert_eq!(s(&from_s["target_page"]), n102, "{from_s}");
    assert!(s(&from_s["text"]).contains(&format!("/{n102}")), "025's style: {from_s}");
    let from_p = refs.iter().find(|r| s(&r["source"]) == prep.hex()).unwrap_or_else(|| panic!("plug half reference: {v}"));
    assert_eq!(s(&from_p["target_page"]), n101, "{from_p}");
    // Go to Reference reaches the other half.
    let g = c.h.run("refgoto", json!({"source": srep.hex()})).unwrap();
    assert_eq!(s(&g["page"]), c.p102.hex(), "{g}");
    assert_eq!(s(&g["target"]), prep.hex(), "{g}");
    // Hover (025 §5.1).
    let d = c.h.doc();
    let lines = ecad_cadcraft::devices::hover_lines(&d, x, srep).join("\n");
    assert!(lines.contains("-X01.S") && lines.contains("mates with -X01.P") && lines.contains(&n102), "{lines}");
    // The reference updates when the plug moves page (undo-based: delete it, place it on 100).
    let pitem = d.representations[&prep].items[0];
    let hnd = c.h.handle_of(c.p102, pitem);
    c.h.run("erase", json!({"handles": [hnd.hex()]})).unwrap();
    // Lone socket now: no references, hover says it has no plug half.
    let v = c.h.run("xrefs", json!({"component": x.hex()})).unwrap();
    assert!(v["references"].as_array().is_none_or(Vec::is_empty), "a lone half has no references: {v}");
    let d = c.h.doc();
    let lines = ecad_cadcraft::devices::hover_lines(&d, x, srep).join("\n");
    assert!(lines.contains("-X01.S has no plug half."), "{lines}");
    c.h.activate(c.p100);
    c.h.run("connector", json!({"device": x.hex(), "halves": "plug", "layout": "row", "at": [250, 100], "page": c.p100.hex()})).unwrap();
    let n100 = page_no(&c.h.doc(), c.p100);
    let v = c.h.run("xrefs", json!({"component": x.hex()})).unwrap();
    let refs = v["references"].as_array().cloned().unwrap_or_default();
    assert!(refs.iter().any(|r| s(&r["source"]) == srep.hex() && s(&r["target_page"]) == n100), "{v}");
    // The kept index equals a rebuild (verify mode does this on every commit too).
    let (kept, built) = c.h.cfg(|p| (p.xref.clone(), ecad_cadcraft::xref::RefIndex::build(p.ledger.document())));
    assert!(kept == built, "kept reference index differs from a rebuild");
}

/// Row 9: pin pairs on one page show nothing extra; a pin whose mate is only on another page
/// shows that page.
#[test]
fn r09_pin_references() {
    let mut c = fixture_c(0x7044_0081);
    let (x2, _) = with_x02(&mut c);
    let d = c.h.doc();
    let v = c.h.run("xrefs", json!({"component": x2.hex()})).unwrap();
    let refs = v["references"].as_array().cloned().unwrap_or_default();
    assert!(refs.iter().all(|r| r["source_page"] != r["target_page"] || r["source"] == r["target"]), "nothing extra on one page: {v}");
    assert!(refs.iter().all(|r| s(&r["target_page"]) != page_no(&d, c.p100)), "pin pairs on one page: {v}");
    // Socket pins on 100 (pin form), plug pins on 102: each pin shows its mate's page.
    let x = conn_of(
        &c.h.run("connector", json!({"halves": "socket", "pins": 3, "layout": "pins", "line": [[90, 220], [130, 220]], "page": c.p100.hex()}))
            .unwrap(),
    );
    c.h.activate(c.p102);
    drops(&mut c.h, &XS);
    c.h.run("connector", json!({"device": x.hex(), "halves": "plug", "layout": "pins", "line": [[90, 200], [130, 200]], "page": c.p102.hex()}))
        .unwrap();
    let d = c.h.doc();
    let n102 = page_no(&d, c.p102);
    let v = c.h.run("xrefs", json!({"component": x.hex()})).unwrap();
    let refs = v["references"].as_array().cloned().unwrap_or_default();
    for rep in reps_on(&d, x, c.p100) {
        assert!(
            refs.iter().any(|r| s(&r["source"]) == rep.hex() && s(&r["target_page"]) == n102),
            "socket pin {rep:?} shows its mate on {n102}: {v}"
        );
    }
}

// ------------------------------------------------------------------------------- row 10 (AC 5)

/// Row 10: `pinlabel` renames pin 3 on both halves in one transaction; IDs, keys, nets and wires
/// unchanged; `u` restores `3`; duplicates and bad names refused with the way out.
#[test]
fn r10_pinlabel_renames_on_both_halves() {
    let mut c = fixture_c(0x7044_0090);
    let x = with_x01(&mut c);
    let d = c.h.doc();
    let (s3, p3, s4) = (pin(&d, x, ".s", "3"), pin(&d, x, ".p", "3"), pin(&d, x, ".s", "4"));
    let w1 = wire_out(&mut c.h, c.p101, x, s3);
    let w2 = wire_out(&mut c.h, c.p101, x, p3);
    let number = wire_number(&mut c.h, s3);
    let conductors = c.h.doc().conductors.clone();
    let v = step(&mut c.h, "pinlabel", json!({"device": x.hex(), "pin": "3", "to": "PE"}));
    let d = c.h.doc();
    assert_eq!((d.terminals[&s3].label.as_deref(), d.terminals[&s3].key.as_deref()), (Some("PE"), Some(".s")), "{v}");
    assert_eq!((d.terminals[&p3].label.as_deref(), d.terminals[&p3].key.as_deref()), (Some("PE"), Some(".p")));
    assert_eq!(d.conductors, conductors, "wires unchanged");
    assert_eq!(net(&mut c.h, s3), net(&mut c.h, p3), "still mated");
    assert_eq!(wire_number(&mut c.h, s3), number);
    assert_eq!(name(&mut c.h, s3), "-X01.S:PE");
    let _ = (w1, w2);
    let echo = c.h.s.log.iter().rev().take(3).cloned().collect::<Vec<_>>().join("\n");
    assert!(echo.contains("-X01:3 is now -X01:PE (both halves)."), "{echo}");
    // By terminal, from the plug side.
    step(&mut c.h, "pinlabel", json!({"terminal": p3.hex(), "to": "GND"}));
    assert_eq!(c.h.doc().terminals[&s3].label.as_deref(), Some("GND"), "renames the socket side too");
    refused(
        &mut c.h,
        "pinlabel",
        json!({"device": x.hex(), "pin": "4", "to": "GND"}),
        &["-X01 already has a pin GND. Pick another name, or rename that pin first."],
    );
    refused(&mut c.h, "pinlabel", json!({"device": x.hex(), "pin": "4", "to": "P E"}), &["cannot contain spaces"]);
    refused(&mut c.h, "pinlabel", json!({"device": x.hex(), "pin": "99", "to": "Z"}), &[]);
    c.h.cmdline("u");
    c.h.cmdline("u");
    assert_eq!(c.h.doc().terminals[&s3].label.as_deref(), Some("3"), "u restores 3");
    assert_eq!(c.h.doc().terminals[&s4].label.as_deref(), Some("4"));
    // A lone half's pin: renamed on the one side it has.
    let y = conn_of(&c.h.run("connector", json!({"halves": "plug", "pins": 2, "layout": "row", "at": [250, 100]})).unwrap());
    step(&mut c.h, "pinlabel", json!({"device": y.hex(), "pin": "1", "to": "A"}));
    let labels: std::collections::BTreeSet<String> = side_labels(&c.h.doc(), y, ".p").into_iter().collect();
    assert_eq!(labels, ["2", "A"].iter().map(|x| x.to_string()).collect());
}

/// Row 10: a list connector (`L,N,PE`) shares names on both halves and the names come back in
/// list order.
#[test]
fn r10_pin_names_lists_and_letters() {
    let mut c = fixture_c(0x7044_0091);
    let x = conn_of(&step(&mut c.h, "connector", json!({"halves": "both", "pins": 3, "names": ["L", "N", "PE"], "layout": "row", "at": [100, 100]})));
    let d = c.h.doc();
    for l in ["L", "N", "PE"] {
        pin(&d, x, ".s", l);
        pin(&d, x, ".p", l);
    }
    let y = conn_of(&step(&mut c.h, "connector", json!({"halves": "socket", "pins": 4, "names": "letters", "layout": "row", "at": [250, 100]})));
    let labels: std::collections::BTreeSet<String> = side_labels(&c.h.doc(), y, ".s").into_iter().collect();
    assert_eq!(labels, ["A", "B", "C", "D"].iter().map(|x| x.to_string()).collect());
}

// ------------------------------------------------------------------------------- bulkheads (Q4)

/// Owner Q4: bulkheads stay one piece: one point per pin (plain keys, no `.s`/`.p`), one BOM row,
/// no halves, no half tags; `Halves` is not asked; their pins are `-X03:3`.
#[test]
fn bulkheads_stay_one_piece() {
    let mut c = fixture_c(0x7044_00a0);
    for (i, fam) in ["rj45", "usba", "usbb"].iter().enumerate() {
        let v = step(&mut c.h, "connector", json!({"family": fam, "at": [60 + 80 * i, 100], "page": c.p101.hex()}));
        let b = conn_of(&v);
        let d = c.h.doc();
        let bt = tag(&d, b);
        assert!(d.terminals_of(b).count() > 0, "{fam}");
        for (t, x) in d.terminals_of(b) {
            assert!(!matches!(x.key.as_deref(), Some(".s" | ".p")), "{fam}: no side keys");
            assert!(ecad_cadcraft::connectors::pin_name(&d, t).is_none(), "{fam}: a bulkhead pin has no half name");
            let n = ecad_cadcraft::interact::terminal_name(&d, t);
            assert!(!n.contains(".S") && !n.contains(".P"), "{fam}: bulkhead pin {bt}:{n}");
        }
        assert_eq!(bom_rows(&mut c.h, &bt).len(), 1, "{fam}: one BOM row");
        // The 034 definition, unchanged: version 1, no bridges, still `Fixed`.
        let st = ecad_cadcraft::symbols::by_key(fam).unwrap();
        let def = st.definition().unwrap();
        assert!(def.bridges.is_empty(), "{fam}: no bridges");
        assert_eq!(def.origin.as_ref().map(|o| o.library_seq), Some(1), "{fam}: version unchanged");
    }
    // A bulkhead takes no second half.
    let b = conn_of(&c.h.run("connector", json!({"family": "rj45", "at": [300, 100]})).unwrap());
    refused(&mut c.h, "place", json!({"symbol": "plug4", "device": b.hex(), "at": [300, 200]}), &["is not a connector"]);
}

// ------------------------------------------------------------------------------- starters (FP2, FP3)

/// FP3: the 24 mating half definitions are version 2 with the bridge; the three pin starters
/// exist with `.s`/`.p` keys, the grid and bridges of §3.2; `symbols::embedded` matches the content
/// hash, so a project holding version 1 embeds version 2 beside it.
#[test]
fn fp3_starter_definitions_are_versioned() {
    let halves = [
        "socket2", "socket3", "socket4", "socket6", "socket8", "socket12", "plug2", "plug3", "plug4", "plug6", "plug8", "plug12", "dsub9s",
        "dsub15s", "dsub25s", "dsub9p", "dsub15p", "dsub25p", "m12s4", "m12s5", "m12s8", "m12p4", "m12p5", "m12p8",
    ];
    let mut ids = std::collections::BTreeSet::new();
    for k in halves {
        let st = ecad_cadcraft::symbols::by_key(k).unwrap_or_else(|| panic!("{k}"));
        let d = st.definition().unwrap();
        let o = d.origin.as_ref().unwrap();
        assert_eq!(o.library_seq, 2, "{k}");
        assert_eq!(d.bridges, vec![vec![".s".to_string(), ".p".to_string()]], "{k}");
        assert_eq!(o.content_hash, ecad_file::definition_hash(&ecad_model::Definition { origin: None, ..d.clone() }).unwrap(), "{k}: hash of v2");
        assert!(ids.insert(o.definition), "{k}: IDs unchanged and unique");
        // Definition keys stay 1…n (I7).
        let keys: Vec<&str> = d.terminals.iter().map(|t| t.key.as_str()).collect();
        let n = keys.len();
        let want: Vec<String> = (1..=n).map(|i| i.to_string()).collect();
        assert_eq!(keys, want.iter().map(String::as_str).collect::<Vec<_>>(), "{k}");
    }
    for (k, keys) in [("pinpair", &[".s", ".p"][..]), ("pins", &[".s"][..]), ("pinp", &[".p"][..])] {
        let st = ecad_cadcraft::symbols::by_key(k).unwrap_or_else(|| panic!("{k} starter"));
        assert_eq!(st.prefix(), "X", "{k}");
        let d = st.definition().unwrap();
        let got: Vec<&str> = d.terminals.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(got, keys, "{k}");
        assert_eq!(d.bridges, vec![vec![".s".to_string(), ".p".to_string()]], "{k}");
        for t in &d.terminals {
            assert!((t.at.x / 5.0).fract().abs() < 1e-9 && (t.at.y / 5.0).fract().abs() < 1e-9, "{k}: point on the 5 mm grid");
        }
        assert!(ids.insert(d.origin.as_ref().unwrap().definition), "{k}: unique ID");
    }
    for alias in ["pin pair", "mated pin", "socket pin", "plug pin", "male pin", "female pin"] {
        assert!(!ecad_cadcraft::symbols::search(alias).is_empty(), "search {alias:?}");
    }
}

/// FP2's condition: definition validation refuses a bridge naming a key that no placement writes
/// (`.x`); a connector half definition bridging `.s`/`.p` is accepted.
#[test]
fn fp2_a_bridge_to_an_unknown_key_is_refused() {
    let mut h = H::new(0x7044_00b0);
    let mut bad = ecad_cadcraft::symbols::by_key("socket4").unwrap().definition().unwrap().clone();
    bad.origin = None;
    bad.name = "hand made".into();
    bad.bridges = vec![vec![".x".to_string(), "1".to_string()]];
    let id = DefinitionId::from(0x7044_00b0_u128 << 64 | 1);
    let head = h.head();
    let r = h.cfg(|p| p.ledger.commit(Author::User, "hand-made", vec![Op::EmbedDefinition { id, definition: bad }]));
    assert!(r.is_err(), "a bridge to .x is refused");
    assert_eq!(h.head(), head);
    let mut ok = ecad_cadcraft::symbols::by_key("plug4").unwrap().definition().unwrap().clone();
    ok.name = "copy".into();
    let id2 = DefinitionId::from(0x7044_00b0_u128 << 64 | 2);
    h.cfg(|p| p.ledger.commit(Author::User, "copy", vec![Op::EmbedDefinition { id: id2, definition: ok }])).expect("a .s/.p bridge is accepted");
    // Every starter still embeds and places.
    let page = h.first_page();
    h.activate(page);
    for (i, st) in ecad_cadcraft::symbols::STARTERS.iter().enumerate() {
        let x = 20.0 + 30.0 * (i % 12) as f64;
        let y = 40.0 + 40.0 * (i / 12) as f64;
        let mut params = json!({"symbol": st.key, "at": [x, y]});
        if st.key == "continuation" {
            params["name"] = json!("24V");
        }
        h.run("place", params).unwrap_or_else(|e| panic!("{}: {e}", st.key));
    }
}

// ------------------------------------------------------------------------------- row 12 (AC 10)

/// Row 12 and AC 10: hostile input never panics: 100 000 and 10 000 pins, a 10 000-name list, a
/// 10 000-character name, control characters, `half.x.*`, hostile `pins`, coordinates, IDs.
#[test]
fn r12_hostile_input_never_panics() {
    let mut c = fixture_c(0x7044_00c0);
    let x = with_x01(&mut c);
    let line = json!([[90, 200], [130, 200]]);
    c.h.activate(c.p100);
    for pins in [json!(100_000), json!(10_000), json!(65), json!(0), json!(-1), json!(1e300), json!("4"), json!(null), json!(u64::MAX)] {
        survives(&mut c.h, "connector", json!({"pins": pins, "layout": "pins", "line": line}));
    }
    refused(&mut c.h, "connector", json!({"pins": 100_000, "layout": "pins", "line": line}), &[]);
    let many: Vec<String> = (0..10_000).map(|i| format!("P{i}")).collect();
    refused(&mut c.h, "connector", json!({"pins": 10_000, "names": many, "layout": "pins", "line": line}), &[]);
    let names_text = (0..10_000).map(|i| format!("N{i}")).collect::<Vec<_>>().join(",");
    survives(&mut c.h, "connector", json!({"pins": 4, "names": names_text, "layout": "pins", "line": line}));
    let long = "x".repeat(10_000);
    for bad in ["A\u{7}", "\u{0}", "B\r\n", "\u{202e}X", long.as_str()] {
        survives(&mut c.h, "connector", json!({"pins": 2, "names": [bad, "Z"], "layout": "pins", "line": line}));
        assert!(c.h.doc().terminals.values().all(|t| t.label.as_deref() != Some(bad)), "{bad:?} is never a pin name");
        survives(&mut c.h, "pinlabel", json!({"device": x.hex(), "pin": "1", "to": bad}));
        survives(&mut c.h, "pinlabel", json!({"device": x.hex(), "pin": bad, "to": "Q"}));
    }
    refused(
        &mut c.h,
        "setprop",
        json!({"component": x.hex(), "key": "half.x.maker", "value": "Acme"}),
        &["-X01 has no half \"x\". Use half.s.* or half.p.*."],
    );
    for k in ["half.x.part_number", "half.S.maker", "half..maker", "half.sp.location", "half.\u{0}.maker"] {
        survives(&mut c.h, "setprop", json!({"component": x.hex(), "key": k, "value": "v"}));
        assert!(!c.h.doc().components[&x].properties.contains_key(k), "{k:?} is not written");
    }
    let nines = "9".repeat(10_000);
    for v in ["-1", "0", "65", "99999999999999999999", "NaN", "4.5", "", nines.as_str()] {
        survives(&mut c.h, "setprop", json!({"component": x.hex(), "key": "pins", "value": v}));
    }
    for (k, v) in [
        ("family", json!("hdmi")),
        ("halves", json!("three")),
        ("layout", json!("grid")),
        ("names", json!(42)),
        ("rotation", json!(45)),
        ("swap", json!("yes")),
    ] {
        let mut p = json!({"pins": 4, "layout": "row", "at": [100, 100]});
        p[k] = v;
        survives(&mut c.h, "connector", p);
    }
    for at in [json!([1e308, 1e308]), json!([-1e308, 0]), json!([0]), json!("here"), json!([[0, 0]])] {
        survives(&mut c.h, "connector", json!({"pins": 4, "layout": "row", "at": at}));
    }
    for l in [json!([[0, 0], [0, 0]]), json!([[1e308, 0], [-1e308, 0]]), json!([[0, 0]]), json!("x")] {
        survives(&mut c.h, "connector", json!({"pins": 4, "layout": "pins", "line": l}));
    }
    for dev in [json!("ffffffffffffffffffffffffffffffff"), json!("zz"), json!(""), json!(7), json!("-X99"), json!("-X01.Q")] {
        survives(&mut c.h, "connector", json!({"device": dev, "halves": "plug", "layout": "row", "at": [100, 100]}));
        survives(&mut c.h, "pinlabel", json!({"device": dev, "pin": "1", "to": "A"}));
    }
    survives(&mut c.h, "pinlabel", json!({"terminal": "00", "to": "A"}));
    survives(&mut c.h, "pinlabel", json!({}));
    survives(&mut c.h, "connector", json!(null));
    survives(&mut c.h, "place", json!({"symbol": "pinpair", "device": x.hex(), "pin": "x".repeat(10_000), "at": [100, 100]}));
    read_everything(&mut c.h);
    c.h.check();
}

/// Row 12, hand-made files: a connector pin with a side key but no symbol, an unknown side key
/// `.q`, a pin with no label, and 2 000 pins on one side: every reader survives.
#[test]
fn r12_hand_made_connectors_never_panic() {
    let mut c = fixture_c(0x7044_00c1);
    let x = with_x01(&mut c);
    let mut ops = Vec::new();
    for (i, (key, label)) in
        [(Some(".q"), Some("1")), (Some(".s"), None), (Some(".p"), Some("")), (Some("."), Some("2")), (None, Some("3"))].into_iter().enumerate()
    {
        ops.push(Op::AddTerminal {
            id: TerminalId::from(0x7044_00c1_u128 << 64 | i as u128),
            terminal: ecad_model::Terminal { component: x, label: label.map(str::to_string), key: key.map(str::to_string) },
        });
    }
    for i in 0..2_000u128 {
        ops.push(Op::AddTerminal {
            id: TerminalId::from(0x7044_00c2_u128 << 64 | i),
            terminal: ecad_model::Terminal { component: x, label: Some(format!("{}", 100 + i)), key: Some(".p".into()) },
        });
    }
    c.h.cfg(|p| p.ledger.commit(Author::User, "hand-made", ops)).unwrap();
    for (k, v) in [("pins", "99999999999999999999"), ("half.s.location", "\u{0}"), ("connector.side.socket", "P"), ("connector.half.separator", ":")]
    {
        let set = Op::SetProperty { object: ecad_model::ObjectRef::Component(x), key: k.into(), before: None, after: Some(v.into()) };
        let _ = c.h.cfg(|p| p.ledger.commit(Author::User, "hand-made", vec![set]));
    }
    let (mut h, _) = reload(c.h, "hostile", 0x7044_00c3);
    read_everything(&mut h);
    let d = h.doc();
    let _ = ecad_cadcraft::devices::hover_lines(
        &d,
        x,
        reps_on(&d, x, d.page_order[1]).first().copied().unwrap_or_else(|| d.representations.keys().next().copied().unwrap()),
    );
    h.run("xrefs", json!({"component": x.hex()})).unwrap();
    survives(&mut h, "pinlabel", json!({"device": x.hex(), "pin": "1", "to": "Z"}));
    survives(&mut h, "connector", json!({"device": x.hex(), "halves": "plug", "layout": "pins", "line": [[90, 200], [130, 200]]}));
}

// ------------------------------------------------------------------------------- row 13 (AC 11)

/// Row 13: save, close, reopen at a path with spaces and `ä`: the model, BOM, wire list, names,
/// nets and references are unchanged; opening writes nothing.
#[test]
fn r13_save_and_reopen_keep_everything() {
    let mut c = fixture_c(0x7044_00d0);
    let x1 = with_x01(&mut c);
    let (x2, _) = with_x02(&mut c);
    let d = c.h.doc();
    let (s3, p3) = (pin(&d, x1, ".s", "3"), pin(&d, x1, ".p", "3"));
    wire_out(&mut c.h, c.p101, x1, s3);
    wire_out(&mut c.h, c.p101, x1, p3);
    c.h.run("setprop", json!({"component": x1.hex(), "key": "half.s.part_number", "value": "SKT-4"})).unwrap();
    c.h.run("pinlabel", json!({"device": x2.hex(), "pin": "2", "to": "BN"})).unwrap();
    c.h.run("setdefault", json!({"key": "connector.side.plug", "level": "project", "value": "M"})).unwrap();
    let before = c.h.doc();
    let reads = |h: &mut H| (bom(h), wirelist(h), h.run("xrefs", json!({"limit": 2000})).unwrap(), h.run("checks", json!({})).unwrap());
    let r_before = reads(&mut c.h);
    let names_before: Vec<String> = before.terminals.keys().map(|t| ecad_cadcraft::interact::terminal_name(&before, *t)).collect();
    let (mut h, _) = reload(c.h, "reopen", 0x7044_00d1);
    let head = h.head();
    let after = h.doc();
    assert_eq!(model_parts(&after), model_parts(&before), "model after reopen");
    assert_eq!(reads(&mut h), r_before, "reports after reopen");
    let names_after: Vec<String> = after.terminals.keys().map(|t| ecad_cadcraft::interact::terminal_name(&after, *t)).collect();
    assert_eq!(names_after, names_before);
    assert_eq!(h.doc().connectivity().net_of_terminal(s3), h.doc().connectivity().net_of_terminal(p3), "mated after reopen");
    assert_eq!(h.head(), head, "open writes nothing");
    read_everything(&mut h);
    h.check();
}

/// Row 13 and owner Q11: a file with 034 version-1 halves keeps them as separate devices with
/// plain keys; a new half in that project embeds version 2 beside version 1; an old half cannot
/// be joined as a connector.
#[test]
fn r13_old_version_1_halves_stay_separate() {
    let mut h = H::new(0x7044_00d2);
    let page = h.first_page();
    h.activate(page);
    // Version 1 of socket4 and plug4 as 034 shipped them: no bridges, library_seq 1.
    let mut ops = Vec::new();
    let mut v1 = Vec::new();
    for (i, k) in ["socket4", "plug4"].iter().enumerate() {
        let mut d = ecad_cadcraft::symbols::by_key(k).unwrap().definition().unwrap().clone();
        d.bridges.clear();
        let o = d.origin.clone().unwrap();
        d.origin = None;
        let hash = ecad_file::definition_hash(&d).unwrap();
        d.origin = Some(ecad_model::Origin { library_seq: 1, content_hash: hash, ..o });
        let id = DefinitionId::from(0x7044_00d2_u128 << 64 | i as u128);
        ops.push(Op::EmbedDefinition { id, definition: d });
        v1.push(id);
    }
    h.cfg(|p| p.ledger.commit(Author::User, "034 file", ops)).unwrap();
    let (mut h, _) = reload(h, "v1", 0x7044_00d3);
    let page = h.doc().page_order[0];
    h.activate(page);
    let old_s: ComponentId = id(&h.run("place", json!({"definition": v1[0].hex(), "at": [60, 100]})).unwrap(), "component");
    let old_p: ComponentId = id(&h.run("place", json!({"definition": v1[1].hex(), "at": [60, 60]})).unwrap(), "component");
    assert_ne!(old_s, old_p, "two devices, as 034");
    let d = h.doc();
    for c in [old_s, old_p] {
        assert!(d.terminals_of(c).all(|(_, t)| !matches!(t.key.as_deref(), Some(".s" | ".p"))), "plain keys on a version-1 half");
    }
    // A new socket4 embeds version 2 beside version 1.
    let n = h.doc().definitions.len();
    let x: ComponentId = id(&h.run("place", json!({"symbol": "socket4", "at": [200, 100]})).unwrap(), "component");
    let d = h.doc();
    assert_eq!(d.definitions.len(), n + 1, "version 2 embedded beside version 1");
    let sid = ecad_cadcraft::symbols::by_key("socket4").unwrap().definition().unwrap().origin.as_ref().unwrap().definition;
    let seqs: std::collections::BTreeSet<u64> =
        d.definitions.values().filter_map(|x| x.origin.as_ref()).filter(|o| o.definition == sid).map(|o| o.library_seq).collect();
    assert_eq!(seqs, [1, 2].into_iter().collect(), "both versions");
    assert!(d.terminals_of(x).all(|(_, t)| t.key.as_deref() == Some(".s")), "the new half has side keys");
    assert_eq!(
        ecad_cadcraft::symbols::embedded(&d, ecad_cadcraft::symbols::by_key("socket4").unwrap()).map(|i| d.definitions[&i]
            .origin
            .as_ref()
            .unwrap()
            .library_seq),
        Some(2)
    );
    // An old half is not a connector to join.
    refused(&mut h, "place", json!({"symbol": "plug4", "device": old_s.hex(), "at": [200, 60]}), &["is not a connector"]);
    let d2 = h.doc();
    assert!(d2.terminals_of(old_s).all(|(_, t)| !matches!(t.key.as_deref(), Some(".s" | ".p"))), "old halves unconverted");
    read_everything(&mut h);
}

// ------------------------------------------------------------------------------- Place group (041)

/// §4.1: Place ▸ Connector is one action in the Place group, slot 2, starting `connector`, found by
/// its search words; the device wheel and the terminal wheel gain §9.3's slots.
#[test]
fn place_connector_action_row() {
    use ecad_cadcraft::actions::{self, ACTIONS, ToolGroup};
    let a = ACTIONS.iter().find(|a| a.id == "place.connector").expect("place.connector");
    assert_eq!(a.command, "connector");
    assert_eq!(a.label, "Connector");
    assert!(matches!(a.group, Some((ToolGroup::Place, 2))), "{:?}", a.group);
    let place: Vec<&str> = ToolGroup::Place.tools().iter().map(|a| a.label).collect();
    assert_eq!(place, ["Symbol", "Terminal", "Connector"]);
    for w in ["connector", "plug", "socket", "d-sub", "m12", "mated"] {
        assert!(actions::search(w, 20).iter().any(|x| x.id == "place.connector"), "search {w:?}");
    }
    assert!(ACTIONS.iter().any(|a| a.command == "pinlabel"), "Rename pin on the terminal wheel");
}

/// Writes off (row 14) is in the MCP lane test; programmatic calls never open a dialog: the JSON
/// form with only `at` uses the registry's answers.
#[test]
fn r14_json_form_takes_the_registry_defaults() {
    let mut c = fixture_c(0x7044_00e0);
    let v = step(&mut c.h, "connector", json!({"at": [100, 100]}));
    let x = conn_of(&v);
    let d = c.h.doc();
    assert_eq!(side_labels(&d, x, ".s"), ["1", "2", "3", "4"], "registry: both, plain, 4, numbers, row");
    assert_eq!(side_labels(&d, x, ".p"), ["1", "2", "3", "4"]);
    assert!(c.h.s.running.is_none(), "no prompt left open");
}
