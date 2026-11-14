//! The radial wheel (spec §5, brief 041): eight slots on a ring of 150 / 54 px radii, each 41°
//! with 4° gaps (`ecad_tokens::wheel`), centred on the cursor, with a chip above naming the
//! object. It knows nothing of commands: the caller gives each slot's label, icon, whether it is
//! greyed (and why), and an optional short list; the widget reports what was chosen.
//!
//! # Semantics
//! - **Press, move, release**: opened on a right press, the release over a slot runs it; released
//!   in the centre, the wheel stays open for a click; released outside, it cancels.
//! - **Click, click**: with the button up, a click on a slot runs it; a click outside, or Esc,
//!   cancels; a click in the centre does nothing.
//! - A greyed slot never runs. Hovered or clicked, the reason shows under the wheel with a button
//!   for the next step, if the caller offers one ([`Event::Next`]).
//! - A slot with a list opens it beside the slot (at most [`MAX_LIST`] rows; never a second
//!   wheel); a row reports [`Event::Choose`].
//!
//! Each slot carries its label as its accessible name, at the slot's icon, so tests find it.

use std::f32::consts::TAU;

use ecad_tokens::{Icon, radius, space, ui_type, wheel};
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Context, Id, Order, Pos2, Rect, Sense, Shape, Stroke, WidgetInfo, pos2, vec2};

/// Most rows a slot's list shows.
pub const MAX_LIST: usize = 8;

/// One slot as the caller describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotSpec {
    pub label: String,
    pub icon: Icon,
    /// Greyed: why it cannot run now.
    pub greyed: Option<String>,
    /// The next step offered for a greyed slot (its button's label).
    pub next: Option<String>,
    /// The slot's short list, if it opens one.
    pub list: Option<Vec<String>>,
}

impl SlotSpec {
    pub fn new(label: impl Into<String>, icon: Icon) -> Self {
        SlotSpec { label: label.into(), icon, greyed: None, next: None, list: None }
    }
}

/// What happened this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    None,
    /// Run the slot (clockwise index from the top).
    Run(usize),
    /// A row of a slot's list.
    Choose(usize, usize),
    /// The next step of a greyed slot.
    Next(usize),
    Cancel,
}

/// The wheel's state between frames, kept by the caller while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Wheel {
    /// Its centre on screen.
    pub center: Pos2,
    /// The right button that opened it is still held (press, move, release).
    pub held: bool,
    /// The slot whose list is open.
    pub list: Option<usize>,
    /// The greyed slot whose reason is shown.
    pub reason: Option<usize>,
    /// A frame has been shown (input of the opening frame is not read again).
    shown: bool,
}

impl Wheel {
    /// A wheel opened at `center`; `held` when a right press opened it.
    pub fn new(center: Pos2, held: bool) -> Self {
        Wheel { center, held, list: None, reason: None, shown: false }
    }
}

/// The slot under `p` for a wheel centred at `center`: inside the ring and not in a gap.
pub fn slot_at(center: Pos2, p: Pos2) -> Option<usize> {
    let d = p - center;
    let r = d.length();
    if !(wheel::INNER_RADIUS..=wheel::OUTER_RADIUS).contains(&r) {
        return None;
    }
    // Clockwise from the top, in degrees.
    let a = (d.x.atan2(-d.y).to_degrees() + 360.0) % 360.0;
    let step = 360.0 / wheel::SLOTS as f32;
    let i = ((a + step / 2.0) / step).floor() as usize % wheel::SLOTS;
    let off = (a - i as f32 * step + 540.0) % 360.0 - 180.0;
    (off.abs() <= wheel::SLOT_DEGREES / 2.0).then_some(i)
}

/// The middle of slot `i` (its icon's place).
pub fn slot_center(center: Pos2, i: usize) -> Pos2 {
    let a = (i as f32 * 360.0 / wheel::SLOTS as f32).to_radians();
    let r = (wheel::INNER_RADIUS + wheel::OUTER_RADIUS) / 2.0;
    pos2(center.x + r * a.sin(), center.y - r * a.cos())
}

/// The wheel's accessible label.
pub fn label(chip: &str) -> String {
    format!("Wheel: {chip}")
}

