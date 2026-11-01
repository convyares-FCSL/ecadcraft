//! ECAD-SEAM(owner): marks a document's owner draws over its drawing (`DocumentOwner::overlay`).
//! Each mark is centred on its drawing position and keeps its size on screen at every zoom; marks
//! off the drawing area or at positions the screen cannot hold are left out; a plain drawing is
//! drawn as before, and an owned one asks its owner every frame without changing anything.

use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cadcraft_engine::Session;
use cadcraft_engine::doc::color::Rgb;
use cadcraft_engine::ext::{Commit, DocumentOwner, Mark, MarkShape, OwnerRef, Verdict};
use cadcraft_engine::geom::Vec2;
use cadcraft_ui_egui::canvas::{Xf, owner_mark_shapes};
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Pos2, Rect, Shape, pos2, vec2};
use egui_kittest::Harness;
use serde_json::Value;

fn mark(x: f64, y: f64, shape: MarkShape, size: f32) -> Mark {
    Mark { at: Vec2::new(x, y), shape, size, stroke: 1.0, color: Rgb(0, 255, 255) }
}

fn xf(scale: f64) -> Xf {
    Xf { rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)), center: Vec2::new(0.0, 0.0), scale }
}

/// Centre and size on screen of one shape.
fn placed(s: &Shape) -> (Pos2, f32) {
    match s {
        Shape::Circle(c) => (c.center, c.radius * 2.0),
        Shape::Rect(r) => (r.rect.center(), r.rect.width()),
        other => panic!("unexpected shape {other:?}"),
    }
}

#[test]
fn marks_keep_their_screen_size_at_every_zoom() {
    let marks = [mark(0.0, 0.0, MarkShape::Ring, 8.0), mark(1.0, 1.0, MarkShape::Dot, 4.0), mark(-1.0, 0.0, MarkShape::Square, 6.0)];
    for scale in [0.01, 1.0, 100.0] {
        let x = xf(scale);
        let shapes = owner_mark_shapes(&marks, &x, x.rect);
        let visible: Vec<&Mark> = marks.iter().filter(|m| x.rect.expand(m.size).contains(x.to_screen(m.at))).collect();
        assert_eq!(shapes.len(), visible.len(), "scale {scale}");
        for (shape, m) in shapes.iter().zip(visible) {
            let (centre, size) = placed(shape);
            assert!((centre - x.to_screen(m.at)).length() < 1e-3, "centred on the drawing position at scale {scale}");
            assert!((size - m.size).abs() < 1e-3, "{size} points across at scale {scale}, as asked");
        }
    }
    let x = xf(1.0);
    let shapes = owner_mark_shapes(&marks, &x, x.rect);
    assert!(matches!(&shapes[0], Shape::Circle(c) if c.fill == egui::Color32::TRANSPARENT && c.stroke.width == 1.0), "a ring is open");
    assert!(matches!(&shapes[1], Shape::Circle(c) if c.fill == egui::Color32::from_rgb(0, 255, 255)), "a dot is filled");
    assert!(matches!(&shapes[2], Shape::Rect(_)), "a square");
}

#[test]
fn marks_off_the_drawing_area_or_off_any_screen_are_left_out() {
    let x = xf(1.0);
    let marks = [mark(1e4, 0.0, MarkShape::Ring, 8.0), mark(1e300, 1e300, MarkShape::Dot, 4.0), mark(399.0, 0.0, MarkShape::Ring, 8.0)];
    let shapes = owner_mark_shapes(&marks, &x, x.rect);
    assert_eq!(shapes.len(), 1, "only the one at the edge, which still shows in part");
}

struct Owner {
    asked: Arc<AtomicUsize>,
    marks: Vec<Mark>,
}

impl DocumentOwner for Owner {
    fn commit(&mut self, _s: &mut Session, _c: &Commit<'_>) -> Verdict {
        Verdict::Accept
    }
    fn undo(&mut self, _s: &mut Session, _uid: u64) -> cadcraft_engine::Result<Option<String>> {
        Ok(None)
    }
    fn redo(&mut self, _s: &mut Session, _uid: u64) -> cadcraft_engine::Result<Option<String>> {
        Ok(None)
    }
    fn save(&mut self, _s: &mut Session, _uid: u64, _path: Option<&str>, _save_as: bool) -> cadcraft_engine::Result<Value> {
        Ok(Value::Null)
    }
    fn is_unsaved(&self) -> bool {
        false
    }
    fn close_needs_prompt(&self, _uid: u64) -> bool {
        false
    }
    fn closed(&mut self, _s: &mut Session, _uid: u64) -> bool {
        true
    }
    fn overlay(&self, _s: &Session, _uid: u64) -> Vec<Mark> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        self.marks.clone()
    }
    fn recover(&mut self, _s: &mut Session) {}
    fn label(&self) -> String {
        "overlay test".into()
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[test]
fn an_owned_drawing_asks_its_owner_every_frame_and_changes_nothing() {
    let mut app = CadApp::new(Session::new(), Services::default());
    let asked = Arc::new(AtomicUsize::new(0));
    let marks = vec![mark(0.0, 0.0, MarkShape::Ring, 8.0), mark(5.0, 5.0, MarkShape::Dot, 4.0), mark(f64::NAN, 0.0, MarkShape::Square, 6.0)];
    let owner: OwnerRef = Arc::new(Mutex::new(Owner { asked: asked.clone(), marks }));
    let uid = app.session.state().unwrap().uid;
    app.session.ext.bind(uid, owner);
    let revision = app.session.state().unwrap().revision;
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    assert!(asked.load(Ordering::Relaxed) >= 4, "asked for its marks each frame");
    let app = h.state();
    assert_eq!(app.session.state().unwrap().revision, revision, "drawing them changed nothing");
    assert_eq!(app.session.owner_overlay().len(), 2, "the mark at no position is dropped");
}
