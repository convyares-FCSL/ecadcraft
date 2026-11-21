//! Lane 072 D (overseer ruling 1, 2026-11-18): the junction symbol is gone; a T is a line landing on
//! a line. A file that still holds a junction (a tagless component of `kind = junction`) is read
//! strictly: it opens read-only, naming the word, and the file is kept as it is (pre-v1, no
//! conversion). Step F3: the T's dot is drawn with the page from the routes (design 072 §4.4),
//! and the hover names the line a T end joins.

use std::path::PathBuf;

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use ecad_cadcraft::interact::hover_lines;
use ecad_cadcraft::project::Mapped;
use ecad_cadcraft::{Persistence, ProjectHandle};
const KIND: &str = "kind";
use ecad_model::{Author, Device, DeviceId, Op, PageId, Pt, RouteEnd, RouteId};
use serde_json::json;

use crate::common::{H, id, pts, small_project};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn close_all(h: &mut H) {
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
}

#[test]
fn a_file_holding_a_junction_opens_read_only() {
    let dir = TempDir(std::env::temp_dir().join(format!("ecad-072d-junction-{}", ecad_store::disk::unique())));
    std::fs::create_dir_all(&dir.0).unwrap();
    // A file made by an earlier build: a junction component on its own.
    let mut h = H::new(0x72_d7);
    h.cfg(|p| {
        let mut junction = Device::default();
        junction.properties.insert(KIND.into(), "junction".into());
        let op = Op::CreateDevice { id: DeviceId(0x72_d7), device: junction };
        p.ledger.commit(Author::User, "an earlier build's junction", vec![op]).unwrap();
    });
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.0.join("junction072d").display().to_string();
    let v = h.run("saveas", json!({"path": path})).unwrap();
    let file = h.cfg(|p| p.path.clone()).unwrap_or_else(|| panic!("saved: {v}"));
    close_all(&mut h);

    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &file, 0x72_d8).unwrap();
    assert!(summary.contains("Opened read-only"), "{summary}");
    for named in ["`junction`", "`kind`"] {
        assert!(summary.contains(named), "the message names {named}: {summary}");
    }
    let mut h = H { s, project, trail: Vec::new() };
    assert!(h.run("save", json!({})).is_err(), "saving is refused");
    // `place` no longer offers it.
    let e = h.run("place", json!({"symbol": "junction", "at": [10, 10]})).unwrap_err();
    assert_eq!(e, "No symbol \"junction\". Type List to see them.");
    close_all(&mut h);

    // The junction is still stored as it was: nothing converted it.
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_read_only(&mut s, &file, 0x72_d9).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let kind = h.cfg(|p| p.ledger.document().devices.get(&DeviceId(0x72_d7)).and_then(|d| d.properties.get(KIND).cloned()));
    assert_eq!(kind.as_deref(), Some("junction"), "the file is kept as it is");
    close_all(&mut h);
}

/// The T dots drawn on `page`: (handle, what it is mapped to, centre, radius, layer).
fn dots(h: &mut H, page: PageId) -> Vec<(Handle, Option<Mapped>, Pt, f64, String)> {
    let ents = h.pv(|p, pg| pg.entities_of(p, page));
    let map = h.p(|p| p.pages[&page].map.clone());
    ents.iter()
        .filter(|e| matches!(map.get(e.handle), Some(Mapped::Tee(_))))
        .filter_map(|e| match &e.kind {
            EntityKind::Circle(c) => Some((e.handle, map.get(e.handle), Pt::new(c.center.x, c.center.y), c.radius, e.common.layer.clone())),
            _ => None,
        })
        .collect()
}

/// F3: a line drawn onto the middle of a wire's line (a T) draws a dot there, on the line's own
/// (plotted) layer, so it prints; the dot follows the line (moved off: gone; undo: back), and the
/// rebuilt page (the recovery check) agrees. A line on its own has none.
#[test]
fn a_tee_draws_a_dot_that_follows_its_line() {
    let (mut h, _, _, w) = small_project(0x72_da);
    let page = h.first_page();
    assert!(dots(&mut h, page).is_empty(), "no T yet, no dot");
    let (a, b) = h.route_ends(page, w);
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let v = h.run("wire", json!({"points": pts(&[(mid.x, mid.y + 30.0), (mid.x, mid.y)])})).unwrap();
    let tee: RouteId = id(&v, "route");
    let main = h.route_of(page, w);
    assert!(matches!(h.doc().routes[&tee].ends[1], RouteEnd::On { route, .. } if route == main), "a T");

    let d = dots(&mut h, page);
    assert_eq!(d.len(), ecad_cadcraft::wires::TEE_DOT_RADII.len(), "one dot of rings: {d:?}");
    assert!(d.iter().all(|(_, m, at, _, _)| *m == Some(Mapped::Tee(tee)) && at.near(mid, 1e-6)), "{d:?}");
    let radii: Vec<f64> = d.iter().map(|x| x.3).collect();
    assert!(ecad_cadcraft::wires::TEE_DOT_RADII.iter().all(|r| radii.iter().any(|x| (x - r).abs() < 1e-9)), "{radii:?}");
    // On the T line's own layer, which plots: the dot prints with the drawing.
    let line = h.line_handle(page, tee);
    let ents = h.pv(|p, pg| pg.entities_of(p, page));
    let layer = ents.iter().find(|e| e.handle == line).map(|e| e.common.layer.clone()).unwrap();
    assert!(d.iter().all(|x| x.4 == layer), "{d:?} on {layer}");
    let plots = h.pv(|p, pg| pg.drawing_of(p, page).unwrap().layer(&layer).is_none_or(|l| l.plot));
    assert!(plots, "layer {layer} plots");
    h.check();

    // Moved off the wire: the end is free again and the dot goes; undo brings both back.
    h.run("move", json!({"handles": [line.hex()], "delta": [0, 5]})).unwrap();
    assert_eq!(h.doc().routes[&tee].ends[1], RouteEnd::Free);
    assert!(dots(&mut h, page).is_empty());
    h.check();
    h.cmdline("u");
    assert_eq!(dots(&mut h, page).len(), 3);
    // Erasing the T line takes its dot with it.
    let line = h.line_handle(page, tee);
    h.run("erase", json!({"handles": [line.hex()]})).unwrap();
    assert!(dots(&mut h, page).is_empty());
    h.check();
}

