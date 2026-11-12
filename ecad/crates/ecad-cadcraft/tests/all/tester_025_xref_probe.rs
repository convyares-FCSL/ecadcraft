//! Tester probes for brief 025 (device cross-references on the general reference index).
//!
//! What they try to break:
//! 1. The incremental index and the per-commit update map against a full rebuild, after seeded
//!    random edits across three pages (moves, rotations, mirrors, joins, erasures, offers,
//!    placements, renumbering, page moves, settings, cables, undo and redo), and through branch
//!    switch, merge and update-from-main.
//! 2. Save and reopen of `.ecad` v2: the index and every drawn reference part match; a project
//!    without the 025 properties saves back byte-identical.
//! 3. Contact labelling: offers, too many contacts, contacts with no coil, a coil with no
//!    contacts, duplicate tags.
//! 4. `translate.rs`: texts and tables move with their owner, are refused alone; rotate and
//!    mirror of a selection holding a coil.
//!
//! Oracles after each step: `RefIndex` equals `RefIndex::build`; every page whose drawn reference
//! parts changed is named by `changes_since` (the update map, design 025 §6); the harness's
//! projection checks; no command reports an internal error.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};

use crate::common::{H, id, pts};

use cadcraft_engine::Session;
use ecad_cadcraft::xref::{self, RefIndex};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_inspect::EventKind;
use ecad_model::{ComponentId, ConductorId, PageId, RepresentationId};
use serde_json::{Value, json};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        if v.is_empty() { None } else { v.get(self.below(v.len())) }
    }
}

struct X {
    h: H,
    pages: Vec<PageId>,
    k1: ComponentId,
    coil: RepresentationId,
    a2: ConductorId,
}

fn rep(v: &Value) -> RepresentationId {
    id(v, "representation")
}

/// Design 025 §10 fixture X, plus a third page "Field" (110) with one more `-K1` contact.
fn fixture(seed: u64) -> X {
    let mut h = H::new(seed);
    h.s.viewport_px = (1280.0, 800.0);
    let p100 = h.first_page();
    h.run("pageprops", json!({"page": p100.hex(), "number": "100", "title": "Supply"})).unwrap();
    let coil = h.run("place", json!({"symbol": "coil", "at": [75, 180]})).unwrap();
    let k1: ComponentId = id(&coil, "component");
    let a2: ConductorId = id(&h.run("wire", json!({"points": pts(&[(75.0, 170.0), (75.0, 150.0)])})).unwrap(), "conductor");
    h.run("wire", json!({"points": pts(&[(40.0, 150.0), (200.0, 150.0)])})).unwrap();
    h.run("place", json!({"symbol": "power3", "at": [125, 180], "device": k1.hex()})).unwrap();
    let p105: PageId = id(&h.run("newpage", json!({"title": "Motor control", "number": "105"})).unwrap(), "page");
    h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k1.hex()})).unwrap();
    h.run("place", json!({"symbol": "nc", "at": [100, 100], "device": k1.hex()})).unwrap();
    let mcb = h.run("place", json!({"symbol": "mcb1", "at": [250, 150]})).unwrap();
    let q1: ComponentId = id(&mcb, "component");
    h.run("place", json!({"symbol": "nc", "at": [300, 150], "device": q1.hex()})).unwrap();
    let p110: PageId = id(&h.run("newpage", json!({"title": "Field", "number": "110"})).unwrap(), "page");
    h.run("place", json!({"symbol": "no", "at": [150, 120], "device": k1.hex()})).unwrap();
    X { h, pages: vec![p100, p105, p110], k1, coil: rep(&coil), a2 }
}

/// Every page's drawn reference parts, derived from the document alone.
fn parts(h: &mut H) -> BTreeMap<PageId, Vec<xref::Part>> {
    h.p(|p| {
        let doc = p.ledger.document();
        doc.page_order.iter().map(|pg| (*pg, xref::parts_on_page(doc, *pg))).collect()
    })
}

fn index_is_a_rebuild(h: &mut H, what: &str) {
    let r = h.p(|p| {
        let doc = p.ledger.document();
        let fresh = RefIndex::build(doc);
        (p.xref.verify(doc), p.xref == fresh)
    });
    assert!(r.0.is_ok() && r.1, "{what}: {:?}", r.0);
}

