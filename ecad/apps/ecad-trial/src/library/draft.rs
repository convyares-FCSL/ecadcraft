//! The symbol builder's draft (design 048 §3.2): the symbol being drawn, as temporary state that
//! is not a project page. No transaction, no history, nothing in the file: its own undo and redo,
//! kept in eframe storage under [`DRAFTS_KEY`] (at most [`MAX_DRAFTS`] per project) so leaving the
//! workspace or a crash does not lose it. **Save** is the only write, through `symbolnew`,
//! `symboledit` or `makesymbol` with [`Draft::params`].
//!
//! Pure data and rules: no egui, no project lock. The rules themselves are the commands'
//! ([`symbol_cmds::problems`]), so the builder and the JSON form refuse in the same words.

use std::collections::BTreeMap;

use ecad_cadcraft::symbol_cmds::{self, Content, POINT_GRID_MM};
use ecad_model::{Document, Primitive, Pt, TerminalDef, TextHAlign, TextSlot, TextVAlign};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The eframe storage key of the drafts (design 048 §3.2).
pub const DRAFTS_KEY: &str = "symbol_drafts";
/// Most drafts kept per project (the oldest goes first).
pub const MAX_DRAFTS: usize = 8;
/// Most undo steps a draft keeps.
pub const MAX_UNDO: usize = 200;
/// The builder's drawing grid (`ecad.builder-grid-minor`), millimetres.
pub const GRID_MM: f64 = 2.5;
/// A new text's height, millimetres.
pub const TEXT_MM: f64 = 2.5;
/// The name field's placeholder: a new draft has no name yet.
pub const NEW_NAME: &str = "New symbol";

/// Where the draft came from, which says how Save writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Source {
    /// Library ▸ New symbol: Save is `symbolnew`.
    #[default]
    New,
    /// Edit symbol on a version (hex IDs): Save opens the review, then `symboledit`.
    Edit {
        definition: String,
        version: u64,
        /// The edited symbol is a built-in starter: Save makes a project copy.
        builtin: bool,
        /// The placed copy the builder was opened from, if any (its page is where Back goes).
        rep: Option<String>,
    },
    /// Make symbol from plain geometry on a page (design 048 §4.1): Save is `makesymbol`.
    Make {
        page: String,
        page_number: String,
        handles: Vec<u64>,
        /// The page point the draft's (0, 0) sits on.
        at: [f64; 2],
        /// Replace the drawing on the page with the new symbol (Q7: on by default).
        replace: bool,
    },
    /// Promote a drawn component (design 048 §4.2): its drawing and points are kept as they are.
    Promote {
        component: String,
        tag: String,
        /// The page point of the draft's (0, 0): promotion takes slots as page points.
        #[serde(default)]
        at: [f64; 2],
    },
}

/// What the builder offers and does not apply until accepted (design 048 §4.1 step 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Offer {
    /// A free line end on the point grid: a connection point there.
    Point { at: Pt },
    /// A text that reads like a tag: a Tag slot in its place (the text then goes).
    Slot { text: String, at: Pt, graphic: usize },
}

/// The part of a draft its undo steps over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Shape {
    pub graphics: Vec<Primitive>,
    pub points: Vec<TerminalDef>,
    pub slots: Vec<TextSlot>,
    pub offers: Vec<Offer>,
}

/// One symbol being drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Draft {
    /// Its number among the project's drafts (storage only).
    pub id: u64,
    pub source: Source,
    pub name: String,
    pub key: String,
    pub prefix: String,
    pub description: String,
    pub rating: String,
    /// `join: part` (a part of a named device) instead of a device of its own.
    pub part: bool,
    /// A new symbol drawn from a built-in (hex definition ID): `symbolnew {from}`, so the copy
    /// records `derived_from` (FP4).
    #[serde(default)]
    pub from: Option<String>,
    pub shape: Shape,
    /// What it held when opened (Cancel asks only when it changed).
    pub start: Shape,
    /// The details as opened (name, key, prefix, description, rating, part).
    start_details: [String; 5],
    start_part: bool,
    #[serde(skip)]
    undo: Vec<Shape>,
    #[serde(skip)]
    redo: Vec<Shape>,
}

