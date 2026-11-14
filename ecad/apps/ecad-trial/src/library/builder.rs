//! The builder's main view and tool strip (design 048 §3.1–§3.4): the symbol on an empty grid,
//! `sheet.sheet` ground, `sheet.grid` dots every 2.5 mm and a heavier dot every 5 mm, the origin
//! as a `sheet.ink_2` cross; connection points as `sheet.connection_point` rings with their key,
//! text slots as samples in `sheet.ink_2` in a dashed `sheet.placement_preview` box with an anchor
//! dot; offers dashed. The six tools: Select (V), Draw (D: Line, Polyline, Rectangle, Circle,
//! Arc), Text (T), Connection point (C), Text slot (S), Origin (O).
//!
//! The draft is drawn and edited here; nothing reaches the project until Save.

use ecad_model::{Primitive, Pt, TextHAlign, TextVAlign};
use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, Shape, Stroke, pos2, vec2};

use super::draft::{self, Draft, GRID_MM, Offer, Picked, TEXT_MM};
use super::review::Review;

/// Element names (design 048 §3.1).
pub const BUILDER: &str = "library.builder";
pub const CANVAS: &str = "library.builder.canvas";
pub const BREADCRUMB: &str = "library.builder.breadcrumb";
pub const HINT: &str = "library.builder.hint";
pub const INTENT: &str = "library.builder.intent";
pub const BACK: &str = "library.builder.back";
pub const ENTRY: &str = "library.builder.entry";
pub const VERSION_NEWEST: &str = "library.builder.version.newest";
pub const VERSION_SHOWN: &str = "library.builder.version.shown";
/// The empty state's hint (design 048 §3.4).
pub const EMPTY_HINT: &str = "Draw the symbol, then add connection points and a tag slot";

/// Symbol outline weight (spec §4), millimetres.
pub const OUTLINE_MM: f64 = 0.35;
/// The origin cross's arms (`ecad.builder-origin`), millimetres.
pub const ORIGIN_ARM_MM: f64 = 4.0;
/// A connection point's ring (`sheet.connection_point`), pixels across.
pub const RING_PX: f32 = 8.0;
/// How near a click must be to pick or snap, pixels.
pub const PICK_PX: f32 = 8.0;
/// Zoom limits, pixels per millimetre (print size is about 3.78).
pub const MIN_SCALE: f32 = 2.0;
pub const MAX_SCALE: f32 = 60.0;
/// The scale a new builder opens at: a 2.5 mm grid 25 px apart.
pub const START_SCALE: f32 = 10.0;

/// The builder's tools (design 048 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Select,
    Line,
    Polyline,
    Rectangle,
    Circle,
    Arc,
    Text,
    Point,
    Slot,
    Origin,
}

impl Tool {
    /// The Draw group's tools, in flyout order.
    pub const DRAW: [Tool; 5] = [Tool::Line, Tool::Polyline, Tool::Rectangle, Tool::Circle, Tool::Arc];

    pub fn is_draw(self) -> bool {
        Tool::DRAW.contains(&self)
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Line => "Line",
            Tool::Polyline => "Polyline",
            Tool::Rectangle => "Rectangle",
            Tool::Circle => "Circle",
            Tool::Arc => "Arc",
            Tool::Text => "Text",
            Tool::Point => "Connection point",
            Tool::Slot => "Text slot",
            Tool::Origin => "Origin",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Tool::Select => Icon::Select,
            Tool::Line => Icon::Line,
            Tool::Polyline => Icon::Polyline,
            Tool::Rectangle => Icon::Rectangle,
            Tool::Circle => Icon::Circle,
            Tool::Arc => Icon::Arc,
            Tool::Text => Icon::Text,
            Tool::Point => Icon::ConnectionPoint,
            Tool::Slot => Icon::TextSlot,
            Tool::Origin => Icon::Origin,
        }
    }
}

/// A strip button (design 048 §3.1): its element name, key and tooltip.
pub struct StripButton {
    pub name: &'static str,
    pub key: Key,
    pub tip: &'static str,
}

/// The strip's six buttons, in order.
pub const STRIP: [StripButton; 6] = [
    StripButton { name: "library.builder.tool.select", key: Key::V, tip: "Select (V)" },
    StripButton { name: "library.builder.tool.draw", key: Key::D, tip: "Draw (D)" },
    StripButton { name: "library.builder.tool.text", key: Key::T, tip: "Text (T)" },
    StripButton { name: "library.builder.tool.point", key: Key::C, tip: "Connection point (C)" },
    StripButton { name: "library.builder.tool.slot", key: Key::S, tip: "Text slot (S)" },
    StripButton { name: "library.builder.tool.origin", key: Key::O, tip: "Origin (O)" },
];

/// Editing an older version than the newest asks first (design 048 §6.1 step 1).
#[derive(Debug, Clone, PartialEq)]
pub struct AskVersion {
    pub newest: ecad_model::DefinitionId,
    pub shown: u64,
    pub newest_seq: u64,
}

/// What is being typed in place.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// A new text at this point.
    Text(Pt),
    /// Point `n`'s default label ("Adds point 3. Label?").
    Label(usize),
}

/// The builder's state between frames.
#[derive(Debug)]
pub struct Builder {
    pub draft: Draft,
    pub tool: Tool,
    /// The Draw button's last tool.
    pub draw_tool: Tool,
    /// The text the Slot tool places (a chip).
    pub slot_text: String,
    /// Millimetres at the view's centre, and pixels per millimetre.
    pub center: Pt,
    pub scale: f32,
    /// Clicks of the shape being drawn.
    pub pending: Vec<Pt>,
    pub selected: Option<Picked>,
    /// A drag in Select: what, and the snapped point it was last at.
    drag: Option<(Picked, Pt)>,
    /// Typing in place: what, and the text so far.
    pub entry: Option<(Entry, String)>,
    /// A refusal at the cursor (design 048 §3.3 step 3): the words and where.
    pub at_cursor: Option<(String, Pt)>,
    /// Save's refusal, shown above the buttons.
    pub refusal: Option<String>,
    /// The review in the drawer, after Save of an edit.
    pub review: Option<Review>,
    pub ask_version: Option<AskVersion>,
    /// Cancel of a changed draft asks: "Discard the draft of …?".
    pub ask_discard: bool,
    /// The snapped cursor, in millimetres.
    pub cursor: Option<Pt>,
    /// The intent line's text last frame (tests read it).
    pub intent: String,
    /// The Draw flyout is open.
    pub flyout: bool,
    /// A Details field to focus next frame (a Missing row's button).
    pub focus: Option<&'static str>,
    fitted: bool,
}

