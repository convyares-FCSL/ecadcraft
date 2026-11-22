//! Title block layouts (brief 055, design 055 §1.2–§1.3, FP1): the typed body a `frame` style
//! record carries.
//!
//! # Semantics
//! - A [`TitleBlockLayout`] is the block's size and its items in drawing order: lines, static
//!   texts, fields (a box showing one bound value), a revision table and vector logos. Coordinates
//!   are millimetres from the block's bottom-left corner, whatever the project's length unit.
//! - A [`Binding`] names what a field shows (`project:drawing_number`, `page:number`,
//!   `revision:ecr`, `print:sheet`). It is written as that text. Strict (design 072 §5.4): an
//!   unknown source word, a binding without its `source:`, a revision binding naming no revision
//!   field, or an unknown item kind is a read error.
//! - The frame style holding a title block is typed (design 072 §8, `crate::objects::styles::FrameStyle`):
//!   an unknown body field is a read error too.
//! - Bounds (house rule 12): [`TitleBlockLayout::check`] refuses a breach in words (commands);
//!   [`TitleBlockLayout::usable`] is what drawing uses: items past [`MAX_ITEMS`] or breaking a
//!   bound are skipped and counted, never a panic.
//!
//! This module only holds data and its shape rules. Drawing, binding values and the built-in
//! house block are `ecad-engine`'s (`title_block`).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::doc::Primitive;

/// Most items in one block. Guard: a hostile file cannot make a block unbounded, and it bounds
/// the cost of drawing a frame and of the editor's frame.
pub const MAX_ITEMS: usize = 512;
/// Most shapes in one logo. Guard: a logo that would slow every page's frame.
pub const MAX_LOGO_PRIMS: usize = 2_000;
/// Most logo shapes in one body.
pub const MAX_BODY_PRIMS: usize = 4_000;
/// Longest text, label, binding key or logo name, in characters. Guard: unbounded records.
pub const MAX_BODY_TEXT: usize = 256;
/// Largest coordinate, in millimetres (the widest sheet). Guard: huge geometry.
pub const MAX_COORD_MM: f64 = 2_400.0;
/// Smallest box side, in millimetres.
pub const MIN_BOX_MM: f64 = 0.5;
/// Tallest block, in millimetres. Guard: a block taller than any sheet.
pub const MAX_BLOCK_HEIGHT_MM: f64 = 400.0;
/// Most rows of a revision table.
pub const MAX_TABLE_ROWS: u8 = 20;
/// Longest "If empty" text, in characters (design 055 §4.4).
pub const MAX_IF_EMPTY_TEXT: usize = 32;
/// Text sizes the editor offers (cap height, mm; design 055 §4.4, Q10).
pub const SIZES: [f64; 5] = [1.8, 2.5, 3.5, 5.0, 7.0];
/// Smallest and largest text size a block accepts, in millimetres.
pub const MIN_TEXT_MM: f64 = 1.0;
pub const MAX_TEXT_MM: f64 = 20.0;

/// A title block: its size (width × height, mm) and its items in drawing order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TitleBlockLayout {
    pub size: (f64, f64),
    #[serde(default)]
    pub items: Vec<BlockItem>,
}

/// A rectangle in millimetres from the block's bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }
    /// Right edge.
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    /// Top edge.
    pub fn top(&self) -> f64 {
        self.y + self.h
    }
    /// Whether every number is finite and the box is inside `0..=MAX_COORD_MM` with sides of at
    /// least [`MIN_BOX_MM`].
    pub fn sane(&self) -> bool {
        [self.x, self.y, self.w, self.h].iter().all(|v| v.is_finite())
            && self.x >= 0.0
            && self.y >= 0.0
            && self.w >= MIN_BOX_MM
            && self.h >= MIN_BOX_MM
            && self.right() <= MAX_COORD_MM
            && self.top() <= MAX_COORD_MM
    }
}

/// A line's weight: the frame's inner or border weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineWeight {
    #[default]
    Thin,
    Thick,
}