/// The handles of representation `r`'s items on its (active) page.
fn item_handles(h: &mut H, r: RepresentationId) -> (PageId, Vec<String>) {
    let doc = h.doc();
    let Some(x) = doc.representations.get(&r) else { return (doc.page_order[0], Vec::new()) };
    let page = x.page;
    h.activate(page);
    let hs = h.p(|p| x.items.iter().filter_map(|i| p.pages[&page].map.item(*i)).map(|hd| hd.hex()).collect());
    (page, hs)
}

/// The handles of the drawn reference parts owned by `r` (its text, or its table).
fn xref_handles(h: &mut H, page: PageId, r: RepresentationId) -> Vec<String> {
    h.p(|p| (0u32..400).map_while(|i| p.pages[&page].map.xref(r, i)).map(|hd| hd.hex()).collect())
}

fn children(h: &mut H, c: ComponentId) -> Vec<RepresentationId> {
    h.p(|p| xref::device(p.ledger.document(), c).children)
}

fn components_with_head(h: &mut H) -> Vec<ComponentId> {
    h.p(|p| {
        let doc = p.ledger.document();
        doc.components.keys().copied().filter(|c| xref::device(doc, *c).head.is_some()).collect()
    })
}

fn bad(e: &str) -> bool {
    let l = e.to_ascii_lowercase();
    l.contains("internal") || l.contains("panic") || l.contains("differs from a rebuild")
}

const OFFERS: &[&str] = &["3P+1NO+1NC", "3P+2NO+1NC", "1NO", "2NC+2NO", "13/14 NO, 21/22 NC", "3P", "", "3X", "1P+1PNC+1NO"];
const MIRRORS: &[&str] = &["auto", "below", "right", "off", "at 10,-40", "at -30,5", "", "sideways"];

