//! Tester 045 (PLC racks, cards, channels and addresses): acceptance checks from outside, written
//! from brief 045a, brief 045b and design 045 (§13's table A–T, §14's FP1 conditions) only, before
//! the implementation existed. They drive the JSON forms the design names (`place` with `rack` and
//! `slot`, `plcslot`, `plcracks`, `plcchannels`, `plcchannel`, `plcaddress`, `setdefault`,
//! `setprop`, `checks`, `pagetexts`) and the ledger; every test that needs the build is ignored
//! until phase 2. The worker's own tests are `plc_contracts`; these are the tester's, kept apart.
//!
//! Fixture P is simplified (as tester 036 did): plain vertical drops stand in for the contacts,
//! sensors and lamps of design §13; a terminal strip across the input drops gives each fenced
//! channel a connection on its net, so those channels are `used` (design §6).
//!
//! IDs are passed as hex (as every ECAD command takes them); the design's examples name devices
//! by tag. Every test name starts with the §13 letter it checks.

use std::collections::BTreeMap;

use cadcraft_engine::Session;
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{Author, ComponentId, Document, ObjectRef, Op, PageId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

mod address;
mod channels;
mod rack;

// ------------------------------------------------------------------------------- helpers

pub(crate) fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn num(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

pub(crate) fn prop(d: &Document, c: ComponentId, k: &str) -> Option<String> {
    d.components.get(&c).and_then(|x| x.properties.get(k).cloned())
}

pub(crate) fn tag(d: &Document, c: ComponentId) -> String {
    prop(d, c, "tag").unwrap_or_default()
}

/// The component whose tag is `t`.
pub(crate) fn by_tag(d: &Document, t: &str) -> ComponentId {
    d.components.iter().find(|(_, c)| c.properties.get("tag").map(String::as_str) == Some(t)).map(|(id, _)| *id).unwrap_or_else(|| panic!("no {t}"))
}

/// A card's channel points: key (`.3.in`) to (label, terminal).
pub(crate) fn channel_points(d: &Document, card: ComponentId) -> BTreeMap<String, (String, TerminalId)> {
    d.terminals
        .iter()
        .filter(|(_, t)| t.component == card && t.key.as_deref().is_some_and(|k| k.starts_with('.')))
        .map(|(id, t)| (t.key.clone().unwrap_or_default(), (t.label.clone().unwrap_or_default(), *id)))
        .collect()
}

/// The pin (label) at `.<i>.<role>`.
pub(crate) fn pin(d: &Document, card: ComponentId, i: u32, role: &str) -> Option<String> {
    channel_points(d, card).get(&format!(".{i}.{role}")).map(|(l, _)| l.clone())
}

/// The channel indexes drawn on `card`.
pub(crate) fn drawn_channels(d: &Document, card: ComponentId) -> Vec<u32> {
    let mut v: Vec<u32> = channel_points(d, card).keys().filter_map(|k| k.split('.').nth(1)?.parse().ok()).collect();
    v.sort_unstable();
    v.dedup();
    v
}

pub(crate) fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind]})).unwrap()["checks"].as_array().cloned().unwrap_or_default()
}

/// Every row of `kind` has a fix or a go-to (design §9: every row has a way out).
pub(crate) fn rows_have_a_way_out(h: &mut H, kind: &str) {
    let rows = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    let mine: Vec<_> = rows.iter().filter(|c| c.kind.name() == kind).collect();
    assert!(!mine.is_empty(), "no {kind} row");
    for c in mine {
        assert!(c.fix.is_some() || c.object != ecad_cadcraft::checks::Object::None, "{kind} row without a fix or go-to: {c:?}");
    }
}

pub(crate) fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.components, &d.terminals, &d.conductors, &d.pages, &d.page_order, &d.representations, &d.items, &d.properties)
}