/// A row of the Missing list (design 048 §3.4): which item, in the rules' words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// `name`, `key`, `prefix`, `point`, `slot` or `other`: what its button does.
    pub item: &'static str,
    pub text: String,
}

/// The device texts a slot may show, in chip order, with the sample each draws (design 048 §3.3
/// step 4).
pub const SLOT_TEXTS: [(&str, &str, &str); 8] = [
    ("tag", "Tag", "-K01"),
    ("description", "Description", "Description"),
    ("rating", "Rating", "10 A"),
    ("part_number", "Part number", "Part number"),
    ("maker", "Maker", "Maker"),
    ("article_number", "Article number", "Article"),
    ("function", "Function", "Function"),
    ("location", "Location", "+L1"),
];

/// The chip label of slot text `text`.
pub fn slot_label(text: &str) -> String {
    SLOT_TEXTS.iter().find(|(t, _, _)| *t == text).map_or_else(|| text.to_string(), |(_, l, _)| l.to_string())
}

/// The sample slot text `text` draws in the builder.
pub fn slot_sample(text: &str, prefix: &str) -> String {
    if text == "tag" {
        let p = if prefix.trim().is_empty() { "K" } else { prefix.trim() };
        return format!("-{p}01");
    }
    SLOT_TEXTS.iter().find(|(t, _, _)| *t == text).map_or_else(|| text.to_string(), |(_, _, s)| s.to_string())
}

/// Whether `p` is on the point grid.
pub fn on_point_grid(p: Pt) -> bool {
    let on = |x: f64| ((x / POINT_GRID_MM).round() * POINT_GRID_MM - x).abs() < 1e-6;
    on(p.x) && on(p.y)
}

fn same(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// Whether `text` reads like a tag (`-` then letters then digits), so it is offered as the Tag slot.
pub fn looks_like_tag(text: &str) -> bool {
    let Some(rest) = text.trim().strip_prefix('-') else { return false };
    let letters = rest.chars().take_while(char::is_ascii_alphabetic).count();
    let digits = rest.chars().skip(letters).take_while(char::is_ascii_digit).count();
    letters > 0 && digits > 0 && letters + digits == rest.chars().count()
}

/// The offers for geometry `graphics` (design 048 §4.1 step 3): a connection point at every free
/// line end on the point grid that no other shape touches, and a Tag slot for the first text that
/// reads like a tag.
pub fn offers_for(graphics: &[Primitive]) -> Vec<Offer> {
    let mut out = Vec::new();
    let mut ends: Vec<(usize, Pt)> = Vec::new();
    for (i, g) in graphics.iter().enumerate() {
        if let Some((a, b)) = g.path_ends()
            && !matches!(g, Primitive::Polyline { closed: true, .. })
        {
            ends.extend([(i, a), (i, b)]);
        }
    }
    for (i, at) in &ends {
        let touched = graphics.iter().enumerate().any(|(j, g)| j != *i && touches(g, *at));
        if on_point_grid(*at) && !touched && !out.iter().any(|o| matches!(o, Offer::Point { at: x } if same(*x, *at))) {
            out.push(Offer::Point { at: *at });
        }
    }
    if let Some((i, at, value)) = graphics.iter().enumerate().find_map(|(i, g)| match g {
        Primitive::Text { at, value, .. } if looks_like_tag(value) => Some((i, *at, value.clone())),
        _ => None,
    }) {
        out.push(Offer::Slot { text: value, at, graphic: i });
    }
    out
}

/// Whether shape `g` passes through `p`.
fn touches(g: &Primitive, p: Pt) -> bool {
    let on_seg = |a: Pt, b: Pt| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-12 {
            return same(a, p);
        }
        let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
        let (qx, qy) = (a.x + t * dx, a.y + t * dy);
        ((qx - p.x).powi(2) + (qy - p.y).powi(2)).sqrt() < 1e-6
    };
    match g {
        Primitive::Line { a, b } => on_seg(*a, *b),
        Primitive::Polyline { vertices, closed } => {
            let n = vertices.len();
            let segs = if *closed { n } else { n.saturating_sub(1) };
            (0..segs).any(|k| match (vertices.get(k), vertices.get((k + 1) % n.max(1))) {
                (Some(a), Some(b)) => on_seg(*a, *b),
                _ => false,
            })
        }
        Primitive::Circle { center, radius } | Primitive::Arc { center, radius, .. } => {
            (((p.x - center.x).powi(2) + (p.y - center.y).powi(2)).sqrt() - radius).abs() < 1e-6
        }
        Primitive::Text { .. } | Primitive::Point { .. } => false,
    }
}