/// One random edit; returns its name (`None` when it did not apply).
fn step(x: &mut X, rng: &mut Rng, n: usize) -> Option<&'static str> {
    let d = [(rng.below(9) as f64 - 4.0) * 5.0, (rng.below(9) as f64 - 4.0) * 5.0];
    let r = |h: &mut H, cmd: &str, v: Value| {
        if let Err(e) = h.run(cmd, v.clone()) {
            assert!(!bad(&e), "step {n}: {cmd} {v}: {e}");
        }
    };
    let devices = components_with_head(&mut x.h);
    let dev = *rng.pick(&devices).unwrap_or(&x.k1);
    Some(match rng.below(20) {
        0 | 1 => {
            // Move a head symbol.
            let head = x.h.p(|p| xref::device(p.ledger.document(), dev).head)?;
            let (_, hs) = item_handles(&mut x.h, head);
            r(&mut x.h, "move", json!({"handles": hs, "delta": d}));
            "move head"
        }
        2 | 3 => {
            let ch = children(&mut x.h, dev);
            let c = *rng.pick(&ch)?;
            let (_, hs) = item_handles(&mut x.h, c);
            r(&mut x.h, "move", json!({"handles": hs, "delta": d}));
            "move contact"
        }
        4 => {
            // A window round the head and its table.
            let head = x.h.p(|p| xref::device(p.ledger.document(), dev).head)?;
            let (page, mut hs) = item_handles(&mut x.h, head);
            hs.extend(xref_handles(&mut x.h, page, head));
            r(&mut x.h, "move", json!({"handles": hs, "delta": d}));
            "window move head+table"
        }
        5 => {
            let all: Vec<RepresentationId> = {
                let mut v = children(&mut x.h, dev);
                v.extend(x.h.p(|p| xref::device(p.ledger.document(), dev).head));
                v
            };
            let one = *rng.pick(&all)?;
            let (page, mut hs) = item_handles(&mut x.h, one);
            hs.extend(xref_handles(&mut x.h, page, one));
            let at = x.h.p(|p| xref::anchor(p.ledger.document(), &p.ledger.document().representations[&one]))?;
            r(&mut x.h, "rotate", json!({"handles": hs, "base": [at.x, at.y], "angle": 90.0 * (1 + rng.below(3)) as f64}));
            "rotate symbol+parts"
        }
        6 => {
            let head = x.h.p(|p| xref::device(p.ledger.document(), dev).head)?;
            let (page, mut hs) = item_handles(&mut x.h, head);
            hs.extend(xref_handles(&mut x.h, page, head));
            let at = x.h.p(|p| xref::anchor(p.ledger.document(), &p.ledger.document().representations[&head]))?;
            r(&mut x.h, "mirror", json!({"handles": hs, "p1": [at.x + 5.0, at.y], "p2": [at.x + 5.0, at.y + 10.0], "erase": true}));
            "mirror head+table"
        }
        7 | 8 => {
            let page = *rng.pick(&x.pages)?;
            x.h.activate(page);
            let sym = if rng.below(2) == 0 { "no" } else { "nc" };
            let at = [20.0 + 15.0 * rng.below(20) as f64, 20.0 + 15.0 * rng.below(12) as f64];
            r(&mut x.h, "place", json!({"symbol": sym, "at": at, "device": dev.hex()}));
            "join contact"
        }
        9 => {
            let ch = children(&mut x.h, dev);
            if ch.len() < 2 {
                return None;
            }
            let c = *rng.pick(&ch)?;
            let (_, hs) = item_handles(&mut x.h, c);
            r(&mut x.h, "erase", json!({"handles": hs}));
            "erase contact"
        }
        10 => {
            let o = *rng.pick(OFFERS)?;
            r(&mut x.h, "setprop", json!({"component": dev.hex(), "key": "contacts", "value": o}));
            "set contacts"
        }
        11 => {
            let m = *rng.pick(MIRRORS)?;
            r(&mut x.h, "setprop", json!({"component": dev.hex(), "key": "mirror", "value": m}));
            "set mirror"
        }
        12 => {
            let page = *rng.pick(&x.pages)?;
            let num = format!("{}", 100 + 5 * rng.below(8));
            r(&mut x.h, "pageprops", json!({"page": page.hex(), "number": num}));
            "renumber page"
        }
        13 => {
            let page = *rng.pick(&x.pages)?;
            r(&mut x.h, "movepage", json!({"page": page.hex(), "index": 1 + rng.below(3)}));
            "move page"
        }
        14 => {
            if rng.below(2) == 0 {
                let m = *rng.pick(&["auto", "below", "right", "off"])?;
                r(&mut x.h, "projectprops", json!({"contact_mirror": m}));
            } else {
                let s = *rng.pick(&["page_column_row", "page_column"])?;
                r(&mut x.h, "projectprops", json!({"reference_style": s}));
            }
            "project settings"
        }
        15 => {
            // Two wires and a cable slash over them on a random page, with a reference end, or
            // continued onto another page.
            let page = *rng.pick(&x.pages)?;
            x.h.activate(page);
            let y = 30.0 + 10.0 * rng.below(6) as f64;
            r(&mut x.h, "wire", json!({"points": pts(&[(300.0, y), (360.0, y)])}));
            r(&mut x.h, "wire", json!({"points": pts(&[(300.0, y + 4.0), (360.0, y + 4.0)])}));
            let k = x.h.run("cableslash", json!({"from": [330, y - 3.0], "to": [332, y + 7.0]}));
            if let Ok(k) = k {
                let k = k["cable"].as_str().unwrap_or_default().to_string();
                r(&mut x.h, "cableref", json!({"cable": k, "at": [360, y + 2.0]}));
                let other = *rng.pick(&x.pages)?;
                if other != page {
                    x.h.activate(other);
                    r(&mut x.h, "cablecontinue", json!({"cable": k, "at": [20, y], "toward": [60, y]}));
                }
            }
            "cable"
        }
        16 => {
            // The coil's A2 wire: the auto table follows its run.
            let page = x.h.p(|p| p.ledger.document().representations.get(&x.coil).map(|r| r.page))?;
            x.h.activate(page);
            let doc = x.h.doc();
            let (item, _) = ecad_model::derive::route_on_page(&doc, x.a2, page)?;
            let hd = x.h.handle_of(page, item);
            r(
                &mut x.h,
                "grip.stretch",
                json!({"handle": hd.hex(), "index": 2, "to": [doc.representations[&x.coil].items.first().and_then(|_| xref::anchor(&doc, &doc.representations[&x.coil])).map_or(75.0, |a| a.x), 100.0 + 10.0 * rng.below(6) as f64]}),
            );
            "stretch A2 wire"
        }
        17 => {
            r(&mut x.h, "u", json!({}));
            "undo"
        }
        18 => {
            r(&mut x.h, "redo", json!({}));
            "redo"
        }
        _ => {
            let page = *rng.pick(&x.pages)?;
            x.h.activate(page);
            let at = [30.0 + 20.0 * rng.below(10) as f64, 220.0];
            r(&mut x.h, "place", json!({"symbol": "coil", "at": at}));
            "new coil"
        }
    })
}