/// F3: hovering a T end names the line it joins and never says "Open end"; the dot hovers as
/// its line; the dot alone cannot be erased or moved (it is drawn from the lines).
#[test]
fn a_tee_end_is_joined_not_open() {
    let (mut h, _, _, w) = small_project(0x72_db);
    let page = h.first_page();
    let (a, b) = h.route_ends(page, w);
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let tee: RouteId = id(&h.run("wire", json!({"points": pts(&[(mid.x, mid.y + 30.0), (mid.x, mid.y)])})).unwrap(), "route");
    let uid = h.p(|p| p.pages[&page].uid).expect("the page is open");
    let line = h.line_handle(page, tee);
    let lines = h.p(|p| hover_lines(p, uid, line)).unwrap();
    let number = h.doc().wires[&w].number_text().map(str::to_string).unwrap_or_default();
    assert!(lines.contains(&format!("End B joins wire {number} (a T).")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("Open end")), "{lines:?}");

    let dot = dots(&mut h, page)[0].0;
    assert_eq!(h.p(|p| hover_lines(p, uid, dot)), Some(lines), "the dot hovers as its line");
    for (cmd, extra) in [("erase", json!({})), ("move", json!({"delta": [0, 10]}))] {
        let mut params = extra;
        params["handles"] = json!([dot.hex()]);
        let e = h.run(cmd, params).unwrap_err();
        assert_eq!(e, ecad_cadcraft::wires::TEE_DERIVED, "{cmd}");
    }
    assert_eq!(dots(&mut h, page).len(), 3, "nothing changed");
}

/// Ink in the page's raster (the preview rasteriser, hairlines) in a box beside the point (30, 0):
/// 0.25 to 0.6 mm right of and above it, clear of a line along x = 30 and one along y = 0.
fn ink_beside_the_tee(h: &mut H, page: PageId) -> usize {
    use ecad_cadcraft::preview::{PreviewView, preview_drawing, rasterise};
    let drawing = h.p(|p| preview_drawing(p, page)).unwrap();
    // 200 px over 4 mm: 50 px per mm, (30, 0) at pixel (100, 100), y up.
    let view = PreviewView { center: Pt::new(30.0, 0.0), height: 4.0 };
    let img = rasterise(&drawing, view, 200, 200, [255, 255, 255], 0.0).unwrap();
    let mut n = 0;
    for y in 70..=87usize {
        for x in 113..=130usize {
            let i = (y * 200 + x) * 4;
            if img.rgba.get(i..i + 3).is_some_and(|c| c.iter().any(|v| *v < 200)) {
                n += 1;
            }
        }
    }
    n
}

/// F3 drawing check: the page as rasterised shows the T's dot beside the T point (blank there
/// before the T), and Print Book draws its rings with the page.
#[test]
fn a_tee_dot_shows_in_the_raster_and_the_print() {
    let mut h = H::new(0x72_dc);
    let page = h.first_page();
    h.run("wire", json!({"points": pts(&[(0.0, 0.0), (60.0, 0.0)])})).unwrap();
    let circles = |h: &mut H| {
        let doc = h.doc();
        let (d, _) = ecad_cadcraft::printbook::build_in(&doc, &[page], &Default::default(), 0.01, None, false, false).unwrap();
        d.model.iter().filter(|e| matches!(e.kind, EntityKind::Circle(_))).count()
    };
    let (before, printed) = (ink_beside_the_tee(&mut h, page), circles(&mut h));
    let tee: RouteId = id(&h.run("wire", json!({"points": pts(&[(30.0, 30.0), (30.0, 0.0)])})).unwrap(), "route");
    assert!(h.doc().routes[&tee].ends[1].route().is_some(), "a T");
    let after = ink_beside_the_tee(&mut h, page);
    assert!(after > before + 5, "the dot is drawn: {before} inked pixels before, {after} after");
    assert_eq!(circles(&mut h), printed + ecad_cadcraft::wires::TEE_DOT_RADII.len(), "the dot prints");
}