impl Builder {
    pub fn new(draft: Draft) -> Self {
        Builder {
            draft,
            tool: Tool::Select,
            draw_tool: Tool::Line,
            slot_text: "tag".into(),
            center: Pt::new(0.0, 0.0),
            scale: START_SCALE,
            pending: Vec::new(),
            selected: None,
            drag: None,
            entry: None,
            at_cursor: None,
            refusal: None,
            review: None,
            ask_version: None,
            ask_discard: false,
            cursor: None,
            intent: String::new(),
            flyout: false,
            focus: None,
            fitted: false,
        }
    }

    /// Pick a tool: what was being drawn is dropped.
    pub fn set_tool(&mut self, tool: Tool) {
        if self.draft.fixed_drawing() && (tool.is_draw() || tool == Tool::Text || tool == Tool::Point || tool == Tool::Origin) {
            self.at_cursor = Some(("A promoted drawing keeps its shapes and points; add slots and details here.".into(), Pt::new(0.0, 0.0)));
            return;
        }
        self.tool = tool;
        if tool.is_draw() {
            self.draw_tool = tool;
        }
        if tool == Tool::Slot {
            let used = self.draft.used_slot_texts();
            if used.contains(&self.slot_text.as_str())
                && let Some((t, _, _)) = draft::SLOT_TEXTS.iter().find(|(t, _, _)| !used.contains(t))
            {
                self.slot_text = (*t).to_string();
            }
        }
        self.pending.clear();
        self.entry = None;
    }

    /// Esc: the shape being drawn goes, then the tool ends; in Select with nothing selected,
    /// nothing happens (the draft stays).
    pub fn escape(&mut self) {
        if self.entry.is_some() {
            self.entry = None;
        } else if !self.pending.is_empty() {
            self.pending.clear();
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
        } else {
            self.selected = None;
        }
        self.at_cursor = None;
    }

    /// The intent line for the tool and the clicks so far.
    pub fn intent_text(&self) -> String {
        let n = self.pending.len();
        match self.tool {
            Tool::Select => match self.selected {
                Some(Picked::Slot(_)) => "Drag to move the slot; R turns it 90°, H and V step its alignment; Delete removes it.".into(),
                Some(_) => "Drag to move it; Delete removes it.".into(),
                None => "Click a shape, point or slot to select it.".into(),
            },
            Tool::Line if n == 0 => "Line: click where it starts.".into(),
            Tool::Line => "Line: click where it ends (Esc stops).".into(),
            Tool::Polyline if n == 0 => "Polyline: click where it starts.".into(),
            Tool::Polyline => "Polyline: click the next corner; Enter or right-click finishes.".into(),
            Tool::Rectangle if n == 0 => "Rectangle: click a corner.".into(),
            Tool::Rectangle => "Rectangle: click the opposite corner.".into(),
            Tool::Circle if n == 0 => "Circle: click its centre.".into(),
            Tool::Circle => "Circle: click a point on it.".into(),
            Tool::Arc if n == 0 => "Arc: click its centre.".into(),
            Tool::Arc if n == 1 => "Arc: click where it starts.".into(),
            Tool::Arc => "Arc: click where it ends (counter-clockwise).".into(),
            Tool::Text => "Text: click where it starts, then type.".into(),
            Tool::Point => match &self.entry {
                Some((Entry::Label(i), _)) => {
                    let key = self.draft.shape.points.get(*i).map_or(String::new(), |p| p.key.clone());
                    format!("Adds point {key}. Label? (Enter keeps none)")
                }
                _ => format!(
                    "Adds point {}. Click on the {} mm grid.",
                    self.draft.next_point_key(),
                    draft::fmt_mm(ecad_cadcraft::symbol_cmds::POINT_GRID_MM)
                ),
            },
            Tool::Slot => format!("Text slot: click where the {} goes.", draft::slot_label(&self.slot_text)),
            Tool::Origin => "Origin: click the symbol's base point.".into(),
        }
    }

    // ------------------------------------------------------------ view

    /// Where millimetre point `p` is drawn in `rect`.
    pub fn to_screen(&self, rect: Rect, p: Pt) -> Pos2 {
        let c = rect.center();
        pos2(c.x + ((p.x - self.center.x) as f32) * self.scale, c.y - ((p.y - self.center.y) as f32) * self.scale)
    }

    fn to_mm(&self, rect: Rect, p: Pos2) -> Pt {
        let c = rect.center();
        Pt::new(self.center.x + f64::from((p.x - c.x) / self.scale), self.center.y - f64::from((p.y - c.y) / self.scale))
    }

    /// Fit the draft in the view once (a made or edited symbol opens with all of it shown).
    fn fit(&mut self, rect: Rect) {
        self.fitted = true;
        let Some((lo, hi)) = bounds(&self.draft) else { return };
        self.center = Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        let (w, h) = ((hi.x - lo.x).max(10.0) as f32, (hi.y - lo.y).max(10.0) as f32);
        let s = ((rect.width() * 0.6) / w).min((rect.height() * 0.6) / h);
        if s.is_finite() {
            self.scale = s.clamp(MIN_SCALE, 20.0);
        }
    }

