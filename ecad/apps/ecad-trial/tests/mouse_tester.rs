//! Independent tester's UI checks for brief 008, written from the brief and design 008 only, driving
//! the trial headless (egui_kittest, elements by name) in a 1280×800 window.
//!
//! - AC3: the quick-start example built with terminals placed by **clicks** (Midpoint override
//!   typed, as design §5 describes) gives the same structure as the same example built by JSON.
//! - Owner answer Q4: a typed point near (not on) a terminal does not attach, even with the pointer
//!   resting on the drawing (Dynamic Input must not turn a typed value into a cursor capture).
//! - Design §9: Ctrl+Z during Wire removes the last point; Space works as Enter.
//! - AC5: hovering a terminal names it; hovering writes nothing (head, selection, unsaved state);
//!   the tooltip goes when a command starts.
//! - Design check H: the overlay glyphs are drawn at their token sizes at three zooms.
//! - AC6: reference screenshots (design check K), written when `ECAD_TESTER_SHOTS` names a folder
//!   and a wgpu adapter exists; they are for the owner's approval, never committed.

use std::sync::{Arc, Mutex};

use cadcraft_engine::geom::Vec2;
use ecad_model::{Document, End, ItemKind, Primitive, Subject};
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

/// Texture updates in order: `Some(image)` sets, `None` frees.
type Deltas = Arc<Mutex<Vec<(egui::TextureId, Option<egui::epaint::ImageDelta>)>>>;

/// Keeps every texture delta so a frame can be rendered later (see `shot`).
struct Collect(Deltas);

impl egui_kittest::TestRenderer for Collect {
    fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
        let mut out = self.0.lock().unwrap();
        for (id, imgs) in delta.set.drain() {
            out.extend(imgs.into_iter().map(|i| (id, Some(i))));
        }
        out.extend(delta.free.drain().map(|id| (id, None)));
    }
}

fn harness_with(deltas: Option<Deltas>) -> Harness<'static, Trial> {
    ecad_cadcraft::install();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
    let mut b = Harness::builder().with_size(egui::vec2(1280.0, 800.0));
    if let Some(d) = deltas {
        b = b.renderer(Collect(d));
    }
    let mut h = b.build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(5);
    h
}

fn harness() -> Harness<'static, Trial> {
    harness_with(None)
}

fn doc(h: &mut Harness<'static, Trial>) -> Document {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
}