/// Horizontal alignment of a text in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Centre,
    Right,
}

/// A text's colour: ink, or red (house style: the status; Q8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ink {
    #[default]
    Ink,
    Red,
}

/// How a text or a field's value looks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextDraw {
    /// Cap height, mm.
    pub size: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub align: Align,
    #[serde(default, skip_serializing_if = "is_default")]
    pub colour: Ink,
}

impl Default for TextDraw {
    fn default() -> Self {
        TextDraw { size: 2.5, bold: false, align: Align::Left, colour: Ink::Ink }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// What a field shows when its value is empty.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IfEmpty {
    #[default]
    Blank,
    Dash,
    /// 033's missing mark (`?`).
    Missing,
    /// A short text (at most [`MAX_IF_EMPTY_TEXT`] characters).
    Text(String),
}

/// Where the newest revision sits in a revision table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Newest {
    #[default]
    Top,
    Bottom,
}

crate::word_enum! {
    /// A binding's source.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Source {
        Project,
        Page,
        Revision,
        Print,
    }
}

/// A revision field a binding or a revision table column names (design 072 §5.4): a built-in
/// field or a custom one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevisionColumn {
    Field(crate::document::project::RevField),
    Custom(crate::document::project::CustomField),
}

impl RevisionColumn {
    /// The column `word` names (`rev`, `ecr`), if it names a revision field.
    pub fn parse(word: &str) -> Option<RevisionColumn> {
        match <crate::document::project::RevField as crate::Word>::parse(word) {
            Some(f) => Some(RevisionColumn::Field(f)),
            None => crate::document::project::CustomField::new(word).map(RevisionColumn::Custom),
        }
    }

    /// Its word.
    pub fn word(&self) -> &str {
        match self {
            RevisionColumn::Field(f) => crate::Word::word(*f),
            RevisionColumn::Custom(c) => c.as_str(),
        }
    }
}

/// Why a binding does not read (design 072 §5.4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    #[error("\"{0}\" is not source:key")]
    NoSource(String),
    #[error("\"{0}\" is not a binding source; use project, page, revision or print")]
    Source(String),
    #[error("\"{key}\" is not a {of} key")]
    Key { of: &'static str, key: String },
}

crate::word_enum! {
    /// A page value a field shows (design 055 §7).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum PageField {
        Title,
        Number,
        Next,
        Total,
        Function,
        Location,
    }
}

crate::word_enum! {
    /// A print value a field shows.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum PrintField {
        Sheet,
    }
}

/// Longest project key a field may name, in characters (design 055 §4.3).
pub const MAX_PROJECT_KEY: usize = 32;

/// A project key a field shows (`title`, `drawing_number`, `company`, any project property):
/// 1 to [`MAX_PROJECT_KEY`] characters of `a-z 0-9 _ .`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectKey(Box<str>);

impl ProjectKey {
    /// `key`, if a field may name it.
    pub fn new(key: &str) -> Option<ProjectKey> {
        let ok = !key.is_empty() && key.len() <= MAX_PROJECT_KEY && key.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.'));
        ok.then(|| ProjectKey(key.into()))
    }

    /// The key of a field of the project's info (its words are keys by construction).
    pub fn of_info(f: crate::document::project::InfoField) -> ProjectKey {
        ProjectKey(crate::Word::word(f).into())
    }

    /// The key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a field shows (design 072 §5.4): each source with its own typed key, written
/// `source:key`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Binding {
    Project(ProjectKey),
    Page(PageField),
    Revision(RevisionColumn),
    Print(PrintField),
}

impl Binding {
    /// The binding of `key` under `source`.
    ///
    /// # Errors
    /// [`BindingError::Key`] when `key` is not a key of `source`.
    pub fn of(source: Source, key: &str) -> Result<Binding, BindingError> {
        let bad = || BindingError::Key { of: crate::Word::word(source), key: key.to_string() };
        Ok(match source {
            Source::Project => Binding::Project(ProjectKey::new(key).ok_or_else(bad)?),
            Source::Page => Binding::Page(<PageField as crate::Word>::parse(key).ok_or_else(bad)?),
            Source::Revision => Binding::Revision(RevisionColumn::parse(key).ok_or_else(bad)?),
            Source::Print => Binding::Print(<PrintField as crate::Word>::parse(key).ok_or_else(bad)?),
        })
    }