    /// The point `raw` snaps to: a shape's end, middle or centre within [`PICK_PX`], else the
    /// 2.5 mm grid.
    fn snap(&self, raw: Pt) -> Pt {
        let tol = f64::from(PICK_PX / self.scale);
        let mut best: Option<(f64, Pt)> = None;
        for c in snap_points(&self.draft) {
            let d = dist(c, raw);
            if d <= tol && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, c));
            }
        }
        best.map_or_else(|| Pt::new((raw.x / GRID_MM).round() * GRID_MM, (raw.y / GRID_MM).round() * GRID_MM), |(_, p)| p)
    }

    /// What is under `at` (millimetres): offers, then points, slots, shapes.
    fn pick(&self, at: Pt) -> Option<Picked> {
        let tol = f64::from(PICK_PX / self.scale);
        let sh = &self.draft.shape;
        if let Some(i) = sh.offers.iter().position(|o| match o {
            Offer::Point { at: p } => dist(*p, at) <= tol.max(f64::from(RING_PX / self.scale)),
            Offer::Slot { at: p, .. } => dist(*p, at) <= tol * 2.0,
        }) {
            return Some(Picked::Offer(i));
        }
        if let Some(i) = sh.points.iter().position(|p| dist(p.at, at) <= tol.max(f64::from(RING_PX / self.scale))) {
            return Some(Picked::Point(i));
        }
        if let Some(i) = sh.slots.iter().position(|s| slot_box(s, &self.draft.prefix).contains(at, tol)) {
            return Some(Picked::Slot(i));
        }
        sh.graphics.iter().position(|g| near(g, at, tol)).map(Picked::Graphic)
    }

    /// A click of the running tool at `raw` (millimetres), already snapped by the caller.
    fn click(&mut self, at: Pt) {
        self.at_cursor = None;
        match self.tool {
            Tool::Select => {
                self.selected = self.pick(at);
            }
            Tool::Line => {
                if let Some(a) = self.pending.first().copied() {
                    if dist(a, at) > 1e-9 {
                        self.draft.add_graphic(Primitive::Line { a, b: at });
                    }
                    self.pending = vec![at];
                } else {
                    self.pending.push(at);
                }
            }
            Tool::Polyline => {
                if self.pending.last().is_some_and(|l| dist(*l, at) < 1e-9) {
                    self.finish();
                } else {
                    self.pending.push(at);
                }
            }
            Tool::Rectangle => {
                if let Some(a) = self.pending.first().copied() {
                    if (a.x - at.x).abs() > 1e-9 && (a.y - at.y).abs() > 1e-9 {
                        let v = vec![a, Pt::new(at.x, a.y), at, Pt::new(a.x, at.y)];
                        self.draft.add_graphic(Primitive::Polyline { vertices: v, closed: true });
                    }
                    self.pending.clear();
                } else {
                    self.pending.push(at);
                }
            }
            Tool::Circle => {
                if let Some(c) = self.pending.first().copied() {
                    let r = dist(c, at);
                    if r > 1e-9 {
                        self.draft.add_graphic(Primitive::Circle { center: c, radius: r });
                    }
                    self.pending.clear();
                } else {
                    self.pending.push(at);
                }
            }
            Tool::Arc => {
                if self.pending.len() == 2 {
                    if let (Some(c), Some(s)) = (self.pending.first().copied(), self.pending.get(1).copied()) {
                        let r = dist(c, s);
                        let start = (s.y - c.y).atan2(s.x - c.x);
                        let end = (at.y - c.y).atan2(at.x - c.x);
                        if r > 1e-9 && (start - end).abs() > 1e-9 {
                            self.draft.add_graphic(Primitive::Arc { center: c, radius: r, start, end });
                        }
                    }
                    self.pending.clear();
                } else {
                    self.pending.push(at);
                }
            }
            Tool::Text => self.entry = Some((Entry::Text(at), String::new())),
            Tool::Point => match self.draft.add_point(at) {
                Ok(i) => self.entry = Some((Entry::Label(i), String::new())),
                Err(e) => self.at_cursor = Some((e, at)),
            },
            Tool::Slot => match self.draft.add_slot(&self.slot_text.clone(), at) {
                Ok(i) => {
                    self.selected = Some(Picked::Slot(i));
                    self.tool = Tool::Select;
                }
                Err(e) => self.at_cursor = Some((e, at)),
            },
            Tool::Origin => {
                self.draft.set_origin(at);
                self.center = Pt::new(self.center.x - at.x, self.center.y - at.y);
                self.tool = Tool::Select;
            }
        }
    }

    /// Enter or a right-click: a polyline being drawn is finished; a line chain stops.
    fn finish(&mut self) {
        if self.tool == Tool::Polyline && self.pending.len() >= 2 {
            let v = std::mem::take(&mut self.pending);
            self.draft.add_graphic(Primitive::Polyline { vertices: v, closed: false });
        }
        self.pending.clear();
    }

    /// The typed text is done (Enter).
    fn commit_entry(&mut self) {
        let Some((e, text)) = self.entry.take() else { return };
        match e {
            Entry::Text(at) if !text.trim().is_empty() => self.draft.add_graphic(Primitive::Text {
                at,
                height: TEXT_MM,
                rotation: 0.0,
                value: text.trim().chars().take(200).collect(),
                halign: TextHAlign::Left,
                valign: TextVAlign::Baseline,
            }),
            Entry::Label(i) => self.draft.set_label(i, &text),
            Entry::Text(_) => {}
        }
    }

    /// The builder's keys while the pointer is over it and nothing is typed (design 048 §3.3).
    ///
    /// Taken by the shell's key pass (`shell::keys`), which runs before CADCraft's hidden frame:
    /// otherwise CADCraft's own Ctrl+Z and Ctrl+Y would take the keys first (tester 048b/c).
    pub fn keys(&mut self, ctx: &egui::Context) {
        if self.entry.is_some() || self.ask_version.is_some() {
            return;
        }
        let take = |m: egui::Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if take(egui::Modifiers::COMMAND, Key::Z) {
            self.draft.undo();
            self.selected = None;
        }
        if take(egui::Modifiers::COMMAND, Key::Y) {
            self.draft.redo();
            self.selected = None;
        }
        if take(egui::Modifiers::NONE, Key::Escape) {
            self.escape();
        }
        if take(egui::Modifiers::NONE, Key::Enter) {
            self.finish();
        }
        if let Some(sel) = self.selected
            && (take(egui::Modifiers::NONE, Key::Delete) || take(egui::Modifiers::NONE, Key::Backspace))
        {
            if !(self.draft.fixed_drawing() && matches!(sel, Picked::Graphic(_) | Picked::Point(_))) {
                self.draft.delete(sel);
            }
            self.selected = None;
        }
        if let Some(Picked::Slot(i)) = self.selected {
            if take(egui::Modifiers::NONE, Key::R) {
                self.draft.turn_slot(i);
            }
            if take(egui::Modifiers::NONE, Key::H) {
                self.draft.step_align(i, true);
            }
            if take(egui::Modifiers::NONE, Key::V) {
                self.draft.step_align(i, false);
                return;
            }
        }
        for (b, tool) in STRIP.iter().zip([Tool::Select, self.draw_tool, Tool::Text, Tool::Point, Tool::Slot, Tool::Origin]) {
            if take(egui::Modifiers::NONE, b.key) {
                // D again steps through the Draw tools.
                let tool = if b.key == Key::D && self.tool.is_draw() {
                    let i = Tool::DRAW.iter().position(|t| *t == self.tool).unwrap_or(0);
                    Tool::DRAW.get((i + 1) % Tool::DRAW.len()).copied().unwrap_or(Tool::Line)
                } else {
                    tool
                };
                self.set_tool(tool);
            }
        }
    }
}

