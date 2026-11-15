//! The drawn cable at the top of Project ▸ Cables (board 2.06; brief 057, owner): the cabinet end
//! and the field end with their connection points, the cable body with its tag, type and length,
//! a gland at each end, each core in its own colour through to its point, and open ends in red.
//! **Swap ends** draws it mirrored; **Cores only** hides it. The core colours come from the
//! cable's cable type (`core_colours`, [`ecad_cadcraft::styles::cable::core_colours`]); a core
//! labelled with a colour code (`GNYE`) is drawn in it.
//!
//! **Update map.** Nothing of its own: it draws what the tab's view holds (the core table's copy
//! and the selected cable's colours, read when the selection or the project changes).

use ecad_ui::tokens::{line, radius, space, ui_type};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

/// One core as drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Core {
    /// Its label (`1`, `GNYE`).
    pub label: String,
    /// Its colour code, when known.
    pub colour: Option<String>,
    /// The wire number it carries.
    pub wire: String,
    /// Its connection point at each end (`-X01:1`), empty when open.
    pub from: String,
    pub to: String,
    /// The wire beyond the field end (`L1-04`), when there is one.
    pub field_wire: String,
    /// Ticked Not used (owner rule): drawn as a spare, no open end.
    pub unused: bool,
}

impl Core {
    /// Whether an end is open (owner rule): not ticked Not used and attached to nothing.
    pub fn open(&self, end_from: bool) -> bool {
        let this = if end_from { &self.from } else { &self.to };
        !self.unused && this.is_empty()
    }
}

/// The drawn cable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Drawing {
    pub tag: String,
    /// `H07RN-F 4G1.5 · 12 m`.
    pub words: String,
    /// Each end's location and device (`+CAB1 -X01`).
    pub from: String,
    pub to: String,
    pub cores: Vec<Core>,
    /// Brief 059 (design 059 §5.5): not drawn yet: the ends show the plan (`=PUMP +CAB1
    /// (planned)`), the cores are thin and grey, with no points and no open ends.
    pub not_drawn: bool,
    /// Brief 059 (059d): each end's gland size, cabinet then field (`M20`); empty when none.
    pub glands: [String; 2],
}

/// The core colour of row `i` labelled `label`: the label itself when it is a colour code, else the
/// cable type's colour at that place.
pub fn core_colour(label: &str, i: usize, colours: &[String]) -> Option<String> {
    let l = label.trim();
    if l.chars().any(|c| c.is_ascii_alphabetic()) && ecad_cadcraft::look::colour_code(l).is_some() {
        return Some(l.to_ascii_uppercase());
    }
    colours.get(i).cloned()
}

/// The point part of `-X01:1` (`1`), or the whole text.
fn pin(of: &str) -> &str {
    of.rsplit_once(':').map_or(of, |(_, p)| p)
}

/// The colours a core is drawn in: solid, and dashes for a two-colour code. A core is drawn in its
/// true (paper) colour on either sheet, so black stays black; [`core_line`] gives a dark core an
/// outline that keeps it readable on the dark sheet (owner look check).
fn colours(code: Option<&str>) -> (Color32, Option<Color32>) {
    match code.and_then(|c| ecad_cadcraft::look::wire_colours(c, &ecad_drawing_style::sheet::PAPER)) {
        Some(w) => (ecad_ui::color(w.solid), w.dashes.map(ecad_ui::color)),
        None => (ecad_ui::color(ecad_ui::SHELL.text_2), None),
    }
}

/// The bars of colour code `code` (one, or two for a two-colour code), in its true colours.
pub fn bars(code: &str) -> Vec<ecad_ui::tokens::Color> {
    match ecad_cadcraft::look::wire_colours(code, &ecad_drawing_style::sheet::PAPER) {
        Some(w) => std::iter::once(w.solid).chain(w.dashes).collect(),
        None => Vec::new(),
    }
}

