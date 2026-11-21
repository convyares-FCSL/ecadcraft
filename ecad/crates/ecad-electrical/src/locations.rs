//! Device locations and functions, and location boxes (briefs 024 and 052, design 052 §5).
//!
//! # Semantics
//! - A device's **location** (IEC 81346 `+` aspect) and **function** (`=`) are what is stored on
//!   it: its `location` and `function` properties, each an entry key of the project's list
//!   ([`crate::designations`]). Unset means none, shown `?`. Nothing is inherited from boxes,
//!   pages or the project (design 052 §5.1, FP2): those only **propose** ([`context_in`]).
//! - A representation's location ([`rep_location`], §5.2): a strip's for every position and level;
//!   else the device's override `location.<type>` (`socket`/`plug` for connector halves); else the
//!   device's.
//! - A symbol's **context** for an aspect is the innermost box carrying that aspect at its anchor,
//!   else its page's value, else the project's. Its tag shows the device's value as a prefix where
//!   the context differs (`=F1+CP1-K01`, [`shown_tag`]); that reads the symbol's own page only.
//! - A **location box** is a drawing aid ([`ecad_model::Aid::Box`], design 072 §6.2) carrying a
//!   `location`, a `function` or both (FP9), drawn as one closed polyline item (layer [`LAYER`])
//!   of a drawn representation with role [`ecad_model::Role::LocationBox`] (design 024 FP2). Its
//!   label is derived.
//! - Containment ([`contains`]) is point in polygon by the even-odd rule; a point within
//!   [`EDGE_TOL`] of the outline counts as inside. Innermost is the smallest polygon area, ties by
//!   aid ID.
//!
//! # Update map (design 024 §9)
//! Every function here reads one page's boxes and the representations asked about, never the
//! whole project. Callers decide when to ask: the page drawing on every draw of the active page
//! (tag prefixes and box labels), [`crate::outputs`] for the devices a change could move.
//!
//! This module records nothing; [`plan_box`] and [`plan_from_item`] only build ops.

use std::collections::BTreeMap;

use crate::CmdError;
use ecad_model::book::LOCATION_KEY;
use ecad_model::{
    Aid, AidId, Color, DeviceId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Layer, LayerId, LocationBox, Op, PageId, Primitive, Pt,
    Representation, RepresentationId, Style, Subject,
};

use crate::designations::{self, Aspect};

/// A location box's panel temperature key (`setprop {aids}`, design 072 §6.2).
pub const PANEL_TEMPERATURE_KEY: &str = "panel_temperature";
/// A location box's own location style key (`setprop {aids}`, brief 050a, design 072 §6.2).
pub const BOX_STYLE_KEY: &str = "style";
/// The reserved layer of location boxes (`ecad.location-box`).
pub const LAYER: &str = "ECAD-LOCATIONS";
/// The linetype of a box outline.
pub const LINETYPE: &str = "DASHED";
/// How far the label sits inside the top-left vertex.
pub const LABEL_INSET: f64 = 1.5;
/// Smallest outline area (mm²): 10 x 10 mm.
pub const MIN_AREA: f64 = 100.0;
/// A point this close to the outline is inside.
pub const EDGE_TOL: f64 = 0.01;
/// Most vertices of one box.
pub const MAX_VERTICES: usize = 10_000;
/// Refusal of a too small outline (design 024 §3.5).
pub const TOO_SMALL: &str = "A location box needs at least 10 x 10 mm.";
/// Refusal of an item that is not a closed polyline.
pub const NOT_CLOSED: &str = "Pick a closed polyline (an open one can be closed with PEDIT first).";

/// One location box on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct LocBox {
    pub aid: AidId,
    pub rep: RepresentationId,
    pub item: ItemId,
    pub page: PageId,
    /// Its location's code (`None`: no location, or a key with no entry).
    pub location: Option<String>,
    /// Its stored location key.
    pub location_key: Option<String>,
    /// Its function's code.
    pub function: Option<String>,
    /// Its stored function key.
    pub function_key: Option<String>,
    pub vertices: Vec<Pt>,
    pub lo: Pt,
    pub hi: Pt,
    pub area: f64,
}

/// A device in the sense of locations and the BOM (design 024 §6.1): a component with a tag that
/// is not a BOM item.
pub fn is_device(doc: &Document, c: DeviceId) -> bool {
    doc.devices.get(&c).is_some_and(|x| x.tag.is_some() && !crate::bom::is_item_component(x))
}

/// Location box `a`'s values, when `a` is one.
pub fn box_values(doc: &Document, a: AidId) -> Option<&LocationBox> {
    doc.aids.get(&a).and_then(Aid::as_box)
}