/// Probe 1: seeded random edits; the kept index is a rebuild and the update map names every page
/// whose drawn references changed.
#[test]
#[ignore = "slow: random-edit index probe, about a minute in a debug build"]
fn tester_025_random_edits_keep_the_index_and_the_update_map() {
    let seed = std::env::var("ECAD_T025_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x25_0025_u64);
    let steps: usize = std::env::var("ECAD_T025_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(250);
    let mut x = fixture(seed);
    let mut rng = Rng(seed | 1);
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut misses: Vec<String> = Vec::new();
    let mut fidelity = BTreeMap::<&str, usize>::new();
    for n in 0..steps {
        let before = parts(&mut x.h);
        let mark = x.h.p(|p| p.change_count());
        let fid0 = x.h.p(|p| p.events.count(EventKind::FidelityMismatch));
        let Some(kind) = step(&mut x, &mut rng, n) else { continue };
        *counts.entry(kind).or_default() += 1;
        index_is_a_rebuild(&mut x.h, &format!("step {n} ({kind})"));
        let after = parts(&mut x.h);
        let named = x.h.p(|p| p.changes_since(mark));
        if let Some(named) = named {
            for (page, a) in &after {
                if before.get(page) != Some(a) && !named.contains(page) {
                    misses.push(format!("step {n} ({kind}): page {} changed, not named", page.hex()));
                }
            }
        }
        let fid = x.h.p(|p| p.events.count(EventKind::FidelityMismatch)) - fid0;
        if fid > 0 {
            *fidelity.entry(kind).or_default() += fid;
        }
    }
    eprintln!("t025 seed {seed:#x}: {counts:?}; fidelity mismatches by kind {fidelity:?}; update-map misses {}", misses.len());
    assert!(misses.is_empty(), "{misses:#?}");
}

// ------------------------------------------------------------------ branches

fn raw(h: &mut H, cmd: &str, params: Value) -> Result<Value, String> {
    let r = h.s.execute(cmd, &params).map_err(|e| e.to_string());
    h.check();
    r
}

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-t025-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn close_all(s: &mut Session) {
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
}

/// Probe 1b: branch switch, merge and update-from-main keep the index a rebuild.
#[test]
#[ignore = "slow: branch round trips on a saved fixture"]
fn tester_025_branch_switch_merge_and_update_keep_the_index() {
    let dir = TempDir::new("branch");
    let mut x = fixture(2501);
    x.h.cfg(|p| p.persistence = Persistence::File);
    x.h.run("saveas", json!({"path": dir.path("b.ecad")})).unwrap();
    raw(&mut x.h, "branch", json!({"name": "B"})).unwrap();
    // On B: an offer, a moved table, a new contact on 110, page 105 renumbered.
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3P+2NO+1NC"})).unwrap();
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "mirror", "value": "right"})).unwrap();
    x.h.activate(x.pages[2]);
    x.h.run("place", json!({"symbol": "nc", "at": [200, 60], "device": x.k1.hex()})).unwrap();
    x.h.run("pageprops", json!({"page": x.pages[1].hex(), "number": "106"})).unwrap();
    index_is_a_rebuild(&mut x.h, "on B");
    let on_b = parts(&mut x.h);
    raw(&mut x.h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    index_is_a_rebuild(&mut x.h, "back on main");
    // On main: move the coil, so the merge brings B's changes into a moved table.
    let (_, hs) = item_handles(&mut x.h, x.coil);
    x.h.run("move", json!({"handles": hs, "delta": [10, 0]})).unwrap();
    index_is_a_rebuild(&mut x.h, "main moved");
    x.h.run("qsave", Value::Null).unwrap();
    let m = raw(&mut x.h, "merge", json!({"from": "B"}));
    assert!(m.is_ok(), "{m:?}");
    index_is_a_rebuild(&mut x.h, "after merge");
    assert!(x.h.doc().components[&x.k1].properties.contains_key("contacts"));
    raw(&mut x.h, "u", json!({})).ok();
    raw(&mut x.h, "undomerge", json!({})).ok();
    index_is_a_rebuild(&mut x.h, "after undoing the merge");
    raw(&mut x.h, "redo", json!({})).ok();
    index_is_a_rebuild(&mut x.h, "after redoing the merge");
    // Update a second branch from main.
    x.h.run("qsave", Value::Null).ok();
    raw(&mut x.h, "branch", json!({"name": "C"})).unwrap();
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "1NO"})).unwrap();
    raw(&mut x.h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    x.h.run("pageprops", json!({"page": x.pages[2].hex(), "number": "120"})).unwrap();
    x.h.run("qsave", Value::Null).unwrap();
    raw(&mut x.h, "switchbranch", json!({"branch": "C", "save": true})).unwrap();
    index_is_a_rebuild(&mut x.h, "on C");
    let u = raw(&mut x.h, "merge", json!({"from": "main", "into": "C"}));
    assert!(u.is_ok(), "{u:?}");
    index_is_a_rebuild(&mut x.h, "after update from main");
    let _ = on_b;
    close_all(&mut x.h.s);
}

