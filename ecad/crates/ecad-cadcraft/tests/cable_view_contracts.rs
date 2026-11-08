//! Brief 005 (traditional / fan cable view), written from the brief and design 005 (§2 rules, §3
//! switching, §4 what each view shows, §5 fan geometry, §6 editing in fan view, §7 remembering).
//!
//! Fixture G (design §9): two drawn components with terminals at x = 0 and x = 60, y = 0, 5, 10,
//! three attached wires W0, W5, W10 between them, and the slash C1 from (28,-3) to (32,18).
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::path::PathBuf;

use cadcraft_engine::doc::{Entity, EntityKind, Handle};
use cadcraft_engine::ext::MarkShape;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use common::{H, id};
use ecad_cadcraft::interact::{hover_lines, overlay_marks};
use ecad_cadcraft::views::{self, FAN_REFUSED, FanOnPage, Settings, View};
use ecad_cadcraft::{Mapped, Persistence, ProjectHandle};
use ecad_model::{CableId, ConductorId, ItemId, PageId, Pt, Transaction};
use serde_json::{Value, json};

const SEED: u64 = 50_05;

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-views-{name}-{}", ecad_store::disk::unique()));
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

struct G {
    h: H,
    page: PageId,
    c1: CableId,
    slash: ItemId,
    wires: [ConductorId; 3],
}

fn rectangle(h: &mut H, page: PageId, line: &str) -> Vec<Handle> {
    let before = h.lines_on(page);
    h.cmdline(line);
    h.lines_on(page).into_iter().filter(|x| !before.contains(x)).collect()
}

fn component(h: &mut H, page: PageId, line: &str, x: f64) {
    let lines = rectangle(h, page, line);
    h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    for y in [0.0, 5.0, 10.0] {
        h.run("addterminal", json!({"handle": lines[0].hex(), "at": [x, y]})).unwrap();
    }
}

fn wire(h: &mut H, points: Value, tag: &str) -> ConductorId {
    let c: ConductorId = id(&h.run("wire", json!({ "points": points })).unwrap(), "conductor");
    h.run("setprop", json!({"conductor": c.hex(), "key": "tag", "value": tag})).unwrap();
    c
}

fn fixture() -> G {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    let page = h.first_page();
    component(&mut h, page, "line -10,-5 0,-5 0,15 -10,15 c", 0.0);
    component(&mut h, page, "line 60,-5 70,-5 70,15 60,15 c", 60.0);
    let wires = [
        wire(&mut h, json!([[0, 0], [60, 0]]), "W0"),
        wire(&mut h, json!([[0, 5], [60, 5]]), "W5"),
        wire(&mut h, json!([[0, 10], [60, 10]]), "W10"),
    ];
    let d = h.doc();
    assert!(wires.iter().all(|w| d.conductors[w].ends.iter().all(Option::is_some)), "attached at both ends");
    let r = h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap();
    h.run("zoom", json!({"mode": "window", "p1": [-20, -30], "p2": [80, 50]})).unwrap();
    G { h, page, c1: id(&r, "cable"), slash: id(&r, "item"), wires }
}

fn view(h: &mut H, k: CableId, v: &str) -> Value {
    h.run("cableview", json!({"cable": k.hex(), "view": v})).unwrap()
}

fn views_of(h: &mut H) -> Value {
    h.run("cableview", json!({})).unwrap()["views"].clone()
}