/// The box representation `r` (`rid`) stands for, if it is one with a drawable outline.
pub fn box_of_rep(doc: &Document, rid: RepresentationId, r: &Representation) -> Option<LocBox> {
    let Subject::Aid(c) = r.subject else { return None };
    if r.form != Form::Drawn || r.role != ecad_model::Role::LocationBox {
        return None;
    }
    let values = box_values(doc, c)?;
    let item = *r.items.first()?;
    let vertices = match &doc.items.get(&item)?.kind {
        ItemKind::Primitive(Primitive::Polyline { vertices, closed: true }) => vertices.clone(),
        _ => return None,
    };
    if vertices.len() < 3 || vertices.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (lo, hi) = bounds(&vertices);
    let location_key = designations::box_key(values, Aspect::Location);
    let function_key = designations::box_key(values, Aspect::Function);
    Some(LocBox {
        aid: c,
        rep: rid,
        item,
        page: r.page,
        location: designations::shown(doc, Aspect::Location, location_key.as_deref()),
        function: designations::shown(doc, Aspect::Function, function_key.as_deref()),
        location_key,
        function_key,
        area: area(&vertices),
        vertices,
        lo,
        hi,
    })
}

/// Every location box on `page` (reads that page's representations only).
pub fn boxes_on(doc: &Document, page: PageId) -> Vec<LocBox> {
    doc.representations_on(page).filter_map(|(rid, r)| box_of_rep(doc, rid, r)).collect()
}

/// Bounding box of a point list.
pub fn bounds(v: &[Pt]) -> (Pt, Pt) {
    let mut lo = Pt::new(f64::INFINITY, f64::INFINITY);
    let mut hi = Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in v {
        lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo, hi)
}

/// The area of a polygon (shoelace, absolute).
pub fn area(v: &[Pt]) -> f64 {
    let n = v.len();
    let mut s = 0.0;
    for i in 0..n {
        let (Some(a), Some(b)) = (v.get(i), v.get((i + 1) % n)) else { continue };
        s += a.x * b.y - b.x * a.y;
    }
    (s / 2.0).abs()
}

fn dist_to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.dist(Pt::new(a.x + t * dx, a.y + t * dy))
}

/// Whether `p` is inside the closed outline `v` (even-odd rule; on the outline counts as inside).
pub fn contains(v: &[Pt], p: Pt) -> bool {
    let n = v.len();
    if n < 3 || !p.is_finite() {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let (Some(a), Some(b)) = (v.get(i), v.get((i + 1) % n)) else { continue };
        if dist_to_segment(p, *a, *b) <= EDGE_TOL {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Whether the closed outline `v` crosses itself (two non-adjacent segments intersect).
pub fn crosses_itself(v: &[Pt]) -> bool {
    let n = v.len();
    if n < 4 {
        return false;
    }
    let seg = |i: usize| (v.get(i).copied(), v.get((i + 1) % n).copied());
    let orient = |a: Pt, b: Pt, c: Pt| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let ((Some(a), Some(b)), (Some(c), Some(d))) = (seg(i), seg(j)) else { continue };
            let (d1, d2, d3, d4) = (orient(c, d, a), orient(c, d, b), orient(a, b, c), orient(a, b, d));
            if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
                return true;
            }
        }
    }
    false
}

/// A representation's anchor (design 024 §3.1): a symbol's insertion point; for a drawn one its
/// first connection point, else its first item's first point.
pub fn anchor(doc: &Document, r: &Representation) -> Option<Pt> {
    let first = r.items.first().and_then(|i| doc.items.get(i));
    match r.form {
        Form::Symbol => first.and_then(|i| match &i.kind {
            ItemKind::Symbol(s) => Some(s.placement.origin),
            ItemKind::Primitive(_) => None,
        }),
        Form::Drawn => r.connection_points.values().next().copied().or_else(|| first.and_then(|i| first_point(&i.kind))),
    }
    .filter(|p| p.is_finite())
}

fn first_point(k: &ItemKind) -> Option<Pt> {
    match k {
        ItemKind::Symbol(s) => Some(s.placement.origin),
        ItemKind::Primitive(p) => match p {
            Primitive::Line { a, .. } => Some(*a),
            Primitive::Polyline { vertices, .. } => vertices.first().copied(),
            Primitive::Arc { center, .. } | Primitive::Circle { center, .. } => Some(*center),
            Primitive::Text { at, .. } | Primitive::Point { at } => Some(*at),
        },
    }
}