/// One write: exactly one transaction; `u` gives back the document before it, `redo` the one after.
pub(crate) fn step(h: &mut H, cmd: &str, params: Value) -> Value {
    let before = h.doc();
    let head = h.head();
    let v = h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    assert_eq!(h.head(), head + 1, "{cmd} {params}: one transaction ({v})");
    let after = h.doc();
    h.cmdline("u");
    assert_eq!(model_parts(&h.doc()), model_parts(&before), "{cmd} {params}: undo");
    h.run("redo", Value::Null).unwrap();
    assert_eq!(model_parts(&h.doc()), model_parts(&after), "{cmd} {params}: redo");
    v
}

/// A refused write: an error whose text contains each of `words`, and nothing written.
pub(crate) fn refused(h: &mut H, cmd: &str, params: Value, words: &[&str]) -> String {
    let head = h.head();
    let Err(e) = h.run(cmd, params.clone()) else { panic!("{cmd} {params} must be refused") };
    assert_eq!(h.head(), head, "{cmd} {params}: a refusal writes nothing");
    for w in words {
        assert!(e.contains(w), "{cmd} {params}: {e:?} does not say {w:?}");
    }
    e
}

/// Hostile input: any outcome but a panic; a refusal is in words and writes nothing.
pub(crate) fn survives(h: &mut H, cmd: &str, params: Value) {
    let head = h.head();
    match h.run(cmd, params.clone()) {
        Ok(_) => assert!(h.head() <= head + 1, "{cmd} {params}"),
        Err(e) => {
            assert_eq!(h.head(), head, "{cmd} {params}: refused ({e}) but wrote");
            assert!(!e.trim().is_empty(), "{cmd} {params}: a refusal in words");
        }
    }
}

/// Every read the PLC touches, once: none may panic.
pub(crate) fn read_everything(h: &mut H) {
    h.run("plcracks", json!({})).unwrap();
    h.run("checks", json!({})).unwrap();
    h.run("bom", json!({"limit": 5000})).unwrap();
    h.run("defaults", json!({"group": "PLC"})).unwrap();
    let pages = h.doc().page_order.clone();
    for p in pages {
        h.run("pagetexts", json!({"page": p.hex()})).unwrap();
    }
    let d = h.doc();
    let _ = d.connectivity();
    let _ = ecad_cadcraft::checks::all(&d, 1e-6);
}

/// The last `n` echo lines, joined.
pub(crate) fn echo(h: &H, n: usize) -> String {
    h.s.log.iter().rev().take(n).cloned().collect::<Vec<_>>().join("\n")
}

/// The shown texts on `page` (`pagetexts`), as values.
pub(crate) fn page_texts(h: &mut H, page: PageId) -> Vec<Value> {
    let v = h.run("pagetexts", json!({"page": page.hex()})).unwrap_or_else(|e| panic!("pagetexts: {e}"));
    v["texts"].as_array().cloned().unwrap_or_default().into_iter().filter(|r| r["shown"] != json!(false)).collect()
}

pub(crate) fn text_values(h: &mut H, page: PageId) -> Vec<String> {
    let mut v: Vec<String> = page_texts(h, page).iter().map(|r| s(&r["value"])).collect();
    // Brief 051 (design 051 §2.6): a framed card's tag and caption are in its frame's name block.
    let d = h.doc();
    v.extend(ecad_cadcraft::plc::look::parts_on_page(&d, page).into_iter().filter_map(|p| match p.shape {
        ecad_cadcraft::xref::Shape::Text { value, .. } => Some(value),
        ecad_cadcraft::xref::Shape::Line(..) => None,
    }));
    v
}

/// The text row whose value is `v`.
pub(crate) fn text_row(h: &mut H, page: PageId, v: &str) -> Option<Value> {
    page_texts(h, page).into_iter().find(|r| r["value"] == v)
}

/// The rotation of the drawn text whose value is `v` on `page` (the page drawing's entity; the
/// `pagetexts` box is not rotated).
pub(crate) fn drawn_rotation(h: &mut H, page: PageId, v: &str) -> Option<f64> {
    h.pv(|p, pg| pg.entities_of(p, page)).iter().find_map(|e| match &e.kind {
        cadcraft_engine::doc::EntityKind::Text(t) if t.value == v => Some(t.rotation),
        _ => None,
    })
}