/// What the builder's view asks of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Back to the page the builder was opened from.
    Back,
    /// Edit that version instead (the older-version question).
    EditVersion(ecad_model::DefinitionId),
}

// ---------------------------------------------------------------- geometry helpers

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn seg_dist(a: Pt, b: Pt, p: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return dist(a, p);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    dist(Pt::new(a.x + t * dx, a.y + t * dy), p)
}

/// The points along an arc from `start` to `end` (counter-clockwise), for drawing and picking.
pub fn arc_points(c: Pt, r: f64, start: f64, end: f64) -> Vec<Pt> {
    let mut sweep = end - start;
    while sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
    }
    let n = ((sweep / std::f64::consts::TAU) * 48.0).ceil().clamp(4.0, 96.0) as usize;
    (0..=n).map(|k| start + sweep * k as f64 / n as f64).map(|a| Pt::new(c.x + r * a.cos(), c.y + r * a.sin())).collect()
}

/// A text's box (rough: 0.6 of its height per character), in its own frame.
fn text_size(value: &str, height: f64) -> (f64, f64) {
    (value.chars().count() as f64 * height * 0.6, height)
}

fn near(g: &Primitive, p: Pt, tol: f64) -> bool {
    match g {
        Primitive::Line { a, b } => seg_dist(*a, *b, p) <= tol,
        Primitive::Polyline { vertices, closed } => {
            let n = vertices.len();
            let segs = if *closed { n } else { n.saturating_sub(1) };
            (0..segs).any(|k| match (vertices.get(k), vertices.get((k + 1) % n.max(1))) {
                (Some(a), Some(b)) => seg_dist(*a, *b, p) <= tol,
                _ => false,
            })
        }
        Primitive::Circle { center, radius } => (dist(*center, p) - radius).abs() <= tol,
        Primitive::Arc { center, radius, start, end } => {
            arc_points(*center, *radius, *start, *end).windows(2).any(|w| matches!(w, [a, b] if seg_dist(*a, *b, p) <= tol))
        }
        Primitive::Text { at, height, value, .. } => {
            let (w, h) = text_size(value, *height);
            p.x >= at.x - tol && p.x <= at.x + w + tol && p.y >= at.y - tol && p.y <= at.y + h + tol
        }
        Primitive::Point { at } => dist(*at, p) <= tol,
    }
}

/// Ends, middles and centres the cursor snaps to.
fn snap_points(d: &Draft) -> Vec<Pt> {
    let mut out = vec![Pt::new(0.0, 0.0)];
    for g in &d.shape.graphics {
        match g {
            Primitive::Line { a, b } => out.extend([*a, *b, Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)]),
            Primitive::Polyline { vertices, closed } => {
                out.extend(vertices.iter().copied());
                let n = vertices.len();
                let segs = if *closed { n } else { n.saturating_sub(1) };
                for k in 0..segs {
                    if let (Some(a), Some(b)) = (vertices.get(k), vertices.get((k + 1) % n.max(1))) {
                        out.push(Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
                    }
                }
            }
            Primitive::Circle { center, .. } => out.push(*center),
            Primitive::Arc { center, radius, start, end } => {
                out.push(*center);
                out.push(Pt::new(center.x + radius * start.cos(), center.y + radius * start.sin()));
                out.push(Pt::new(center.x + radius * end.cos(), center.y + radius * end.sin()));
            }
            Primitive::Text { at, .. } | Primitive::Point { at } => out.push(*at),
        }
    }
    out.extend(d.shape.points.iter().map(|p| p.at));
    out
}