/// The innermost of `boxes` holding `p` (smallest area, then aid ID).
pub fn innermost(boxes: &[LocBox], p: Pt) -> Option<&LocBox> {
    innermost_where(boxes, p, |_| true)
}

/// The innermost of `boxes` carrying aspect `a` and holding `p` (design 052 §5.8: a box without a
/// function is ignored when looking for the function context, and the other way round).
pub fn innermost_in(boxes: &[LocBox], a: Aspect, p: Pt) -> Option<&LocBox> {
    innermost_where(boxes, p, |b| b.key(a).is_some())
}

fn innermost_where(boxes: &[LocBox], p: Pt, keep: impl Fn(&LocBox) -> bool) -> Option<&LocBox> {
    boxes
        .iter()
        .filter(|b| keep(b))
        .filter(|b| p.x >= b.lo.x - EDGE_TOL && p.x <= b.hi.x + EDGE_TOL && p.y >= b.lo.y - EDGE_TOL && p.y <= b.hi.y + EDGE_TOL)
        .filter(|b| contains(&b.vertices, p))
        .min_by(|a, b| a.area.total_cmp(&b.area).then(a.aid.cmp(&b.aid)))
}

impl LocBox {
    /// Its stored key for aspect `a`.
    pub fn key(&self, a: Aspect) -> Option<&str> {
        match a {
            Aspect::Function => self.function_key.as_deref(),
            Aspect::Location => self.location_key.as_deref(),
        }
    }

    /// Its code for aspect `a`.
    pub fn code(&self, a: Aspect) -> Option<&str> {
        match a {
            Aspect::Function => self.function.as_deref(),
            Aspect::Location => self.location.as_deref(),
        }
    }
}

/// Where a value comes from: stored ([`Source::Device`], [`Source::Override`]) or, for a context
/// (a proposal), a box, a page or the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// The device's own value.
    Device,
    /// The device's override for a type of its symbols (`location.<type>`).
    Override,
    /// A location box on a page.
    Box { aid: AidId, page: PageId },
    /// A page's `location`.
    Page(PageId),
    /// The project's `location`.
    Project,
    /// Nowhere: no location.
    None,
}

impl Source {
    /// The JSON word (`device`, `box`, `page`, `project`, `none`).
    pub fn word(self) -> &'static str {
        match self {
            Source::Device => "device",
            Source::Override => "override",
            Source::Box { .. } => "box",
            Source::Page(_) => "page",
            Source::Project => "project",
            Source::None => "none",
        }
    }
}

/// A location (or function) and where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Located {
    /// The code shown (`None`: unset, or a stored key with no entry).
    pub value: Option<String>,
    pub source: Source,
    /// The stored entry key.
    pub key: Option<String>,
}

impl Located {
    fn none() -> Located {
        Located { value: None, source: Source::None, key: None }
    }

    fn of(doc: &Document, a: Aspect, key: &str, source: Source) -> Located {
        Located { value: designations::code_of(doc, a, key), source, key: Some(key.to_string()) }
    }

    /// Whether a value is stored but names no entry of the list (design 052 §5.9).
    pub fn missing(&self) -> bool {
        self.key.is_some() && self.value.is_none()
    }

    /// `(from box on 105)`, `(from page 105)`, `(project)`; empty for an own value or none.
    pub fn from_words(&self, doc: &Document) -> String {
        match self.source {
            Source::Box { page, .. } => format!("(from box on {})", ecad_model::book::page_number(doc, page)),
            Source::Page(p) => format!("(from page {})", ecad_model::book::page_number(doc, p)),
            Source::Project => "(project)".into(),
            Source::Device | Source::Override | Source::None => String::new(),
        }
    }

    /// The value as shown: `+CP1 (from page 105)`, `+CP1`, or `?`.
    pub fn shown(&self, doc: &Document) -> String {
        match &self.value {
            None => "?".into(),
            Some(v) => {
                let w = self.from_words(doc);
                if w.is_empty() { v.clone() } else { format!("{v} {w}") }
            }
        }
    }

    /// The value, or `?`.
    pub fn text(&self) -> String {
        self.value.clone().unwrap_or_else(|| "?".into())
    }
}

/// The boxes of each page asked about, read once per page.
#[derive(Debug, Default)]
pub struct BoxCache {
    pages: BTreeMap<PageId, Vec<LocBox>>,
}

impl BoxCache {
    /// The boxes on `page`.
    pub fn on(&mut self, doc: &Document, page: PageId) -> &[LocBox] {
        self.pages.entry(page).or_insert_with(|| boxes_on(doc, page))
    }
}