/// `g` moved by `d`.
pub fn moved(g: &Primitive, d: Pt) -> Primitive {
    symbol_cmds::shifted(g, Pt::new(-d.x, -d.y))
}

impl Draft {
    /// An empty draft (Library ▸ New symbol): no name yet, join `device`.
    pub fn new() -> Self {
        Draft::default()
    }

    /// A draft of stored content `c` (Edit symbol, or a promoted drawing) from `source`.
    pub fn of_content(c: &Content, source: Source) -> Self {
        let get = |k: &str| c.defaults.get(k).cloned().unwrap_or_default();
        let mut d = Draft {
            source,
            name: c.name.clone(),
            key: get("key"),
            prefix: get("prefix"),
            description: get("description"),
            rating: get("rating"),
            part: get("join").eq_ignore_ascii_case("part"),
            shape: Shape { graphics: c.graphics.clone(), points: c.points.clone(), slots: c.slots.clone(), offers: Vec::new() },
            ..Draft::default()
        };
        d.mark_start();
        d
    }

    /// A draft of page geometry `prims` (Make symbol, design 048 §4.1): moved so its bounding box's
    /// centre is on the nearest grid point at (0, 0), with the offers for it.
    pub fn of_geometry(prims: &[Primitive], page: String, page_number: String, handles: Vec<u64>) -> Self {
        let o = symbol_cmds::grid_origin(prims);
        let graphics: Vec<Primitive> = prims.iter().map(|g| symbol_cmds::shifted(g, o)).collect();
        let offers = offers_for(&graphics);
        let mut d = Draft {
            source: Source::Make { page, page_number, handles, at: [o.x, o.y], replace: true },
            shape: Shape { graphics, offers, ..Shape::default() },
            ..Draft::default()
        };
        d.mark_start();
        d
    }

    /// Remember what it holds now as what it was opened with.
    pub fn mark_start(&mut self) {
        self.start = self.shape.clone();
        self.start_details = self.details();
        self.start_part = self.part;
    }

    fn details(&self) -> [String; 5] {
        [self.name.clone(), self.key.clone(), self.prefix.clone(), self.description.clone(), self.rating.clone()]
    }

    /// Whether it changed since it was opened (Cancel asks first only then).
    pub fn changed(&self) -> bool {
        self.shape != self.start || self.details() != self.start_details || self.part != self.start_part
    }

    /// The name it is shown by: its name, else "New symbol".
    pub fn shown_name(&self) -> &str {
        if self.name.trim().is_empty() { NEW_NAME } else { self.name.trim() }
    }

    /// Whether its drawing and points come from the page as they are (a promoted component).
    pub fn fixed_drawing(&self) -> bool {
        matches!(self.source, Source::Promote { .. })
    }

    // ------------------------------------------------------------ undo

