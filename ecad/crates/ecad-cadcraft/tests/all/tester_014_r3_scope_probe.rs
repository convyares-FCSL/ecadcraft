//! Tester probe for brief 014 re-test round 3: brief 014's scoped movement merged with briefs 009
//! and 010.
//!
//! The probe builds a two-page fixture: cable C1 runs from K1 on `Supply` to K2 on `Field` through
//! a reference on each page. It then makes 400 seeded random drawing edits on both pages:
//! - moving components, stubs and reference points;
//! - dragging ends by their grips onto terminals and into free space;
//! - window-style moves of a component together with its wires;
//! - rotations, and an occasional undo.
//!
//! The test oracle (`translate-oracle`, on through this crate's dev-dependencies) compares every
//! translation with the pre-014 code and aborts on any difference. The harness runs its full
//! project checks after each command.
//!
//! After each edit the probe also checks three things:
//! - Every end attached on the other page is still attached: the M3 gate never detaches it.
//! - Each drawn reference text reads `/page.column`, or `/?` for an orphan.
//! - The 010 note is echoed when a stub end leaving toward a reference is dropped on a terminal.
#![cfg(not(target_arch = "wasm32"))]

use crate::common;

use cadcraft_engine::doc::Handle;
use common::{H, id};
use ecad_cadcraft::references::markers_on_page;
use ecad_model::derive::{cable_references, terminal_positions};
use ecad_model::{CableId, ConductorId, End, PageId, Pt, TerminalId};
use serde_json::json;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn coord(&mut self) -> f64 {
        (self.below(161) as f64 - 80.0) * 0.5
    }
}

fn component(h: &mut H, page: PageId, outline: &str, at: [(f64, f64); 3], name: &str) -> [TerminalId; 3] {
    h.cmdline(outline);
    let lines = h.lines_on(page);
    let r = h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    let c = r["component"].as_str().unwrap().to_string();
    h.run("setprop", json!({"component": c, "key": "tag", "value": name})).unwrap();
    at.map(|(x, y)| id(&h.run("addterminal", json!({"handle": lines[0].hex(), "at": [x, y]})).unwrap(), "terminal"))
}

struct R {
    h: H,
    supply: PageId,
    field: PageId,
    c1: CableId,
    w: [ConductorId; 3],
    k1: [TerminalId; 3],
    k2: [TerminalId; 3],
}