/// The location context of a point `at` on `page` with that page's `boxes` (design 024 §3.1):
/// what is proposed there.
pub fn context_at(doc: &Document, page: PageId, at: Option<Pt>, boxes: &[LocBox]) -> Located {
    context_in(doc, Aspect::Location, page, at, boxes)
}

/// The context of aspect `a` at `at` on `page` (design 052 §5.3, §5.8): the innermost box
/// carrying `a`, else the page's value, else the project's.
pub fn context_in(doc: &Document, a: Aspect, page: PageId, at: Option<Pt>, boxes: &[LocBox]) -> Located {
    if let Some(b) = at.and_then(|p| innermost_in(boxes, a, p))
        && let Some(k) = b.key(a)
    {
        return Located::of(doc, a, k, Source::Box { aid: b.aid, page });
    }
    if let Some(k) = doc.pages.get(&page).and_then(|p| designations::page_key(p, a)) {
        return Located::of(doc, a, &k, Source::Page(page));
    }
    if let Some(k) = designations::default_key(&doc.project, a) {
        return Located::of(doc, a, &k, Source::Project);
    }
    Located::none()
}

/// The location context of representation `r`.
pub fn rep_context(doc: &Document, r: &Representation, cache: &mut BoxCache) -> Located {
    rep_context_in(doc, Aspect::Location, r, cache)
}

/// The context of aspect `a` of representation `r`.
pub fn rep_context_in(doc: &Document, a: Aspect, r: &Representation, cache: &mut BoxCache) -> Located {
    let at = anchor(doc, r);
    let boxes = cache.on(doc, r.page);
    context_in(doc, a, r.page, at, boxes)
}

/// The box (if any) holding the anchor of `r`.
pub fn box_holding(doc: &Document, r: &Representation, cache: &mut BoxCache) -> Option<LocBox> {
    let at = anchor(doc, r)?;
    innermost(cache.on(doc, r.page), at).cloned()
}

/// The code of the one context of aspect `a` all of device `c`'s symbols share, if they share
/// one that has a value (the Checks row's **Use it**, design 052 §6).
pub fn shared_context(doc: &Document, c: DeviceId, a: Aspect) -> Option<String> {
    let mut cache = BoxCache::default();
    let mut shared: Option<Located> = None;
    for (_, r) in doc.representations_of(Subject::Device(c)) {
        let x = rep_context_in(doc, a, r, &mut cache);
        match &shared {
            None => shared = Some(x),
            Some(s) if s.key != x.key => return None,
            Some(_) => {}
        }
    }
    shared.and_then(|s| s.value)
}

/// Device `c`'s stored value of aspect `a` (design 052 §5.1): its own field, else none.
pub fn resolve_in(doc: &Document, c: DeviceId, a: Aspect) -> Located {
    match doc.devices.get(&c).and_then(|x| designations::device_key(x, a)) {
        Some(k) => Located::of(doc, a, &k, Source::Device),
        None => Located::none(),
    }
}

/// Device `c`'s location: its stored `location`, else none (no inheritance, FP2). `cache` is not
/// read; it stays for callers that also read contexts.
pub fn resolve_with(doc: &Document, c: DeviceId, _cache: &mut BoxCache) -> Located {
    resolve_in(doc, c, Aspect::Location)
}

/// Device `c`'s location.
pub fn resolve(doc: &Document, c: DeviceId) -> Located {
    resolve_in(doc, c, Aspect::Location)
}

/// Device `c`'s function.
pub fn resolve_function(doc: &Document, c: DeviceId) -> Located {
    resolve_in(doc, c, Aspect::Function)
}

/// The override property of representation `r` (design 052 §5.2): `location.socket` or
/// `location.plug` for a connector half or pin (a pin pair uses the socket's), else
/// `location.<type>`; `None` for a head.
pub fn override_key(doc: &Document, r: &Representation) -> Option<String> {
    use crate::symbols::Side;
    if let Some(side) = crate::connectors::rep_side(doc, r) {
        return Some(format!("{LOCATION_KEY}.{}", if side == Side::Plug { "plug" } else { "socket" }));
    }
    let t = crate::reps::rep_type(doc, r);
    (t != ecad_model::PartRole::Head).then(|| format!("{LOCATION_KEY}.{}", ecad_model::Word::word(t)))
}

/// Whether `t` may name an override (design 052 FP3): a §3.1 type word other than `head`, or
/// `socket` / `plug`.
pub fn override_type_ok(t: &str) -> bool {
    matches!(t, "socket" | "plug") || <ecad_model::PartRole as ecad_model::Word>::parse(t).is_some_and(|x| x != ecad_model::PartRole::Head)
}