    /// Read `source:key`.
    ///
    /// # Errors
    /// [`BindingError`]: no colon, an unknown source word, or a key its source does not have.
    pub fn parse(text: &str) -> Result<Binding, BindingError> {
        let (s, k) = text.split_once(':').ok_or_else(|| BindingError::NoSource(text.to_string()))?;
        let source = <Source as crate::Word>::parse(s.trim()).ok_or_else(|| BindingError::Source(s.trim().to_string()))?;
        Binding::of(source, k.trim())
    }

    /// Its source.
    pub fn source(&self) -> Source {
        match self {
            Binding::Project(_) => Source::Project,
            Binding::Page(_) => Source::Page,
            Binding::Revision(_) => Source::Revision,
            Binding::Print(_) => Source::Print,
        }
    }

    /// Its key's word (`drawing_number`, `number`, `ecr`, `sheet`).
    pub fn key(&self) -> &str {
        match self {
            Binding::Project(k) => k.as_str(),
            Binding::Page(f) => crate::Word::word(*f),
            Binding::Revision(c) => c.word(),
            Binding::Print(f) => crate::Word::word(*f),
        }
    }

    /// The revision column a revision binding shows.
    pub fn revision_column(&self) -> Option<&RevisionColumn> {
        match self {
            Binding::Revision(c) => Some(c),
            Binding::Project(_) | Binding::Page(_) | Binding::Print(_) => None,
        }
    }

    /// `source:key`.
    pub fn text(&self) -> String {
        format!("{}:{}", crate::Word::word(self.source()), self.key())
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Binding::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// One item of a title block, in drawing order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockItem {
    Line {
        a: (f64, f64),
        b: (f64, f64),
        #[serde(default, skip_serializing_if = "is_default")]
        weight: LineWeight,
    },
    Text {
        at: Rect,
        text: String,
        #[serde(default)]
        look: TextDraw,
    },
    Field {
        at: Rect,
        bind: Binding,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default)]
        look: TextDraw,
        #[serde(default, skip_serializing_if = "is_default")]
        if_empty: IfEmpty,
    },
    RevTable {
        at: Rect,
        rows: u8,
        #[serde(default = "yes")]
        header: bool,
        #[serde(default, skip_serializing_if = "is_default")]
        newest: Newest,
    },
    /// A vector logo, fitted into `at` keeping its proportions.
    Logo {
        at: Rect,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
        #[serde(default)]
        prims: Vec<Primitive>,
    },
}

fn yes() -> bool {
    true
}