/// Draw the wheel over everything and read this frame's input.
pub fn show(ctx: &Context, w: &mut Wheel, chip: &str, slots: &[Option<SlotSpec>; 8]) -> Event {
    let first = !w.shown;
    w.shown = true;
    let (pos, pressed_primary, released_secondary, esc) = ctx.input(|i| {
        (
            i.pointer.hover_pos().or(i.pointer.interact_pos()),
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.pointer.button_released(egui::PointerButton::Secondary),
            i.key_pressed(egui::Key::Escape),
        )
    });
    let o = wheel::OUTER_RADIUS;
    let bounds = Rect::from_center_size(w.center, vec2(2.0 * o, 2.0 * o)).expand(space::XL * 2.0);
    let hovered = pos.and_then(|p| slot_at(w.center, p));
    let mut ev = Event::None;
    let list_rect = w.list.map(|i| list_rect(w.center, i, slots.get(i).and_then(|s| s.as_ref()).and_then(|s| s.list.as_ref()).map_or(0, Vec::len)));
    let reason_rect = Rect::from_center_size(pos2(w.center.x, w.center.y + o + space::L + 24.0), vec2(320.0, 48.0));

    // The area covers the ring, the chip, the reason and the list, so the page under them hears
    // no click.
    let cover = bounds.union(reason_rect).union(list_rect.unwrap_or(bounds));
    egui::Area::new(Id::unique(("ecad.wheel", chip))).order(Order::Foreground).fixed_pos(cover.min).interactable(true).show(ctx, |ui| {
        let bg = ui.allocate_rect(cover, Sense::click_and_drag());
        bg.widget_info(|| WidgetInfo::labeled(egui::Role::Group, true, label(chip)));
        let painter = ui.painter().clone();
        let s = crate::SHELL;
        for (i, slot) in slots.iter().enumerate() {
            let on = hovered == Some(i) || w.list == Some(i);
            let (fill, ink) = match slot {
                None => (crate::color(s.panel).gamma_multiply(0.85), crate::color(s.text_3)),
                Some(sp) if sp.greyed.is_some() => (crate::color(s.panel), crate::color(s.text_3)),
                Some(_) if on => (crate::color(s.accent_soft), crate::color(s.accent_text)),
                Some(_) => (crate::color(s.raised), crate::color(s.text)),
            };
            painter.add(sector(w.center, i, fill));
            painter.add(Shape::closed_line(outline(w.center, i), Stroke::new(1.0, crate::color(s.line))));
            let Some(sp) = slot else { continue };
            let c = slot_center(w.center, i);
            let icon = Rect::from_center_size(c - vec2(0.0, 8.0), vec2(20.0, 20.0));
            crate::icons::paint(&painter, icon, sp.icon, ink);
            let font = crate::fonts::font(ui.ctx(), ui_type::CAPTION);
            painter.text(c + vec2(0.0, 12.0), egui::Align2::CENTER_CENTER, &sp.label, font, ink);
            let r = ui.interact(Rect::from_center_size(c, vec2(36.0, 36.0)), Id::unique(("ecad.wheel.slot", i)), Sense::hover());
            let greyed = sp.greyed.clone();
            r.widget_info(|| {
                let mut info = WidgetInfo::labeled(egui::Role::Button, greyed.is_none(), sp.label.clone());
                info.selected = None;
                info
            });
        }
        // The centre keeps the object visible; the chip names it.
        painter.circle_stroke(w.center, wheel::INNER_RADIUS, Stroke::new(1.0, crate::color(s.line_strong)));
        let font = crate::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS);
        let galley = painter.layout_no_wrap(chip.to_string(), font, crate::color(s.text));
        let chip_rect = Rect::from_center_size(pos2(w.center.x, w.center.y - o - space::L - 6.0), galley.size() + vec2(2.0 * space::M, space::S));
        painter.rect_filled(chip_rect, crate::style::corner(radius::CHIP), crate::color(s.raised));
        painter.rect_stroke(chip_rect, crate::style::corner(radius::CHIP), Stroke::new(1.0, crate::color(s.line_strong)), egui::StrokeKind::Inside);
        painter.galley(chip_rect.center() - galley.size() / 2.0, galley, crate::color(s.text));
        let chip_r = ui.interact(chip_rect, Id::unique("ecad.wheel.chip"), Sense::hover());
        chip_r.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, chip.to_string()));

        // A greyed slot's reason, with its next step.
        let shown_reason = hovered.filter(|i| slots.get(*i).and_then(|s| s.as_ref()).is_some_and(|s| s.greyed.is_some())).or(w.reason);
        if let Some(i) = shown_reason
            && let Some(Some(sp)) = slots.get(i)
            && let Some(why) = &sp.greyed
        {
            painter.rect_filled(reason_rect, crate::style::corner(radius::TILE), crate::color(s.raised));
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(reason_rect.shrink(space::S)).layout(egui::Layout::left_to_right(egui::Align::Center)));
            crate::prim::read_value(&mut child, why);
            if let Some(next) = &sp.next
                && child.add(crate::prim::Button::new(next).small()).clicked()
            {
                ev = Event::Next(i);
            }
        }
        // A slot's short list.
        if let (Some(i), Some(lr)) = (w.list, list_rect)
            && let Some(Some(sp)) = slots.get(i)
            && let Some(items) = &sp.list
        {
            painter.rect_filled(lr, crate::style::corner(radius::TILE), crate::color(s.raised));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(lr.shrink(space::XS)).layout(egui::Layout::top_down(egui::Align::Min)));
            for (j, item) in items.iter().take(MAX_LIST).enumerate() {
                if child.add(crate::prim::Button::new(item).kind(crate::prim::Kind::Quiet).small()).clicked() {
                    ev = Event::Choose(i, j);
                }
            }
        }
    });
    if ev != Event::None {
        return ev;
    }
    if esc {
        return Event::Cancel;
    }
    let in_extra = |p: Pos2| list_rect.is_some_and(|r| r.contains(p)) || (w.reason.is_some() && reason_rect.contains(p));
    let activate = |w: &mut Wheel, i: usize| -> Event {
        match slots.get(i).and_then(|s| s.as_ref()) {
            None => Event::None,
            Some(sp) if sp.greyed.is_some() => {
                w.reason = Some(i);
                Event::None
            }
            Some(sp) if sp.list.is_some() => {
                w.list = Some(i);
                Event::None
            }
            Some(_) => Event::Run(i),
        }
    };
    if w.held {
        if released_secondary {
            w.held = false;
            let Some(p) = pos else { return Event::Cancel };
            let r = (p - w.center).length();
            if r < wheel::INNER_RADIUS {
                return Event::None;
            }
            return match hovered {
                Some(i) => activate(w, i),
                None => Event::Cancel,
            };
        }
        return Event::None;
    }
    if first {
        return Event::None;
    }
    if pressed_primary && let Some(p) = pos {
        if in_extra(p) {
            return Event::None;
        }
        if (p - w.center).length() < wheel::INNER_RADIUS {
            return Event::None;
        }
        return match hovered {
            Some(i) => activate(w, i),
            None => Event::Cancel,
        };
    }
    Event::None
}