/// Representation `r`'s location (design 052 §5.2), first match wins: a strip's own (a strip has
/// no overrides); the device's override for `r`'s type; the device's; none.
pub fn rep_location(doc: &Document, c: DeviceId, r: &Representation) -> Located {
    if crate::strips::strip_of(doc, c).is_some() {
        return resolve(doc, c);
    }
    let props = doc.devices.get(&c).map(|x| &x.properties);
    if let Some(k) = override_key(doc, r).and_then(|key| props.and_then(|p| p.get(&key))) {
        return Located::of(doc, Aspect::Location, k, Source::Override);
    }
    resolve(doc, c)
}

/// The location of half `side` of connector `c` (design 052 §5.2): `location.socket` /
/// `location.plug`, else the connector's.
pub fn half_location(doc: &Document, c: DeviceId, side: crate::symbols::Side) -> Located {
    let key = format!("{LOCATION_KEY}.{}", if side == crate::symbols::Side::Plug { "plug" } else { "socket" });
    match doc.devices.get(&c).and_then(|x| x.properties.get(&key)) {
        Some(k) => Located::of(doc, Aspect::Location, k, Source::Override),
        None => resolve(doc, c),
    }
}

/// The tag a symbol representation `r` of device `c` shows (design 024 §3.3, design 052 §5.8):
/// the tag after the device's function and its representation's location, each where the
/// symbol's context for that aspect differs (`=F1+CP1-K01`). Reads `r`'s page only.
pub fn shown_tag(doc: &Document, c: DeviceId, r: &Representation, tag: &str, cache: &mut BoxCache) -> String {
    let mut prefix = String::new();
    for a in Aspect::ALL {
        let own = match a {
            Aspect::Function => resolve_function(doc, c),
            Aspect::Location => rep_location(doc, c, r),
        };
        let (Some(key), Some(code)) = (own.key, own.value) else { continue };
        if rep_context_in(doc, a, r, cache).key.as_deref() != Some(key.as_str()) {
            prefix.push_str(&code);
        }
    }
    if prefix.is_empty() { tag.to_string() } else { located_tag(&prefix, tag) }
}

/// `tag` after location `loc` (`+X1`). A tag stored without its `-` (the project's tag dash off)
/// gets it back here: IEC 81346 separates the location and product aspects by their prefix signs,
/// and `+X1K01` would read as one location (pack re-run, bug 3).
pub fn located_tag(loc: &str, tag: &str) -> String {
    let t = tag.trim_start();
    if t.is_empty() || t.starts_with(['-', '=', '+']) { format!("{loc}{t}") } else { format!("{loc}-{t}") }
}

/// The label of a box at its top-left vertex (greatest y, then least x): `+CP1`, `=F1`, or both
/// (`=F1 +CP1`); a key with no entry shows its sign and `?`.
pub fn label_of(b: &LocBox) -> (String, Pt) {
    let tl = b.vertices.iter().copied().max_by(|p, q| p.y.total_cmp(&q.y).then(q.x.total_cmp(&p.x))).unwrap_or(b.lo);
    let part = |a: Aspect| b.key(a).map(|_| b.code(a).map_or_else(|| format!("{}?", a.sign()), str::to_string));
    let text = [part(Aspect::Function), part(Aspect::Location)].into_iter().flatten().collect::<Vec<_>>().join(" ");
    (if text.is_empty() { "+?".into() } else { text }, Pt::new(tl.x + LABEL_INSET, tl.y - LABEL_INSET))
}

/// The style of box `b` on layer `layer`: dash-dot when it carries a function (design 052 §5.8,
/// `ecad.function-box.linetype`), else dashed.
fn box_style_for(layer: LayerId, b: &LocationBox) -> Style {
    let mut s = box_style(layer);
    if b.function.is_some() {
        s.linetype = FUNCTION_LINETYPE.into();
    }
    s
}

/// The linetype of a box carrying a function (design 052 §9.5, our own definition).
pub const FUNCTION_LINETYPE: &str = "DASHDOT";

/// The style of a box outline on layer `layer`.
fn box_style(layer: LayerId) -> Style {
    Style { layer, color: Color::ByLayer, linetype: LINETYPE.into(), lineweight: crate::words::WIRE_WEIGHT as i16 }
}

/// The ID of layer [`LAYER`], with the op that adds it when the document has none.
fn layer_ops(doc: &Document, ids: &mut IdAllocator) -> (LayerId, Vec<Op>) {
    if let Some((id, _)) = doc.layers.iter().find(|(_, l)| l.name == LAYER) {
        return (*id, Vec::new());
    }
    let id: LayerId = ids.fresh();
    (id, vec![Op::AddLayer { id, layer: Layer { name: LAYER.into() } }])
}