/// Fixture R of design 010 §11, connected on both pages.
fn connected(seed: u64) -> R {
    let mut h = H::new(seed);
    h.s.viewport_px = (1280.0, 800.0);
    let supply = h.first_page();
    h.run("renamepage", json!({"page": supply.hex(), "title": "Supply"})).unwrap();
    let k1 = component(&mut h, supply, "line -10,-5 0,-5 0,15 -10,15 c", [(0.0, 0.0), (0.0, 5.0), (0.0, 10.0)], "K1");
    let mut w = Vec::new();
    for y in [0.0, 5.0, 10.0] {
        let c: ConductorId = id(&h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap(), "conductor");
        h.run("setprop", json!({"conductor": c.hex(), "key": "tag", "value": format!("W{y}")})).unwrap();
        w.push(c);
    }
    let c1: CableId =
        id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap(), "cable");
    let field: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    let k2 = component(
        &mut h,
        field,
        "line 100,-5 110,-5 110,15 100,15 c",
        [(100.0, 0.0), (100.0, 5.0), (100.0, 10.0)],
        "K2",
    );
    h.activate(supply);
    let _ = h.s.zoom_extents();
    h.run("cableref", json!({"cable": c1.hex(), "at": [65, 5]})).unwrap();
    h.activate(field);
    h.run("cablecontinue", json!({"cable": c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    for (c, t) in w.iter().zip(k2) {
        h.run("connect", json!({"conductor": c.hex(), "end": "B", "terminal": t.hex()})).unwrap();
    }
    R { h, supply, field, c1, w: [w[0], w[1], w[2]], k1, k2 }
}

fn route(h: &mut H, page: PageId, c: ConductorId) -> Option<(Handle, Vec<Pt>)> {
    let doc = h.doc();
    let (item, prim) = ecad_model::derive::route_on_page(&doc, c, page)?;
    let pts = match &prim {
        ecad_model::Primitive::Polyline { vertices, .. } => vertices.clone(),
        ecad_model::Primitive::Line { a, b, .. } => vec![*a, *b],
        _ => return None,
    };
    Some((h.p(|p| p.pages[&page].map.item(item))?, pts))
}

fn terminals_on(h: &mut H, page: PageId) -> Vec<Pt> {
    terminal_positions(&h.doc(), page).into_iter().map(|(_, _, p)| p).collect()
}

/// `/page.column` or `/?`.
fn well_formed(text: &str) -> bool {
    if text == "/?" {
        return true;
    }
    let Some(rest) = text.strip_prefix('/') else { return false };
    let mut parts = rest.split('.');
    let (Some(p), Some(c), None) = (parts.next(), parts.next(), parts.next()) else { return false };
    !p.is_empty() && !c.is_empty() && p.chars().all(|x| x.is_ascii_digit()) && c.chars().all(|x| x.is_ascii_digit())
}

#[test]
#[ignore = "slow: random-edit oracle probe, about 37 s"]
fn tester_014_r3_random_edits_across_two_pages_match_the_oracle() {
    let seed = std::env::var("ECAD_R3_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x14_3010_u64);
    let mut r = connected(seed);
    let mut rng = Rng(seed | 1);
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut notes = 0usize;
    let mut texts_seen = std::collections::BTreeSet::<String>::new();
    for step in 0..400 {
        let page = if rng.below(3) == 0 { r.supply } else { r.field };
        r.h.activate(page);
        let wi = rng.below(3);
        let delta = [rng.coord(), rng.coord()];
        let n_log = r.h.s.log.len();
        // Ends attached to a terminal not shown on this page before the edit.
        let doc0 = r.h.doc();
        let here: std::collections::BTreeSet<TerminalId> =
            terminal_positions(&doc0, page).into_iter().map(|(t, _, _)| t).collect();
        let elsewhere: Vec<(ConductorId, End, TerminalId)> =
            r.w.iter()
                .flat_map(|c| [End::A, End::B].map(|e| (*c, e)))
                .filter_map(|(c, e)| doc0.conductors[&c].end(e).map(|t| (c, e, t)))
                .filter(|(_, _, t)| !here.contains(t))
                .collect();
        let kind: &str = match rng.below(9) {
            0 => {
                // Move the page's component (K1 or K2) by its handles.
                let doc = r.h.doc();
                let terms = if page == r.supply { r.k1 } else { r.k2 };
                let Some(comp) = doc.terminals.get(&terms[0]).map(|t| t.component) else { continue };
                let hs = r.h.rep_handles(page, comp);
                let _ = r.h.run("move", json!({"handles": hs, "delta": delta}));
                "move component"
            }
            1 => {
                // Move a whole route (a stub on Field: end A leaves toward the reference).
                let Some((h, _)) = route(&mut r.h, page, r.w[wi]) else { continue };
                let _ = r.h.run("move", json!({"handles": [h.hex()], "delta": delta}));
                "move route"
            }
            2 | 3 => {
                // Drag one end by its grip onto a terminal of this page, or into free space.
                let Some((h, pts)) = route(&mut r.h, page, r.w[wi]) else { continue };
                let last = pts.len() - 1;
                let index = if rng.below(2) == 0 { 0 } else { 2 * last };
                let terms = terminals_on(&mut r.h, page);
                let to = if !terms.is_empty() && rng.below(2) == 0 {
                    let t = terms[rng.below(terms.len())];
                    [t.x, t.y]
                } else {
                    [rng.coord() + 40.0, rng.coord()]
                };
                let _ = r.h.run("grip.stretch", json!({"handle": h.hex(), "index": index, "to": to}));
                "drag one end"
            }
            4 => {
                // Move the route so its first end lands exactly on a terminal of this page.
                let Some((h, pts)) = route(&mut r.h, page, r.w[wi]) else { continue };
                let terms = terminals_on(&mut r.h, page);
                if terms.is_empty() {
                    continue;
                }
                let t = terms[rng.below(terms.len())];
                let _ = r.h.run("move", json!({"handles": [h.hex()], "delta": [t.x - pts[0].x, t.y - pts[0].y]}));
                "move route onto a terminal"
            }
            5 => {
                // Move the reference point (the text itself refuses to move).
                let doc = r.h.doc();
                let Some(rid) = cable_references(&doc).into_iter().find(|x| x.page == page).map(|x| x.reference) else {
                    continue;
                };
                let Some(item) = doc.representations.get(&rid).and_then(|x| x.items.first().copied()) else { continue };
                let hp = r.h.handle_of(page, item);
                let _ = r.h.run("move", json!({"handles": [hp.hex()], "delta": delta}));
                "move reference point"
            }
            6 => {
                // The component with every route on the page, as a crossing window would select it.
                let doc = r.h.doc();
                let terms = if page == r.supply { r.k1 } else { r.k2 };
                let Some(comp) = doc.terminals.get(&terms[0]).map(|t| t.component) else { continue };
                let mut hs = r.h.rep_handles(page, comp);
                for c in r.w {
                    if let Some((h, _)) = route(&mut r.h, page, c) {
                        hs.push(h.hex());
                    }
                }
                let _ = r.h.run("move", json!({"handles": hs, "delta": delta}));
                "move component with wires"
            }
            7 => {
                let Some((h, pts)) = route(&mut r.h, page, r.w[wi]) else { continue };
                let _ = r.h.run(
                    "rotate",
                    json!({"handles": [h.hex()], "base": [pts[0].x, pts[0].y], "angle": 90.0 * (1 + rng.below(3)) as f64}),
                );
                "rotate route"
            }
            _ => {
                let _ = r.h.run("undo", json!({"count": 1}));
                "undo"
            }
        };
        *counts.entry(kind).or_default() += 1;
        let log = r.h.s.log.get(n_log..).unwrap_or_default().join("\n");
        if log.contains("is connected on another page, so it was not connected here") {
            notes += 1;
        }
        // Ends attached on the other page are never detached by an edit on this page.
        let doc = r.h.doc();
        if kind != "undo" {
            for (c, e, t) in &elsewhere {
                let now = doc.conductors[c].end(*e);
                assert_eq!(now, Some(*t), "step {step} ({kind}): end {e:?} of {c:?} attached elsewhere changed");
                *counts.entry("checked ends elsewhere").or_default() += 1;
            }
        }
        for p in [r.supply, r.field] {
            for m in markers_on_page(&doc, p) {
                assert!(well_formed(&m.text), "step {step} ({kind}): reference text {:?}", m.text);
                texts_seen.insert(m.text.clone());
            }
        }
        assert!(r.h.p(|p| p.ledger.replay_matches().unwrap()), "step {step}: replay");
        // Worker, 014 fix round 3: both pages are open; a commit redraws only the page shown and
        // marks the other stale when its reference ends changed. The page shown must equal a fresh
        // projection now, and each page must once it is brought to the front.
        r.h.check_with(true);
        for p in [r.supply, r.field] {
            r.h.activate(p);
            assert!(!r.h.p(|x| x.pages[&p].stale), "step {step}: page stale in front");
            r.h.check_with(true);
        }
        // And the per-cable derivation is the whole-document one, cable by cable.
        let all = ecad_model::derive::cable_references(&doc);
        for k in doc.cables.keys() {
            let one = ecad_model::derive::cable_references_of(&doc, &std::collections::BTreeSet::from([*k]));
            let want: Vec<_> = all.iter().filter(|x| x.cable == *k).cloned().collect();
            assert_eq!(one, want, "step {step}: references of cable {k:?}");
        }
        let every: std::collections::BTreeSet<_> = doc.cables.keys().copied().collect();
        assert_eq!(ecad_model::derive::cable_references_of(&doc, &every), all, "step {step}: all cables");
    }
    let _ = r.c1;
    eprintln!("r3 probe seed {seed:#x}: {counts:?}, 010 notes {notes}, texts {texts_seen:?}");
    assert!(notes > 0, "no edit dropped a leaving end on a terminal");
}
