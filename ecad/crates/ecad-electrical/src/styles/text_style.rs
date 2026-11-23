//! Text styles: the kinds of text ECAD draws, their resolved height,
//! weight and CADCraft style name, and whether a text is shown.
//!
//! Every value comes from the kind's text style ([`crate::styles::text`]): the
//! registry's `text.<kind>.style` choice says which style, and [`ecad_model::registry::styles::field`] what it
//! says. Nothing here writes a size or a weight as a literal. The readers are a few map lookups
//! and never fail: a hostile stored value counts as absent.
//!
//! This module draws nothing and stores nothing.

use ecad_model::{DeviceId, Document, PageId, RepresentationId, StyleRef, TextHAlign, TextVAlign};

use crate::defaults::text as d;
use ecad_model::look::{Look, OwnStyle, RunRule, Shown, TextColour, TextKind, TextStyle, TextTarget};
use ecad_model::objects::styles::text::Weight;
use ecad_model::registry::{Entry, Scope};

pub mod fonts;

/// The CADCraft text style of regular ECAD text (the project font, regular).
pub const STYLE_REGULAR: &str = "Standard";
/// The CADCraft text style of bold ECAD text (the project font, bold).
pub const STYLE_BOLD: &str = "ECAD-BOLD";
/// The style name a text entity is built with before its kind's style is applied:
/// every drawn text then takes its kind's style through `DrawApply`; symbol line art, which is
/// exempt, keeps it).
pub const PLACEHOLDER_STYLE: &str = STYLE_REGULAR;

/// A colour field's value as a [`TextColour`]: `None` for `none` (ink) and anything unknown.
pub(crate) fn text_colour(value: &str) -> Option<TextColour> {
    let t = value.trim();
    if t.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some((r, g, b)) = ecad_model::registry::styles::hex_rgb(t) {
        return Some(TextColour::Rgb(r, g, b));
    }
    crate::cables::words::colour_code(t).map(|(first, _)| TextColour::Code(first))
}

/// The CADCraft text style of a weight.
pub(crate) trait WeightStyle {
    /// The CADCraft text style for this weight.
    fn style_name(self) -> &'static str;
}

impl WeightStyle for Weight {
    fn style_name(self) -> &'static str {
        match self {
            Weight::Regular => STYLE_REGULAR,
            Weight::Bold => STYLE_BOLD,
        }
    }
}

/// The registry entry choosing `kind`'s text style (`text.<kind>.style`).
pub fn style_entry(kind: TextKind) -> &'static Entry {
    let i = TextKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    crate::defaults::styles::TEXT.get(i).copied().unwrap_or(&crate::defaults::styles::TAG)
}

/// The registry entry of `kind`'s visibility, for the kinds that have one; `None` for kinds
/// that are always shown.
pub fn show_entry(kind: TextKind) -> Option<&'static Entry> {
    match kind {
        TextKind::Tag => Some(&d::TAG_SHOW),
        TextKind::PointLabel => Some(&d::POINT_LABEL_SHOW),
        TextKind::Description => Some(&d::DESCRIPTION_SHOW),
        TextKind::PartNumber => Some(&d::PART_NUMBER_SHOW),
        TextKind::ContinuationName => Some(&d::CONTINUATION_NAME_SHOW),
        TextKind::Reference => Some(&d::REFERENCE_SHOW),
        TextKind::ContinuationCaption => Some(&d::CONTINUATION_CAPTION_SHOW),
        TextKind::LocationLabel => Some(&d::LOCATION_SHOW),
        TextKind::WireLabel => Some(&d::WIRE_LABEL_SHOW),
        _ => None,
    }
}

/// The style of `kind` in this project: its text style (the project's `text.<kind>.style`, else
/// the standard's, else the kind's built-in style) and that style's fields. The
/// wire label's height is `settings.wiring.label_height`. Never fails; a hostile stored
/// value gives the built-in.
///
/// # Example
///
/// ```
/// use ecad_electrical::styles::text_style::text_style;
/// use ecad_model::look::TextKind;
/// use ecad_model::objects::styles::text::Weight;
/// let doc = ecad_model::Document::default();
/// let tag = text_style(&doc, TextKind::Tag);
/// assert_eq!((tag.height, tag.weight, tag.style_name), (2.5, Weight::Bold, "ECAD-BOLD"));
/// ```
pub fn text_style(doc: &Document, kind: TextKind) -> TextStyle {
    text_style_at(doc, kind, Scope::project())
}

/// [`text_style`] at `at`: an object's own `text.<kind>.style`
/// (S P O) chooses its style. The CADCraft style name and font stay the kind's project style's (an
/// object's style changes height, weight and colour).
pub fn text_style_at(doc: &Document, kind: TextKind, at: Scope) -> TextStyle {
    let id = style_of(doc, kind, at);
    let f = |k: &str| ecad_model::registry::styles::field_of(doc, &crate::styles::text::CATALOGUE, &id, k, None);
    let height = f("height").map_or_else(|| builtin_height(doc), |r| r.length(doc));
    let weight = match f("weight").map(|r| r.word()) {
        Some("bold") => Weight::Bold,
        _ => Weight::Regular,
    };
    let style_name = match kind_font(doc, kind) {
        Some(_) => kind_style_name(kind, weight),
        None => weight.style_name(),
    };
    let colour = f("colour").and_then(|r| text_colour(&r.text()));
    TextStyle { height, weight, style_name, colour }
}