/// The draft's extent, if it has anything.
fn bounds(d: &Draft) -> Option<(Pt, Pt)> {
    let mut pts: Vec<Pt> = snap_points(d);
    for g in &d.shape.graphics {
        if let Primitive::Circle { center, radius } | Primitive::Arc { center, radius, .. } = g {
            pts.push(Pt::new(center.x - radius, center.y - radius));
            pts.push(Pt::new(center.x + radius, center.y + radius));
        }
    }
    pts.extend(d.shape.slots.iter().map(|s| s.at));
    if pts.len() <= 1 {
        return None;
    }
    let lo = pts.iter().fold(Pt::new(f64::MAX, f64::MAX), |a, p| Pt::new(a.x.min(p.x), a.y.min(p.y)));
    let hi = pts.iter().fold(Pt::new(f64::MIN, f64::MIN), |a, p| Pt::new(a.x.max(p.x), a.y.max(p.y)));
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// A slot's sample box, in millimetres (turned with the slot).
struct SlotBox {
    corners: [Pt; 4],
}

impl SlotBox {
    fn contains(&self, p: Pt, tol: f64) -> bool {
        let xs = self.corners.iter().map(|c| c.x);
        let ys = self.corners.iter().map(|c| c.y);
        let (x0, x1) = (xs.clone().fold(f64::MAX, f64::min), xs.fold(f64::MIN, f64::max));
        let (y0, y1) = (ys.clone().fold(f64::MAX, f64::min), ys.fold(f64::MIN, f64::max));
        p.x >= x0 - tol && p.x <= x1 + tol && p.y >= y0 - tol && p.y <= y1 + tol
    }
}

/// The height a slot's sample is drawn at: the tag's size for the tag, the label size else.
pub fn slot_height(text: &str) -> f64 {
    if text == "tag" { 3.5 } else { 2.5 }
}

fn slot_box(s: &ecad_model::TextSlot, prefix: &str) -> SlotBox {
    let h = slot_height(&s.text);
    let (w, _) = text_size(&draft::slot_sample(&s.text, prefix), h);
    let dx = match s.h {
        TextHAlign::Left => 0.0,
        TextHAlign::Center => -w / 2.0,
        TextHAlign::Right => -w,
    };
    let dy = match s.v {
        TextVAlign::Baseline => -0.2 * h,
        TextVAlign::Bottom => 0.0,
        TextVAlign::Middle => -h / 2.0,
        TextVAlign::Top => -h,
    };
    let (sn, cs) = s.rotation.sin_cos();
    let rot = |x: f64, y: f64| Pt::new(s.at.x + x * cs - y * sn, s.at.y + x * sn + y * cs);
    SlotBox { corners: [rot(dx, dy), rot(dx + w, dy), rot(dx + w, dy + h), rot(dx, dy + h)] }
}

// ---------------------------------------------------------------- painting

fn c32(c: ecad_ui::tokens::Color) -> Color32 {
    ecad_ui::color(c)
}

/// Paint text `value` with its alignment point at `at` (screen), turned `rotation` radians
/// counter-clockwise, `px` tall.
/// The text's box on the screen, turned with it.
fn paint_text(p: &egui::Painter, at: Pos2, value: &str, px: f32, rotation: f64, h: TextHAlign, v: TextVAlign, color: Color32) -> [Pos2; 4] {
    let galley = p.layout_no_wrap(value.to_string(), FontId::proportional(px.max(4.0)), color);
    let size = galley.size();
    let dx = match h {
        TextHAlign::Left => 0.0,
        TextHAlign::Center => -size.x / 2.0,
        TextHAlign::Right => -size.x,
    };
    let dy = match v {
        TextVAlign::Baseline => -size.y * 0.8,
        TextVAlign::Bottom => -size.y,
        TextVAlign::Middle => -size.y / 2.0,
        TextVAlign::Top => 0.0,
    };
    let a = -(rotation as f32);
    let (sn, cs) = a.sin_cos();
    let turn = |x: f32, y: f32| at + vec2(x * cs - y * sn, x * sn + y * cs);
    let off = turn(dx, dy);
    p.add(egui::epaint::TextShape::new(off, galley, color).with_angle(a));
    [turn(dx, dy), turn(dx + size.x, dy), turn(dx + size.x, dy + size.y), turn(dx, dy + size.y)]
}

fn paint_prim(p: &egui::Painter, b: &Builder, rect: Rect, g: &Primitive, stroke: Stroke) {
    let s = |q: Pt| b.to_screen(rect, q);
    match g {
        Primitive::Line { a, b: e } => {
            p.line_segment([s(*a), s(*e)], stroke);
        }
        Primitive::Polyline { vertices, closed } => {
            let pts: Vec<Pos2> = vertices.iter().map(|v| s(*v)).collect();
            if *closed {
                p.add(Shape::closed_line(pts, stroke));
            } else {
                p.add(Shape::line(pts, stroke));
            }
        }
        Primitive::Circle { center, radius } => {
            p.circle_stroke(s(*center), (*radius as f32) * b.scale, stroke);
        }
        Primitive::Arc { center, radius, start, end } => {
            p.add(Shape::line(arc_points(*center, *radius, *start, *end).into_iter().map(s).collect(), stroke));
        }
        Primitive::Text { at, height, rotation, value, halign, valign } => {
            let _ = paint_text(p, s(*at), value, (*height as f32) * b.scale * 1.25, *rotation, *halign, *valign, stroke.color);
        }
        Primitive::Point { at } => {
            p.circle_filled(s(*at), stroke.width.max(1.5), stroke.color);
        }
    }
}

fn dashed_poly(p: &egui::Painter, pts: &[Pos2], stroke: Stroke) {
    let mut closed = pts.to_vec();
    if let Some(f) = pts.first() {
        closed.push(*f);
    }
    p.extend(Shape::dashed_line(&closed, stroke, 4.0, 3.0));
}

fn dashed_ring(p: &egui::Painter, c: Pos2, r: f32, stroke: Stroke) {
    let pts: Vec<Pos2> = (0..=24).map(|k| (k as f32) * std::f32::consts::TAU / 24.0).map(|a| c + vec2(r * a.cos(), r * a.sin())).collect();
    p.extend(Shape::dashed_line(&pts, stroke, 2.5, 2.0));
}

/// Draw the builder in `rect` and act on its input; what it asks of the window.
pub fn show(ui: &mut egui::Ui, rect: Rect, b: &mut Builder, sheet: &ecad_ui::tokens::Sheet, writable: bool) -> Vec<Act> {
    let _ = writable;
    let mut acts = Vec::new();
    if !b.fitted {
        b.fit(rect);
    }
    let p = ui.painter_at(rect);
    let shell = ecad_ui::SHELL;
    p.rect_filled(rect, 0.0, c32(sheet.sheet));
    let resp = ui.interact(rect, egui::Id::unique(CANVAS), egui::Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, CANVAS));

    // Zoom about the cursor, pan with the middle button.
    if let Some(hover) = resp.hover_pos() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 200.0);
        if scroll.abs() > 0.0 {
            let before = b.to_mm(rect, hover);
            b.scale = (b.scale * (scroll / 400.0).exp()).clamp(MIN_SCALE, MAX_SCALE);
            let after = b.to_mm(rect, hover);
            b.center = Pt::new(b.center.x + before.x - after.x, b.center.y + before.y - after.y);
        }
    }
    if resp.dragged_by(egui::PointerButton::Middle) {
        let d = resp.drag_delta();
        b.center = Pt::new(b.center.x - f64::from(d.x / b.scale), b.center.y + f64::from(d.y / b.scale));
    }

    // The grid: 2.5 mm dots, 5 mm heavier (ecad.builder-grid-minor/-major).
    let grid = c32(sheet.grid);
    let lo = b.to_mm(rect, rect.left_bottom());
    let hi = b.to_mm(rect, rect.right_top());
    let pitch = GRID_MM as f32 * b.scale;
    let step = if pitch >= 6.0 { GRID_MM } else { GRID_MM * 2.0 * ((6.0 / pitch).ceil() as f64) };
    let (i0, i1) = ((lo.x / step).floor() as i64, (hi.x / step).ceil() as i64);
    let (j0, j1) = ((lo.y / step).floor() as i64, (hi.y / step).ceil() as i64);
    if (i1 - i0).saturating_mul(j1 - j0) < 40_000 {
        for i in i0..=i1 {
            for j in j0..=j1 {
                let q = Pt::new(i as f64 * step, j as f64 * step);
                let major = draft::on_point_grid(q);
                p.circle_filled(b.to_screen(rect, q), if major { 1.25 } else { 0.6 }, grid);
            }
        }
    }

    // The origin cross (ecad.builder-origin).
    let ink2 = c32(sheet.ink_2);
    let o = b.to_screen(rect, Pt::new(0.0, 0.0));
    let arm = (ORIGIN_ARM_MM as f32) * b.scale;
    let os = Stroke::new((0.18 * b.scale).max(1.0), ink2);
    p.line_segment([o - vec2(arm, 0.0), o + vec2(arm, 0.0)], os);
    p.line_segment([o - vec2(0.0, arm), o + vec2(0.0, arm)], os);

    let ink = c32(sheet.ink);
    let sel = c32(sheet.selection);
    let preview = c32(sheet.placement_preview);
    let outline = Stroke::new(((OUTLINE_MM as f32) * b.scale).max(1.0), ink);
    for (i, g) in b.draft.shape.graphics.iter().enumerate() {
        let st = if b.selected == Some(Picked::Graphic(i)) { Stroke::new(outline.width + 1.0, sel) } else { outline };
        paint_prim(&p, b, rect, g, st);
    }

    // Connection points: ring, key above, default label beside in ink.
    let cp = c32(sheet.connection_point);
    for (i, pt) in b.draft.shape.points.iter().enumerate() {
        let c = b.to_screen(rect, pt.at);
        let chosen = b.selected == Some(Picked::Point(i));
        p.circle_stroke(c, RING_PX / 2.0, Stroke::new(if chosen { 2.5 } else { 1.5 }, if chosen { sel } else { cp }));
        p.text(c + vec2(-RING_PX, 0.0), Align2::RIGHT_CENTER, &pt.key, FontId::proportional(11.0), cp);
        if let Some(l) = &pt.label {
            p.text(c + vec2(RING_PX, RING_PX / 2.0), Align2::LEFT_TOP, l, FontId::proportional((2.5 * b.scale).clamp(8.0, 28.0)), ink);
        }
    }

    // Text slots: the sample in ink_2, a dashed box, the anchor dot (ecad.slot-anchor).
    for (i, s) in b.draft.shape.slots.iter().enumerate() {
        let chosen = b.selected == Some(Picked::Slot(i));
        let h = slot_height(&s.text);
        let corners = paint_text(
            &p,
            b.to_screen(rect, s.at),
            &draft::slot_sample(&s.text, &b.draft.prefix),
            (h as f32) * b.scale * 1.25,
            s.rotation,
            s.h,
            s.v,
            ink2,
        );
        dashed_poly(&p, &corners, Stroke::new(if chosen { 2.0 } else { 1.0 }, if chosen { sel } else { preview }));
        p.circle_filled(b.to_screen(rect, s.at), 2.0, preview);
    }

    // Offers, dashed (design 048 §4.1 step 3).
    for o in &b.draft.shape.offers {
        match o {
            Offer::Point { at } => {
                let c = b.to_screen(rect, *at);
                dashed_ring(&p, c, RING_PX, Stroke::new(2.0, preview));
                p.text(c + vec2(-RING_PX - 3.0, 0.0), Align2::RIGHT_CENTER, "point?", FontId::proportional(11.0), preview);
            }
            Offer::Slot { at, text, .. } => {
                let (w, h) = text_size(text, 3.0);
                let c = [Pt::new(at.x, at.y - 0.6), Pt::new(at.x + w, at.y - 0.6), Pt::new(at.x + w, at.y + h), Pt::new(at.x, at.y + h)];
                let pts: Vec<Pos2> = c.iter().map(|q| b.to_screen(rect, *q)).collect();
                dashed_poly(&p, &pts, Stroke::new(1.5, preview));
            }
        }
    }

    // Input.
    let pointer = resp.hover_pos();
    let raw = pointer.map(|q| b.to_mm(rect, q));
    b.cursor = raw.map(|r| if b.tool == Tool::Select { r } else { b.snap(r) });
    if b.ask_version.is_none() {
        if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(r) = raw
        {
            if b.entry.is_some() {
                b.commit_entry();
            }
            let at = if b.tool == Tool::Select { r } else { b.snap(r) };
            b.click(at);
        }
        if resp.clicked_by(egui::PointerButton::Secondary) {
            b.finish();
        }
        // Select drags what is under the press, snapped to the grid.
        if b.tool == Tool::Select
            && resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(r) = raw
        {
            b.selected = b.pick(r);
            b.drag = b.selected.filter(|w| !matches!(w, Picked::Offer(_))).map(|w| (w, b.snap(r)));
        }
        if let Some((what, last)) = b.drag
            && let Some(r) = raw
            && resp.dragged_by(egui::PointerButton::Primary)
        {
            let now = b.snap(r);
            let d = Pt::new(now.x - last.x, now.y - last.y);
            if (d.x.abs() > 1e-9 || d.y.abs() > 1e-9) && !(b.draft.fixed_drawing() && matches!(what, Picked::Graphic(_) | Picked::Point(_))) {
                match b.draft.move_by(what, d) {
                    Ok(()) => b.drag = Some((what, now)),
                    Err(e) => b.at_cursor = Some((e, now)),
                }
            }
        }
        if resp.drag_stopped() {
            b.drag = None;
        }
    }

    // The shape being drawn, rubber-banded to the cursor.
    if let Some(c) = b.cursor.filter(|_| b.tool != Tool::Select) {
        let st = Stroke::new(1.0, preview);
        let cs = b.to_screen(rect, c);
        let mut chain: Vec<Pos2> = b.pending.iter().map(|q| b.to_screen(rect, *q)).collect();
        match (b.tool, b.pending.first().copied()) {
            (Tool::Line | Tool::Polyline, Some(_)) => {
                chain.push(cs);
                p.extend(Shape::dashed_line(&chain, st, 4.0, 3.0));
            }
            (Tool::Rectangle, Some(a)) => {
                let q = [a, Pt::new(c.x, a.y), c, Pt::new(a.x, c.y)].map(|x| b.to_screen(rect, x));
                dashed_poly(&p, &q, st);
            }
            (Tool::Circle, Some(a)) => dashed_ring(&p, b.to_screen(rect, a), (dist(a, c) as f32) * b.scale, st),
            (Tool::Arc, Some(a)) => {
                if let Some(s0) = b.pending.get(1).copied() {
                    let r = dist(a, s0);
                    let pts: Vec<Pos2> = arc_points(a, r, (s0.y - a.y).atan2(s0.x - a.x), (c.y - a.y).atan2(c.x - a.x))
                        .into_iter()
                        .map(|q| b.to_screen(rect, q))
                        .collect();
                    p.extend(Shape::dashed_line(&pts, st, 4.0, 3.0));
                } else {
                    dashed_ring(&p, b.to_screen(rect, a), (dist(a, c) as f32) * b.scale, st);
                }
            }
            (Tool::Point, _) => {
                let ok = b.draft.point_refusal(c, None).is_none();
                p.circle_stroke(cs, RING_PX / 2.0, Stroke::new(1.5, if ok { cp } else { c32(sheet.error) }));
            }
            _ => {}
        }
        // The snap marker.
        p.rect_stroke(Rect::from_center_size(cs, vec2(7.0, 7.0)), 0.0, Stroke::new(1.0, preview), egui::StrokeKind::Middle);
    }

    // A refusal at the cursor (design 048 §3.3 step 3).
    if let Some((why, at)) = &b.at_cursor {
        let q = b.to_screen(rect, *at) + vec2(12.0, -12.0);
        let galley = p.layout_no_wrap(why.clone(), FontId::proportional(12.0), c32(shell.text));
        let r = Rect::from_min_size(q, galley.size()).expand(5.0);
        p.rect_filled(r, 4.0, c32(shell.raised));
        p.rect_stroke(r, 4.0, Stroke::new(1.0, c32(shell.error)), egui::StrokeKind::Inside);
        p.galley(q, galley, c32(shell.text));
    }

    // Breadcrumb, Back, the empty state's hint.
    let crumb = match &b.draft.source {
        draft::Source::Edit { version, .. } => format!("Library ▸ {} v{version}", b.draft.shown_name()),
        draft::Source::Make { page_number, .. } => format!("Library ▸ Make symbol from page {page_number}"),
        draft::Source::Promote { tag, .. } => format!("Library ▸ Make symbol of {tag}"),
        draft::Source::New => format!("Library ▸ {}", b.draft.shown_name()),
    };
    let head = Rect::from_min_size(rect.min + vec2(space::L, space::M), vec2(rect.width() - 2.0 * space::L, 24.0));
    let back = matches!(b.draft.source, draft::Source::Make { .. } | draft::Source::Promote { .. })
        || matches!(&b.draft.source, draft::Source::Edit { rep: Some(_), .. });
    ui.scope_builder(egui::UiBuilder::new().max_rect(head), |ui| {
        ui.horizontal(|ui| {
            if back {
                let r = ui.add(ecad_ui::prim::Button::new("Back").kind(ecad_ui::prim::Kind::Quiet).small());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, BACK));
                if r.clicked() {
                    acts.push(Act::Back);
                }
            }
            let r = ui.label(egui::RichText::new(&crumb).color(c32(shell.text_2)));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, BREADCRUMB));
        });
    });
    if b.tool == Tool::Slot {
        let chips = Rect::from_min_size(head.left_bottom() + vec2(0.0, space::XS), vec2(head.width(), 28.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(chips), |ui| {
            ui.horizontal(|ui| {
                let used: Vec<String> = b.draft.used_slot_texts().into_iter().map(str::to_string).collect();
                for (t, label, _) in draft::SLOT_TEXTS {
                    let taken = used.iter().any(|u| u == t);
                    let r = ui.add_enabled_ui(!taken, |ui| ecad_ui::prim::Chip::new(label).selected(b.slot_text == t).show(ui).response).inner;
                    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, !taken, b.slot_text == t, format!("library.builder.slot.{t}")));
                    if r.clicked() && !taken {
                        b.slot_text = t.to_string();
                    }
                }
            });
        });
    }
    let empty = b.draft.shape.graphics.is_empty() && b.draft.shape.points.is_empty() && b.draft.shape.slots.is_empty();
    if empty && b.pending.is_empty() {
        let r = p.text(rect.center() + vec2(0.0, 80.0), Align2::CENTER_CENTER, EMPTY_HINT, FontId::proportional(14.0), c32(shell.text_3));
        ui.interact(r, egui::Id::unique(HINT), egui::Sense::hover()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, EMPTY_HINT));
    }

    // Typing in place: a text, or a point's label.
    let entry_at = b.entry.as_ref().map(|(e, _)| match e {
        Entry::Text(at) => *at,
        Entry::Label(i) => b.draft.shape.points.get(*i).map_or(Pt::new(0.0, 0.0), |p| p.at),
    });
    let entry_q = entry_at.map(|at| b.to_screen(rect, at) + vec2(RING_PX, -10.0));
    if let (Some((e, text)), Some(q)) = (&mut b.entry, entry_q) {
        let placeholder = if matches!(e, Entry::Label(_)) { "Label?" } else { "Text" };
        let mut done = None;
        egui::Area::new(egui::Id::unique(ENTRY)).order(egui::Order::Foreground).fixed_pos(q).show(ui.ctx(), |ui| {
            let r = ui.add(egui::TextEdit::singleline(text).hint_text(placeholder).desired_width(140.0));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, ENTRY));
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            if r.lost_focus() {
                done = Some(ui.input(|i| !i.key_pressed(Key::Escape)));
            }
        });
        match done {
            Some(true) => b.commit_entry(),
            Some(false) => b.entry = None,
            None => {}
        }
    }

    // Offers: Accept / Skip on hover (design 048 §4.1 step 3).
    if let Some(Picked::Offer(i)) = raw.and_then(|r| b.pick(r)).or(b.selected.filter(|s| matches!(s, Picked::Offer(_))))
        && let Some(o) = b.draft.shape.offers.get(i).cloned()
    {
        let at = match o {
            Offer::Point { at } | Offer::Slot { at, .. } => at,
        };
        b.selected = Some(Picked::Offer(i));
        let q = b.to_screen(rect, at) + vec2(10.0, 8.0);
        egui::Area::new(egui::Id::unique(("library.builder.offer", i))).order(egui::Order::Foreground).fixed_pos(q).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let what = match &o {
                        Offer::Point { .. } => "Connection point".to_string(),
                        Offer::Slot { text, .. } => format!("Tag slot for \"{text}\""),
                    };
                    ui.label(egui::RichText::new(what).small());
                    let a = ui.add(ecad_ui::prim::Button::new("Accept").small());
                    a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("library.builder.offer.{i}.accept")));
                    if a.clicked() {
                        if let Err(e) = b.draft.accept(i) {
                            b.at_cursor = Some((e, at));
                        }
                        b.selected = None;
                    }
                    let s = ui.add(ecad_ui::prim::Button::new("Skip").kind(ecad_ui::prim::Kind::Quiet).small());
                    s.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("library.builder.offer.{i}.skip")));
                    if s.clicked() {
                        b.draft.delete(Picked::Offer(i));
                        b.selected = None;
                    }
                });
            });
        });
    }

    // The older-version question (design 048 §6.1 step 1).
    if let Some(a) = b.ask_version.clone() {
        let card = Rect::from_center_size(rect.center(), vec2(420.0, 110.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(card.width() - 20.0);
                ui.label(format!("This copy shows v{}; v{} is newer.", a.shown, a.newest_seq));
                ui.add_space(space::S);
                ui.horizontal(|ui| {
                    let n = ui.add(ecad_ui::prim::Button::new(&format!("Edit v{}", a.newest_seq)));
                    n.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, VERSION_NEWEST));
                    if n.clicked() {
                        acts.push(Act::EditVersion(a.newest));
                    }
                    let s = ui.add(ecad_ui::prim::Button::new(&format!("Edit v{}", a.shown)).kind(ecad_ui::prim::Kind::Quiet));
                    s.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, VERSION_SHOWN));
                    if s.clicked() {
                        b.ask_version = None;
                    }
                });
            });
        });
    }

    // The intent line along the bottom.
    b.intent = b.intent_text();
    let line = Rect::from_min_max(pos2(rect.left(), rect.bottom() - size::STATUS_BAR), rect.max);
    p.rect_filled(line, 0.0, c32(shell.panel).gamma_multiply(0.9));
    let r = p.text(line.left_center() + vec2(space::L, 0.0), Align2::LEFT_CENTER, &b.intent, FontId::proportional(12.0), c32(shell.text_2));
    ui.interact(r, egui::Id::unique(INTENT), egui::Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, b.intent.clone()));
    acts
}