impl crate::Trial {
    /// An open core's fix (brief 057, owner): `0` ticks it Not used (one `setprop`); `1` goes to
    /// its open end (its page, the end selected), or to the cable when the core has no wire.
    pub(crate) fn core_fix(&mut self, f: ecad_cadcraft::checks::Fix, k: usize) {
        use ecad_cadcraft::checks::{Check, Fix, Kind, Object};
        let Fix::CoreOpen(cable, i) = f else { return };
        let Some(p) = self.project.clone() else { return };
        let read = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let open = ecad_cadcraft::styles::cable::open_cores(doc, cable).into_iter().nth(usize::from(i))?;
            let place = match open.end {
                Some((c, end)) => {
                    let (page, at) = ecad_cadcraft::checks::conductor_place(doc, c, Some(end));
                    (page, at, Object::Conductor(c))
                }
                None => {
                    let (page, at) =
                        ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Cable(cable)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
                    (page, at, Object::Cable(cable))
                }
            };
            Some((open.label, place))
        });
        let Ok(Some((label, (page, at, object)))) = read else { return };
        if k == 0 {
            let key = ecad_cadcraft::styles::cable::unused_key(&label);
            let _ = self.run_value("setprop", json!({"cable": cable.hex(), "key": key, "value": "yes"}));
            return;
        }
        let check = Check { kind: Kind::CoreOpen, page, at, object, which: label, fix: None };
        self.panel_action(crate::project_panel::Action::GotoCheck(Box::new(check)));
    }
}

/// Relative luminance (sRGB, 0–1) of `c`.
fn luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let x = f32::from(v) / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

/// Whether `fg` needs an outline to read on `bg` (contrast ratio under 3:1).
fn needs_outline(fg: Color32, bg: Color32) -> bool {
    let (a, b) = (luminance(fg), luminance(bg));
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05) < 3.0
}

/// A line of `points` in `solid`, with `dashes` laid over it; a core too close to the ground
/// colour gets an outline in the strong line colour first.
fn core_line(p: &egui::Painter, points: &[Pos2], solid: Color32, dashes: Option<Color32>, width: f32, ground: Color32) {
    if needs_outline(solid, ground) {
        p.add(egui::Shape::line(points.to_vec(), Stroke::new(width + 2.0, ecad_ui::color(ecad_ui::SHELL.line_strong))));
    }
    p.add(egui::Shape::line(points.to_vec(), Stroke::new(width, solid)));
    if let Some(d) = dashes {
        p.add(egui::Shape::dashed_line(points, Stroke::new(width, d), 6.0, 6.0));
    }
}

/// The row pitch of the drawing.
const PITCH: f32 = 34.0;
/// An end box's width.
const END_W: f32 = 300.0;

/// The height the drawing takes for `n` cores.
pub fn height(n: usize) -> f32 {
    (n.max(1) as f32) * PITCH + 120.0
}

