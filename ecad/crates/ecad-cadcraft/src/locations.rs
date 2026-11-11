//! Device locations and location boxes (brief 024, design 024 §3).
//!
//! # Semantics
//! - A device's **location** (IEC 81346 `+` aspect) is its own `location` property when set,
//!   else the **context** of its main symbol ([`main_rep`]): the innermost location box on that
//!   symbol's page whose outline holds the symbol's anchor, else the page's `location`, else the
//!   project's. Nothing of this is stored: it is worked out from where symbols and boxes are.
//! - A **location box** is a tagless component with property `kind: "location_box"` and its own
//!   `location`, drawn as one closed polyline item (layer [`LAYER`], linetype `DASHED`) of a
//!   drawn representation with role [`ROLE`] (design 024 FP2). Its `+…` label is derived.
//! - Containment ([`contains`]) is point in polygon by the even-odd rule; a point within
//!   [`EDGE_TOL`] of the outline counts as inside. Innermost is the smallest polygon area, ties by
//!   component ID.
//!
//! # Update map (design 024 §9)
//! Every function here reads one page's boxes and the representations asked about, never the
//! whole project. Callers decide when to ask: the page drawing on every draw of the active page
//! (tag prefixes and box labels), [`crate::outputs`] for the devices a change could move.
//!
//! This module records nothing; [`plan_box`] and [`plan_from_item`] only build ops.

use std::collections::BTreeMap;

use ecad_model::book::LOCATION_KEY;
use ecad_model::connectivity::KIND;
use ecad_model::{
    Color, Component, ComponentId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Layer, LayerId, Op, PageId, Primitive, Pt, Representation,
    RepresentationId, Style, Subject,
};

/// The `kind` of a location box component.
pub const KIND_VALUE: &str = "location_box";
/// The reserved layer of location boxes (`ecad.location-box`).
pub const LAYER: &str = "ECAD-LOCATIONS";
/// The linetype of a box outline.
pub const LINETYPE: &str = "DASHED";
/// The representation role of a box outline.
pub const ROLE: &str = "box";
/// `ecad.location-label-height`.
pub const LABEL_HEIGHT: f64 = 3.5;
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
    pub component: ComponentId,
    pub rep: RepresentationId,
    pub item: ItemId,
    pub page: PageId,
    pub location: Option<String>,
    pub vertices: Vec<Pt>,
    pub lo: Pt,
    pub hi: Pt,
    pub area: f64,
}

/// Whether component `c` is a location box.
pub fn is_box(doc: &Document, c: ComponentId) -> bool {
    doc.components.get(&c).is_some_and(is_box_component)
}

/// Whether `c` is a location box component.
pub fn is_box_component(c: &Component) -> bool {
    c.properties.get(KIND).is_some_and(|k| k == KIND_VALUE)
}

/// A device in the sense of locations and the BOM (design 024 §6.1): a component with a tag and
/// no `kind`.
pub fn is_device(doc: &Document, c: ComponentId) -> bool {
    doc.components.get(&c).is_some_and(|x| x.properties.contains_key(ecad_model::tag::TAG_KEY) && !x.properties.contains_key(KIND))
}

/// The box representation `r` (`rid`) stands for, if it is one with a drawable outline.
pub fn box_of_rep(doc: &Document, rid: RepresentationId, r: &Representation) -> Option<LocBox> {
    let Subject::Component(c) = r.subject else { return None };
    if r.form != Form::Drawn || r.role != ROLE {
        return None;
    }
    let comp = doc.components.get(&c).filter(|x| is_box_component(x))?;
    let item = *r.items.first()?;
    let vertices = match &doc.items.get(&item)?.kind {
        ItemKind::Primitive(Primitive::Polyline { vertices, .. }) => vertices.clone(),
        _ => return None,
    };
    if vertices.len() < 3 || vertices.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (lo, hi) = bounds(&vertices);
    Some(LocBox {
        component: c,
        rep: rid,
        item,
        page: r.page,
        location: comp.properties.get(LOCATION_KEY).cloned(),
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

/// The innermost of `boxes` holding `p` (smallest area, then component ID).
pub fn innermost(boxes: &[LocBox], p: Pt) -> Option<&LocBox> {
    boxes
        .iter()
        .filter(|b| p.x >= b.lo.x - EDGE_TOL && p.x <= b.hi.x + EDGE_TOL && p.y >= b.lo.y - EDGE_TOL && p.y <= b.hi.y + EDGE_TOL)
        .filter(|b| contains(&b.vertices, p))
        .min_by(|a, b| a.area.total_cmp(&b.area).then(a.component.cmp(&b.component)))
}

/// Where a location comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// The device's own value.
    Device,
    /// A location box on a page.
    Box { component: ComponentId, page: PageId },
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
            Source::Box { .. } => "box",
            Source::Page(_) => "page",
            Source::Project => "project",
            Source::None => "none",
        }
    }
}