/// The CADCraft text style of an own text style drawn in `family`: `ECAD-FONT-<FAMILY>` and its
/// `-BOLD` twin (letters and digits kept, the rest `-`).
pub fn family_style_name(family: &str, weight: Weight) -> String {
    let w: String = family.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '-' }).take(48).collect();
    match weight {
        Weight::Regular => format!("ECAD-FONT-{w}"),
        Weight::Bold => format!("ECAD-FONT-{w}-BOLD"),
    }
}

/// The own text style of `kind` that the object with `props` chooses (its `text.<kind>.style`),
/// resolved at `at`; `None` when it has no valid choice of its own.
pub(crate) fn own_style(doc: &Document, kind: TextKind, at: Scope, props: &ecad_model::Properties) -> Option<OwnStyle> {
    let e = crate::styles::text_style::style_entry(kind);
    props.get(e.key).and_then(|v| e.parse_in(doc, v))?;
    let style = text_style_at(doc, kind, at);
    let id = style_of(doc, kind, at);
    Some(own_with(doc, style, &id))
}

/// The text style `id` of the `text` catalogue as drawn: its height, weight and
/// colour, and the CADCraft style naming its font, as [`own_style`] gives an object's own style.
/// For a text whose style is chosen by something other than its kind (a location box's label).
pub fn own_style_of(doc: &Document, id: &StyleRef) -> OwnStyle {
    let f = |k: &str| ecad_model::registry::styles::field_of(doc, &crate::styles::text::CATALOGUE, id, k, None);
    let height = f("height").map_or_else(|| builtin_height(doc), |r| r.length(doc));
    let weight = match f("weight").map(|r| r.word()) {
        Some("bold") => Weight::Bold,
        _ => Weight::Regular,
    };
    let colour = f("colour").and_then(|r| text_colour(&r.text()));
    own_with(doc, TextStyle { height, weight, style_name: weight.style_name(), colour }, id)
}

/// The text catalogue's built-in cap height in drawing units (only when a field cannot be read).
fn builtin_height(doc: &Document) -> f64 {
    let mm = match crate::styles::text::CATALOGUE.field("height").map(|f| f.builtin()) {
        Some(ecad_model::registry::Value::Length(x)) => x,
        _ => 0.0,
    };
    ecad_model::registry::Resolved { value: ecad_model::registry::Value::Length(mm), from: ecad_model::registry::Source::Builtin }.length(doc)
}

/// [`OwnStyle`] for `style` of text style `id`: its font family when not the project font.
fn own_with(doc: &Document, style: TextStyle, id: &StyleRef) -> OwnStyle {
    let font = ecad_model::registry::styles::field_of(doc, &crate::styles::text::CATALOGUE, id, "font", None).map(|r| r.text()).unwrap_or_default();
    let project = match ecad_model::registry::resolve(doc, &d::FONT, Scope::project()).value {
        ecad_model::registry::Value::Text(t) => t.to_string(),
        _ => default_family().to_string(),
    };
    let family =
        (!font.is_empty() && !font.eq_ignore_ascii_case(crate::styles::text::PROJECT_FONT) && !font.eq_ignore_ascii_case(&project)).then_some(font);
    let name = match &family {
        Some(f) => family_style_name(f, style.weight),
        None => style.weight.style_name().to_string(),
    };
    OwnStyle { style, name, family }
}

/// The text style `kind` uses at `at`.
pub fn style_of(doc: &Document, kind: TextKind, at: Scope) -> StyleRef {
    ecad_model::registry::styles::chosen(doc, crate::styles::text_style::style_entry(kind), at)
}

/// The CADCraft text styles of kinds with a font of their own: `ECAD-TAG`, `ECAD-TAG-BOLD`…, in
/// the order of [`TextKind::ALL`].
const KIND_STYLES: [(&str, &str); TextKind::ALL.len()] = [
    ("ECAD-TAG", "ECAD-TAG-BOLD"),
    ("ECAD-POINT-LABEL", "ECAD-POINT-LABEL-BOLD"),
    ("ECAD-DESCRIPTION", "ECAD-DESCRIPTION-BOLD"),
    ("ECAD-PART-NUMBER", "ECAD-PART-NUMBER-BOLD"),
    ("ECAD-CONTINUATION-NAME", "ECAD-CONTINUATION-NAME-BOLD"),
    ("ECAD-REFERENCE", "ECAD-REFERENCE-BOLD"),
    ("ECAD-CONTINUATION-CAPTION", "ECAD-CONTINUATION-CAPTION-BOLD"),
    ("ECAD-WIRE-LABEL", "ECAD-WIRE-LABEL-BOLD"),
    ("ECAD-CABLE-TEXT", "ECAD-CABLE-TEXT-BOLD"),
    ("ECAD-LOCATION", "ECAD-LOCATION-BOLD"),
    ("ECAD-TITLE-VALUE", "ECAD-TITLE-VALUE-BOLD"),
    ("ECAD-TITLE-CAPTION", "ECAD-TITLE-CAPTION-BOLD"),
    ("ECAD-TITLE-MAIN", "ECAD-TITLE-MAIN-BOLD"),
    ("ECAD-GRID", "ECAD-GRID-BOLD"),
    ("ECAD-TABLE", "ECAD-TABLE-BOLD"),
    ("ECAD-TABLE-HEADER", "ECAD-TABLE-HEADER-BOLD"),
    ("ECAD-FREE", "ECAD-FREE-BOLD"),
    ("ECAD-TITLE-PAGE", "ECAD-TITLE-PAGE-BOLD"),
    ("ECAD-TITLE-REV", "ECAD-TITLE-REV-BOLD"),
    ("ECAD-TABLE-HEADING", "ECAD-TABLE-HEADING-BOLD"),
    ("ECAD-COVER", "ECAD-COVER-BOLD"),
    ("ECAD-CHANNEL-TEXT", "ECAD-CHANNEL-TEXT-BOLD"),
    ("ECAD-ADDRESS", "ECAD-ADDRESS-BOLD"),
];