/// `plcracks`: the rack entry whose tag is `t`.
pub(crate) fn rack_entry(h: &mut H, t: &str) -> Value {
    let v = h.run("plcracks", json!({})).unwrap();
    v["racks"].as_array().and_then(|a| a.iter().find(|r| r["tag"] == t).cloned()).unwrap_or_else(|| panic!("plcracks has no rack {t}: {v}"))
}

/// The slot rows of rack `t` (`{slot, card, tag, type, channels, …}`, design §2.5; the build calls
/// the list `cards` and gives the slot count as `slots`; a free slot is `{slot, free: true}`).
pub(crate) fn slots(h: &mut H, t: &str) -> Vec<Value> {
    rack_entry(h, t)["cards"].as_array().cloned().unwrap_or_default()
}

/// The slot row holding card `t` in any rack.
pub(crate) fn slot_row(h: &mut H, card: &str) -> Option<Value> {
    let v = h.run("plcracks", json!({})).unwrap();
    v["racks"].as_array()?.iter().flat_map(|r| r["cards"].as_array().cloned().unwrap_or_default()).find(|x| x["tag"] == card)
}

/// Raw ops by hand (a hand-made file or a merge result).
pub(crate) fn raw(h: &mut H, ops: Vec<Op>) {
    h.cfg(|p| p.ledger.commit(Author::User, "hand-made", ops)).unwrap();
}

pub(crate) fn raw_setprop(h: &mut H, c: ComponentId, key: &str, value: Option<&str>) {
    let before = prop(&h.doc(), c, key);
    raw(h, vec![Op::SetProperty { object: ObjectRef::Component(c), key: key.into(), before, after: value.map(str::to_string) }]);
}

/// Relabel or rekey a terminal by hand: Remove + Add.
pub(crate) fn rekey_raw(h: &mut H, t: TerminalId, new_label: Option<&str>, new_key: Option<&str>) {
    let d = h.doc();
    let before = d.terminals[&t].clone();
    let mut after = before.clone();
    if let Some(l) = new_label {
        after.label = Some(l.to_string());
    }
    if let Some(k) = new_key {
        after.key = Some(k.to_string());
    }
    raw(h, vec![Op::RemoveTerminal { id: t, terminal: before }, Op::AddTerminal { id: t, terminal: after }]);
}

pub(crate) fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t045 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn close_all(h: &mut H) {
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
}

pub(crate) fn reopen(path: &str, seed: u64) -> H {
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, path, seed).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    h.trail.push(d);
    h
}

/// Save to a file, close, reopen as a user would (also the hand-made-file path after raw commits).
pub(crate) fn reload(mut h: H, tag: &str, seed: u64) -> H {
    let dir = tmp_dir(tag);
    let path = dir.join("PLC -A01 (Rack).ecad").display().to_string();
    h.cfg(|p| p.persistence = Persistence::File);
    h.s.execute("saveas", &json!({"path": path})).unwrap();
    close_all(&mut h);
    drop(h);
    reopen(&path, seed)
}

/// Vertical wires at `xs`, from y 250 to y 150.
pub(crate) fn drops(h: &mut H, xs: &[f64]) {
    for x in xs {
        h.run("wire", json!({"points": pts(&[(*x, 250.0), (*x, 150.0)])})).unwrap();
    }
}

// ------------------------------------------------------------------------------- fixture P

// Brief 051 (design 051 §2.2, FP3): the pack's 20 mm pitch; a fence across closer wires is refused.
pub(crate) const IN_XS: [f64; 8] = [100.0, 120.0, 140.0, 160.0, 180.0, 200.0, 220.0, 240.0];
pub(crate) const OUT_XS: [f64; 6] = [100.0, 120.0, 140.0, 160.0, 180.0, 200.0];