/// A location value as stored: trimmed, with a leading `+`.
///
/// # Errors
/// Empty, too long or a control character.
pub fn location_value(doc: &Document, text: &str) -> Result<String, CmdError> {
    crate::devices::property_value(doc, LOCATION_KEY, Some(text))?.ok_or(CmdError::refused("A location box needs a location, such as +CP1."))
}

/// Check an outline for a box.
///
/// # Errors
/// Fewer than three points, a non-finite point, too many points, or too small.
pub fn check_outline(v: &[Pt]) -> Result<(), CmdError> {
    if v.len() < 3 {
        return Err(CmdError::refused("A location box needs at least three points."));
    }
    if v.len() > MAX_VERTICES {
        return Err(CmdError::refused(format!("A location box has at most {MAX_VERTICES} points.")));
    }
    if v.iter().any(|p| !p.is_finite()) {
        return Err(CmdError::refused("Every point must be a finite [x, y]."));
    }
    if area(v) < MIN_AREA {
        return Err(CmdError::refused(TOO_SMALL));
    }
    Ok(())
}

/// What a planned box will be.
#[derive(Debug, Clone)]
pub struct Planned {
    pub aid: AidId,
    pub rep: RepresentationId,
    pub item: ItemId,
    pub ops: Vec<Op>,
    /// The echo for codes the box added to the project's list (design 052 §5.9).
    pub added: Option<String>,
}

/// A new box for `values` typed per aspect (design 052 FP9): at least one of function and
/// location, each stored as an entry's id ([`designations::Pending`]), typed so fixed.
fn new_box(
    doc: &Document, ids: &mut IdAllocator, values: &[(Aspect, &str)], pending: &mut designations::Pending, ops: &mut Vec<Op>,
) -> Result<LocationBox, CmdError> {
    let mut b = LocationBox::default();
    for (a, text) in values {
        if text.trim().is_empty() {
            continue;
        }
        let key = pending.store(doc, ids, *a, text, ops)?;
        b = designations::with_box_key(&b, *a, Some(&key));
    }
    if b.location.is_none() && b.function.is_none() {
        return Err(CmdError::refused(NO_VALUE));
    }
    Ok(b)
}

/// Refusal of a box with neither a function nor a location (design 052 §5.8).
pub const NO_VALUE: &str = "A box needs a function or a location, such as =F1 or +CP1.";

/// The ops of a new location box `location` drawn on `page` with outline `vertices`
/// (design 024 §7): `CreateAid`, `AddItem` (closed polyline), `LinkRepresentation`.
///
/// # Errors
/// As [`plan_box_with`].
pub fn plan_box(doc: &Document, ids: &mut IdAllocator, page: PageId, location: &str, vertices: Vec<Pt>) -> Result<Planned, CmdError> {
    plan_box_with(doc, ids, page, &[(Aspect::Location, location)], vertices)
}

/// The ops of a new box carrying `values` (a function, a location or both, design 052 FP9),
/// drawn on `page` with outline `vertices`. A code not in the project's list adds its entry in
/// the same ops.
///
/// # Errors
/// As [`check_outline`]; a value that is not a valid code; neither value given.
pub fn plan_box_with(doc: &Document, ids: &mut IdAllocator, page: PageId, values: &[(Aspect, &str)], vertices: Vec<Pt>) -> Result<Planned, CmdError> {
    if !doc.pages.contains_key(&page) {
        return Err(CmdError::refused(crate::msg::NO_SUCH_PAGE));
    }
    check_outline(&vertices)?;
    let mut pending = designations::Pending::default();
    let mut list_ops = Vec::new();
    let values = new_box(doc, ids, values, &mut pending, &mut list_ops)?;
    let (layer, mut ops) = layer_ops(doc, ids);
    ops.extend(list_ops);
    let aid: AidId = ids.fresh();
    let item: ItemId = ids.fresh();
    let rep: RepresentationId = ids.fresh();
    let style = box_style_for(layer, &values);
    ops.push(Op::CreateAid { id: aid, aid: Aid::Box(values) });
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Polyline { vertices, closed: true }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Aid(aid),
            page,
            form: Form::Drawn,
            role: ecad_model::Role::LocationBox,
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(Planned { aid, rep, item, ops, added: pending.words() })
}

/// The ops that turn the existing closed polyline `item` into location box `location`
/// (design 024 §3.5, §7).
///
/// # Errors
/// As [`plan_from_item_with`].
pub fn plan_from_item(doc: &Document, ids: &mut IdAllocator, item: ItemId, location: &str) -> Result<Planned, CmdError> {
    plan_from_item_with(doc, ids, item, &[(Aspect::Location, location)])
}