impl BlockItem {
    /// The item's box, or for a line the box spanning its ends.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            BlockItem::Line { a, b, .. } => Some(Rect::new(a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs())),
            BlockItem::Text { at, .. } | BlockItem::Field { at, .. } | BlockItem::RevTable { at, .. } | BlockItem::Logo { at, .. } => Some(*at),
        }
    }

    /// The item's word, for messages: `line`, `text`, `field`, `revision table`, `logo`.
    pub fn word(&self) -> &'static str {
        match self {
            BlockItem::Line { .. } => "line",
            BlockItem::Text { .. } => "text",
            BlockItem::Field { .. } => "field",
            BlockItem::RevTable { .. } => "revision table",
            BlockItem::Logo { .. } => "logo",
        }
    }

    /// Why this item breaks a bound (design 055 §1.3), in words; `None` when it keeps them.
    pub fn fault(&self) -> Option<String> {
        let coord = |v: f64| v.is_finite() && (0.0..=MAX_COORD_MM).contains(&v);
        let draw_ok = |l: &TextDraw| l.size.is_finite() && (MIN_TEXT_MM..=MAX_TEXT_MM).contains(&l.size);
        let long = |s: &str| s.chars().count() > MAX_BODY_TEXT;
        match self {
            BlockItem::Line { a, b, .. } => {
                (![a.0, a.1, b.0, b.1].into_iter().all(coord)).then(|| format!("A line's ends must be 0 to {MAX_COORD_MM} mm."))
            }
            BlockItem::Text { at, text, look } => {
                if !at.sane() {
                    Some(box_words())
                } else if long(text) {
                    Some(format!("A text is longer than {MAX_BODY_TEXT} characters."))
                } else if !draw_ok(look) {
                    Some(size_words())
                } else {
                    None
                }
            }
            BlockItem::Field { at, bind, label, look, if_empty } => {
                if !at.sane() {
                    Some(box_words())
                } else if long(bind.key()) || bind.key().is_empty() || label.as_deref().is_some_and(long) {
                    Some(format!("A field's key must be 1 to {MAX_BODY_TEXT} characters, and its label at most {MAX_BODY_TEXT}."))
                } else if !draw_ok(look) {
                    Some(size_words())
                } else if matches!(if_empty, IfEmpty::Text(t) if t.chars().count() > MAX_IF_EMPTY_TEXT) {
                    Some(format!("An \"if empty\" text is at most {MAX_IF_EMPTY_TEXT} characters."))
                } else {
                    None
                }
            }
            BlockItem::RevTable { at, rows, .. } => {
                if !at.sane() {
                    Some(box_words())
                } else if !(1..=MAX_TABLE_ROWS).contains(rows) {
                    Some(format!("A revision table has 1 to {MAX_TABLE_ROWS} rows."))
                } else {
                    None
                }
            }
            BlockItem::Logo { at, name, prims } => {
                if !at.sane() {
                    Some(box_words())
                } else if long(name) {
                    Some(format!("A logo's name is at most {MAX_BODY_TEXT} characters."))
                } else if prims.len() > MAX_LOGO_PRIMS {
                    Some(logo_words(prims.len()))
                } else if !prims.iter().all(prim_sane) {
                    Some("A logo holds a shape with numbers that are not finite.".to_string())
                } else {
                    None
                }
            }
        }
    }
}

/// The refusal for a logo with too many shapes (design 055 §6).
pub fn logo_words(n: usize) -> String {
    format!("The logo has {} shapes; {} at most. Simplify it first.", group(n), group(MAX_LOGO_PRIMS))
}

/// `3400` → `3 400`.
pub fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn box_words() -> String {
    format!("A box must be inside 0 to {MAX_COORD_MM} mm, with sides of at least {MIN_BOX_MM} mm.")
}

fn size_words() -> String {
    format!("A text size must be {MIN_TEXT_MM} to {MAX_TEXT_MM} mm.")
}

fn prim_sane(p: &Primitive) -> bool {
    let pt = |p: &crate::Pt| p.x.is_finite() && p.y.is_finite() && p.x.abs() < 1e9 && p.y.abs() < 1e9;
    let n = |v: f64| v.is_finite() && v.abs() < 1e9;
    match p {
        Primitive::Line { a, b } => pt(a) && pt(b),
        Primitive::Polyline { vertices, .. } => vertices.len() <= 10_000 && vertices.iter().all(pt),
        Primitive::Arc { center, radius, start, end } => pt(center) && n(*radius) && n(*start) && n(*end),
        Primitive::Circle { center, radius } => pt(center) && n(*radius),
        Primitive::Text { at, height, rotation, value, .. } => pt(at) && n(*height) && n(rotation.0) && value.chars().count() <= MAX_BODY_TEXT,
        Primitive::Point { at } => pt(at),
    }
}

/// What [`TitleBlockLayout::usable`] skipped, for Checks (`title-binding-unknown`'s family).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Skipped {
    /// Items past [`MAX_ITEMS`], breaking a bound, past the body's shape budget, or unknown.
    pub items: usize,
}