/// The style a kind with its own font names, for `weight`.
pub fn kind_style_name(kind: TextKind, weight: Weight) -> &'static str {
    let i = TextKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    let (regular, bold) = KIND_STYLES.get(i).copied().unwrap_or((STYLE_REGULAR, STYLE_BOLD));
    match weight {
        Weight::Regular => regular,
        Weight::Bold => bold,
    }
}

/// A kind's own font family (its text style's `font`), when it names a family that
/// differs from the project font; `None` when the kind uses the project font (`project`, the
/// built-in).
pub(crate) fn kind_font(doc: &Document, kind: TextKind) -> Option<&str> {
    let id = style_of(doc, kind, Scope::project());
    let r = ecad_model::registry::styles::field_of(doc, &crate::styles::text::CATALOGUE, &id, "font", None)?;
    let ecad_model::registry::Value::Text(f) = r.value else { return None };
    if f.eq_ignore_ascii_case(crate::styles::text::PROJECT_FONT) {
        return None;
    }
    let project = match ecad_model::registry::resolve(doc, &d::FONT, Scope::project()).value {
        ecad_model::registry::Value::Text(t) => t,
        _ => default_family(),
    };
    (!f.eq_ignore_ascii_case(project)).then_some(f)
}

/// Whether a text of `kind` on `target` is shown: the narrowest level that has a
/// value wins (symbol, device or conductor, page, project); then, for a tag with a run rule, the
/// project's `text.tag.show.strip` / `.plc`; then the built-in. `First` is only ever returned for
/// a tag. Kinds with no visibility entry are always `On`. Never fails.
pub(crate) fn text_shown(doc: &Document, target: TextTarget, kind: TextKind) -> Shown {
    let Some(entry) = crate::styles::text_style::show_entry(kind) else { return Shown::On };
    let r = ecad_model::registry::resolve(doc, entry, target.scope());
    if r.from == ecad_model::registry::Source::Builtin && kind == TextKind::Tag {
        let rule = match target.run {
            RunRule::None => None,
            RunRule::Strip => Some(&d::TAG_SHOW_STRIP),
            RunRule::Plc => Some(&d::TAG_SHOW_PLC),
        };
        if let Some(rule) = rule {
            return <Shown as ecad_model::Word>::parse(ecad_model::registry::resolve(doc, rule, Scope::project()).word()).unwrap_or_default();
        }
    }
    <Shown as ecad_model::Word>::parse(r.word()).unwrap_or_default()
}

// ---------------------------------------------------------------- Fonts

/// The family CADCraft's resolver looks for when the project's family is missing: the built-in of
/// `text.font`.
pub(crate) fn default_family() -> &'static str {
    match d::FONT.builtin() {
        ecad_model::registry::Value::Text(t) => t,
        _ => "",
    }
}

// ---------------------------------------------------------------- Show Hidden Text

// ---------------------------------------------------------------- Justification and boxes

/// A two-letter justification code (`L`/`C`/`R` then `T`/`M`/`B`; baseline counts as `B`).
pub(crate) fn justify_code(h: TextHAlign, v: TextVAlign) -> String {
    let a = match h {
        TextHAlign::Center => 'C',
        TextHAlign::Right => 'R',
        _ => 'L',
    };
    let b = match v {
        TextVAlign::Top => 'T',
        TextVAlign::Middle => 'M',
        TextVAlign::Bottom | TextVAlign::Baseline => 'B',
    };
    format!("{a}{b}")
}

/// The alignment a two-letter code names (any case); `None` for anything else.
pub fn parse_justify(code: &str) -> Option<(TextHAlign, TextVAlign)> {
    let c = code.trim().to_ascii_uppercase();
    let mut it = c.chars();
    let h = match it.next()? {
        'L' => TextHAlign::Left,
        'C' => TextHAlign::Center,
        'R' => TextHAlign::Right,
        _ => return None,
    };
    let v = match it.next()? {
        'T' => TextVAlign::Top,
        'M' => TextVAlign::Middle,
        'B' => TextVAlign::Bottom,
        _ => return None,
    };
    it.next().is_none().then_some((h, v))
}

/// The box of a one-line text of `width` and `height` at `at` with this justification.
pub(crate) fn text_box(width: f64, height: f64, at: ecad_model::Pt, h: TextHAlign, v: TextVAlign) -> (ecad_model::Pt, ecad_model::Pt) {
    let x0 = match h {
        TextHAlign::Center => at.x - width / 2.0,
        TextHAlign::Right => at.x - width,
        _ => at.x,
    };
    let y0 = match v {
        TextVAlign::Top => at.y - height,
        TextVAlign::Middle => at.y - height / 2.0,
        TextVAlign::Bottom | TextVAlign::Baseline => at.y,
    };
    (ecad_model::Pt::new(x0, y0), ecad_model::Pt::new(x0 + width, y0 + height))
}

// ---------------------------------------------------------------- Symbol keys

/// The prefix of a symbol's own text keys on its component: `text.<rep>.`.
pub(crate) fn symbol_prefix(rep: RepresentationId) -> String {
    format!("text.{}.", rep.hex())
}