/// A location and where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Located {
    pub value: Option<String>,
    pub source: Source,
}

impl Located {
    fn none() -> Located {
        Located { value: None, source: Source::None }
    }

    /// `(from box on 105)`, `(from page 105)`, `(project)`; empty for an own value or none.
    pub fn from_words(&self, doc: &Document) -> String {
        match self.source {
            Source::Box { page, .. } => format!("(from box on {})", ecad_model::book::page_number(doc, page)),
            Source::Page(p) => format!("(from page {})", ecad_model::book::page_number(doc, p)),
            Source::Project => "(project)".into(),
            Source::Device | Source::None => String::new(),
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

/// The context of a point `at` on `page` with that page's `boxes` (design 024 §3.1).
pub fn context_at(doc: &Document, page: PageId, at: Option<Pt>, boxes: &[LocBox]) -> Located {
    if let Some(b) = at.and_then(|p| innermost(boxes, p))
        && let Some(v) = &b.location
    {
        return Located { value: Some(v.clone()), source: Source::Box { component: b.component, page } };
    }
    if let Some(v) = doc.pages.get(&page).and_then(|p| p.properties.get(LOCATION_KEY)) {
        return Located { value: Some(v.clone()), source: Source::Page(page) };
    }
    if let Some(v) = doc.properties.get(LOCATION_KEY) {
        return Located { value: Some(v.clone()), source: Source::Project };
    }
    Located::none()
}

/// The context of representation `r`.
pub fn rep_context(doc: &Document, r: &Representation, cache: &mut BoxCache) -> Located {
    let at = anchor(doc, r);
    let boxes = cache.on(doc, r.page);
    context_at(doc, r.page, at, boxes)
}

/// The box (if any) holding the anchor of `r`.
pub fn box_holding(doc: &Document, r: &Representation, cache: &mut BoxCache) -> Option<LocBox> {
    let at = anchor(doc, r)?;
    innermost(cache.on(doc, r.page), at).cloned()
}

/// A device's main representation (design 024 §3.2): role `main`; if none, the first in book
/// order, then top-left to bottom-right.
pub fn main_rep(doc: &Document, c: ComponentId) -> Option<(RepresentationId, &Representation)> {
    let reps: Vec<(RepresentationId, &Representation)> = doc.representations_of(Subject::Component(c)).collect();
    if let Some(m) = reps.iter().find(|(_, r)| r.role == "main") {
        return Some(*m);
    }
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    reps.into_iter().min_by(|(ia, a), (ib, b)| {
        let pa = order.get(&a.page).copied().unwrap_or(usize::MAX);
        let pb = order.get(&b.page).copied().unwrap_or(usize::MAX);
        let qa = anchor(doc, a).unwrap_or(Pt::new(0.0, 0.0));
        let qb = anchor(doc, b).unwrap_or(Pt::new(0.0, 0.0));
        pa.cmp(&pb).then(qb.y.total_cmp(&qa.y)).then(qa.x.total_cmp(&qb.x)).then(ia.cmp(ib))
    })
}

/// Device `c`'s location (design 024 §3.2), with `cache` for the pages read.
pub fn resolve_with(doc: &Document, c: ComponentId, cache: &mut BoxCache) -> Located {
    if let Some(v) = doc.components.get(&c).and_then(|x| x.properties.get(LOCATION_KEY)) {
        return Located { value: Some(v.clone()), source: Source::Device };
    }
    match main_rep(doc, c) {
        Some((_, r)) => rep_context(doc, r, cache),
        None => match doc.properties.get(LOCATION_KEY) {
            Some(v) => Located { value: Some(v.clone()), source: Source::Project },
            None => Located::none(),
        },
    }
}

/// Device `c`'s location.
pub fn resolve(doc: &Document, c: ComponentId) -> Located {
    resolve_with(doc, c, &mut BoxCache::default())
}

/// The box device `c`'s main symbol sits in when `c` has an own location that differs from it
/// (design 024 §8.3): the box and its location.
pub fn own_differs(doc: &Document, c: ComponentId, cache: &mut BoxCache) -> Option<LocBox> {
    let own = doc.components.get(&c)?.properties.get(LOCATION_KEY)?;
    let (_, r) = main_rep(doc, c)?;
    let b = box_holding(doc, r, cache)?;
    (b.location.as_ref().is_some_and(|l| l != own)).then_some(b)
}

/// The tag a symbol representation `r` of device `c` shows (design 024 §3.3): the tag, with the
/// device's location as a prefix when the symbol's context differs from it.
pub fn shown_tag(doc: &Document, c: ComponentId, r: &Representation, tag: &str, cache: &mut BoxCache) -> String {
    let device = resolve_with(doc, c, cache);
    let Some(loc) = device.value else { return tag.to_string() };
    let here = rep_context(doc, r, cache);
    if here.value.as_deref() == Some(loc.as_str()) { tag.to_string() } else { format!("{loc}{tag}") }
}

/// The label of a box: its location at the top-left vertex (greatest y, then least x).
pub fn label_of(b: &LocBox) -> (String, Pt) {
    let tl = b.vertices.iter().copied().max_by(|p, q| p.y.total_cmp(&q.y).then(q.x.total_cmp(&p.x))).unwrap_or(b.lo);
    (b.location.clone().unwrap_or_else(|| "+?".into()), Pt::new(tl.x + LABEL_INSET, tl.y - LABEL_INSET))
}

/// The style of a box outline on layer `layer`.
fn box_style(layer: LayerId) -> Style {
    Style { layer, color: Color::ByLayer, linetype: LINETYPE.into(), lineweight: crate::look::WIRE_WEIGHT as i16 }
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
pub fn location_value(doc: &Document, text: &str) -> Result<String, String> {
    crate::devices::property_value(doc, LOCATION_KEY, Some(text))?.ok_or_else(|| "A location box needs a location, such as +CP1.".to_string())
}

/// Check an outline for a box.
///
/// # Errors
/// Fewer than three points, a non-finite point, too many points, or too small.
pub fn check_outline(v: &[Pt]) -> Result<(), String> {
    if v.len() < 3 {
        return Err("A location box needs at least three points.".into());
    }
    if v.len() > MAX_VERTICES {
        return Err(format!("A location box has at most {MAX_VERTICES} points."));
    }
    if v.iter().any(|p| !p.is_finite()) {
        return Err("Every point must be a finite [x, y].".into());
    }
    if area(v) < MIN_AREA {
        return Err(TOO_SMALL.into());
    }
    Ok(())
}

/// What a planned box will be.
#[derive(Debug, Clone)]
pub struct Planned {
    pub component: ComponentId,
    pub rep: RepresentationId,
    pub item: ItemId,
    pub ops: Vec<Op>,
}

/// The ops of a new location box `location` drawn on `page` with outline `vertices`
/// (design 024 §7): `CreateComponent`, `AddItem` (closed polyline), `LinkRepresentation`.
///
/// # Errors
/// As [`check_outline`] and [`location_value`].
pub fn plan_box(doc: &Document, ids: &mut IdAllocator, page: PageId, location: &str, vertices: Vec<Pt>) -> Result<Planned, String> {
    if !doc.pages.contains_key(&page) {
        return Err("no such page".into());
    }
    check_outline(&vertices)?;
    let location = location_value(doc, location)?;
    let (layer, mut ops) = layer_ops(doc, ids);
    let component: ComponentId = ids.fresh();
    let item: ItemId = ids.fresh();
    let rep: RepresentationId = ids.fresh();
    let mut props = ecad_model::Properties::new();
    props.insert(KIND.into(), KIND_VALUE.into());
    props.insert(LOCATION_KEY.into(), location);
    ops.push(Op::CreateComponent { id: component, component: Component { properties: props, definition: None } });
    ops.push(Op::AddItem {
        id: item,
        item: Item { page, style: box_style(layer), kind: ItemKind::Primitive(Primitive::Polyline { vertices, closed: true }) },
    });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Component(component),
            page,
            form: Form::Drawn,
            role: ROLE.into(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(Planned { component, rep, item, ops })
}

/// The ops that turn the existing closed polyline `item` into location box `location`
/// (design 024 §3.5, §7).
///
/// # Errors
/// Not a closed polyline, part of another object, or as [`plan_box`].
pub fn plan_from_item(doc: &Document, ids: &mut IdAllocator, item: ItemId, location: &str) -> Result<Planned, String> {
    let it = doc.items.get(&item).ok_or("no such item")?;
    if let Some((_, r)) = doc.representation_of_item(item) {
        let who = match r.subject {
            Subject::Component(c) if is_box(doc, c) => "another location box".to_string(),
            Subject::Component(c) => crate::devices::tag_words(doc, c),
            Subject::Conductor(_) => "a wire".into(),
            Subject::Cable(_) => "a cable".into(),
        };
        return Err(format!("That outline belongs to {who}; draw a box instead."));
    }
    let vertices = match &it.kind {
        ItemKind::Primitive(Primitive::Polyline { vertices, closed: true }) => vertices.clone(),
        _ => return Err(NOT_CLOSED.into()),
    };
    check_outline(&vertices)?;
    let location = location_value(doc, location)?;
    let (layer, mut ops) = layer_ops(doc, ids);
    let component: ComponentId = ids.fresh();
    let rep: RepresentationId = ids.fresh();
    let mut props = ecad_model::Properties::new();
    props.insert(KIND.into(), KIND_VALUE.into());
    props.insert(LOCATION_KEY.into(), location);
    ops.push(Op::CreateComponent { id: component, component: Component { properties: props, definition: None } });
    let after = Item { style: box_style(layer), ..it.clone() };
    if after != *it {
        ops.push(Op::ModifyItem { id: item, before: it.clone(), after });
    }
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Component(component),
            page: it.page,
            form: Form::Drawn,
            role: ROLE.into(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(Planned { component, rep, item, ops })
}

/// How many device symbols on `b`'s page are inside it, and how many of those keep an own
/// location that differs (for the echo and the box's Properties).
pub fn inside_counts(doc: &Document, b: &LocBox) -> (usize, usize) {
    let mut cache = BoxCache::default();
    let (mut n, mut keeps) = (0, 0);
    for (_, r) in doc.representations_on(b.page) {
        let Subject::Component(c) = r.subject else { continue };
        if !is_device(doc, c) {
            continue;
        }
        let Some(at) = anchor(doc, r) else { continue };
        if innermost(cache.on(doc, b.page), at).map(|x| x.component) != Some(b.component) {
            continue;
        }
        n += 1;
        if doc.components.get(&c).and_then(|x| x.properties.get(LOCATION_KEY)).is_some_and(|own| Some(own) != b.location.as_ref()) {
            keeps += 1;
        }
    }
    (n, keeps)
}

/// The JSON of a device's location (`devices` results gain these, design 024 §3.4).
pub fn json_fields(doc: &Document, c: ComponentId) -> (serde_json::Value, &'static str) {
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

/// A box label as an entity on [`LAYER`], top-left aligned.
pub fn label_entity(value: &str, at: Pt, handle: cadcraft_engine::doc::Handle) -> cadcraft_engine::doc::Entity {
    use cadcraft_engine::doc::{HAlign, VAlign};
    crate::devices::text_entity_with(value, at, LABEL_HEIGHT, HAlign::Left, VAlign::Top, handle, LAYER)
}

/// Load [`LINETYPE`] into `d` from CADCraft's standard linetypes when it is missing, so box
/// outlines draw and plot dashed.
pub fn add_linetype(d: &mut cadcraft_engine::doc::Drawing) {
    if d.linetype(LINETYPE).is_some() {
        return;
    }
    if let Some(lt) = cadcraft_engine::doc::library::standard_linetypes_for(d).into_iter().find(|l| l.name == LINETYPE) {
        d.linetypes.push(lt);
    }
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