// ------------------------------------------------------------------ save and reopen

/// Probe 2: save and reopen keep the index and every drawn reference part; a project without
/// the 025 properties saves back byte-identical, and opening adds no transaction.
#[test]
fn tester_025_save_reopen_keeps_index_and_texts() {
    let dir = TempDir::new("reopen");
    let path = dir.path("x.ecad");
    let mut x = fixture(2502);
    x.h.cfg(|p| p.persistence = Persistence::File);
    x.h.run("saveas", json!({"path": path})).unwrap();
    let parts_before = parts(&mut x.h);
    let idx_before = x.h.p(|p| p.xref.clone());
    let head_before = x.h.head();
    close_all(&mut x.h.s);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 99).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(h2.head(), head_before, "opening records nothing");
    assert_eq!(h2.p(|p| p.xref.clone()), idx_before);
    assert_eq!(parts(&mut h2), parts_before);
    for p in parts_before.keys() {
        h2.activate(*p);
        h2.check();
    }
    // Nothing of 025 is written on its own: no offer, no placement, and the project default
    // (`auto`) is left out of the settings. (A qsave always writes a new session record, so the
    // file bytes are not compared.)
    let doc = h2.doc();
    assert!(doc.components.values().all(|c| !c.properties.contains_key("contacts") && !c.properties.contains_key("mirror")));
    let settings = serde_json::to_value(&doc.settings).unwrap();
    assert!(settings.get("contact_mirror").is_none(), "{settings}");
    // With an offer, a moved table and a project default, a second round trip keeps them all.
    h2.cfg(|p| p.persistence = Persistence::File);
    h2.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3P+3NO+1NC"})).unwrap();
    h2.run("mirrormove", json!({"component": x.k1.hex(), "at": [60, 120]})).unwrap();
    h2.run("projectprops", json!({"contact_mirror": "below"})).unwrap();
    h2.run("qsave", Value::Null).unwrap();
    let parts2 = parts(&mut h2);
    let idx2 = h2.p(|p| p.xref.clone());
    let doc2 = h2.doc();
    close_all(&mut h2.s);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 98).unwrap();
    let mut h3 = H { s, project, trail: Vec::new() };
    assert_eq!(h3.doc(), doc2);
    assert_eq!(h3.p(|p| p.xref.clone()), idx2);
    assert_eq!(parts(&mut h3), parts2);
    assert!(
        parts2.values().flatten().any(|p| matches!(&p.shape, xref::Shape::Text { value, .. } if value == xref::SPARE_PLACE)),
        "the spare row survives"
    );
    close_all(&mut h3.s);
}