/// Where slot `i`'s list of `n` rows goes: beside the slot, away from the centre.
fn list_rect(center: Pos2, i: usize, n: usize) -> Rect {
    let c = slot_center(center, i);
    let h = (n.min(MAX_LIST) as f32) * 26.0 + space::S;
    let w = 180.0;
    let x = if c.x >= center.x { center.x + wheel::OUTER_RADIUS + space::S } else { center.x - wheel::OUTER_RADIUS - space::S - w };
    Rect::from_min_size(pos2(x, c.y - h / 2.0), vec2(w, h))
}

/// The points along slot `i`'s outline: the outer arc, then the inner arc back.
fn outline(center: Pos2, i: usize) -> Vec<Pos2> {
    let (a0, a1) = span(i);
    let steps = 12;
    let at = |r: f32, a: f32| pos2(center.x + r * a.sin(), center.y - r * a.cos());
    let mut v: Vec<Pos2> = (0..=steps).map(|k| at(wheel::OUTER_RADIUS, a0 + (a1 - a0) * k as f32 / steps as f32)).collect();
    v.extend((0..=steps).rev().map(|k| at(wheel::INNER_RADIUS, a0 + (a1 - a0) * k as f32 / steps as f32)));
    v
}

/// Slot `i`'s start and end angle, radians clockwise from the top.
fn span(i: usize) -> (f32, f32) {
    let mid = i as f32 * TAU / wheel::SLOTS as f32;
    let half = (wheel::SLOT_DEGREES / 2.0).to_radians();
    (mid - half, mid + half)
}

/// Slot `i` filled (a ring sector as a triangle strip).
fn sector(center: Pos2, i: usize, fill: Color32) -> Shape {
    let (a0, a1) = span(i);
    let steps = 12u32;
    let at = |r: f32, a: f32| pos2(center.x + r * a.sin(), center.y - r * a.cos());
    let mut mesh = Mesh::default();
    for k in 0..=steps {
        let a = a0 + (a1 - a0) * k as f32 / steps as f32;
        mesh.vertices.push(Vertex { pos: at(wheel::OUTER_RADIUS, a), uv: WHITE_UV, color: fill });
        mesh.vertices.push(Vertex { pos: at(wheel::INNER_RADIUS, a), uv: WHITE_UV, color: fill });
    }
    for k in 0..steps {
        let b = 2 * k;
        mesh.add_triangle(b, b + 1, b + 2);
        mesh.add_triangle(b + 1, b + 3, b + 2);
    }
    Shape::mesh(mesh)
}