fn head(h: &mut Harness<'static, Trial>) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

fn unsaved(h: &mut Harness<'static, Trial>) -> (bool, Vec<bool>) {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    let project = p.read(&mut t.cad.session, |p, _| p.unsaved()).unwrap();
    let tabs = (0..t.cad.session.docs.len()).map(|i| t.cad.session.doc_unsaved(i)).collect();
    (project, tabs)
}

fn screen(h: &Harness<'static, Trial>, x: f64, y: f64) -> Pos2 {
    h.state().cad.canvas.xf.expect("canvas laid out").to_screen(Vec2::new(x, y))
}

fn pointer_to(h: &mut Harness<'static, Trial>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
}

fn click(h: &mut Harness<'static, Trial>, at: Pos2) {
    pointer_to(h, at);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}

fn type_enter(h: &mut Harness<'static, Trial>, text: &str) {
    if !text.is_empty() {
        h.event(Event::Text(text.into()));
        h.run_steps(1);
    }
    h.key_press(Key::Enter);
    h.run_steps(2);
}

fn button(h: &mut Harness<'static, Trial>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn prompt(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn ok(h: &mut Harness<'static, Trial>, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

fn view(h: &mut Harness<'static, Trial>) {
    h.state_mut().cad.run("zoom", json!({"mode": "window", "p1": [-20, -40], "p2": [130, 50]})).unwrap();
    h.run_steps(3);
}

fn r6(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// A primitive with its coordinates rounded to 1e-6 (the captured end of a wire is the terminal's
/// computed position, which may differ from the typed 90,0 by rounding).
fn prim(p: &Primitive) -> String {
    match p {
        Primitive::Line { a, b } => format!("line {},{} {},{}", r6(a.x), r6(a.y), r6(b.x), r6(b.y)),
        Primitive::Polyline { vertices, closed } => {
            let v: Vec<String> = vertices.iter().map(|v| format!("{},{}", r6(v.x), r6(v.y))).collect();
            format!("polyline {v:?} closed {closed}")
        }
        other => format!("{other:?}"),
    }
}

/// The document without IDs: what each representation shows, terminals by label and position,
/// wires by route and the labels their ends attach to.
fn canon(d: &Document) -> Vec<String> {
    let mut out = Vec::new();
    let label = |t| d.terminals.get(&t).and_then(|x| x.label.clone()).unwrap_or_else(|| "-".into());
    for r in d.representations.values() {
        let mut items: Vec<String> = r
            .items
            .iter()
            .filter_map(|i| d.items.get(i))
            .map(|i| match &i.kind {
                ItemKind::Primitive(p) => prim(p),
                ItemKind::Symbol(s) => format!(
                    "symbol at {},{} rot {} mirror {} scale {}",
                    r6(s.placement.origin.x),
                    r6(s.placement.origin.y),
                    r6(s.placement.rotation),
                    s.placement.mirror,
                    s.placement.scale
                ),
                #[allow(unreachable_patterns)]
                other => format!("{other:?}").chars().take(40).collect(),
            })
            .collect();
        items.sort();
        let mut pts: Vec<String> =
            r.connection_points.iter().map(|(t, p)| format!("{}@{},{}", label(*t), r6(p.x), r6(p.y))).collect();
        pts.sort();
        let subject = match r.subject {
            Subject::Component(_) => "component".to_string(),
            Subject::Conductor(c) => {
                let ends: Vec<String> = End::BOTH
                    .iter()
                    .map(|e| d.conductors.get(&c).and_then(|x| x.end(*e)).map_or("loose".into(), label))
                    .collect();
                format!("wire ends {ends:?}")
            }
            Subject::Cable(_) => "cable".into(),
        };
        out.push(format!("{subject} {:?} items {items:?} terminals {pts:?}", r.form));
    }
    out.sort();
    out
}

/// Rectangle, window selection, Convert by button; then terminals by **clicks** with the Midpoint
/// override typed first (design §5: `mid` typed at the prompt).
fn component_by_clicks(h: &mut Harness<'static, Trial>) {
    ok(h, "line 0,0 40,0 40,20 0,20 c");
    view(h);
    let (a, b) = (screen(h, -10.0, 30.0), screen(h, 50.0, -10.0));
    click(h, a);
    click(h, b);
    assert_eq!(h.state().cad.session.selection().len(), 4);
    button(h, tools::TAB);
    button(h, "Convert to Component");
    assert_eq!(doc(h).components.len(), 1);
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(2);
    button(h, "Add Terminal");
    for (x, y) in [(40.0, 10.0), (0.0, 10.0)] {
        type_enter(h, "mid");
        // A few pixels off the middle of the edge: the Midpoint override snaps there.
        let near = screen(h, x, y) + egui::vec2(1.0, 3.0);
        click(h, near);
    }
    type_enter(h, "");
    assert!(h.state().cad.session.running.is_none(), "Enter finished Add Terminal");
}

fn coil_and_wire_by_clicks(h: &mut Harness<'static, Trial>) {
    button(h, "Place Symbol");
    type_enter(h, "R");
    type_enter(h, "100,0");
    type_enter(h, "");
    button(h, "Wire");
    let first = screen(h, 40.0, 10.0) + egui::vec2(3.0, 2.0);
    click(h, first);
    for (x, y) in [(70.0, 10.0), (70.0, 0.0)] {
        let p = screen(h, x, y);
        click(h, p);
    }
    let last = screen(h, 90.0, 0.0) + egui::vec2(-3.0, -3.0);
    click(h, last);
    assert!(h.state().cad.session.running.is_none(), "a click on a terminal finishes the wire");
}

/// AC3 / design check E with terminals placed by clicks: the result has the structure of the same
/// example built by JSON lines (IDs aside; bends to screen precision are snapped to the grid below).
#[test]
fn ac3_quick_start_with_clicked_terminals_matches_the_json_example() {
    let mut h = harness();
    component_by_clicks(&mut h);
    let d = doc(&mut h);
    let mut terms: Vec<String> = d.terminals.values().map(|t| t.label.clone().unwrap_or_default()).collect();
    terms.sort();
    assert_eq!(terms, ["1", "2"], "default names 1 and 2");
    coil_and_wire_by_clicks(&mut h);
    // The JSON version of the same example in a second trial.
    let mut j = harness();
    ok(&mut j, "line 0,0 40,0 40,20 0,20 c");
    let lines: Vec<String> = j.state().cad.session.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    ok(&mut j, &format!("convertcomponent {}", json!({ "handles": lines })));
    let edge = |j: &Harness<'static, Trial>, x: f64| {
        j.state()
            .cad
            .session
            .doc()
            .unwrap()
            .model
            .iter()
            .find(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Line(l) if l.a.x == x && l.b.x == x))
            .map(|e| e.handle.hex())
            .unwrap()
    };
    let (right, left) = (edge(&j, 40.0), edge(&j, 0.0));
    ok(&mut j, &format!("addterminal {}", json!({"handle": right, "at": [40, 10], "label": "1"})));
    ok(&mut j, &format!("addterminal {}", json!({"handle": left, "at": [0, 10], "label": "2"})));
    ok(&mut j, r#"place {"at":[100,0],"rotation":90}"#);
    ok(&mut j, r#"wire {"points":[[40,10],[70,10],[70,0],[90,0]]}"#);
    let mut by_clicks = doc(&mut h);
    // Clicked bends land within a pixel of 70,10 and 70,0; put them on the grid to compare.
    for item in by_clicks.items.values_mut() {
        if let ItemKind::Primitive(Primitive::Polyline { vertices, .. }) = &mut item.kind {
            for v in vertices.iter_mut() {
                let px = 1.0;
                if (v.x - 70.0).abs() < px {
                    v.x = 70.0;
                }
                for y in [0.0, 10.0] {
                    if (v.y - y).abs() < px && (v.x - 70.0).abs() < 1e-9 {
                        v.y = y;
                    }
                }
            }
        }
    }
    let (a, b) = (canon(&by_clicks), canon(&doc(&mut j)));
    assert_eq!(a, b, "the clicked example differs from the JSON one");
    assert_eq!(head(&mut h), head(&mut j), "the same number of transactions");
}

/// Owner answer Q4 in the real UI: a value typed at the prompt near (not on) a terminal is taken
/// literally, even while the pointer rests on the drawing.
#[test]
fn q4_a_typed_point_near_a_terminal_does_not_attach_in_the_ui() {
    let mut h = harness();
    ok(&mut h, "line 0,0 40,0 40,20 0,20 c");
    view(&mut h);
    ok(&mut h, r#"place {"at":[100,0],"rotation":90}"#);
    button(&mut h, tools::TAB);
    button(&mut h, "Wire");
    // Pointer on the drawing, away from terminals.
    let rest = screen(&h, 60.0, 30.0);
    pointer_to(&mut h, rest);
    type_enter(&mut h, "60,-20");
    let px = h.state().cad.session.pixel_size();
    let near = format!("{},0", 90.0 + 3.0 * px);
    type_enter(&mut h, &near);
    let ended_by_capture = h.state().cad.session.running.is_none();
    if !ended_by_capture {
        type_enter(&mut h, "");
    }
    let d = doc(&mut h);
    let c = d.conductors.values().next().expect("a wire was drawn");
    assert!(
        c.end(End::B).is_none() && !ended_by_capture,
        "typed {near} (3 px from A1) was captured: Q4 says typed points are literal"
    );
}

/// Design §9: during Wire, Ctrl+Z removes the last point (it does not undo the last committed
/// object nor end the command); Space is Enter at the Add Terminal prompt.
#[test]
fn ctrl_z_during_wire_removes_the_last_point_and_space_is_enter() {
    let mut h = harness();
    ok(&mut h, "line 0,0 40,0 40,20 0,20 c");
    view(&mut h);
    ok(&mut h, r#"place {"at":[100,0],"rotation":90}"#);
    let before = (head(&mut h), doc(&mut h));
    button(&mut h, tools::TAB);
    button(&mut h, "Wire");
    for (x, y) in [(0.0, 40.0), (30.0, 40.0)] {
        let p = screen(&h, x, y);
        click(&mut h, p);
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    let still = h.state().cad.session.running.as_ref().map(|r| r.id.clone());
    let after = (head(&mut h), doc(&mut h));
    assert!(after == before, "Ctrl+Z during Wire changed the project (it undid a committed object)");
    assert_eq!(still.as_deref(), Some("wire"), "Ctrl+Z ended the Wire command");
    assert_eq!(prompt(&h), "Specify next point or [Undo]:", "one point is left");
    h.key_press(Key::Escape);
    h.run_steps(2);
    // Space as Enter ends Add Terminal at its point prompt.
    button(&mut h, "Add Terminal");
    let started = h.state().cad.session.running.is_some();
    // A space typed at a point prompt (egui sends it as text) is Enter.
    h.event(Event::Text(" ".into()));
    h.run_steps(2);
    assert!(started && h.state().cad.session.running.is_none(), "Space ends Add Terminal");
}

/// AC5: hovering the terminal ring names the terminal; hovering writes nothing (head, selection,
/// unsaved state of the project and of the page tab); the tooltip goes when a command starts.
#[test]
fn ac5_hovering_a_terminal_names_it_and_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("ecad-tester-008-hover-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness();
    ok(&mut h, "line 0,0 40,0 40,20 0,20 c");
    view(&mut h);
    let lines: Vec<String> = h.state().cad.session.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    ok(&mut h, &format!("convertcomponent {}", json!({ "handles": lines })));
    let right = h
        .state()
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Line(l) if l.a.x == 40.0 && l.b.x == 40.0))
        .map(|e| e.handle.hex())
        .unwrap();
    ok(&mut h, &format!("addterminal {}", json!({"handle": right, "at": [40, 10], "label": "1"})));
    let left = h
        .state()
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Line(l) if l.a.x == 0.0 && l.b.x == 0.0))
        .map(|e| e.handle.hex())
        .unwrap();
    ok(&mut h, &format!("addterminal {}", json!({"handle": left, "at": [0, 10], "label": "2"})));
    ok(&mut h, r#"place {"at":[100,0],"rotation":90}"#);
    ok(&mut h, r#"wire {"points":[[40,10],[70,10],[70,0],[90,0]]}"#);
    h.state_mut().save_as(&dir.join("hover.ecad").display().to_string()).unwrap();
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(3);
    let before = (head(&mut h), h.state().cad.session.selection(), unsaved(&mut h));
    assert!(!before.2.0, "saved");
    // Each terminal ring in turn: free drawn (2 at 0,10), free symbol (A2 at 110,0), connected
    // drawn (1 at 40,10, where the wire ends), connected symbol (A1 at 90,0).
    let mut seen = Vec::new();
    for (x, y, want) in
        [(0.0, 10.0, "Terminal 2"), (110.0, 0.0, "Terminal A2"), (40.0, 10.0, "Terminal 1"), (90.0, 0.0, "Terminal A1")]
    {
        let p = screen(&h, x, y);
        pointer_to(&mut h, p);
        h.run_steps(3);
        let first = h.state().hover_lines().and_then(|l| l.first().cloned()).unwrap_or_default();
        seen.push((want, first));
    }
    // Over the coil, then the wire, then away.
    for (x, y) in [(110.0, 3.0), (55.0, 10.0), (60.0, 40.0)] {
        let p = screen(&h, x, y);
        pointer_to(&mut h, p);
        h.run_steps(3);
    }
    let after = (head(&mut h), h.state().cad.session.selection(), unsaved(&mut h));
    assert_eq!(after, before, "hovering wrote something");
    // A command starting hides the tooltip.
    let p = screen(&h, 110.0, 3.0);
    pointer_to(&mut h, p);
    h.run_steps(3);
    assert!(h.state().hover_lines().is_some(), "tooltip over the coil");
    button(&mut h, tools::TAB);
    button(&mut h, "Wire");
    pointer_to(&mut h, p);
    h.run_steps(3);
    assert!(h.state().hover_lines().is_none(), "the tooltip goes while a command runs");
    assert!(h.query_by_label("Component (no tag)").is_none());
    h.key_press(Key::Escape);
    h.run_steps(2);
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
    // Design §4.3 / check G: hovering a terminal ring shows "Terminal <label>".
    let wrong: Vec<_> = seen.iter().filter(|(want, got)| got != want).collect();
    assert!(wrong.is_empty(), "terminal hovers (wanted, shown): {wrong:?}");
}

/// The overlay shapes the canvas painted this frame: (centre, size, colour) of circles and rects.
fn overlay_shapes(h: &Harness<'static, Trial>) -> Vec<(Pos2, f32, egui::Color32, bool)> {
    fn walk(s: &egui::Shape, out: &mut Vec<(Pos2, f32, egui::Color32, bool)>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Circle(c) => {
                let filled = c.fill != egui::Color32::TRANSPARENT;
                out.push((c.center, c.radius * 2.0, if filled { c.fill } else { c.stroke.color }, filled));
            }
            egui::Shape::Rect(r) => out.push((r.rect.center(), r.rect.width(), r.stroke.color, false)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, &mut out);
    }
    out
}

/// Design check H: the terminal ring (8 px, cyan), attached dot (4 px, green) and loose square
/// (6 px, neutral) are painted at those sizes at zoom extents, ×0.01 and ×100.
#[test]
fn h_glyphs_keep_their_token_sizes_at_every_zoom() {
    let mut h = harness();
    ok(&mut h, "line 0,0 40,0 40,20 0,20 c");
    let lines: Vec<String> = h.state().cad.session.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    ok(&mut h, &format!("convertcomponent {}", json!({ "handles": lines })));
    ok(&mut h, r#"place {"at":[100,0],"rotation":90}"#);
    ok(&mut h, r#"wire {"points":[[90,0],[90,-20]]}"#);
    let cyan = egui::Color32::from_rgb(0, 255, 255);
    let green = egui::Color32::from_rgb(0, 255, 0);
    for (what, zoom) in [
        ("extents", json!({"mode": "extents"})),
        ("x0.01", json!({"mode": "scale", "factor": 0.01})),
        ("x100 at A1", json!({"mode": "window", "p1": [89.9, -0.1], "p2": [90.1, 0.1]})),
    ] {
        if what == "x0.01" {
            h.state_mut().cad.run("zoom", json!({"mode": "extents"})).unwrap();
            h.run_steps(2);
        }
        let r = h.state_mut().cad.run("zoom", zoom.clone());
        assert!(r.is_ok(), "zoom {what}: {r:?}");
        h.run_steps(3);
        let a1 = screen(&h, 90.0, 0.0);
        let shapes = overlay_shapes(&h);
        let at_a1: Vec<_> = shapes.iter().filter(|s| (s.0 - a1).length() < 0.5).collect();
        assert!(
            at_a1.iter().any(|s| s.2 == cyan && (s.1 - 8.0).abs() < 0.01 && !s.3),
            "{what}: 8 px cyan ring at A1: {at_a1:?}"
        );
        assert!(
            at_a1.iter().any(|s| s.2 == green && (s.1 - 4.0).abs() < 0.01 && s.3),
            "{what}: 4 px green dot at A1: {at_a1:?}"
        );
        if what != "x100 at A1" {
            let loose = screen(&h, 90.0, -20.0);
            let sq: Vec<_> = shapes.iter().filter(|s| (s.0 - loose).length() < 0.5).collect();
            assert!(
                sq.iter().any(|s| (s.1 - 6.0).abs() < 0.01 && s.2 != egui::Color32::from_rgb(255, 0, 0)),
                "{what}: 6 px neutral square: {sq:?}"
            );
        }
    }
}

// ------------------------------------------------------------------ AC6 reference screenshots

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct T(std::thread::Thread);
    impl Wake for T {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(T(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park_timeout(std::time::Duration::from_millis(5));
    }
}

/// Render the harness's last frame with egui-wgpu (the canvas uses its CPU path in tests, so the
/// frame is plain egui shapes) and save it as PNG. `None` when no adapter is available.
fn shot(h: &Harness<'static, Trial>, deltas: &Deltas, path: &std::path::Path) -> Option<egui::ColorImage> {
    use eframe::egui_wgpu::{self, wgpu};
    let setup = egui_wgpu::WgpuSetup::without_display_handle();
    let instance = block_on(setup.new_instance());
    let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
    let rs =
        block_on(egui_wgpu::RenderState::create(&config, &instance, None, egui_wgpu::RendererOptions::PREDICTABLE))
            .ok()?;
    let ctx = &h.ctx;
    let ppp = ctx.pixels_per_point();
    {
        let mut r = rs.renderer.write();
        for (id, img) in deltas.lock().unwrap().iter() {
            match img {
                Some(img) => r.update_texture(&rs.device, &rs.queue, *id, img),
                None => r.free_texture(id),
            }
        }
    }
    let size = ctx.content_rect().size() * ppp;
    let screen = egui_wgpu::ScreenDescriptor {
        pixels_per_point: ppp,
        size_in_pixels: [size.x.round() as u32, size.y.round() as u32],
    };
    let tess = ctx.tessellate(h.output().shapes.clone(), ppp);
    let mut encoder = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: screen.size_in_pixels[0],
            height: screen.size_in_pixels[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: rs.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    {
        let mut r = rs.renderer.write();
        let user = r.update_buffers(&rs.device, &rs.queue, &mut encoder, &tess, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            r.render(&mut pass, &tess, &screen);
        }
        rs.queue.submit(user.into_iter().chain(std::iter::once(encoder.finish())));
    }
    let (w, hgt) = (texture.width() as usize, texture.height() as usize);
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded = (w * 4).div_ceil(align) * align;
    let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded * hgt) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: None },
        },
        wgpu::Extent3d { width: w as u32, height: hgt as u32, depth_or_array_layers: 1 },
    );
    let idx = rs.queue.submit(std::iter::once(enc.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |v| drop(tx.send(v)));
    rs.device
        .poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: Some(std::time::Duration::from_secs(20)) })
        .ok()?;
    rx.recv().ok()?.ok()?;
    let data = buffer.slice(..).get_mapped_range().ok()?.to_vec();
    let bgra = matches!(rs.target_format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut pixels = Vec::with_capacity(w * hgt);
    for row in data.chunks_exact(padded) {
        for &[c0, c1, c2, _] in row[..w * 4].as_chunks::<4>().0 {
            let (r, g, b) = if bgra { (c2, c1, c0) } else { (c0, c1, c2) };
            pixels.push(egui::Color32::from_rgb(r, g, b));
        }
    }
    let image = egui::ColorImage { size: [w, hgt], pixels, source_size: egui::vec2(w as f32, hgt as f32) };
    let r = cadcraft_ui_egui::control::save_screenshot(&image, Some(&path.display().to_string()), 0);
    assert_eq!(r["ok"], true, "{r}");
    Some(image)
}

fn pixel(img: &egui::ColorImage, at: Pos2) -> egui::Color32 {
    let (x, y) = (at.x.round() as usize, at.y.round() as usize);
    img.pixels.get(y * img.size[0] + x).copied().unwrap_or_default()
}

/// Whether a colour near `want` is painted within `r` pixels of `at`.
fn painted_near(img: &egui::ColorImage, at: Pos2, r: f32, want: egui::Color32) -> bool {
    let close = |c: egui::Color32| {
        c.r().abs_diff(want.r()) < 60 && c.g().abs_diff(want.g()) < 60 && c.b().abs_diff(want.b()) < 60
    };
    let ri = r.ceil() as i32;
    (-ri..=ri).any(|dx| (-ri..=ri).any(|dy| close(pixel(img, at + egui::vec2(dx as f32, dy as f32)))))
}

/// AC6 / design check K: the three reference screenshots, for the owner's approval. They are
/// written to `$ECAD_TESTER_SHOTS` only; without it (CI), or without a wgpu adapter, this test only
/// builds the scenes.
#[test]
fn ac6_reference_screenshots() {
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(std::path::PathBuf::from);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    // Check E's drawing (terminals typed, as in E), then `zoom e`.
    ok(&mut h, "line 0,0 40,0 40,20 0,20 c");
    view(&mut h);
    let (a, b) = (screen(&h, -10.0, 30.0), screen(&h, 50.0, -10.0));
    click(&mut h, a);
    click(&mut h, b);
    button(&mut h, tools::TAB);
    button(&mut h, "Convert to Component");
    button(&mut h, "Add Terminal");
    type_enter(&mut h, "40,10");
    type_enter(&mut h, "0,10");
    type_enter(&mut h, "");
    coil_and_wire_by_clicks(&mut h);
    ok(&mut h, r#"wire {"points":[[0,30],[20,30]]}"#);
    h.state_mut().cad.session.set_selection(Vec::new());
    ok(&mut h, "zoom e");
    let away = screen(&h, 60.0, 40.0);
    pointer_to(&mut h, away);
    h.run_steps(3);
    let Some(dir) = out else { return };
    std::fs::create_dir_all(&dir).unwrap();
    let Some(img) = shot(&h, &deltas, &dir.join("008-terminals.png")) else {
        eprintln!("no wgpu adapter: screenshots skipped");
        return;
    };
    // Rings at the four terminals, green dots at the attached ends, neutral squares on the loose wire.
    let cyan = egui::Color32::from_rgb(0, 255, 255);
    let green = egui::Color32::from_rgb(0, 255, 0);
    for (x, y) in [(40.0, 10.0), (0.0, 10.0), (90.0, 0.0), (110.0, 0.0)] {
        assert!(painted_near(&img, screen(&h, x, y), 5.0, cyan), "ring at {x},{y}");
    }
    for (x, y) in [(40.0, 10.0), (90.0, 0.0)] {
        assert!(painted_near(&img, screen(&h, x, y), 2.0, green), "dot at {x},{y}");
    }
    for (x, y) in [(0.0, 30.0), (20.0, 30.0)] {
        assert!(!painted_near(&img, screen(&h, x, y), 4.0, egui::Color32::from_rgb(255, 0, 0)), "no red at {x},{y}");
    }
    // 2. Wire started at 40,10; the cursor within the aperture of A1.
    button(&mut h, "Wire");
    let s = screen(&h, 40.0, 10.0);
    click(&mut h, s);
    let near = screen(&h, 90.0, 0.0) + egui::vec2(4.0, -3.0);
    pointer_to(&mut h, near);
    h.run_steps(3);
    let img = shot(&h, &deltas, &dir.join("008-wire-snap.png")).unwrap();
    let ring = screen(&h, 90.0, 0.0) + egui::vec2(8.0, 0.0);
    assert!(painted_near(&img, ring, 2.0, green), "attach ring around A1");
    h.key_press(Key::Escape);
    h.run_steps(2);
    // 3. Place Symbol after one R, cursor at 100,0 before the click.
    button(&mut h, "Place Symbol");
    type_enter(&mut h, "R");
    let at = screen(&h, 100.0, 0.0);
    pointer_to(&mut h, at);
    h.run_steps(3);
    shot(&h, &deltas, &dir.join("008-place-preview.png")).unwrap();
    h.key_press(Key::Escape);
    h.run_steps(2);
}

/// Design §3.2: a bare name in the banner's Command field or at CADCraft's command line starts the
/// interactive form; `name {json}` in the banner runs the JSON form with no prompt.
#[test]
fn bare_names_start_the_forms_in_both_fields_and_json_lines_do_not_prompt() {
    let mut h = harness();
    ok(&mut h, "line 0,0 40,0 40,20 0,20 c");
    view(&mut h);
    for name in ["convertcomponent", "addterminal", "place", "wire", "connect", "disconnect"] {
        let e = h.state_mut().submit_command(name);
        h.run_steps(2);
        assert!(e.is_none(), "banner {name}: {e:?}");
        assert_eq!(
            h.state().cad.session.running.as_ref().map(|r| r.id.clone()).as_deref(),
            Some(name),
            "banner {name}"
        );
        h.key_press(Key::Escape);
        h.run_steps(2);
        type_enter(&mut h, name);
        assert_eq!(
            h.state().cad.session.running.as_ref().map(|r| r.id.clone()).as_deref(),
            Some(name),
            "command line {name}"
        );
        h.key_press(Key::Escape);
        h.run_steps(2);
    }
    let before = head(&mut h);
    ok(&mut h, r#"wire {"points":[[0,40],[30,40]]}"#);
    assert!(h.state().cad.session.running.is_none());
    assert_eq!(head(&mut h), before + 1);
}

/// The overlay marks are drawn in model coordinates; in a paper-space layout of an ECAD page they
/// must not be painted at the model positions over the sheet.
#[test]
fn marks_are_not_painted_over_a_paper_space_layout_at_model_positions() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    ok(&mut h, "line 0,0 40,0 40,20 0,20 c");
    let lines: Vec<String> = h.state().cad.session.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    ok(&mut h, &format!("convertcomponent {}", json!({ "handles": lines })));
    ok(&mut h, r#"place {"at":[100,0],"rotation":90}"#);
    view(&mut h);
    let cyan = egui::Color32::from_rgb(0, 255, 255);
    assert!(overlay_shapes(&h).iter().any(|s| s.2 == cyan), "rings in model space");
    h.get_by_label("Layout1").click();
    h.run_steps(5);
    let in_paper = h.state().cad.session.space();
    if format!("{in_paper:?}").contains("Model") {
        eprintln!("could not switch to Layout1; skipped");
        return;
    }
    // In paper space the model is seen through a viewport (if any) at another scale: rings painted
    // at the paper transform of model coordinates would not sit on the terminals.
    if let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(std::path::PathBuf::from) {
        let _ = shot(&h, &deltas, &dir.join("008-layout1-marks.png"));
    }
    let xf = h.state().cad.canvas.xf.unwrap();
    let ring_at_model_pos =
        overlay_shapes(&h).iter().any(|s| s.2 == cyan && (s.0 - xf.to_screen(Vec2::new(90.0, 0.0))).length() < 0.5);
    assert!(!ring_at_model_pos, "a terminal ring is painted at the model position 90,0 on the paper sheet");
}