/// Probe 2b: the golden v1 project from before 025 opens with no transaction and no error.
#[test]
fn tester_025_old_v1_golden_opens_with_an_index_and_no_transaction() {
    let golden = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ecad-file/tests/golden/project-v1.ecad");
    let dir = TempDir::new("golden");
    let path = dir.path("old.ecad");
    std::fs::copy(&golden, &path).unwrap();
    let (_, v2) = ecad_cadcraft::file::convert(&path, None, "tester-025").unwrap();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &v2, 7).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    index_is_a_rebuild(&mut h, "golden v1");
    let log_len = h.p(|p| p.ledger.log().len());
    let pages = h.doc().page_order.clone();
    for p in pages {
        h.activate(p);
        h.check();
    }
    assert_eq!(h.p(|p| p.ledger.log().len()), log_len, "showing pages records nothing");
    close_all(&mut h.s);
}

// ------------------------------------------------------------------ contact labelling

fn rows(h: &mut H, c: ComponentId) -> Vec<(String, String, String)> {
    let v = h.run("contacts", json!({"component": c.hex()})).unwrap();
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["labels"].as_str().unwrap().into(), r["kind"].as_str().unwrap().into(), r["state"].as_str().unwrap().into()))
        .collect()
}

/// Probe 3a: the offer grammar's IEC labels and the join labels that follow from them.
#[test]
fn tester_025_offer_labels() {
    let labels =
        |t: &str| xref::parse_offer(t).map(|v| v.iter().map(|o| format!("{}/{}{}", o.labels[0], o.labels[1], o.kind.word())).collect::<Vec<_>>());
    assert_eq!(labels("3P+2NO+1NC").unwrap(), ["1/2NO", "3/4NO", "5/6NO", "13/14NO", "23/24NO", "31/32NC"]);
    assert_eq!(labels(" 3p , 1no + 1nc ").unwrap(), ["1/2NO", "3/4NO", "5/6NO", "13/14NO", "21/22NC"]);
    assert_eq!(labels("1P+1PNC").unwrap(), ["1/2NO", "3/4NC"]);
    for bad in ["", "+3P", "3P+", "0NO", "33NO", "16NO+16NC+1P", "3PNO", "13/14", "/14 NO", "1/2/3 NO", "99999999999999999999999P"] {
        assert!(xref::parse_offer(bad).is_err(), "{bad:?} accepted");
    }
    assert!(xref::parse_offer("32NO").is_ok());
}

/// Bug: an explicit pole written before a counted one gets the same labels, so the part offers
/// `13/14` twice and the table shows a spare `13/14` beside the drawn one.
#[test]
fn tester_025_bug_explicit_then_counted_offer_duplicates_labels() {
    let o = xref::parse_offer("13/14 NO + 1NO").unwrap();
    let pairs: BTreeSet<_> = o.iter().map(|p| p.labels.clone()).collect();
    assert_eq!(pairs.len(), o.len(), "duplicate labels offered: {o:?}");
    let o = xref::parse_offer("1/2 P + 3P").unwrap();
    let pairs: BTreeSet<_> = o.iter().map(|p| p.labels.clone()).collect();
    assert_eq!(pairs.len(), o.len(), "duplicate power labels offered: {o:?}");
}