impl TitleBlockLayout {
    /// An empty block of `w` × `h` mm.
    pub fn blank(w: f64, h: f64) -> TitleBlockLayout {
        TitleBlockLayout { size: (w, h), items: Vec::new() }
    }

    /// Whether the size keeps its bounds: 1–2 400 mm wide, 1–400 mm high.
    pub fn size_ok(&self) -> bool {
        let (w, h) = self.size;
        w.is_finite() && h.is_finite() && (1.0..=MAX_COORD_MM).contains(&w) && (1.0..=MAX_BLOCK_HEIGHT_MM).contains(&h)
    }

    /// The block's height for keep-out: its height when the size keeps its bounds, else `None`.
    pub fn height(&self) -> Option<f64> {
        self.size_ok().then_some(self.size.1)
    }

    /// Whether the block keeps every bound of design 055 §1.3.
    ///
    /// # Errors
    /// The first bound broken, in words (commands refuse with it; nothing is written).
    pub fn check(&self) -> Result<(), String> {
        if !self.size_ok() {
            return Err(format!("A block is 1 to {MAX_COORD_MM} mm wide and 1 to {MAX_BLOCK_HEIGHT_MM} mm high."));
        }
        if self.items.len() > MAX_ITEMS {
            return Err(format!("A block holds at most {MAX_ITEMS} items; this one has {}.", self.items.len()));
        }
        let mut prims = 0usize;
        for (i, item) in self.items.iter().enumerate() {
            if let Some(f) = item.fault() {
                return Err(format!("Item {} ({}): {f}", i + 1, item.word()));
            }
            if let BlockItem::Logo { prims: p, .. } = item {
                prims += p.len();
            }
        }
        if prims > MAX_BODY_PRIMS {
            return Err(format!("The logos have {} shapes together; {} at most.", group(prims), group(MAX_BODY_PRIMS)));
        }
        Ok(())
    }

    /// The items drawing uses, in order, and how many were skipped: the first [`MAX_ITEMS`]
    /// that keep their bounds, logos only while the body's shape budget lasts. Never fails;
    /// allocates only the index list.
    pub fn usable(&self) -> (Vec<usize>, Skipped) {
        let mut out = Vec::new();
        let mut skipped = Skipped { items: self.items.len().saturating_sub(MAX_ITEMS) };
        let mut prims = 0usize;
        for (i, item) in self.items.iter().enumerate().take(MAX_ITEMS) {
            if item.fault().is_some() {
                skipped.items += 1;
                continue;
            }
            if let BlockItem::Logo { prims: p, .. } = item {
                if prims + p.len() > MAX_BODY_PRIMS {
                    skipped.items += 1;
                    continue;
                }
                prims += p.len();
            }
            out.push(i);
        }
        (out, skipped)
    }

    /// Every binding a field of the block reads, in item order (with repeats).
    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.items.iter().take(MAX_ITEMS).filter_map(|i| match i {
            BlockItem::Field { bind, .. } => Some(bind),
            _ => None,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::*;

    #[test]
    fn bindings_read_and_write() {
        let b = Binding::parse("project:drawing_number").unwrap();
        assert_eq!(b, Binding::Project(ProjectKey::new("drawing_number").unwrap()));
        assert_eq!(b.text(), "project:drawing_number");
        assert_eq!(Binding::parse("page:number").unwrap(), Binding::Page(PageField::Number));
        assert_eq!(Binding::parse("print:sheet").unwrap(), Binding::Print(PrintField::Sheet));
        for bad in ["weather:today", "nocolon", "revision:Bad Name", "revision:issued", "page:weather", "print:x", "project:Bad"] {
            assert!(Binding::parse(bad).is_err(), "{bad}");
        }
        let r = Binding::parse("revision:ecr").unwrap();
        assert_eq!(r.revision_column().map(|c| c.word().to_string()).as_deref(), Some("ecr"));
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(group(3400), "3 400");
        assert_eq!(group(2000), "2 000");
        assert_eq!(group(120), "120");
        assert_eq!(group(1_234_567), "1 234 567");
    }
}