fn history(h: &mut H) -> Vec<Transaction> {
    h.p(|p| p.ledger.log().iter().map(|t| Transaction { time_ms: None, ..t.clone() }).collect())
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn click(h: &mut H, x: f64, y: f64) {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    h.s.input(Input::Point(at)).unwrap();
    h.check();
}

fn last_echo(h: &H) -> String {
    h.s.log.last().cloned().unwrap_or_default()
}

fn uid(h: &mut H, page: PageId) -> u64 {
    h.p(|p| p.pages[&page].uid).expect("page open")
}

fn entities(h: &mut H, page: PageId) -> Vec<Entity> {
    h.pv(|p, pg| pg.entities_of(p, page)).iter().map(|e| (**e).clone()).collect()
}

/// The paths of the entities on the reserved cable layer (brief 009 adds arcs and texts there,
/// which are not paths).
fn fan_paths(h: &mut H, page: PageId) -> Vec<Vec<Pt>> {
    entities(h, page)
        .into_iter()
        .filter(|e| e.common.layer == views::LAYER)
        .filter_map(|e| match e.kind {
            EntityKind::Line(l) => Some(vec![Pt::new(l.a.x, l.a.y), Pt::new(l.b.x, l.b.y)]),
            EntityKind::LwPolyline(p) => Some(p.vertices.iter().map(|v| Pt::new(v.p.x, v.p.y)).collect()),
            EntityKind::Arc(_) | EntityKind::Text(_) => None,
            other => panic!("unexpected fan entity {other:?}"),
        })
        .collect()
}

/// The tails of a cable's fan on a page (brief 009: one per member and fanned end).
fn tails(fan: &views::Fan) -> Vec<Vec<Pt>> {
    fan.parts
        .iter()
        .filter(|p| p.role == ecad_cadcraft::look::Role::Tail)
        .filter_map(|p| match &p.shape {
            ecad_cadcraft::look::Shape::Path { points, .. } => Some(points.clone()),
            _ => None,
        })
        .collect()
}

/// Handles of fan lines on the page.
fn fan_handles(h: &mut H, page: PageId) -> Vec<Handle> {
    h.p(|p| p.pages[&page].map.fan_handles().collect())
}

/// Design check B (AC1, AC2): ten switches, by the JSON form and by clicks, leave the document,
/// the log, the head, every ID and the nets as they were, and never mark the project unsaved.
#[test]
fn ten_switches_change_nothing_in_the_project() {
    let mut g = fixture();
    let (doc, log, head) = (g.h.doc(), history(&mut g.h), g.h.head());
    let nets = ecad_model::derive::nets(&doc);
    let unsaved = g.h.p(|p| p.unsaved_transactions());
    let tabs: Vec<bool> = (0..g.h.s.docs.len()).map(|i| g.h.s.doc_unsaved(i)).collect();
    for n in 0..10 {
        let fan_before = g.h.p(|p| p.view_of(g.c1)) == View::Fan;
        if n % 2 == 0 {
            g.h.run("cableview", json!({"cable": g.c1.hex()})).unwrap();
        } else {
            g.h.s.start("cableview").unwrap();
            assert_eq!(prompt(&g.h), "Select a cable to switch its view:");
            // The slash in traditional view, the trunk in fan view (brief 009: joining the middle
            // of the spine at y = 5, left of the label box).
            if fan_before {
                click(&mut g.h, 24.0, 5.0)
            } else {
                click(&mut g.h, 30.0, 7.5)
            }
            g.h.s.input(Input::Enter).unwrap();
            assert!(g.h.s.running.is_none());
        }
        let now = g.h.p(|p| p.view_of(g.c1));
        assert_eq!(now == View::Fan, !fan_before, "switch {n}");
        assert_eq!(g.h.head(), head, "switch {n} moved the head");
        g.h.check();
    }
    assert_eq!(g.h.doc(), doc);
    assert_eq!(history(&mut g.h), log);
    assert_eq!(ecad_model::derive::nets(&g.h.doc()), nets);
    assert_eq!(g.h.p(|p| p.unsaved_transactions()), unsaved, "the change count is unchanged");
    let now: Vec<bool> = (0..g.h.s.docs.len()).map(|i| g.h.s.doc_unsaved(i)).collect();
    assert_eq!(now, tabs);
}

/// Design check C (AC3, AC5): the fan of G is §5's worked example, on the reserved layer; the
/// routes and the slash are not drawn; terminal rings and attached dots stay where they were.
#[test]
fn the_fan_of_g_is_the_worked_example() {
    let mut g = fixture();
    let uid = uid(&mut g.h, g.page);
    let marks = |h: &mut H, shape: MarkShape| -> Vec<(i64, i64)> {
        let mut v: Vec<(i64, i64)> = h
            .p(|p| overlay_marks(p, uid))
            .into_iter()
            .filter(|m| m.shape == shape && (m.color.0, m.color.1, m.color.2) != (0xff, 0x00, 0xff))
            .map(|m| ((m.at.x * 1e6) as i64, (m.at.y * 1e6) as i64))
            .collect();
        v.sort();
        v
    };
    let (rings, dots) = (marks(&mut g.h, MarkShape::Ring), marks(&mut g.h, MarkShape::Dot));
    assert_eq!((rings.len(), dots.len()), (6, 6));
    let r = view(&mut g.h, g.c1, "fan");
    assert_eq!(r["view"], "fan");
    // Brief 009 replaces §5's geometry (design 009 §4): a tail ends at every wire end; the
    // spines stand 22.5 in from each end (tail-min 20 + bend radius 2.5).
    let doc = g.h.doc();
    let FanOnPage::Drawn(fan) = views::fan_on_page(&doc, g.page, g.c1, 1e-6) else { panic!("drawn") };
    let tails: Vec<Vec<Pt>> = fan
        .parts
        .iter()
        .filter(|p| p.role == ecad_cadcraft::look::Role::Tail)
        .filter_map(|p| match &p.shape {
            ecad_cadcraft::look::Shape::Path { points, .. } => Some(points.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tails.len(), 6, "{tails:?}");
    for (x, y) in [(0.0, 0.0), (0.0, 5.0), (0.0, 10.0), (60.0, 0.0), (60.0, 5.0), (60.0, 10.0)] {
        let w = Pt::new(x, y);
        assert!(tails.iter().any(|t| t.last().is_some_and(|q| q.near(w, 1e-9))), "{w:?} in {tails:?}");
    }
    let paths = fan_paths(&mut g.h, g.page);
    for x in [22.5, 37.5] {
        assert!(paths.iter().any(|p| p.iter().all(|q| (q.x - x).abs() < 1e-9)), "spine at x = {x}: {paths:?}");
    }
    // No entity for the routes or the slash; the fan lines are mapped to the cable.
    let doc = g.h.doc();
    let map = g.h.p(|p| p.pages[&g.page].map.clone());
    for w in g.wires {
        let (item, _) = ecad_model::derive::route_on_page(&doc, w, g.page).unwrap();
        assert!(map.item(item).is_none(), "the route of {w:?} is not drawn");
    }
    assert!(map.item(g.slash).is_none(), "the slash is not drawn");
    let fans = fan_handles(&mut g.h, g.page);
    assert_eq!(fans.len(), fan.parts.len());
    assert!(fans.iter().all(|h| map.get(*h) == Some(Mapped::Fan(g.c1))));
    // The marks did not move; the magenta crossing dots are gone.
    assert_eq!(marks(&mut g.h, MarkShape::Ring), rings);
    assert_eq!(marks(&mut g.h, MarkShape::Dot), dots);
    let magenta = g.h.p(|p| overlay_marks(p, uid)).iter().filter(|m| (m.color.0, m.color.2) == (0xff, 0xff)).count();
    assert_eq!(magenta, 0);
    // Back to traditional: the page is drawn as before.
    view(&mut g.h, g.c1, "traditional");
    assert!(fan_paths(&mut g.h, g.page).is_empty());
    assert!(g.h.p(|p| p.pages[&g.page].map.item(g.slash)).is_some());
}

/// Design check D (AC4): undo and redo act on the last real edit, never on the switch, and the
/// view survives them.
#[test]
fn undo_and_redo_skip_the_switch() {
    let mut g = fixture();
    let head = g.h.head();
    view(&mut g.h, g.c1, "fan");
    g.h.run("u", Value::Null).unwrap();
    assert!(!g.h.doc().cables.contains_key(&g.c1), "undo took the cable slash");
    assert!(fan_paths(&mut g.h, g.page).is_empty());
    g.h.run("redo", Value::Null).unwrap();
    assert!(g.h.doc().cables.contains_key(&g.c1));
    assert_eq!(g.h.head(), head + 2);
    assert_eq!(views_of(&mut g.h)[g.c1.hex()], "fan", "still fan after redo");
    assert!(!fan_paths(&mut g.h, g.page).is_empty(), "drawn as a fan");
}

/// Design check E (AC6): edits of the fan are refused with the spec's message and change
/// nothing; JSON forms are never refused because of a view (§6.3).
#[test]
fn edits_on_the_fan_are_refused_and_json_forms_still_work() {
    let mut g = fixture();
    view(&mut g.h, g.c1, "fan");
    let head = g.h.head();
    let before = entities(&mut g.h, g.page);
    let fans = fan_handles(&mut g.h, g.page);
    for (cmd, params) in [
        ("move", json!({"handles": [fans[0].hex()], "delta": [0, 10]})),
        ("erase", json!({"handles": [fans[1].hex()]})),
        ("copy", json!({"handles": [fans[2].hex()], "delta": [0, 30]})),
    ] {
        let e = g.h.run(cmd, params).unwrap_err();
        assert!(e.contains(FAN_REFUSED), "{cmd}: {e}");
        assert_eq!(g.h.head(), head, "{cmd}");
        assert_eq!(entities(&mut g.h, g.page), before, "{cmd}: the drawing is restored");
    }
    // An ECAD pick on a fan line: the message, the prompt repeats.
    g.h.s.start("connect").unwrap();
    // A tail (brief 009: along the wire near its end).
    click(&mut g.h, 10.0, 0.0);
    assert_eq!(last_echo(&g.h), FAN_REFUSED);
    assert_eq!(prompt(&g.h), "Select wire near the end to connect:");
    g.h.s.input(Input::Enter).unwrap();
    assert_eq!(g.h.head(), head);
    // Convert to Component and Add Terminal on a fan line.
    let e = g.h.run("convertcomponent", json!({"handles": [fans[3].hex()]})).unwrap_err();
    assert!(e.contains(FAN_REFUSED), "{e}");
    let e = g.h.run("addterminal", json!({"handle": fans[3].hex(), "at": [1, 1]})).unwrap_err();
    assert!(e.contains(FAN_REFUSED), "{e}");
    // New geometry on the reserved layer.
    let e = ecad_cadcraft::convert::item_from(
        &g.h.doc(),
        g.page,
        &Entity {
            handle: Handle(0x99999),
            common: cadcraft_engine::doc::Common { layer: views::LAYER.into(), ..Default::default() },
            kind: EntityKind::Line(cadcraft_engine::doc::Line {
                a: cadcraft_engine::geom::Vec3::new(0.0, 0.0, 0.0),
                b: cadcraft_engine::geom::Vec3::new(1.0, 1.0, 0.0),
            }),
        },
    )
    .unwrap_err();
    assert_eq!(e, views::LAYER_RESERVED);
    // JSON forms act on IDs whatever the view.
    g.h.run("disconnect", json!({"conductor": g.wires[0].hex(), "end": "A"})).unwrap();
    assert!(g.h.doc().conductors[&g.wires[0]].ends[0].is_none());
    assert_eq!(g.h.head(), head + 1);
    g.h.run("u", Value::Null).unwrap();
    assert_eq!(g.h.head(), head + 2);
    assert!(!fan_paths(&mut g.h, g.page).is_empty(), "drawn as a fan");
}

/// Design §6.2 (owner answer O3): moving a component with fan-view wires attached is allowed;
/// the hidden route ends follow, and the fan is recomputed.
#[test]
fn moving_equipment_beside_a_fan_moves_the_hidden_routes() {
    let mut g = fixture();
    view(&mut g.h, g.c1, "fan");
    let doc = g.h.doc();
    let right = doc
        .components
        .keys()
        .copied()
        .find(|c| {
            ecad_model::derive::terminal_positions(&doc, g.page)
                .iter()
                .any(|(t, _, at)| doc.terminals[t].component == *c && at.x > 30.0)
        })
        .unwrap();
    let handles = g.h.rep_handles(g.page, right);
    g.h.run("move", json!({"handles": handles, "delta": [10, 0]})).unwrap();
    for w in g.wires {
        assert!(g.h.route_ends(g.page, w).1.x > 69.0, "the hidden route end followed");
    }
    let paths = fan_paths(&mut g.h, g.page);
    assert!(paths.iter().any(|p| p.iter().any(|q| q.near(Pt::new(70.0, 0.0), 1e-9))), "{paths:?}");
    let events = g.h.p(|p| p.events.events().filter(|e| format!("{:?}", e.kind).contains("Fidelity")).count());
    assert_eq!(events, 0, "the recomputed fan is not a fidelity mismatch");
}

/// Design check F (AC2): two cables in different views keep their own views across save, close
/// and reopen; the settings file lists exactly the fan cable under the project ID.
#[test]
fn views_are_remembered_per_cable_across_reopen() {
    let dir = TempDir::new("reopen");
    let settings = TempDir::new("settings");
    let mut g = fixture();
    for (y, tag) in [(30, "W30"), (35, "W35"), (40, "W40")] {
        wire(&mut g.h, json!([[0, y], [60, y]]), tag);
    }
    let c2: CableId =
        id(&g.h.run("cableslash", json!({"from": [30, 25], "to": [30, 45], "tag": "C2"})).unwrap(), "cable");
    g.h.cfg(|p| {
        p.persistence = Persistence::File;
        p.settings = Settings::at(&settings.0);
    });
    g.h.run("saveas", json!({"path": dir.path("g")})).unwrap();
    let r = view(&mut g.h, g.c1, "fan");
    assert_eq!(r["remembered"], true, "{r}");
    assert!(!g.h.p(|p| p.unsaved()), "a switch is not an unsaved change");
    let doc = g.h.doc();
    let project = g.h.p(|p| p.id);
    while !g.h.s.docs.is_empty() {
        g.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let text = std::fs::read_to_string(settings.0.join(views::FILE_NAME)).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"], views::FORMAT);
    assert_eq!(v["projects"].as_object().unwrap().len(), 1);
    assert_eq!(v["projects"][project.hex()]["fan"], json!([g.c1.hex()]));
    assert!(text.ends_with('\n') && !text.contains('\r'));

    let mut s = Session::empty();
    let path = dir.path("g.ecad");
    let (handle, summary) = ProjectHandle::open_file_in(&mut s, &path, 9, Settings::at(&settings.0)).unwrap();
    assert!(!summary.contains(views::UNREADABLE), "{summary}");
    let mut h = H { s, project: handle, trail: Vec::new() };
    h.s.viewport_px = (1280.0, 800.0);
    assert_eq!(h.doc(), doc, "conductors and cables unchanged");
    let vs = views_of(&mut h);
    assert_eq!(vs[g.c1.hex()], "fan");
    assert_eq!(vs[c2.hex()], "traditional");
    assert!(!fan_paths(&mut h, g.page).is_empty(), "drawn as a fan");
    assert!(!h.p(|p| p.unsaved()));
    h.check();
}

/// Design check G: an unreadable settings file opens every cable traditional with one message,
/// never fails the open, and is replaced at the next switch.
#[test]
fn an_unreadable_settings_file_starts_traditional() {
    let dir = TempDir::new("garbage");
    let settings = TempDir::new("garbage-settings");
    let mut g = fixture();
    g.h.cfg(|p| {
        p.persistence = Persistence::File;
        p.settings = Settings::at(&settings.0);
    });
    g.h.run("saveas", json!({"path": dir.path("g")})).unwrap();
    view(&mut g.h, g.c1, "fan");
    while !g.h.s.docs.is_empty() {
        g.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let file = settings.0.join(views::FILE_NAME);
    std::fs::write(&file, [0xff, 0x00, 0x7b, 0x22, 0xfe]).unwrap();
    let mut s = Session::empty();
    let (handle, summary) =
        ProjectHandle::open_file_in(&mut s, &dir.path("g.ecad"), 9, Settings::at(&settings.0)).unwrap();
    assert_eq!(summary.matches(views::UNREADABLE).count(), 1, "{summary}");
    let mut h = H { s, project: handle, trail: Vec::new() };
    assert_eq!(views_of(&mut h)[g.c1.hex()], "traditional");
    assert!(fan_paths(&mut h, g.page).is_empty());
    let r = view(&mut h, g.c1, "fan");
    assert_eq!(r["remembered"], true);
    let project = h.p(|p| p.id);
    assert_eq!(Settings::at(&settings.0).load(project).fan.into_iter().collect::<Vec<_>>(), vec![g.c1]);
    // Too large a file is not read either.
    std::fs::write(&file, vec![b' '; views::MAX_FILE_BYTES as usize + 1]).unwrap();
    assert!(Settings::at(&settings.0).load(project).unreadable);
    // An unwritable folder: the switch still happens, with the reason.
    let blocked = settings.0.join("file-not-folder");
    std::fs::write(&blocked, b"x").unwrap();
    h.cfg(|p| p.settings = Settings::at(&blocked));
    let r = view(&mut h, g.c1, "traditional");
    assert_eq!((r["view"].as_str(), r["remembered"].as_bool()), (Some("traditional"), Some(false)), "{r}");
    assert!(r["reason"].is_string(), "{r}");
    assert!(fan_paths(&mut h, g.page).is_empty());
}

/// `n` parallel wires from x 0 to 60 starting at `y0`, 1 apart, slashed into one cable.
fn parallel_cable(h: &mut H, y0: f64, n: usize) -> CableId {
    for i in 0..n {
        let y = y0 + i as f64;
        h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
    }
    let r = h.run("cableslash", json!({"from": [20.0, y0 - 0.5], "to": [20.0, y0 + n as f64 - 0.5]})).unwrap();
    assert_eq!(r["conductors"].as_array().unwrap().len(), n);
    id(&r, "cable")
}

/// Design check H (AC3): fans of 2 and 20 wires, of wires on different routes (a bend, one drawn
/// right to left) and with loose ends draw without panic; a cable left with one wire draws its
/// trunk; a one-wire cable is not offered fan view (owner answer Q4).
#[test]
fn fans_of_many_shapes_draw_sensibly() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let two = parallel_cable(&mut h, 100.0, 2);
    let twenty = parallel_cable(&mut h, 200.0, 20);
    view(&mut h, two, "fan");
    view(&mut h, twenty, "fan");
    let paths = fan_paths(&mut h, page);
    // Brief 009: one tail per member and end (the old fan's one line per member and end).
    let doc = h.doc();
    let tail_count: usize = [two, twenty]
        .iter()
        .map(|k| match views::fan_on_page(&doc, page, *k, 1e-6) {
            FanOnPage::Drawn(f) => tails(&f).len(),
            other => panic!("{other:?}"),
        })
        .sum();
    assert_eq!(tail_count, 2 * 2 + 2 * 20, "{paths:?}");
    // Loose ends keep their grey squares, at the end of a fan line.
    let uid = uid(&mut h, page);
    let squares: Vec<Pt> = h
        .p(|p| overlay_marks(p, uid))
        .into_iter()
        .filter(|m| m.shape == MarkShape::Square)
        .map(|m| Pt::new(m.at.x, m.at.y))
        .collect();
    assert_eq!(squares.len(), 2 * 22);
    for q in &squares {
        assert!(
            paths
                .iter()
                .any(|p| p.first().is_some_and(|a| a.near(*q, 1e-9)) || p.last().is_some_and(|b| b.near(*q, 1e-9)))
        );
    }
    // Different routes: one with bends, the reference drawn right to left.
    h.run("wire", json!({"points": [[0, 300], [60, 300]]})).unwrap();
    let rl: ConductorId = id(&h.run("wire", json!({"points": [[60, 305], [0, 305]]})).unwrap(), "conductor");
    h.run("wire", json!({"points": [[0, 310], [30, 310], [30, 320], [60, 320]]})).unwrap();
    let odd: CableId = id(&h.run("cableslash", json!({"from": [10, 295], "to": [10, 315]})).unwrap(), "cable");
    assert_eq!(h.doc().cables[&odd].members[1].conductor, rl, "the right-to-left wire is the reference");
    view(&mut h, odd, "fan");
    let doc = h.doc();
    let FanOnPage::Drawn(fan) = views::fan_on_page(&doc, page, odd, 1e-6) else { panic!("drawn") };
    // Brief 009: every wire end, on both sides, is the end of a tail, whatever the route.
    let tails = tails(&fan);
    for e in [(0.0, 300.0), (60.0, 300.0), (60.0, 305.0), (0.0, 305.0), (0.0, 310.0), (60.0, 320.0)] {
        let e = Pt::new(e.0, e.1);
        assert!(tails.iter().any(|t| t.last().is_some_and(|q| q.near(e, 1e-9))), "{e:?} in {tails:?}");
    }
    // A cable left with one wire is drawn traditional on the page (design 009 §4.1).
    let members: Vec<ConductorId> = h.doc().cables[&two].members.iter().map(|m| m.conductor).collect();
    h.run("ecaddelete", json!({"conductor": members[0].hex()})).unwrap();
    let doc = h.doc();
    assert_eq!(views::fan_on_page(&doc, page, two, 1e-6), FanOnPage::TooFew);
    // A new one-wire cable is not offered fan view.
    h.run("wire", json!({"points": [[0, 400], [60, 400]]})).unwrap();
    let single: CableId = id(&h.run("cableslash", json!({"from": [20, 395], "to": [20, 405]})).unwrap(), "cable");
    let e = h.run("cableview", json!({"cable": single.hex()})).unwrap_err();
    assert!(e.contains(views::ONE_WIRE), "{e}");
    assert_eq!(views_of(&mut h)[single.hex()], "traditional");
    h.check();
}

/// Design check I: hostile geometry never panics; a reference route of no length draws the cable
/// traditional.
#[test]
fn hostile_geometry_falls_back_to_traditional() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    h.run("wire", json!({"points": [[-1e12, 0], [1e12, 0]]})).unwrap();
    h.run("wire", json!({"points": [[-1e12, 1], [1e12, 1]]})).unwrap();
    let big: CableId = id(&h.run("cableslash", json!({"from": [0, -1], "to": [0, 2]})).unwrap(), "cable");
    view(&mut h, big, "fan");
    h.check();
    // Two wires of no length, made one cable by their IDs.
    let a: ConductorId = id(&h.run("wire", json!({"points": [[5, 5], [5, 5]]})).unwrap(), "conductor");
    let b: ConductorId = id(&h.run("wire", json!({"points": [[6, 6], [6, 6]]})).unwrap(), "conductor");
    let k: CableId = id(&h.run("cableadd", json!({"conductors": [a.hex(), b.hex()]})).unwrap(), "cable");
    view(&mut h, k, "fan");
    let doc = h.doc();
    assert_eq!(views::fan_on_page(&doc, page, k, 1e-6), FanOnPage::Failed);
    let (item, _) = ecad_model::derive::route_on_page(&doc, a, page).unwrap();
    assert!(h.p(|p| p.pages[&page].map.item(item)).is_some(), "drawn traditional");
    h.check();
}

/// Design check J: hovering a fan line names the cable, its view and its wires in order.
#[test]
fn hovering_a_fan_line_shows_the_cable() {
    let mut g = fixture();
    let uid = uid(&mut g.h, g.page);
    view(&mut g.h, g.c1, "fan");
    let h = fan_handles(&mut g.h, g.page)[0];
    let lines = g.h.p(|p| hover_lines(p, uid, h)).unwrap();
    assert_eq!(
        lines,
        [
            "Cable C1".to_string(),
            format!("id {}", g.c1.hex()),
            "View: fan".into(),
            "Wires: 3".into(),
            "  W0".into(),
            "  W5".into(),
            "  W10".into()
        ]
    );
}

/// Design check K and §3.2: a selection switches every cable it names (all fan → traditional,
/// otherwise all → fan) and is cleared; clicks that name no cable say why.
#[test]
fn the_selection_and_clicks_switch_cables() {
    let mut g = fixture();
    for (y, tag) in [(30, "W30"), (35, "W35"), (40, "W40")] {
        wire(&mut g.h, json!([[0, y], [60, y]]), tag);
    }
    let r = g.h.run("cableslash", json!({"from": [30, 25], "to": [30, 45], "tag": "C2"})).unwrap();
    let (c2, slash2): (CableId, ItemId) = (id(&r, "cable"), id(&r, "item"));
    view(&mut g.h, g.c1, "fan");
    let head = g.h.head();
    let s2 = g.h.handle_of(g.page, slash2);
    let f1 = fan_handles(&mut g.h, g.page)[0];
    g.h.s.set_selection(vec![s2, f1]);
    g.h.s.start("cableview").unwrap();
    assert!(g.h.s.running.is_none(), "a selection switches at once");
    let vs = views_of(&mut g.h);
    assert_eq!((vs[g.c1.hex()].as_str(), vs[c2.hex()].as_str()), (Some("fan"), Some("fan")));
    assert!(g.h.s.selection().is_empty());
    assert!(g.h.s.log.iter().any(|l| l == "Cable C2 now shown in fan view."), "{:?}", g.h.s.log);
    let fans = fan_handles(&mut g.h, g.page);
    let map = g.h.p(|p| p.pages[&g.page].map.clone());
    let of = |k: CableId| fans.iter().copied().find(|h| map.get(*h) == Some(Mapped::Fan(k))).unwrap();
    g.h.s.set_selection(vec![of(g.c1), of(c2)]);
    g.h.s.start("cableview").unwrap();
    let vs = views_of(&mut g.h);
    assert_eq!((vs[g.c1.hex()].as_str(), vs[c2.hex()].as_str()), (Some("traditional"), Some("traditional")));
    assert_eq!(g.h.head(), head);

    // Clicks that name no cable.
    g.h.s.start("cableview").unwrap();
    click(&mut g.h, 40.0, -20.0);
    assert_eq!(last_echo(&g.h), "Nothing there. Click a cable slash, a fan or a wire in a cable.");
    assert_eq!(prompt(&g.h), "Select a cable to switch its view:");
    g.h.s.input(Input::Enter).unwrap();
    assert!(g.h.s.running.is_none());
    g.h.run("wire", json!({"points": [[0, -20], [60, -20]]})).unwrap();
    g.h.s.start("cableview").unwrap();
    click(&mut g.h, 40.0, -20.0);
    assert_eq!(last_echo(&g.h), "That wire is not in a cable.");
    click(&mut g.h, -5.0, -5.0);
    assert_eq!(last_echo(&g.h), "That is not a cable or a wire in a cable.");
    // A wire in a cable names its cable.
    click(&mut g.h, 40.0, 35.0);
    assert_eq!(last_echo(&g.h), "Cable C2 now shown in fan view.");
    assert_eq!(prompt(&g.h), "Select a cable to switch its view:");
    g.h.s.input(Input::Enter).unwrap();
    assert!(g.h.s.running.is_none());
}

/// Design §3.4: the JSON form's errors, its listing, an unchanged view, and the one-time notice
/// when no settings folder exists; it works on an unavailable page's project.
#[test]
fn the_json_form() {
    let mut g = fixture();
    let head = g.h.head();
    let e = |h: &mut H, p: Value| h.run("cableview", p).unwrap_err();
    assert!(e(&mut g.h, json!({"cable": "zz"})).contains("`cable` (hex id) is required"));
    assert!(e(&mut g.h, json!({"cable": "1234"})).contains("no such cable"));
    assert!(
        e(&mut g.h, json!({"cable": g.c1.hex(), "view": "both"})).contains(r#"`view` must be "traditional" or "fan""#)
    );
    assert!(e(&mut g.h, json!({"cable": g.c1.hex(), "view": 3})).contains(r#"`view` must be "traditional" or "fan""#));
    assert!(e(&mut g.h, json!({"view": "fan"})).contains("`view` needs `cable`"));
    assert_eq!(views_of(&mut g.h), json!({ g.c1.hex(): "traditional" }));
    // The test store remembers nothing: said once.
    let r = view(&mut g.h, g.c1, "fan");
    assert_eq!(r["remembered"], false);
    assert_eq!(r["notice"], views::NOT_REMEMBERED);
    let r = view(&mut g.h, g.c1, "fan");
    assert_eq!(r["changed"], false);
    assert!(r.get("notice").is_none());
    let r = g.h.run("cableview", json!({"cable": g.c1.hex()})).unwrap();
    assert_eq!(r["view"], "traditional");
    assert!(r.get("notice").is_none());
    assert_eq!(g.h.head(), head);
    // An unavailable page: views can still be switched and listed.
    g.h.cfg(|p| p.faults.fail_projections = 2);
    g.h.run("wire", json!({"points": [[0, 50], [10, 50]]})).unwrap();
    assert!(g.h.p(|p| p.pages[&g.page].unavailable));
    assert_eq!(g.h.run("cableview", json!({"cable": g.c1.hex(), "view": "fan"})).unwrap()["view"], "fan");
    assert_eq!(views_of(&mut g.h)[g.c1.hex()], "fan");
}

/// A settings file with `n` other projects, each listing one cable, last used at `used + i`.
fn crowded(n: usize, used: u64) -> Value {
    let projects: serde_json::Map<String, Value> = (0..n)
        .map(|i| {
            (
                format!("{:032x}", 0xabc0_0000_u128 + i as u128),
                json!({"fan": [format!("{:032x}", i + 1)], "used": used + i as u64}),
            )
        })
        .collect();
    json!({"format": views::FORMAT, "projects": projects})
}

/// Review M1: only a missing file or one that is not JSON is rewritten; a file of another format
/// or over 1 MiB is left alone and the switch is "not remembered", with the reason.
#[test]
fn store_rewrites_only_a_missing_or_non_json_file() {
    let dir = TempDir::new("m1");
    let settings = Settings::at(&dir.0);
    let file = dir.0.join(views::FILE_NAME);
    let project = ecad_model::ProjectId(0x1234);
    let fan: std::collections::BTreeSet<CableId> = [CableId(7)].into_iter().collect();
    // Missing: written.
    settings.store(project, &fan).unwrap();
    assert_eq!(settings.load(project).fan, fan);
    // Not JSON: replaced.
    std::fs::write(&file, [0xde, 0xad, 0x00]).unwrap();
    settings.store(project, &fan).unwrap();
    assert_eq!(settings.load(project).fan, fan);
    // Another format (a newer version) and an oversized file: left byte for byte.
    let newer = br#"{"format":"ecad-cable-views/2","projects":{}}"#.to_vec();
    let mut big = b"{\"format\":\"ecad-cable-views/1\",\"projects\":{},\"pad\":\"".to_vec();
    big.extend(std::iter::repeat_n(b'x', views::MAX_FILE_BYTES as usize));
    big.extend(b"\"}");
    for (what, bytes) in [("newer format", newer), ("over 1 MiB", big)] {
        std::fs::write(&file, &bytes).unwrap();
        let e = settings.store(project, &fan).unwrap_err();
        assert!(e.contains("left as it is"), "{what}: {e}");
        assert_eq!(std::fs::read(&file).unwrap(), bytes, "{what}: untouched");
    }
    // Through the command: the switch happens, not remembered, with the reason.
    let mut g = fixture();
    g.h.cfg(|p| p.settings = Settings::at(&dir.0));
    let before = std::fs::read(&file).unwrap();
    let r = view(&mut g.h, g.c1, "fan");
    assert_eq!((r["view"].as_str(), r["remembered"].as_bool()), (Some("fan"), Some(false)), "{r}");
    assert!(r["reason"].as_str().is_some_and(|x| x.contains("larger than 1 MiB")), "{r}");
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert!(!fan_paths(&mut g.h, g.page).is_empty(), "drawn as a fan");
}

/// Review M2: a project with no fan cables has no entry; the file never grows past what the reader
/// accepts (the least recently used projects go first); too many cables for one project are not
/// remembered and the file is left alone.
#[test]
fn store_keeps_the_file_readable() {
    let dir = TempDir::new("m2");
    let settings = Settings::at(&dir.0);
    let file = dir.0.join(views::FILE_NAME);
    let project = ecad_model::ProjectId(0x5555);
    let one: std::collections::BTreeSet<CableId> = [CableId(9)].into_iter().collect();
    settings.store(project, &one).unwrap();
    settings.store(project, &Default::default()).unwrap();
    let v: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert!(v["projects"].get(project.hex()).is_none(), "no entry without fan cables: {v}");
    // Nearly full of other projects: the oldest are dropped, the newest kept, ours added.
    let mut n = 0;
    let mut full = crowded(n, 1000);
    // Grow in big steps, then small ones, to just under the cap (never over it).
    let cap = views::MAX_FILE_BYTES as usize;
    while full.to_string().len() < cap - 1000 {
        n += if full.to_string().len() < cap - 60_000 { 500 } else { 5 };
        full = crowded(n, 1000);
    }
    assert!(full.to_string().len() < cap);
    std::fs::write(&file, full.to_string()).unwrap();
    let many: std::collections::BTreeSet<CableId> = (1..=50u128).map(CableId).collect();
    settings.store(project, &many).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    assert!(bytes.len() as u64 <= views::MAX_FILE_BYTES);
    assert!(!settings.load(project).unreadable);
    assert_eq!(settings.load(project).fan, many);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let kept = v["projects"].as_object().unwrap();
    assert!(kept.len() < n + 1, "some projects were dropped");
    let oldest = format!("{:032x}", 0xabc0_0000_u128);
    let newest = format!("{:032x}", 0xabc0_0000_u128 + (n - 1) as u128);
    assert!(!kept.contains_key(&oldest) && kept.contains_key(&newest), "least recently used first");
    // Over the per-project limit: nothing written.
    let before = std::fs::read(&file).unwrap();
    assert_eq!(views::MAX_CABLES_PER_PROJECT, 25_000);
    let cap = views::MAX_CABLES_PER_PROJECT as u128;
    let over: std::collections::BTreeSet<CableId> = (1..=cap + 1).map(CableId).collect();
    assert!(settings.store(project, &over).unwrap_err().contains("more than 25000 cables"));
    assert_eq!(std::fs::read(&file).unwrap(), before, "left alone");
    // Exactly the limit is reachable: it fits under 1 MiB beside the other projects and reads back.
    let at_cap: std::collections::BTreeSet<CableId> = (1..=cap).map(CableId).collect();
    settings.store(project, &at_cap).unwrap();
    assert!(std::fs::metadata(&file).unwrap().len() <= views::MAX_FILE_BYTES);
    assert_eq!(settings.load(project).fan, at_cap);
}

/// Review M5: switching a selection of cables reports each cable's result and carries on past
/// one that fails.
#[test]
fn a_failed_switch_does_not_stop_the_others() {
    let mut g = fixture();
    for (y, tag) in [(30, "W30"), (35, "W35"), (40, "W40")] {
        wire(&mut g.h, json!([[0, y], [60, y]]), tag);
    }
    let r = g.h.run("cableslash", json!({"from": [30, 25], "to": [30, 45], "tag": "C2"})).unwrap();
    let (c2, slash2): (CableId, ItemId) = (id(&r, "cable"), id(&r, "item"));
    let (s1, s2) = (g.h.handle_of(g.page, g.slash), g.h.handle_of(g.page, slash2));
    g.h.cfg(|p| p.faults.fail_view_switches = 1);
    g.h.s.set_selection(vec![s1, s2]);
    g.h.s.start("cableview").unwrap();
    assert!(g.h.s.running.is_none());
    let vs = views_of(&mut g.h);
    let fans = [g.c1, c2].iter().filter(|k| vs[k.hex()] == "fan").count();
    assert_eq!(fans, 1, "one switched, one failed: {vs}");
    let log = g.h.s.log.clone();
    assert!(log.iter().any(|l| l.contains("was not switched: injected view switch failure")), "{log:?}");
    assert!(log.iter().any(|l| l.ends_with("now shown in fan view.")), "{log:?}");
    g.h.check();
}