/// Probe 3b: too many contacts, contacts with no coil, a coil with no contacts, duplicate tags.
#[test]
fn tester_025_counts_orphans_and_duplicate_tags() {
    let mut x = fixture(2503);
    // An offer of 1 power pole while three are drawn: two over rows and one warning row.
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "1P+1NO+1NC"})).unwrap();
    let r = rows(&mut x.h, x.k1);
    assert_eq!(r.iter().filter(|r| r.2 == "over").count(), 3, "two power poles and the 110 NO are over: {r:?}");
    let checks = x.h.run("checks", json!({"kinds": ["too-many-contacts"]})).unwrap();
    let list = checks["checks"].as_array().unwrap();
    // One row per kind over its offer (design 025 §4.3).
    let which: Vec<&str> = list.iter().map(|c| c["which"].as_str().unwrap()).collect();
    assert_eq!(which.len(), 2, "{checks}");
    assert!(which.iter().any(|w| w.contains("3 power NO drawn, part offers 1")), "{which:?}");
    assert!(which.iter().any(|w| w.contains("2 auxiliary NO drawn, part offers 1")), "{which:?}");
    // A contact with no coil: no text, no table, no reference, no check.
    let p105 = x.pages[1];
    x.h.activate(p105);
    let lone = x.h.run("place", json!({"symbol": "no", "at": [30, 30]})).unwrap();
    let lone_c: ComponentId = id(&lone, "component");
    assert!(xref_handles(&mut x.h, p105, rep(&lone)).is_empty());
    let v = x.h.run("xrefs", json!({"component": lone_c.hex()})).unwrap();
    assert!(v["references"].as_array().unwrap().is_empty(), "{v}");
    let v = x.h.run("contacts", json!({"component": lone_c.hex()})).unwrap();
    assert!(v["head"].is_null(), "{v}");
    // A coil with no contacts and no offer: no table; with an offer: spare rows only.
    let k2 = x.h.run("place", json!({"symbol": "coil", "at": [200, 220]})).unwrap();
    let k2c: ComponentId = id(&k2, "component");
    assert!(xref_handles(&mut x.h, p105, rep(&k2)).is_empty(), "no table for a bare coil");
    x.h.run("setprop", json!({"component": k2c.hex(), "key": "contacts", "value": "2NO"})).unwrap();
    assert!(rows(&mut x.h, k2c).iter().all(|r| r.2 == "spare"));
    assert!(!xref_handles(&mut x.h, p105, rep(&k2)).is_empty(), "a table of spares");
    // Duplicate tags: give -K2 the tag of -K1 (merge refused or asked): the two devices stay
    // apart in the index.
    let tag = x.h.doc().components[&x.k1].properties.get("tag").cloned().unwrap_or_default();
    let _ = x.h.run("setprop", json!({"component": k2c.hex(), "key": "tag", "value": tag}));
    index_is_a_rebuild(&mut x.h, "duplicate tag");
    let groups: Vec<_> = x.h.p(|p| p.xref.groups().collect());
    assert!(groups.contains(&xref::Group::Device(x.k1)) && groups.contains(&xref::Group::Device(k2c)), "{groups:?}");
}

// ------------------------------------------------------------------ translate.rs

/// Probe 4: texts and tables are refused alone (move, rotate, mirror, erase, copy); they follow
/// a rotated or mirrored coil; a copy of a coil with its table copies no derived parts.
#[test]
fn tester_025_parts_follow_their_owner_and_are_refused_alone() {
    let mut x = fixture(2504);
    let p100 = x.pages[0];
    x.h.activate(p100);
    let table = xref_handles(&mut x.h, p100, x.coil);
    assert!(table.len() > 10, "{table:?}");
    let head = x.h.head();
    for (cmd, v) in [
        ("move", json!({"handles": table, "delta": [5, 0]})),
        ("rotate", json!({"handles": table, "base": [75, 146], "angle": 90})),
        ("mirror", json!({"handles": table, "p1": [80, 0], "p2": [80, 10], "erase": true})),
        ("erase", json!({"handles": table})),
        ("move", json!({"handles": [table[3].clone()], "delta": [5, 0]})),
    ] {
        let e = x.h.run(cmd, v).unwrap_err();
        assert!(e.contains(xref::TABLE_DERIVED), "{cmd}: {e}");
    }
    assert_eq!(x.h.head(), head);
    // Rotate the coil with its table selected: one transaction; the table is drawn again at the
    // rotated coil's placement.
    let (_, coil_h) = item_handles(&mut x.h, x.coil);
    let mut sel = coil_h.clone();
    sel.extend(table.clone());
    x.h.run("rotate", json!({"handles": sel, "base": [75, 180], "angle": 90})).unwrap();
    assert_eq!(x.h.head(), head + 1);
    index_is_a_rebuild(&mut x.h, "rotate");
    let now = parts(&mut x.h);
    assert!(now[&p100].iter().any(|p| p.rep == x.coil), "the rotated coil still has its table");
    // Mirror (keep the source) of the coil with its table: no derived part is copied; the copy is
    // a symbol of its own and the index matches.
    let (_, coil_h) = item_handles(&mut x.h, x.coil);
    let mut sel = coil_h;
    sel.extend(xref_handles(&mut x.h, p100, x.coil));
    let before = x.h.head();
    let m = x.h.run("mirror", json!({"handles": sel, "p1": [150, 0], "p2": [150, 10], "erase": false}));
    if let Err(e) = &m {
        assert!(!bad(e), "{e}");
    } else {
        assert_eq!(x.h.head(), before + 1);
    }
    index_is_a_rebuild(&mut x.h, "mirror copy");
    // A contact's text alone is refused; with its contact it moves with it.
    let p105 = x.pages[1];
    let no = children(&mut x.h, x.k1).into_iter().find(|r| x.h.doc().representations[r].page == p105).unwrap();
    let (_, no_h) = item_handles(&mut x.h, no);
    let text = xref_handles(&mut x.h, p105, no);
    assert_eq!(text.len(), 1);
    let e = x.h.run("move", json!({"handles": text, "delta": [5, 0]})).unwrap_err();
    assert!(e.contains(xref::TEXT_DERIVED), "{e}");
    let at0 = parts(&mut x.h)[&p105].iter().find(|p| p.rep == no).map(|p| p.shape.clone());
    let mut sel = no_h;
    sel.extend(text);
    x.h.run("move", json!({"handles": sel, "delta": [10, 0]})).unwrap();
    let at1 = parts(&mut x.h)[&p105].iter().find(|p| p.rep == no).map(|p| p.shape.clone());
    match (at0, at1) {
        (Some(xref::Shape::Text { at: a, .. }), Some(xref::Shape::Text { at: b, .. })) => {
            assert!((b.x - a.x - 10.0).abs() < 1e-9 && (b.y - a.y).abs() < 1e-9)
        }
        o => panic!("{o:?}"),
    }
    index_is_a_rebuild(&mut x.h, "contact move");
}