    /// Keep the shape as it is now as an undo step, before a change.
    pub fn step(&mut self) {
        self.undo.push(self.shape.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Undo the last change of the draft (Ctrl+Z in the builder).
    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.shape, s));
        true
    }

    /// Redo (Ctrl+Y in the builder).
    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.shape, s));
        true
    }

    // ------------------------------------------------------------ edits

    /// Add a shape.
    pub fn add_graphic(&mut self, g: Primitive) {
        self.step();
        self.shape.graphics.push(g);
    }

    /// The key the next point gets: the lowest number not used yet.
    pub fn next_point_key(&self) -> String {
        (1..).map(|n: u32| n.to_string()).find(|k| !self.shape.points.iter().any(|p| p.key == *k)).unwrap_or_default()
    }

    /// Why a point cannot go at `at` (design 048 §3.3 step 3), if it cannot; `except` is a point
    /// being moved.
    pub fn point_refusal(&self, at: Pt, except: Option<usize>) -> Option<String> {
        if !on_point_grid(at) {
            return Some(format!("A point goes on the {} mm grid.", fmt_mm(POINT_GRID_MM)));
        }
        self.shape
            .points
            .iter()
            .enumerate()
            .find(|(i, p)| Some(*i) != except && same(p.at, at))
            .map(|(_, p)| format!("Point {} is already here.", p.key))
    }

    /// Add a connection point at `at`: its index, or the refusal.
    ///
    /// # Errors
    /// Off the point grid, or on another point.
    pub fn add_point(&mut self, at: Pt) -> Result<usize, String> {
        if let Some(r) = self.point_refusal(at, None) {
            return Err(r);
        }
        if self.shape.points.len() >= symbol_cmds::MAX_POINTS {
            return Err(format!("The symbol has too many connection points ({} at most).", symbol_cmds::MAX_POINTS));
        }
        let key = self.next_point_key();
        self.step();
        self.shape.points.push(TerminalDef { key, label: None, at });
        Ok(self.shape.points.len() - 1)
    }

    /// Set point `i`'s default label (empty: none).
    pub fn set_label(&mut self, i: usize, label: &str) {
        let l = label.trim();
        let new = (!l.is_empty()).then(|| l.to_string());
        if self.shape.points.get(i).is_some_and(|p| p.label != new) {
            self.step();
            if let Some(p) = self.shape.points.get_mut(i) {
                p.label = new;
            }
        }
    }

    /// The slot texts already used (greyed in the chips).
    pub fn used_slot_texts(&self) -> Vec<&str> {
        self.shape.slots.iter().map(|s| s.text.as_str()).collect()
    }

    /// Add a slot for device text `text` at `at`.
    ///
    /// # Errors
    /// The text has a slot already, or there are as many slots as allowed.
    pub fn add_slot(&mut self, text: &str, at: Pt) -> Result<usize, String> {
        if self.shape.slots.iter().any(|s| s.text == text) {
            return Err(format!("The {} already has a slot.", slot_label(text)));
        }
        if self.shape.slots.len() >= symbol_cmds::MAX_SLOTS {
            return Err(format!("The symbol has too many text slots ({} at most).", symbol_cmds::MAX_SLOTS));
        }
        self.step();
        self.shape.slots.push(TextSlot { text: text.to_string(), at, h: TextHAlign::Left, v: TextVAlign::Baseline, rotation: 0.0 });
        Ok(self.shape.slots.len() - 1)
    }

    /// Turn slot `i` by 90° (R).
    pub fn turn_slot(&mut self, i: usize) {
        if self.shape.slots.get(i).is_none() {
            return;
        }
        self.step();
        if let Some(s) = self.shape.slots.get_mut(i) {
            let quarter = std::f64::consts::FRAC_PI_2;
            let r = s.rotation + quarter;
            s.rotation = if r >= 2.0 * std::f64::consts::PI - 1e-9 { 0.0 } else { r };
        }
    }

    /// Step slot `i`'s horizontal (H) or vertical (V) alignment.
    pub fn step_align(&mut self, i: usize, horizontal: bool) {
        if self.shape.slots.get(i).is_none() {
            return;
        }
        self.step();
        if let Some(s) = self.shape.slots.get_mut(i) {
            if horizontal {
                s.h = match s.h {
                    TextHAlign::Left => TextHAlign::Center,
                    TextHAlign::Center => TextHAlign::Right,
                    TextHAlign::Right => TextHAlign::Left,
                };
            } else {
                s.v = match s.v {
                    TextVAlign::Baseline => TextVAlign::Bottom,
                    TextVAlign::Bottom => TextVAlign::Middle,
                    TextVAlign::Middle => TextVAlign::Top,
                    TextVAlign::Top => TextVAlign::Baseline,
                };
            }
        }
    }

    /// Move the origin to `at` (design 048 §3.3 step 5): everything shifts so `at` is (0, 0). A
    /// made symbol's page point moves with it, so it is replaced where it was drawn.
    pub fn set_origin(&mut self, at: Pt) {
        if at.x.abs() < 1e-9 && at.y.abs() < 1e-9 {
            return;
        }
        self.step();
        let d = Pt::new(-at.x, -at.y);
        let sh = &mut self.shape;
        sh.graphics = sh.graphics.iter().map(|g| moved(g, d)).collect();
        for p in &mut sh.points {
            p.at = Pt::new(p.at.x + d.x, p.at.y + d.y);
        }
        for s in &mut sh.slots {
            s.at = Pt::new(s.at.x + d.x, s.at.y + d.y);
        }
        for o in &mut sh.offers {
            match o {
                Offer::Point { at } | Offer::Slot { at, .. } => *at = Pt::new(at.x + d.x, at.y + d.y),
            }
        }
        if let Source::Make { at: page_at, .. } = &mut self.source {
            *page_at = [page_at[0] + at.x, page_at[1] + at.y];
        }
    }

    /// Delete shape, point or slot `what`.
    pub fn delete(&mut self, what: Picked) {
        self.step();
        let sh = &mut self.shape;
        match what {
            Picked::Graphic(i) if i < sh.graphics.len() => {
                sh.graphics.remove(i);
                // A slot offer names its text by index.
                sh.offers.retain(|o| !matches!(o, Offer::Slot { graphic, .. } if *graphic == i));
                for o in &mut sh.offers {
                    if let Offer::Slot { graphic, .. } = o
                        && *graphic > i
                    {
                        *graphic -= 1;
                    }
                }
            }
            Picked::Point(i) if i < sh.points.len() => {
                sh.points.remove(i);
            }
            Picked::Slot(i) if i < sh.slots.len() => {
                sh.slots.remove(i);
            }
            Picked::Offer(i) if i < sh.offers.len() => {
                sh.offers.remove(i);
            }
            _ => {}
        }
    }

    /// Move shape, point or slot `what` by `d` (a point is refused off the grid or onto another).
    ///
    /// # Errors
    /// A point's refusal.
    pub fn move_by(&mut self, what: Picked, d: Pt) -> Result<(), String> {
        if let Picked::Point(i) = what {
            let Some(p) = self.shape.points.get(i) else { return Ok(()) };
            let to = Pt::new(p.at.x + d.x, p.at.y + d.y);
            if let Some(r) = self.point_refusal(to, Some(i)) {
                return Err(r);
            }
        }
        self.step();
        let sh = &mut self.shape;
        match what {
            Picked::Graphic(i) => {
                if let Some(g) = sh.graphics.get_mut(i) {
                    *g = moved(g, d);
                }
            }
            Picked::Point(i) => {
                if let Some(p) = sh.points.get_mut(i) {
                    p.at = Pt::new(p.at.x + d.x, p.at.y + d.y);
                }
            }
            Picked::Slot(i) => {
                if let Some(s) = sh.slots.get_mut(i) {
                    s.at = Pt::new(s.at.x + d.x, s.at.y + d.y);
                }
            }
            Picked::Offer(_) => {}
        }
        Ok(())
    }

    /// Accept offer `i`: a point is added, a slot replaces its text.
    ///
    /// # Errors
    /// The point or slot is refused (on another point, a slot already there).
    pub fn accept(&mut self, i: usize) -> Result<(), String> {
        let Some(o) = self.shape.offers.get(i).cloned() else { return Ok(()) };
        match o {
            Offer::Point { at } => {
                self.add_point(at)?;
            }
            Offer::Slot { at, graphic, .. } => {
                self.add_slot("tag", at)?;
                // The text is the slot now: it goes from the drawing.
                if let (Some(Primitive::Text { halign, valign, .. }), Some(s)) =
                    (self.shape.graphics.get(graphic).cloned(), self.shape.slots.last_mut())
                {
                    s.h = halign;
                    s.v = valign;
                }
                if graphic < self.shape.graphics.len() {
                    self.shape.graphics.remove(graphic);
                }
                for x in &mut self.shape.offers {
                    if let Offer::Slot { graphic: g, .. } = x
                        && *g > graphic
                    {
                        *g -= 1;
                    }
                }
            }
        }
        // The add above kept the undo step; the offer goes with it.
        if i < self.shape.offers.len() {
            self.shape.offers.remove(i);
        }
        Ok(())
    }

    /// Accept every offer (Details' "Accept all"); the first refusal stops it.
    ///
    /// # Errors
    /// An offer's refusal.
    pub fn accept_all(&mut self) -> Result<(), String> {
        while !self.shape.offers.is_empty() {
            self.accept(0)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ the commands

    /// The defaults it saves (design 048 §2.4).
    fn defaults(&self) -> BTreeMap<String, String> {
        let mut d = BTreeMap::new();
        for (k, v) in [("key", &self.key), ("prefix", &self.prefix), ("description", &self.description), ("rating", &self.rating)] {
            if !v.trim().is_empty() {
                d.insert(k.to_string(), v.trim().to_string());
            }
        }
        d.insert("join".to_string(), if self.part { "part" } else { "device" }.to_string());
        d
    }

    /// The content the commands check and save.
    pub fn content(&self) -> Content {
        Content {
            name: self.name.trim().to_string(),
            graphics: self.shape.graphics.clone(),
            points: self.shape.points.clone(),
            slots: self.shape.slots.clone(),
            defaults: self.defaults(),
            bridges: Vec::new(),
        }
    }

    /// The Missing list (design 048 §3.4): the commands' own rules, each with its item. `lineage`
    /// is the lineage being saved (its own key may be kept).
    pub fn missing(&self, doc: &Document, lineage: Option<ecad_model::DefinitionId>) -> Vec<Missing> {
        let grid = !self.fixed_drawing();
        symbol_cmds::problems(&self.content(), &symbol_cmds::Rules { doc, lineage, grid })
            .into_iter()
            .map(|text| Missing { item: item_of(&text), text })
            .collect()
    }

    /// The JSON form's content fields (`name`, `key`, `graphics`, `points`, `slots`, `defaults`).
    pub fn params(&self) -> Value {
        let graphics: Vec<ecad_file::v1::Primitive> = self.shape.graphics.iter().map(ecad_file::v1::Primitive::from).collect();
        let points: Vec<Value> = self.shape.points.iter().map(|p| json!({"key": p.key, "label": p.label, "at": [p.at.x, p.at.y]})).collect();
        let slots: Vec<Value> =
            self.shape.slots.iter().map(|s| json!({"text": s.text, "at": [s.at.x, s.at.y], "h": s.h, "v": s.v, "rotation": s.rotation})).collect();
        json!({
            "name": self.name.trim(), "key": self.key.trim(), "graphics": graphics, "points": points, "slots": slots,
            "defaults": self.defaults(),
        })
    }
}

/// What a shape click picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picked {
    Graphic(usize),
    Point(usize),
    Slot(usize),
    Offer(usize),
}