/// Fixture P (design §13), simplified. Page 1 "PLC CPU": `plccpu` `-A01` at (100, 200). Page 2
/// "Inputs": eight drops 10 mm apart (x 100…170, y 250→150) through a terminal strip `-X01` at
/// y 160. Page 3 "Analogue": empty (cells go in space). Page 4 "Outputs": six drops with a strip at
/// y 160. Page 5 "IO-Link": empty.
pub(crate) struct P {
    pub(crate) h: H,
    pub(crate) cpu_page: PageId,
    pub(crate) inputs: PageId,
    pub(crate) analogue: PageId,
    pub(crate) outputs: PageId,
    pub(crate) iolink: PageId,
    pub(crate) rack: ComponentId,
}

pub(crate) fn fixture_p(seed: u64) -> P {
    let mut h = H::new(seed);
    let cpu_page = h.first_page();
    h.run("renamepage", json!({"page": cpu_page.hex(), "title": "PLC CPU"})).unwrap();
    let r = h.run("place", json!({"symbol": "plccpu", "at": [100, 200]})).unwrap();
    let rack: ComponentId = id(&r, "component");
    let inputs: PageId = id(&h.run("newpage", json!({"title": "Inputs"})).unwrap(), "page");
    h.activate(inputs);
    drops(&mut h, &IN_XS);
    h.run("terminals", json!({"strip": null, "from": [90, 160], "to": [260, 160]})).unwrap();
    let analogue: PageId = id(&h.run("newpage", json!({"title": "Analogue"})).unwrap(), "page");
    let outputs: PageId = id(&h.run("newpage", json!({"title": "Outputs"})).unwrap(), "page");
    h.activate(outputs);
    drops(&mut h, &OUT_XS);
    h.run("terminals", json!({"strip": null, "from": [90, 160], "to": [210, 160]})).unwrap();
    let iolink: PageId = id(&h.run("newpage", json!({"title": "IO-Link"})).unwrap(), "page");
    h.activate(cpu_page);
    P { h, cpu_page, inputs, analogue, outputs, iolink, rack }
}

/// The six card bodies of check A, by JSON form, on the CPU page: `plccm` `-A02` slot 1, `plcdi`
/// `-A03` slot 2, `plcdi` `-A04` slot 3, `plcai` `-A05` slot 4, `plciol` `-A06` slot 5, `plcdq`
/// `-A07` slot 6 (the rack and the next free slot are the defaults).
pub(crate) const CARDS: [(&str, &str, u64); 6] =
    [("plccm", "-A02", 1), ("plcdi", "-A03", 2), ("plcdi", "-A04", 3), ("plcai", "-A05", 4), ("plciol", "-A06", 5), ("plcdq", "-A07", 6)];

pub(crate) fn place_card(h: &mut H, symbol: &str, x: f64, extra: Value) -> Value {
    let mut p = json!({"symbol": symbol, "at": [x, 200]});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    h.run("place", p.clone()).unwrap_or_else(|e| panic!("place {p}: {e}"))
}

/// Fixture P with the six cards in slots 1…6 of `-A01`: returns the cards by tag.
pub(crate) fn with_cards(seed: u64) -> (P, BTreeMap<&'static str, ComponentId>) {
    let mut p = fixture_p(seed);
    p.h.activate(p.cpu_page);
    let mut cards = BTreeMap::new();
    for (i, (sym, t, _)) in CARDS.iter().enumerate() {
        let v = place_card(&mut p.h, sym, 140.0 + 40.0 * i as f64, json!({}));
        let c: ComponentId = id(&v, "component");
        assert_eq!(tag(&p.h.doc(), c), *t, "fixture P tags in order: {v}");
        cards.insert(*t, c);
    }
    (p, cards)
}

/// The DI fence of check F: card `-A03` across the eight input drops at y 200.
pub(crate) fn fence_inputs(p: &mut P, card: ComponentId) -> Value {
    p.h.activate(p.inputs);
    p.h.run("plcchannels", json!({"card": card.hex(), "from": [90, 200], "to": [260, 200]})).unwrap_or_else(|e| panic!("plcchannels fence: {e}"))
}