/// Bug: moving a coil (or a contact) through a drawing command records a `FidelityMismatch`
/// event: the commit path's amended set leaves out the reference parts (`refresh_page` adds fan,
/// reference text, tag and label handles, not `Mapped::Xref` ones), so every moved table or
/// contact text is reported as the engine's drawing differing from the projection (016 H2).
#[test]
fn tester_025_bug_moving_a_coil_records_fidelity_mismatches() {
    let mut x = fixture(2505);
    let (_, hs) = item_handles(&mut x.h, x.coil);
    x.h.run("move", json!({"handles": hs, "delta": [10, 0]})).unwrap();
    let p105 = x.pages[1];
    let no = children(&mut x.h, x.k1).into_iter().find(|r| x.h.doc().representations[r].page == p105).unwrap();
    let (_, hs) = item_handles(&mut x.h, no);
    x.h.run("move", json!({"handles": hs, "delta": [10, 0]})).unwrap();
    let m: Vec<_> = x.h.p(|p| p.events.events().filter(|e| e.kind == EventKind::FidelityMismatch).cloned().collect());
    assert!(m.is_empty(), "{m:?}");
}

/// Bug (wording): copying a coil's table alone (COPY, or MIRROR keeping the source) is refused
/// with the contact-text message instead of the table's: the refusal looks up the copies'
/// handles, which are not in the map, so `xref::refusal(doc, None)` falls back to the text line.
#[test]
fn tester_025_bug_copying_a_table_alone_says_text_not_table() {
    let mut x = fixture(2506);
    let p100 = x.pages[0];
    x.h.activate(p100);
    let table = xref_handles(&mut x.h, p100, x.coil);
    for (cmd, v) in
        [("copy", json!({"handles": table, "delta": [0, -50]})), ("mirror", json!({"handles": table, "p1": [80, 0], "p2": [80, 10], "erase": false}))]
    {
        let e = x.h.run(cmd, v).unwrap_err();
        assert!(e.contains(xref::TABLE_DERIVED), "{cmd}: {e}");
    }
}

/// Bug: a device with no head (a lone contact, a push button) still gets a `too-many-contacts`
/// warning when it has an offer, though design 025 §12 Q2 (accepted) says a device without a head
/// shows no references and no check.
#[test]
fn tester_025_bug_headless_device_with_an_offer_is_checked() {
    let mut x = fixture(2507);
    x.h.activate(x.pages[1]);
    let lone = x.h.run("place", json!({"symbol": "no", "at": [30, 30]})).unwrap();
    let c: ComponentId = id(&lone, "component");
    x.h.run("setprop", json!({"component": c.hex(), "key": "contacts", "value": "1NC"})).unwrap();
    let v = x.h.run("checks", json!({"kinds": ["too-many-contacts"]})).unwrap();
    let mine: Vec<&Value> = v["checks"].as_array().unwrap().iter().filter(|r| r["object"]["component"] == json!(c.hex())).collect();
    assert!(mine.is_empty(), "{mine:?}");
}