/// The Missing item a rule's words are about.
fn item_of(text: &str) -> &'static str {
    let t = text.to_ascii_lowercase();
    if t.contains("needs a name") || t.contains("the name") {
        "name"
    } else if t.contains("tag prefix") {
        "prefix"
    } else if t.contains("key") {
        "key"
    } else if t.contains("connection point") {
        "point"
    } else if t.contains("tag slot") {
        "slot"
    } else {
        "other"
    }
}

/// `5` for 5.0, `2.5` for 2.5.
pub fn fmt_mm(d: f64) -> String {
    let r = (d * 1000.0).round() / 1000.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r}") }
}

/// The drafts of every project (design 048 §3.2), as stored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Drafts {
    pub by_project: BTreeMap<String, Vec<Draft>>,
}

impl Drafts {
    /// The stored drafts, or none when the value cannot be read.
    pub fn from_stored(stored: Option<&str>) -> Self {
        stored.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
    }

    /// The text to store.
    pub fn to_stored(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Keep `d` for `project` (replacing the draft with its id), at most [`MAX_DRAFTS`].
    pub fn keep(&mut self, project: &str, d: &Draft) {
        let list = self.by_project.entry(project.to_string()).or_default();
        list.retain(|x| x.id != d.id);
        list.push(d.clone());
        while list.len() > MAX_DRAFTS {
            list.remove(0);
        }
    }

    /// Forget draft `id` of `project` (saved or discarded).
    pub fn forget(&mut self, project: &str, id: u64) {
        if let Some(list) = self.by_project.get_mut(project) {
            list.retain(|x| x.id != id);
        }
    }

    /// The project's drafts, oldest first.
    pub fn of(&self, project: &str) -> &[Draft] {
        self.by_project.get(project).map_or(&[], Vec::as_slice)
    }

    /// A draft id not used for `project`.
    pub fn next_id(&self, project: &str) -> u64 {
        self.of(project).iter().map(|d| d.id).max().map_or(1, |m| m + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> Primitive {
        Primitive::Line { a: Pt::new(a.0, a.1), b: Pt::new(b.0, b.1) }
    }

    #[test]
    fn a_point_off_the_grid_or_on_another_is_refused_in_the_rules_words() {
        let mut d = Draft::new();
        assert_eq!(d.add_point(Pt::new(2.5, 0.0)), Err("A point goes on the 5 mm grid.".into()));
        assert_eq!(d.add_point(Pt::new(5.0, 0.0)), Ok(0));
        assert_eq!(d.add_point(Pt::new(5.0, 0.0)), Err("Point 1 is already here.".into()));
        assert_eq!(d.add_point(Pt::new(5.0, 5.0)), Ok(1));
        assert_eq!(d.shape.points[1].key, "2");
    }

    #[test]
    fn undo_and_redo_step_the_draft_only() {
        let mut d = Draft::new();
        d.add_graphic(line((0.0, 0.0), (10.0, 0.0)));
        d.add_point(Pt::new(0.0, 0.0)).ok();
        assert!(d.undo());
        assert!(d.shape.points.is_empty());
        assert!(d.redo());
        assert_eq!(d.shape.points.len(), 1);
        assert!(d.changed());
    }

    #[test]
    fn origin_moves_everything_and_a_made_symbol_keeps_its_page_spot() {
        let mut d = Draft::of_geometry(&[line((100.0, 100.0), (120.0, 100.0))], "p".into(), "3".into(), vec![1]);
        let Source::Make { at, .. } = d.source.clone() else { panic!() };
        assert_eq!(at, [110.0, 100.0]);
        d.set_origin(Pt::new(-10.0, 0.0));
        assert_eq!(d.shape.graphics[0].path_ends().map(|(a, _)| a), Some(Pt::new(0.0, 0.0)));
        let Source::Make { at, .. } = d.source.clone() else { panic!() };
        assert_eq!(at, [100.0, 100.0]);
    }

    #[test]
    fn free_ends_on_the_grid_and_a_tag_like_text_are_offered() {
        let text = Primitive::Text {
            at: Pt::new(12.0, 0.0),
            height: 2.5,
            rotation: 0.0,
            value: "-LS1".into(),
            halign: TextHAlign::Left,
            valign: TextVAlign::Baseline,
        };
        let circle = Primitive::Circle { center: Pt::new(0.0, 15.0), radius: 5.0 };
        // Two leads: one free end each on the grid; the third line ends on the circle.
        let g = vec![line((0.0, -10.0), (0.0, -5.0)), line((0.0, 20.0), (0.0, 25.0)), line((10.0, 15.0), (5.0, 15.0)), circle, text];
        let o = offers_for(&g);
        let points: Vec<Pt> = o.iter().filter_map(|x| if let Offer::Point { at } = x { Some(*at) } else { None }).collect();
        assert!(points.contains(&Pt::new(0.0, -10.0)) && points.contains(&Pt::new(0.0, 25.0)));
        assert!(!points.contains(&Pt::new(5.0, 15.0)), "an end on the circle is not free");
        assert!(o.iter().any(|x| matches!(x, Offer::Slot { text, .. } if text == "-LS1")));
        let mut d = Draft::new();
        d.shape = Shape { graphics: g, offers: o, ..Shape::default() };
        d.accept_all().unwrap();
        assert!(d.shape.offers.is_empty());
        assert!(d.shape.slots.iter().any(|s| s.text == "tag"));
        assert!(!d.shape.graphics.iter().any(|g| matches!(g, Primitive::Text { .. })), "the tag text became the slot");
    }

    #[test]
    fn drafts_keep_at_most_eight_per_project() {
        let mut s = Drafts::default();
        for id in 1..=10 {
            let d = Draft { id, ..Draft::new() };
            s.keep("p", &d);
        }
        assert_eq!(s.of("p").len(), MAX_DRAFTS);
        assert_eq!(s.of("p").first().map(|d| d.id), Some(3));
        let back = Drafts::from_stored(Some(&s.to_stored()));
        assert_eq!(back.of("p").len(), MAX_DRAFTS);
    }

    #[test]
    fn looks_like_a_tag() {
        assert!(looks_like_tag("-B03") && looks_like_tag("-LS1"));
        assert!(!looks_like_tag("Level high") && !looks_like_tag("-B") && !looks_like_tag("B03"));
    }
}