/// Draw `d` in the width available; `swapped` draws the field end at the left.
pub fn show(ui: &mut egui::Ui, d: &Drawing, swapped: bool) {
    let s = ecad_ui::SHELL;
    let (area, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height(d.cores.len())), Sense::hover());
    let info = format!("{} drawn end to end, {} cores", d.tag, d.cores.len());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &info));
    let area = area.shrink2(vec2(space::L, space::XS));
    let p = ui.painter().clone();
    p.rect_stroke(area, ecad_ui::style::corner(radius::CARD), Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)), egui::StrokeKind::Inside);
    let small = ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL);
    let body = ecad_ui::fonts::font(ui.ctx(), ui_type::BODY);
    let emph = ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS);
    let n = d.cores.len().max(1);
    let end_w = END_W.min(area.width() / 4.5);
    let top = area.top() + space::XL;
    let rows_top = top + 48.0;
    let end_h = 48.0 + n as f32 * PITCH + space::S;
    let left = Rect::from_min_size(pos2(area.left() + space::XL, top), vec2(end_w, end_h));
    let right = Rect::from_min_size(pos2(area.right() - space::XL - end_w, top), vec2(end_w, end_h));
    let (l_title, r_title, l_sub, r_sub) =
        if swapped { (&d.to, &d.from, "Field end", "Cabinet end") } else { (&d.from, &d.to, "Cabinet end", "Field end") };
    let planned = |t: &String| if d.not_drawn && !t.is_empty() { format!("{t} (planned)") } else { t.clone() };
    let (l_title, r_title) = (&planned(l_title), &planned(r_title));
    // The two end boxes.
    for (r, title, sub) in [(left, l_title, l_sub), (right, r_title, r_sub)] {
        p.rect_filled(r, ecad_ui::style::corner(radius::CARD), ecad_ui::color(s.panel));
        p.rect_stroke(r, ecad_ui::style::corner(radius::CARD), Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)), egui::StrokeKind::Inside);
        let title = if title.is_empty() { "\u{2014}".to_string() } else { title.replace(' ', " \u{b7} ").replace("\u{b7} (planned)", "(planned)") };
        let ink = if d.not_drawn { s.text_3 } else { s.text };
        p.text(r.left_top() + vec2(space::L, space::M), egui::Align2::LEFT_TOP, title, emph.clone(), ecad_ui::color(ink));
        p.text(
            r.left_top() + vec2(space::L, space::M + ui_type::BODY.line + 2.0),
            egui::Align2::LEFT_TOP,
            sub,
            small.clone(),
            ecad_ui::color(s.text_3),
        );
    }
    // The glands: a bar outside each end, the size under it (2.7 gives it; a dash until then).
    let bar_h = n as f32 * PITCH + space::S;
    let l_bar = Rect::from_min_size(pos2(left.right() + space::L, rows_top - space::XS), vec2(10.0, bar_h));
    let r_bar = Rect::from_min_size(pos2(right.left() - space::L - 10.0, rows_top - space::XS), vec2(10.0, bar_h));
    let (l_gland, r_gland) = if swapped { (&d.glands[1], &d.glands[0]) } else { (&d.glands[0], &d.glands[1]) };
    for (b, g) in [(l_bar, l_gland), (r_bar, r_gland)] {
        p.rect_filled(b, ecad_ui::style::corner(3.0), ecad_ui::color(s.line_strong));
        let words = if g.is_empty() { "gland \u{2014}".to_string() } else { format!("gland {g}") };
        p.text(pos2(b.center().x, b.bottom() + space::S), egui::Align2::CENTER_TOP, words, small.clone(), ecad_ui::color(s.text_3));
    }
    // The cable body between the glands, with its words over it.
    let jacket = Rect::from_min_max(pos2(l_bar.right() + 110.0, rows_top - space::S), pos2(r_bar.left() - 110.0, rows_top + bar_h));
    // The cable token, as cables are drawn on the canvas.
    let accent = ecad_ui::color(s.cable);
    if jacket.width() > 40.0 {
        p.rect_filled(jacket, ecad_ui::style::corner(jacket.height() / 2.0), accent.gamma_multiply(0.22));
        p.rect_stroke(jacket, ecad_ui::style::corner(jacket.height() / 2.0), Stroke::new(1.5, accent), egui::StrokeKind::Inside);
        let tag = if d.not_drawn { format!("{} \u{b7} not drawn", d.tag) } else { d.tag.clone() };
        let words = if d.words.is_empty() { tag } else { format!("{tag} \u{b7} {}", d.words) };
        p.text(pos2(jacket.center().x, jacket.top() - space::S), egui::Align2::CENTER_BOTTOM, words, small.clone(), accent);
    }
    // The cores.
    let red = ecad_ui::color(s.error);
    // Brief 059: a cable not drawn: each core thin and grey through the body, no points.
    if d.not_drawn {
        for (i, c) in d.cores.iter().enumerate() {
            let y = rows_top + PITCH * i as f32 + PITCH / 2.0;
            let jy = jacket.top() + jacket.height() * (i as f32 + 0.5) / n as f32;
            let pts = [
                pos2(l_bar.right(), y),
                pos2(jacket.left() - 40.0, y),
                pos2(jacket.left() + 20.0, jy),
                pos2(jacket.right() - 20.0, jy),
                pos2(jacket.right() + 40.0, y),
                pos2(r_bar.left(), y),
            ];
            p.add(egui::Shape::line(pts.to_vec(), Stroke::new(1.5, ecad_ui::color(s.text_3))));
            let name = match &c.colour {
                Some(code) if !code.eq_ignore_ascii_case(&c.label) => format!("{} \u{b7} {code}", c.label),
                _ => c.label.clone(),
            };
            p.text(
                pos2(left.left() + space::L, y),
                egui::Align2::LEFT_CENTER,
                format!("{name} \u{b7} not assigned"),
                body.clone(),
                ecad_ui::color(s.text_3),
            );
        }
        return;
    }
    for (i, c) in d.cores.iter().enumerate() {
        let y = rows_top + PITCH * i as f32 + PITCH / 2.0;
        let (l_pt, r_pt) = if swapped { (&c.to, &c.from) } else { (&c.from, &c.to) };
        let (l_open, r_open) = if swapped { (c.open(false), c.open(true)) } else { (c.open(true), c.open(false)) };
        // The points at each end: a small box with the point's label.
        let l_box = Rect::from_center_size(pos2(left.right() - space::L - 18.0, y), vec2(36.0, 22.0));
        let r_box = Rect::from_center_size(pos2(right.left() + space::L + 18.0, y), vec2(36.0, 22.0));
        for (b, pt, open, label) in [(l_box, l_pt, l_open, &c.label), (r_box, r_pt, r_open, &c.label)] {
            let words = if pt.is_empty() { label.as_str() } else { pin(pt) };
            if open {
                p.add(egui::Shape::dashed_line(
                    &[b.left_top(), b.right_top(), b.right_bottom(), b.left_bottom(), b.left_top()],
                    Stroke::new(1.0, red),
                    3.0,
                    2.0,
                ));
                p.text(b.center(), egui::Align2::CENTER_CENTER, words, small.clone(), red);
            } else if !c.unused || !pt.is_empty() {
                p.rect_filled(b, ecad_ui::style::corner(4.0), ecad_ui::color(s.bg));
                p.rect_stroke(
                    b,
                    ecad_ui::style::corner(4.0),
                    Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line_strong)),
                    egui::StrokeKind::Inside,
                );
                p.text(b.center(), egui::Align2::CENTER_CENTER, words, small.clone(), ecad_ui::color(s.text));
            }
        }
        // The cabinet end's words: the wire and the point.
        // Each end's words: the wire there and the point (owner look check: the field end too).
        let (l_wire, r_wire) = if swapped { (&c.field_wire, &c.wire) } else { (&c.wire, &c.field_wire) };
        let words = |wire: &String, pt: &String| match (wire.is_empty(), pt.is_empty()) {
            (_, true) if c.unused => "not used".to_string(),
            (_, true) => "open".to_string(),
            (true, false) => pt.clone(),
            (false, false) => format!("{wire} \u{b7} {pt}"),
        };
        let ink = |open: bool| {
            if open {
                red
            } else if c.unused {
                ecad_ui::color(s.text_3)
            } else {
                ecad_ui::color(s.text_2)
            }
        };
        p.text(pos2(left.left() + space::L, y), egui::Align2::LEFT_CENTER, words(l_wire, l_pt), body.clone(), ink(l_open));
        p.text(pos2(r_box.right() + space::M, y), egui::Align2::LEFT_CENTER, words(r_wire, r_pt), body.clone(), ink(r_open));
        // The line: from the left point through the glands and the body to the right point.
        let (solid, dashes) = colours(c.colour.as_deref());
        let (solid, dashes) = if c.unused { (solid.gamma_multiply(0.45), dashes.map(|d| d.gamma_multiply(0.45))) } else { (solid, dashes) };
        let jy = jacket.top() + jacket.height() * (i as f32 + 0.5) / n as f32;
        let mut pts = vec![pos2(l_box.right(), y), pos2(jacket.left() - 40.0, y), pos2(jacket.left() + 20.0, jy)];
        pts.extend([pos2(jacket.right() - 20.0, jy), pos2(jacket.right() + 40.0, y), pos2(r_box.left(), y)]);
        if l_open {
            pts.remove(0);
        }
        if r_open {
            pts.pop();
        }
        core_line(&p, &pts, solid, dashes, 3.0, ecad_ui::color(s.panel));
        for (open, at, below) in [(l_open, pts.first().copied(), l_box), (r_open, pts.last().copied(), r_box)] {
            if open && let Some(at) = at {
                p.circle_filled(at, 4.0, red);
                p.text(pos2(below.center().x, below.bottom() + 2.0), egui::Align2::CENTER_TOP, "open end", small.clone(), red);
            }
        }
        // The colour's name where the core leaves the body.
        if let Some(code) = &c.colour {
            p.text(pos2(r_bar.left() - space::M, y - 4.0), egui::Align2::RIGHT_BOTTOM, code, small.clone(), ecad_ui::color(s.text_3));
        }
    }
}