/// The builder's tool strip (design 048 §3.1): six buttons, Draw with its flyout.
pub fn strip(ui: &mut egui::Ui, rect: Rect, b: &mut Builder) {
    let shell = ecad_ui::SHELL;
    ui.painter().rect_filled(rect, 0.0, c32(shell.panel));
    let x = rect.center().x - size::TOOL_BUTTON / 2.0;
    let mut y = rect.top() + space::S;
    let tools = [Tool::Select, b.draw_tool, Tool::Text, Tool::Point, Tool::Slot, Tool::Origin];
    let mut over_flyout = false;
    for (sb, tool) in STRIP.iter().zip(tools) {
        let r = Rect::from_min_size(pos2(x, y), vec2(size::TOOL_BUTTON, size::TOOL_BUTTON));
        y += size::TOOL_BUTTON + space::XS;
        let on = if tool.is_draw() { b.tool.is_draw() } else { b.tool == tool };
        let resp = ui
            .scope_builder(egui::UiBuilder::new().max_rect(r), |ui| ui.add(ecad_ui::prim::IconButton::new(tool.icon(), sb.tip).tool().selected(on)))
            .inner;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, sb.name));
        let resp = resp.on_hover_text(if tool.is_draw() { format!("Draw (D): {}", tool.label()) } else { sb.tip.to_string() });
        if resp.clicked() {
            b.set_tool(tool);
            b.flyout = false;
        }
        if tool.is_draw() {
            if resp.hovered() {
                b.flyout = true;
            }
            if b.flyout {
                let area = egui::Area::new(egui::Id::unique("library.builder.draw.flyout"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(pos2(r.right(), r.top()))
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(160.0);
                            ecad_ui::prim::read_value(ui, "Draw (D) tools");
                            for t in Tool::DRAW {
                                let c = ui.add(ecad_ui::prim::Button::new(t.label()).kind(ecad_ui::prim::Kind::Quiet).small());
                                c.widget_info(|| {
                                    egui::WidgetInfo::labeled(egui::Role::Button, true, format!("library.builder.draw.{}", t.label().to_lowercase()))
                                });
                                if c.clicked() {
                                    b.set_tool(t);
                                    b.flyout = false;
                                }
                            }
                        });
                    });
                over_flyout =
                    ui.ctx().pointer_hover_pos().is_some_and(|p| area.response.rect.expand(space::S).contains(p) || r.expand(space::S).contains(p));
            }
        }
    }
    if !over_flyout {
        b.flyout = false;
    }
}