/// The ops that turn the existing closed polyline `item` into a box carrying `values`.
///
/// # Errors
/// Not a closed polyline, part of another object, or as [`plan_box_with`].
pub fn plan_from_item_with(doc: &Document, ids: &mut IdAllocator, item: ItemId, values: &[(Aspect, &str)]) -> Result<Planned, CmdError> {
    let it = doc.items.get(&item).ok_or(CmdError::refused("no such item"))?;
    if let Some((_, r)) = doc.representation_of_item(item) {
        let who = match r.subject {
            Subject::Device(c) => crate::devices::tag_words(doc, c),
            Subject::Wire(_) | Subject::Route(_) => "a wire".into(),
            Subject::Cable(_) => "a cable".into(),
            Subject::Aid(a) => match doc.aids.get(&a).map(ecad_model::Aid::kind) {
                Some(ecad_model::AidKind::Box) => "another location box".to_string(),
                Some(ecad_model::AidKind::Continuation) | None => "a continuation".into(),
            },
        };
        return Err(CmdError::refused(format!("That outline belongs to {who}; draw a box instead.")));
    }
    let vertices = match &it.kind {
        ItemKind::Primitive(Primitive::Polyline { vertices, closed: true }) => vertices.clone(),
        _ => return Err(CmdError::refused(NOT_CLOSED)),
    };
    check_outline(&vertices)?;
    let mut pending = designations::Pending::default();
    let mut list_ops = Vec::new();
    let values = new_box(doc, ids, values, &mut pending, &mut list_ops)?;
    let (layer, mut ops) = layer_ops(doc, ids);
    ops.extend(list_ops);
    let aid: AidId = ids.fresh();
    let rep: RepresentationId = ids.fresh();
    let after = Item { style: box_style_for(layer, &values), ..it.clone() };
    ops.push(Op::CreateAid { id: aid, aid: Aid::Box(values) });
    if after != *it {
        ops.push(Op::ModifyItem { id: item, before: it.clone(), after });
    }
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Aid(aid),
            page: it.page,
            form: Form::Drawn,
            role: ecad_model::Role::LocationBox,
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(Planned { aid, rep, item, ops, added: pending.words() })
}

/// The box a [`Planned`] box will be, read from its ops (for proposals before it is committed).
pub fn planned_box(doc: &Document, p: &Planned) -> Option<LocBox> {
    let mut props = None;
    let mut vertices = None;
    let mut page = None;
    for op in &p.ops {
        match op {
            Op::CreateAid { id, aid: Aid::Box(b) } if *id == p.aid => props = Some(b.clone()),
            Op::AddItem { id, item } | Op::ModifyItem { id, after: item, .. } if *id == p.item => {
                page = Some(item.page);
                if let ItemKind::Primitive(Primitive::Polyline { vertices: v, .. }) = &item.kind {
                    vertices = Some(v.clone());
                }
            }
            _ => {}
        }
    }
    let (props, page) = (props?, page.or_else(|| doc.items.get(&p.item).map(|i| i.page))?);
    let vertices = vertices.or_else(|| match &doc.items.get(&p.item)?.kind {
        ItemKind::Primitive(Primitive::Polyline { vertices, .. }) => Some(vertices.clone()),
        _ => None,
    })?;
    let (lo, hi) = bounds(&vertices);
    let location_key = designations::box_key(&props, Aspect::Location);
    let function_key = designations::box_key(&props, Aspect::Function);
    // A code the box adds to the list is in the ops, not in `doc` yet.
    let code = |a: Aspect, k: &Option<String>| {
        k.as_ref().and_then(|k| {
            designations::code_of(doc, a, k).or_else(|| {
                p.ops.iter().find_map(|op| match op {
                    Op::ModifyProject { after, .. } => designations::code_in_project(after, a, k),
                    _ => None,
                })
            })
        })
    };
    Some(LocBox {
        aid: p.aid,
        rep: p.rep,
        item: p.item,
        page,
        location: code(Aspect::Location, &location_key),
        function: code(Aspect::Function, &function_key),
        location_key,
        function_key,
        area: area(&vertices),
        vertices,
        lo,
        hi,
    })
}