/// A symbol's own text keys and values on its component (`text.<rep>.*`).
pub(crate) fn symbol_text_keys(props: &ecad_model::Properties, rep: RepresentationId) -> Vec<(String, String)> {
    let prefix = symbol_prefix(rep);
    props.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Whether `key` is a symbol's own text key (`text.<32 hex digits>.…`).
pub(crate) fn is_symbol_key(key: &str) -> bool {
    key.strip_prefix("text.").and_then(|r| r.split_once('.')).is_some_and(|(id, _)| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The properties of a copy of symbol `old` that is now symbol `new`: every symbol key of the
/// source component is dropped, and `old`'s own keys come back under `new`.
pub fn copy_symbol_keys(props: &mut ecad_model::Properties, old: RepresentationId, new: RepresentationId) {
    let mine = symbol_text_keys(props, old);
    props.retain(|k, _| !is_symbol_key(k));
    for (k, v) in mine {
        props.insert(rename_symbol_key(&k, old, new), v);
    }
}

/// `key` of symbol `old` as the same key of symbol `new`.
pub(crate) fn rename_symbol_key(key: &str, old: RepresentationId, new: RepresentationId) -> String {
    match key.strip_prefix(&symbol_prefix(old)) {
        Some(rest) => format!("{}{rest}", symbol_prefix(new)),
        None => key.to_string(),
    }
}

// ---------------------------------------------------------------- Placement

/// Most tag blocks on a page that search for a free place; beyond it a block takes its first
/// candidate (a guard on hostile pages).
const MAX_PLACED_BLOCKS: usize = 5_000;
/// Most obstacles on a page that are looked at; beyond it blocks take their first candidate.
const MAX_OBSTACLES: usize = 50_000;
/// The side of a cell of the obstacle grid, millimetres (a performance guard, not a preference).
const CELL: f64 = 20.0;
/// Most cells one box is filed under (a guard: a huge box is filed under none and never collides).
const MAX_CELLS_PER_BOX: i64 = 400;

/// An axis-aligned box `(lo, hi)`.
pub(crate) type TextBox = (ecad_model::Pt, ecad_model::Pt);

fn finite_box(b: &TextBox) -> bool {
    b.0.is_finite() && b.1.is_finite()
}

fn overlap_area(a: &TextBox, b: &TextBox) -> f64 {
    let w = a.1.x.min(b.1.x) - a.0.x.max(b.0.x);
    let h = a.1.y.min(b.1.y) - a.0.y.max(b.0.y);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}

/// Obstacles filed in a uniform grid of [`CELL`] cells.
#[derive(Default)]
struct Obstacles {
    boxes: Vec<TextBox>,
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl Obstacles {
    fn cell_range(b: &TextBox) -> Option<(i64, i64, i64, i64)> {
        let c = |v: f64| (v / CELL).floor();
        let (x0, y0, x1, y1) = (c(b.0.x), c(b.0.y), c(b.1.x), c(b.1.y));
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite() && v.abs() < 1e12) {
            return None;
        }
        let (x0, y0, x1, y1) = (x0 as i64, y0 as i64, x1 as i64, y1 as i64);
        ((x1 - x0 + 1).saturating_mul(y1 - y0 + 1) <= MAX_CELLS_PER_BOX).then_some((x0, y0, x1, y1))
    }

    fn add(&mut self, b: TextBox) {
        if !finite_box(&b) || self.boxes.len() >= MAX_OBSTACLES {
            return;
        }
        let i = self.boxes.len();
        self.boxes.push(b);
        if let Some((x0, y0, x1, y1)) = Self::cell_range(&b) {
            for x in x0..=x1 {
                for y in y0..=y1 {
                    self.cells.entry((x, y)).or_default().push(i);
                }
            }
        }
    }

    /// The area of `b` that obstacles cover (overlaps counted per obstacle).
    fn overlap(&self, b: &TextBox) -> f64 {
        let Some((x0, y0, x1, y1)) = Self::cell_range(b) else { return 0.0 };
        let mut seen = std::collections::BTreeSet::new();
        let mut area = 0.0;
        for x in x0..=x1 {
            for y in y0..=y1 {
                for i in self.cells.get(&(x, y)).into_iter().flatten() {
                    if seen.insert(*i)
                        && let Some(o) = self.boxes.get(*i)
                    {
                        area += overlap_area(b, o);
                    }
                }
            }
        }
        area
    }
}

/// Where a symbol's tag block goes: its box and its horizontal justification (the lines are
/// stacked down from the box's top).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BlockPlace {
    pub rect: TextBox,
    pub halign: TextHAlign,
    /// Stored by Move Text.
    pub moved: bool,
    /// No candidate was free: the block overlaps something.
    pub overlaps: bool,
}

impl BlockPlace {
    /// The alignment point of a line whose top is at `top`: left, centre or right of the box.
    pub fn line_at(&self, top: f64) -> ecad_model::Pt {
        let x = match self.halign {
            TextHAlign::Right => self.rect.1.x,
            TextHAlign::Center => (self.rect.0.x + self.rect.1.x) / 2.0,
            _ => self.rect.0.x,
        };
        ecad_model::Pt::new(x, top)
    }
}

/// The box of a block `w` × `h` anchored at `at` with a two-letter justification.
pub(crate) fn block_rect(at: ecad_model::Pt, w: f64, h: f64, halign: TextHAlign, v: TextVAlign) -> TextBox {
    let x0 = match halign {
        TextHAlign::Right => at.x - w,
        TextHAlign::Center => at.x - w / 2.0,
        _ => at.x,
    };
    let y1 = match v {
        TextVAlign::Top => at.y,
        TextVAlign::Middle => at.y + h / 2.0,
        TextVAlign::Bottom | TextVAlign::Baseline => at.y + h,
    };
    (ecad_model::Pt::new(x0, y1 - h), ecad_model::Pt::new(x0 + w, y1))
}

/// The candidates for a block `w` × `h` beside a symbol box `(lo, hi)`, in
/// order: (box, halign).
pub fn candidates(lo: ecad_model::Pt, hi: ecad_model::Pt, vertical: bool, w: f64, h: f64, gap: f64) -> [(TextBox, TextHAlign); 5] {
    use ecad_model::Pt;
    let cx = (lo.x + hi.x) / 2.0;
    let cy = (lo.y + hi.y) / 2.0;
    let r = |x0: f64, y0: f64| (Pt::new(x0, y0), Pt::new(x0 + w, y0 + h));
    let left_top = (r(lo.x - gap - w, hi.y - h), TextHAlign::Right);
    let left_mid = (r(lo.x - gap - w, cy - h / 2.0), TextHAlign::Right);
    let right_top = (r(hi.x + gap, hi.y - h), TextHAlign::Left);
    let right_mid = (r(hi.x + gap, cy - h / 2.0), TextHAlign::Left);
    let above = (r(cx - w / 2.0, hi.y + gap), TextHAlign::Center);
    let above_left = (r(lo.x, hi.y + gap), TextHAlign::Left);
    let below = (r(cx - w / 2.0, lo.y - gap - h), TextHAlign::Center);
    if vertical { [left_top, left_mid, right_top, above, below] } else { [above, above_left, below, left_mid, right_mid] }
}

/// A stored text place (`"x,y,J"`): the point in the symbol's own frame and the
/// justification. `None` for anything that does not parse.
pub(crate) fn parse_at(v: &str) -> Option<(ecad_model::Pt, TextHAlign, TextVAlign)> {
    let mut it = v.split(',');
    let x: f64 = it.next()?.trim().parse().ok()?;
    let y: f64 = it.next()?.trim().parse().ok()?;
    let (h, v) = parse_justify(it.next()?)?;
    if it.next().is_some() || !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some((ecad_model::Pt::new(x, y), h, v))
}

/// The stored form of a text place.
pub fn at_text(p: ecad_model::Pt, h: TextHAlign, v: TextVAlign) -> String {
    let r = |x: f64| (x * 1e6).round() / 1e6;
    format!("{},{},{}", r(p.x), r(p.y), justify_code(h, v))
}

/// A page point in a placement's local frame (the inverse of `Placement::apply`).
pub(crate) fn to_local(pl: &ecad_model::Placement, p: ecad_model::Pt) -> Option<ecad_model::Pt> {
    if !pl.is_valid() || !p.is_finite() {
        return None;
    }
    let (dx, dy) = (p.x - pl.origin.x, p.y - pl.origin.y);
    let (s, c) = pl.rotation.sin_cos();
    let (x, y) = ((c * dx + s * dy) / pl.scale, (-s * dx + c * dy) / pl.scale);
    let x = if pl.mirror { -x } else { x };
    let q = ecad_model::Pt::new(x, y);
    q.is_finite().then_some(q)
}

/// One tag block to place: its symbol, the symbol's box, orientation and placement, and the
/// block's size.
#[derive(Debug, Clone)]
pub struct Block {
    pub rep: RepresentationId,
    pub component: DeviceId,
    pub lo: ecad_model::Pt,
    pub hi: ecad_model::Pt,
    pub vertical: bool,
    pub placement: ecad_model::Placement,
    pub w: f64,
    pub h: f64,
}

/// The obstacles of `page` that never move: symbol boxes, conductor routes inflated
/// by `text.wire_clearance`, and the boxes of `fixed` texts (connection point labels, location
/// labels). The frame's inside edge is checked separately ([`place_blocks`]).
fn page_obstacles(doc: &Document, page: PageId, fixed: &[TextBox]) -> Obstacles {
    let mut obs = Obstacles::default();
    let clear = ecad_model::registry::resolve(doc, &d::WIRE_CLEARANCE, Scope::project()).length(doc);
    for (_, r) in doc.representations_on(page) {
        match r.subject {
            ecad_model::Subject::Device(_) if r.form == ecad_model::Form::Symbol => {
                if let Some((def, pl)) = crate::devices::tags::rep_definition(doc, r)
                    && let Some(b) = crate::devices::texts::placed_box(def, &pl)
                {
                    obs.add(b);
                }
            }
            ecad_model::Subject::Route(_) => {
                for item in r.items.iter().filter_map(|i| doc.items().get(i)) {
                    let pts: Vec<ecad_model::Pt> = match &item.kind {
                        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, b }) => vec![*a, *b],
                        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }) => vertices.clone(),
                        _ => Vec::new(),
                    };
                    for s in pts.windows(2) {
                        if let [a, b] = s {
                            obs.add((
                                ecad_model::Pt::new(a.x.min(b.x) - clear, a.y.min(b.y) - clear),
                                ecad_model::Pt::new(a.x.max(b.x) + clear, a.y.max(b.y) + clear),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for b in fixed {
        obs.add(*b);
    }
    obs
}

/// Place every tag block of `page`: moved blocks at their stored places, then
/// the others in reading order, each at its first free candidate (else the one with the least
/// overlap), never crossing the frame's inside edge when a candidate inside it exists. Each placed
/// block becomes an obstacle. Deterministic: the result does not depend on op order.
pub(crate) fn place_blocks(
    doc: &Document, page: PageId, blocks: &[Block], fixed: &[TextBox], look: &dyn Look,
) -> std::collections::BTreeMap<RepresentationId, BlockPlace> {
    let mut out = std::collections::BTreeMap::new();
    let mut obs = page_obstacles(doc, page, fixed);
    let grid = ecad_model::document::sheet::Grid::of(doc, page);
    let frame =
        (ecad_model::Pt::new(grid.frame_left(), grid.frame_bottom()), ecad_model::Pt::new(grid.frame_left() + grid.area_width(), grid.frame_top()));
    let inside = |b: &TextBox| !finite_box(&frame) || (b.0.x >= frame.0.x && b.0.y >= frame.0.y && b.1.x <= frame.1.x && b.1.y <= frame.1.y);
    let gap_factor = ecad_model::registry::resolve(doc, &d::TAG_GAP, Scope::project()).number();
    let tag_height = look.text_style(doc, TextKind::Tag).height;
    let mut free: Vec<&Block> = Vec::new();
    // Moved blocks first.
    for b in blocks {
        let stored = doc
            .devices()
            .get(&b.component)
            .and_then(|c| c.properties.get(&format!("{}tag.at", symbol_prefix(b.rep))))
            .and_then(|v| parse_at(v))
            .and_then(|(p, h, v)| {
                let at = b.placement.apply(p);
                at.is_finite().then(|| (block_rect(at, b.w, b.h, h, v), h))
            })
            .filter(|(r, _)| finite_box(r));
        match stored {
            Some((rect, halign)) => {
                obs.add(rect);
                out.insert(b.rep, BlockPlace { rect, halign, moved: true, overlaps: false });
            }
            None => free.push(b),
        }
    }
    // Reading order: box top (highest first), then box left, then representation ID.
    free.sort_by(|a, b| b.hi.y.total_cmp(&a.hi.y).then(a.lo.x.total_cmp(&b.lo.x)).then(a.rep.cmp(&b.rep)));
    for (i, b) in free.into_iter().enumerate() {
        let gap = gap_factor * tag_height;
        let cands = candidates(b.lo, b.hi, b.vertical, b.w, b.h, gap);
        let search = i < MAX_PLACED_BLOCKS && obs.boxes.len() < MAX_OBSTACLES;
        let mut best: Option<(f64, usize)> = None;
        if search {
            for (k, (rect, _)) in cands.iter().enumerate() {
                if !finite_box(rect) {
                    continue;
                }
                // Outside the frame counts as fully covered: used only when nothing inside is.
                let area = if inside(rect) { obs.overlap(rect) } else { f64::MAX };
                if best.is_none_or(|(a, _)| area < a) {
                    best = Some((area, k));
                }
                if area == 0.0 {
                    break;
                }
            }
        }
        let (area, k) = best.unwrap_or((0.0, 0));
        let Some((rect, halign)) = cands.get(k).copied() else { continue };
        if !finite_box(&rect) {
            continue;
        }
        obs.add(rect);
        out.insert(b.rep, BlockPlace { rect, halign, moved: false, overlaps: search && area > 0.0 });
    }
    out
}

// ---------------------------------------------------------------- Runs

/// Most symbols on a page for which runs are worked out; beyond it every symbol counts as the
/// first of its own run (a guard on hostile pages: the grouping is quadratic in a line's symbols).
const MAX_RUN_SYMBOLS: usize = 20_000;

/// One symbol on a page, as the run grouping sees it.
#[derive(Debug, Clone, Copy)]
struct Placed {
    rep: RepresentationId,
    component: DeviceId,
    /// The run class: the same starter kind (strip terminals, PLC channels, or the starter itself).
    class: u64,
    centre: (f64, f64),
    lo: (f64, f64),
    hi: (f64, f64),
}

/// The symbols of `page` that are the first of their run: leftmost of a row,
/// topmost of a column, or a run of one. A run is a maximal set of symbols of one device, of one
/// starter kind, whose centres are within `text.run.align` across the line, whose neighbouring
/// boxes are at most `text.run.gap` apart along it, with no symbol of another device between
/// them. Rows are tried before columns.
pub(crate) fn run_firsts(doc: &Document, page: PageId) -> std::collections::BTreeSet<RepresentationId> {
    use std::collections::BTreeSet;
    let align = ecad_model::registry::resolve(doc, &d::RUN_ALIGN, Scope::project()).length(doc);
    let gap = ecad_model::registry::resolve(doc, &d::RUN_GAP, Scope::project()).length(doc);
    let mut all: Vec<Placed> = Vec::new();
    for (rid, r) in doc.representations_on(page) {
        let ecad_model::Subject::Device(c) = r.subject else { continue };
        if r.form != ecad_model::Form::Symbol {
            continue;
        }
        let Some((def, pl)) = crate::devices::tags::rep_definition(doc, r) else { continue };
        let Some((lo, hi)) = crate::devices::texts::placed_box(def, &pl) else { continue };
        let class = match crate::devices::texts::run_rule(doc, r) {
            // A connector's pins run per half, so each half tag shows once per run.
            RunRule::Strip => crate::connectors::run_class(doc, r).unwrap_or(1),
            RunRule::Plc => 2,
            RunRule::None => {
                // Other symbols: their starter (a definition ID otherwise), so only like with like.
                let key = crate::devices::tags::rep_starter(doc, r)
                    .map_or(0, |s| ecad_model::Word::word(s.key).bytes().fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b))));
                3 + key
            }
        };
        all.push(Placed { rep: rid, component: c, class, centre: ((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0), lo: (lo.x, lo.y), hi: (hi.x, hi.y) });
    }
    if all.len() > MAX_RUN_SYMBOLS {
        return all.iter().map(|p| p.rep).collect();
    }
    let mut placed_in_row: BTreeSet<RepresentationId> = BTreeSet::new();
    let mut firsts: BTreeSet<RepresentationId> = BTreeSet::new();
    // Groups: one device, one class.
    let mut groups: std::collections::BTreeMap<(DeviceId, u64), Vec<Placed>> = std::collections::BTreeMap::new();
    for p in &all {
        groups.entry((p.component, p.class)).or_default().push(*p);
    }
    for axis in [Axis::Row, Axis::Column] {
        for members in groups.values() {
            let free: Vec<Placed> = members.iter().copied().filter(|p| !placed_in_row.contains(&p.rep)).collect();
            for run in runs_along(&free, &all, axis, align, gap) {
                if run.len() < 2 {
                    continue;
                }
                if let Some(first) = run.first() {
                    firsts.insert(first.rep);
                }
                placed_in_row.extend(run.iter().map(|p| p.rep));
            }
        }
    }
    // Runs of one.
    for p in &all {
        if !placed_in_row.contains(&p.rep) {
            firsts.insert(p.rep);
        }
    }
    firsts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    /// A horizontal row: same y; first is the leftmost.
    Row,
    /// A vertical column: same x; first is the topmost.
    Column,
}

/// The runs of `members` (one device, one class) along `axis`, each in order from its first.
fn runs_along(members: &[Placed], all: &[Placed], axis: Axis, align: f64, gap: f64) -> Vec<Vec<Placed>> {
    // (across, along) coordinates; along grows from the first of a run.
    let key = |p: &Placed| match axis {
        Axis::Row => (p.centre.1, p.centre.0),
        Axis::Column => (p.centre.0, -p.centre.1),
    };
    // The box's extent along the line, in the same direction.
    let span = |p: &Placed| match axis {
        Axis::Row => (p.lo.0, p.hi.0),
        Axis::Column => (-p.hi.1, -p.lo.1),
    };
    let mut sorted: Vec<Placed> = members.to_vec();
    sorted.sort_by(|a, b| key(a).0.total_cmp(&key(b).0).then(key(a).1.total_cmp(&key(b).1)));
    // Lines: consecutive (by across) members within `align` of the line's first.
    let mut lines: Vec<Vec<Placed>> = Vec::new();
    for p in sorted {
        match lines.last_mut() {
            Some(line) if line.first().is_some_and(|f| (key(&p).0 - key(f).0).abs() <= align) => line.push(p),
            _ => lines.push(vec![p]),
        }
    }
    let mut out = Vec::new();
    for mut line in lines {
        line.sort_by(|a, b| key(a).1.total_cmp(&key(b).1));
        let across = line.first().map_or(0.0, |f| key(f).0);
        let Some(component) = line.first().map(|f| f.component) else { continue };
        // Other devices' symbols on this line, by their position along it.
        let mut others: Vec<f64> = all.iter().filter(|o| o.component != component && (key(o).0 - across).abs() <= align).map(|o| key(o).1).collect();
        others.sort_by(f64::total_cmp);
        let mut run: Vec<Placed> = Vec::new();
        for p in line {
            let joins = run.last().is_some_and(|prev| {
                let open = span(&p).0 - span(prev).1;
                let (a, b) = (key(prev).1, key(&p).1);
                let between = others.partition_point(|x| *x <= a) < others.partition_point(|x| *x < b);
                open <= gap && !between
            });
            if !joins && !run.is_empty() {
                out.push(std::mem::take(&mut run));
            }
            run.push(p);
        }
        if !run.is_empty() {
            out.push(run);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::Word;
    use ecad_model::registry::Value;
    use ecad_model::{Device, LengthUnit, Page, Representation};

    /// The built-in table, written out here so a reader and its entry cannot drift.
    const TABLE: [(TextKind, f64, Weight); 23] = [
        (TextKind::Tag, 2.5, Weight::Bold),
        (TextKind::PointLabel, 1.5, Weight::Regular),
        (TextKind::Description, 1.6, Weight::Regular),
        (TextKind::PartNumber, 1.6, Weight::Regular),
        (TextKind::ContinuationName, 2.0, Weight::Regular),
        (TextKind::Reference, 2.5, Weight::Regular),
        (TextKind::ContinuationCaption, 2.5, Weight::Regular),
        (TextKind::WireLabel, 2.5, Weight::Regular),
        (TextKind::CableText, 1.6, Weight::Regular),
        (TextKind::LocationLabel, 1.6, Weight::Regular),
        (TextKind::TitleValue, 2.5, Weight::Regular),
        (TextKind::TitleCaption, 1.8, Weight::Regular),
        (TextKind::TitleMain, 3.5, Weight::Bold),
        (TextKind::Grid, 3.0, Weight::Regular),
        (TextKind::Table, 2.5, Weight::Regular),
        (TextKind::TableHeader, 2.5, Weight::Bold),
        (TextKind::Free, 2.5, Weight::Regular),
        (TextKind::TitlePage, 7.0, Weight::Regular),
        (TextKind::TitleRev, 5.0, Weight::Regular),
        (TextKind::TableHeading, 3.5, Weight::Regular),
        (TextKind::Cover, 10.0, Weight::Regular),
        (TextKind::ChannelText, 2.5, Weight::Regular),
        (TextKind::Address, 2.0, Weight::Regular),
    ];

    #[test]
    fn an_empty_project_gets_the_built_in_of_every_kind() {
        let doc = Document::default();
        for (kind, h, w) in TABLE {
            let s = text_style(&doc, kind);
            assert_eq!(s, TextStyle { height: h, weight: w, style_name: w.style_name(), colour: None }, "{kind:?}");
            // The kind's built-in text style carries exactly these values.
            let b = crate::styles::text::CATALOGUE.builtin(&kind.builtin_style());
            assert_eq!(b.and_then(|b| b.value("height")), Some(Value::Length(h)), "{kind:?}");
            assert_eq!(<TextKind as ecad_model::Word>::parse(kind.word()), Some(kind));
            assert_eq!(crate::styles::text_style::style_entry(kind).key, format!("text.{}.style", kind.word()));
        }
    }

    #[test]
    fn hostile_heights_give_the_default() {
        // A stored height is a number; one outside the field's range counts as absent.
        for bad in [f64::NAN, 0.1, 11.0, 1e308, -2.0, f64::INFINITY] {
            let mut doc = Document::default();
            let body = ecad_model::objects::styles::TextStyle { height: Some(ecad_model::geometry::units::Mm(bad)), ..Default::default() };
            let rec = ecad_model::StyleRecord::new("", ecad_model::objects::styles::StyleBody::Text(body));
            doc.edit_parts(|parts| {
                parts.styles.entry(ecad_model::StyleCatalogue::Text).or_default().insert(TextKind::Tag.builtin_style(), rec);
            });
            let want = TextStyle { height: 2.5, weight: Weight::Bold, style_name: STYLE_BOLD, colour: None };
            assert_eq!(text_style(&doc, TextKind::Tag), want, "{bad:?}");
        }
    }

    #[test]
    fn old_031_properties_are_ignored() {
        // The old project keys are not read.
        let mut doc = Document::default();
        doc.edit_parts(|parts| {
            parts.display.insert("text.tag.height".into(), "3".into());
        });
        doc.edit_parts(|parts| {
            parts.display.insert("text.tag.weight".into(), "regular".into());
        });
        assert_eq!(text_style(&doc, TextKind::Tag), TextStyle { height: 2.5, weight: Weight::Bold, style_name: STYLE_BOLD, colour: None });
    }

    #[test]
    fn project_values_and_inch_projects() {
        let mut doc = Document::default();
        let body = ecad_model::objects::styles::TextStyle {
            height: Some(ecad_model::geometry::units::Mm(3.0)),
            weight: Some(Weight::Regular),
            ..Default::default()
        };
        let rec = ecad_model::StyleRecord::new("", ecad_model::objects::styles::StyleBody::Text(body));
        doc.edit_parts(|parts| {
            parts.styles.entry(ecad_model::StyleCatalogue::Text).or_default().insert(TextKind::Tag.builtin_style(), rec);
        });
        assert_eq!(text_style(&doc, TextKind::Tag), TextStyle { height: 3.0, weight: Weight::Regular, style_name: STYLE_REGULAR, colour: None });
        doc.edit_parts(|parts| {
            parts.settings.length_unit = LengthUnit::Inch;
        });
        assert!((text_style(&doc, TextKind::Tag).height - 3.0 / 25.4).abs() < 1e-12);
        doc.edit_parts(|parts| {
            parts.settings.wiring.label_height = Some(2.0);
        });
        assert!((text_style(&doc, TextKind::WireLabel).height - 2.0 / 25.4).abs() < 1e-12);
    }

    #[test]
    fn shown_resolves_symbol_device_page_project_and_runs() {
        let mut doc = Document::default();
        let page = PageId(1);
        let comp = DeviceId(2);
        let rep = RepresentationId(3);
        doc.edit_parts(|parts| {
            parts.pages.insert(page, Page::titled("p").unwrap());
        });
        doc.edit_parts(|parts| {
            parts.devices.insert(comp, Device::default());
        });
        doc.edit_parts(|parts| {
            parts.representations.insert(
                rep,
                Representation {
                    subject: ecad_model::Subject::Device(comp),
                    page,
                    form: ecad_model::Form::Symbol,
                    role: ecad_model::Role::default(),
                    items: Vec::new(),
                    connection_points: Default::default(),
                },
            );
        });
        let plain = TextTarget::symbol(&doc, rep, RunRule::None);
        let strip = TextTarget::symbol(&doc, rep, RunRule::Strip);
        assert_eq!(plain.component, Some(comp));
        assert_eq!(plain.page, Some(page));
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        assert_eq!(text_shown(&doc, strip, TextKind::Tag), Shown::First);
        assert_eq!(text_shown(&doc, plain, TextKind::Description), Shown::Off);
        assert_eq!(text_shown(&doc, plain, TextKind::ContinuationName), Shown::Off);
        assert_eq!(text_shown(&doc, plain, TextKind::Grid), Shown::On);
        // A project-wide tag rule overrides the run rule; `first` is refused for other kinds.
        doc.edit_parts(|parts| {
            parts.display.insert("text.tag.show".into(), "no".into());
        });
        doc.edit_parts(|parts| {
            parts.display.insert("text.description.show".into(), "first".into());
        });
        assert_eq!(text_shown(&doc, strip, TextKind::Tag), Shown::Off);
        assert_eq!(text_shown(&doc, plain, TextKind::Description), Shown::Off);
        doc.edit_parts(|parts| {
            parts.pages.get_mut(&page).map(|p| p.display.insert("text.tag.show".into(), "yes".into()));
        });
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert("text.tag.show".into(), "no".into()));
        });
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::Off);
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert(ecad_model::registry::symbol_key("text.tag.show", rep), "yes".into()));
        });
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        // The project may turn the strip run rule off.
        let mut doc2 = Document::default();
        doc2.edit_parts(|parts| {
            parts.display.insert("text.tag.show.strip".into(), "yes".into());
        });
        assert_eq!(text_shown(&doc2, TextTarget { run: RunRule::Strip, ..TextTarget::default() }, TextKind::Tag), Shown::On);
    }
}