/// Why changing item `id` of `doc` to `after` is refused (tester 024, owner ruling): a location
/// box's outline cannot be opened or cut below three points. The words give the way out.
pub fn reshape_refusal(doc: &Document, id: ItemId, after: &Item) -> Option<String> {
    let (_, r) = doc.representation_of_item(id)?;
    let Subject::Aid(c) = r.subject else { return None };
    if r.role != ecad_model::Role::LocationBox || box_values(doc, c).is_none() {
        return None;
    }
    let ok = matches!(&after.kind, ItemKind::Primitive(Primitive::Polyline { vertices, closed: true }) if vertices.len() >= 3);
    if ok {
        return None;
    }
    let loc = boxes_on(doc, r.page).into_iter().find(|b| b.aid == c).map_or_else(|| "+?".to_string(), |b| label_of(&b).0);
    Some(format!(
        "Location box {loc} is a closed outline of at least three points, so it cannot be opened or cut down to a line. \
         To use a plain polyline instead, erase the box (its outline or its {loc} label; U brings it back) and draw the polyline."
    ))
}

/// How many device symbols on `b`'s page are inside it, and how many of those keep an own
/// location that differs (for the echo and the box's Properties).
pub fn inside_counts(doc: &Document, b: &LocBox) -> (usize, usize) {
    let mut cache = BoxCache::default();
    let (mut n, mut keeps) = (0, 0);
    for (_, r) in doc.representations_on(b.page) {
        let Subject::Device(c) = r.subject else { continue };
        if !is_device(doc, c) {
            continue;
        }
        let Some(at) = anchor(doc, r) else { continue };
        if innermost(cache.on(doc, b.page), at).map(|x| x.aid) != Some(b.aid) {
            continue;
        }
        n += 1;
        if b.location_key.is_some()
            && doc
                .devices
                .get(&c)
                .and_then(|x| designations::device_key(x, Aspect::Location))
                .is_some_and(|own| Some(&own) != b.location_key.as_ref())
        {
            keeps += 1;
        }
    }
    (n, keeps)
}

/// The JSON of a device's location (`devices` results gain these, design 024 §3.4).
pub fn json_fields(doc: &Document, c: DeviceId) -> (serde_json::Value, &'static str) {
    let l = resolve(doc, c);
    (l.value.map_or(serde_json::Value::Null, serde_json::Value::String), l.source.word())
}

/// Natural order of two location or tag texts: digit runs compare by value (`+CP2` < `+CP10`).
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    natural_key(a).cmp(&natural_key(b))
}

/// The key of [`natural_cmp`].
pub fn natural_key(s: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                out.push((std::mem::take(&mut text), digits.parse().unwrap_or(u64::MAX)));
                digits.clear();
            }
            text.extend(ch.to_lowercase());
        }
    }
    out.push((text, if digits.is_empty() { 0 } else { digits.parse().unwrap_or(u64::MAX) }));
    out
}

/// The labels of the boxes on `page`: (box representation, text, top-left point).
pub fn labels_on_page(doc: &Document, page: PageId) -> Vec<(RepresentationId, String, Pt)> {
    boxes_on(doc, page)
        .iter()
        .map(|b| {
            let (t, at) = label_of(b);
            (b.rep, t, at)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
        vec![Pt::new(x0, y0), Pt::new(x1, y0), Pt::new(x1, y1), Pt::new(x0, y1)]
    }

    #[test]
    fn containment_is_even_odd_with_the_outline_inside() {
        let v = sq(0.0, 0.0, 10.0, 10.0);
        assert!(contains(&v, Pt::new(5.0, 5.0)));
        assert!(contains(&v, Pt::new(10.0, 5.0)));
        assert!(contains(&v, Pt::new(0.0, 0.0)));
        assert!(!contains(&v, Pt::new(10.1, 5.0)));
        // An L shape: the notch is outside.
        let l = vec![Pt::new(0.0, 0.0), Pt::new(20.0, 0.0), Pt::new(20.0, 10.0), Pt::new(10.0, 10.0), Pt::new(10.0, 20.0), Pt::new(0.0, 20.0)];
        assert!(contains(&l, Pt::new(5.0, 15.0)));
        assert!(!contains(&l, Pt::new(15.0, 15.0)));
        assert!((area(&l) - 300.0).abs() < 1e-9);
    }

    #[test]
    fn a_bow_tie_crosses_itself_and_a_square_does_not() {
        assert!(!crosses_itself(&sq(0.0, 0.0, 10.0, 10.0)));
        let bow = vec![Pt::new(0.0, 0.0), Pt::new(10.0, 10.0), Pt::new(10.0, 0.0), Pt::new(0.0, 10.0)];
        assert!(crosses_itself(&bow));
    }

    #[test]
    fn natural_order_compares_numbers_by_value() {
        let mut v = vec!["+CP10", "+CP2", "+CP1", "+FIELD"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["+CP1", "+CP2", "+CP10", "+FIELD"]);
    }
}
