//! Connectivity on the page (brief 022, design 022 §3, §7): conductor labels, continuation texts,
//! and placing junctions and continuations with existing ops.
//!
//! # Semantics
//! - Labels and continuation texts are derived on every projection of a page, never stored
//!   (like 018's tag texts): only the page on screen is projected.
//! - A junction or continuation is a tagless component made from the starter definition
//!   `junction` or `continuation` (FP4), with one connection point keyed `1`.
//!
//! This module reads the document and plans ops; it commits nothing itself.

use std::collections::BTreeMap;

use cadcraft_engine::doc::{Common, Entity, EntityKind, HAlign, Handle, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::connectivity::{CONTINUATION, JUNCTION, KIND, NAME, name_key};
use ecad_model::{
    Component, ComponentId, Conductor, ConductorId, DefinitionId, Document, End, Form, IdAllocator, Item, ItemId,
    ItemKind, Op, PageId, Placement, Primitive, Pt, Representation, RepresentationId, Style, Subject, SymbolInstance,
    Terminal, TerminalId,
};

use crate::devices::{DerivedText, TextOf};
use crate::symbols::{self, Starter};

/// The reserved layer of conductor labels (derived, plotted), like 009's `ECAD-CABLES`.
pub const LABEL_LAYER: &str = "ECAD-WIRE-LABELS";
/// `ecad.label-height`.
pub const LABEL_HEIGHT: f64 = 2.5;
/// `ecad.label-gap`: from the line to the label's baseline.
pub const LABEL_GAP: f64 = 1.0;
/// `ecad.wire-label`: ACI 7, ByLayer on the label layer.
pub const LABEL_ACI: u8 = 7;
/// How near a wire a point must be to lie on it (model units, mm).
pub const ON_WIRE: f64 = 0.5;
/// Most characters of a continuation name.
pub const MAX_NAME_CHARS: usize = 16;
/// Other places a continuation lists before `+n`.
pub const MAX_PLACES: usize = 3;

/// Why a conductor label alone cannot be edited.
pub const LABEL_DERIVED: &str =
    "The wire label is derived; change the number with Fix Wire Number, or the wire's cross-section and colour.";

/// The continuation name rule, in words (design 022 §3.2).
pub const NAME_RULE: &str = "A continuation name is 1 to 16 letters, digits or + - / . _";

/// The starter of a junction.
pub fn junction_starter() -> Option<&'static Starter> {
    symbols::by_key(JUNCTION)
}

/// The starter of a continuation.
pub fn continuation_starter() -> Option<&'static Starter> {
    symbols::by_key(CONTINUATION)
}

/// What a tagless component is (`junction`, `continuation`), from its `kind` property.
pub fn kind_of(doc: &Document, c: ComponentId) -> Option<&str> {
    doc.components.get(&c)?.properties.get(KIND).map(String::as_str)
}

/// A continuation name as typed, checked: trimmed, 1-16 letters, digits or `+ - / . _`.
///
/// # Errors
/// [`NAME_RULE`].
pub fn valid_name(text: &str) -> Result<String, String> {
    let t = text.trim();
    let ok = !t.is_empty()
        && t.chars().count() <= MAX_NAME_CHARS
        && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '+' | '-' | '/' | '.' | '_'));
    if ok { Ok(t.to_string()) } else { Err(NAME_RULE.into()) }
}

// ---------------------------------------------------------------- labels (§7)

/// One conductor label on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct WireLabel {
    pub conductor: ConductorId,
    /// The route representation it labels.
    pub rep: RepresentationId,
    /// The centre of its baseline.
    pub at: Pt,
    /// Radians, within (−90°, 90°], so the text is never upside down.
    pub rotation: f64,
    pub text: String,
    /// Drawn in `ecad.orphan`: the net is in a number error.
    pub error: bool,
}

/// The vertices of a route path.
fn path_vertices(p: &Primitive) -> Option<Vec<Pt>> {
    match p {
        Primitive::Line { a, b } => Some(vec![*a, *b]),
        Primitive::Polyline { vertices, closed: false } => Some(vertices.clone()),
        _ => None,
    }
}

/// The longest segment of a path (the first when equal), as its two ends.
pub fn longest_segment(v: &[Pt]) -> Option<(Pt, Pt)> {
    let mut best: Option<(Pt, Pt, f64)> = None;
    for w in v.windows(2) {
        let (a, b) = (*w.first()?, *w.get(1)?);
        let len = a.dist(b);
        if !len.is_finite() {
            continue;
        }
        if best.is_none_or(|(_, _, l)| len > l) {
            best = Some((a, b, len));
        }
    }
    best.filter(|(_, _, l)| *l > 0.0).map(|(a, b, _)| (a, b))
}

/// Where a label of segment `a`-`b` goes (design 022 §7): its baseline centre and rotation.
pub fn label_place(a: Pt, b: Pt) -> (Pt, f64) {
    let mut theta = (b.y - a.y).atan2(b.x - a.x);
    let half = std::f64::consts::FRAC_PI_2;
    let eps = 1e-9;
    if theta > half + eps {
        theta -= std::f64::consts::PI;
    } else if theta <= -half + eps {
        theta += std::f64::consts::PI;
    }
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    // Above the line in the text's own frame: for a vertical line, its left.
    let n = Pt::new(-theta.sin(), theta.cos());
    (Pt::new(mid.x + n.x * LABEL_GAP, mid.y + n.y * LABEL_GAP), theta)
}

/// Every conductor label on `page`: one per route representation, on its longest segment.
pub fn labels_on_page(doc: &Document, page: PageId) -> Vec<WireLabel> {
    let mut out = Vec::new();
    for (rid, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != Form::Drawn {
            continue;
        }
        let Some(ItemKind::Primitive(p)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) else {
            continue;
        };
        let Some((a, b)) = path_vertices(p).as_deref().and_then(longest_segment) else { continue };
        let Some(label) = ecad_model::wiring::label(doc, c) else { continue };
        let (at, rotation) = label_place(a, b);
        if !(at.is_finite() && rotation.is_finite()) {
            continue;
        }
        out.push(WireLabel { conductor: c, rep: rid, at, rotation, text: label.text, error: label.error });
    }
    out
}

/// A label as an entity on [`LABEL_LAYER`].
pub fn label_entity(l: &WireLabel, handle: Handle) -> Entity {
    let p = Vec3::new(l.at.x, l.at.y, 0.0);
    let color = if l.error {
        cadcraft_engine::doc::color::Color::Index(crate::references::ORPHAN_ACI)
    } else {
        cadcraft_engine::doc::color::Color::ByLayer
    };
    Entity {
        handle,
        common: Common { layer: LABEL_LAYER.into(), color, ..Common::default() },
        kind: EntityKind::Text(Text {
            insert: p,
            align_pt: Some(p),
            height: LABEL_HEIGHT,
            value: l.text.clone(),
            rotation: l.rotation,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::Center,
            valign: VAlign::Bottom,
        }),
    }
}

// ---------------------------------------------------------------- continuation texts (§3.2)

/// Where a continuation is (its symbol on its first page), as references name it (`2-A-6`), with
/// a sort key (page position, column, row).
fn place_of(doc: &Document, c: ComponentId) -> Option<((usize, u32, u32), String)> {
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i + 1)).collect();
    let style = doc.settings.reference_style;
    doc.representations_of(Subject::Component(c))
        .filter_map(|(_, r)| {
            let n = *order.get(&r.page)?;
            let ItemKind::Symbol(s) = &doc.items.get(r.items.first()?)?.kind else { return None };
            let p = ecad_model::wiring::place(doc, r.page, s.placement.origin)?;
            Some(((n, p.column, p.row), p.text(style)))
        })
        .min()
}

/// The other places of continuation `c` (same name), in book order, as its text shows them
/// (`/2.1 /3.1`, more than [`MAX_PLACES`] as `+n`), and their components.
pub fn other_places(doc: &Document, c: ComponentId) -> (String, Vec<ComponentId>) {
    let Some(key) = doc.components.get(&c).and_then(|x| x.properties.get(NAME)).map(|n| name_key(n)) else {
        return (String::new(), Vec::new());
    };
    let mut others: Vec<(((usize, u32, u32), String), ComponentId)> = doc
        .continuations_named(&key)
        .into_iter()
        .filter(|o| *o != c)
        .filter_map(|o| place_of(doc, o).map(|p| (p, o)))
        .collect();
    others.sort();
    let ids = others.iter().map(|(_, o)| *o).collect();
    let mut words: Vec<String> = others.iter().take(MAX_PLACES).map(|((_, w), _)| w.clone()).collect();
    if others.len() > MAX_PLACES {
        words.push(format!("+{}", others.len() - MAX_PLACES));
    }
    (words.join(" "), ids)
}

/// The texts beside a continuation symbol (design 022 §3.2): its name, and its other places
/// under it; `None` when `c` is not a continuation.
pub fn continuation_texts(
    doc: &Document,
    c: ComponentId,
    rep: RepresentationId,
    pl: &Placement,
) -> Option<Vec<DerivedText>> {
    if kind_of(doc, c) != Some(CONTINUATION) {
        return None;
    }
    let mut out = Vec::new();
    let name = doc.components.get(&c)?.properties.get(NAME).cloned().unwrap_or_default();
    let at = pl.apply(Pt::new(2.5, -4.0));
    if !at.is_finite() {
        return Some(out);
    }
    out.push(DerivedText {
        rep,
        of: TextOf::Name,
        value: name,
        at,
        height: crate::devices::TAG_HEIGHT,
        halign: HAlign::Left,
        valign: VAlign::Bottom,
        aci: None,
    });
    let (places, _) = other_places(doc, c);
    if !places.is_empty() {
        out.push(DerivedText {
            rep,
            of: TextOf::Places,
            value: places,
            at: Pt::new(at.x, at.y - 1.0),
            height: crate::references::TEXT_HEIGHT_MM,
            halign: HAlign::Left,
            valign: VAlign::Top,
            aci: Some(crate::references::REFERENCE_ACI),
        });
    }
    Some(out)
}

// ---------------------------------------------------------------- placing (§3)

/// A planned junction or continuation.
#[derive(Debug, Clone)]
pub struct Tagless {
    pub ops: Vec<Op>,
    pub component: ComponentId,
    pub terminal: TerminalId,
    pub item: ItemId,
    pub representation: RepresentationId,
    pub definition: DefinitionId,
    /// Loose ends found at the point and attached.
    pub attached: Vec<(ConductorId, End)>,
}

/// The route of `c` on `page`: its item, the item, and its vertices.
pub fn route_vertices(doc: &Document, c: ConductorId, page: PageId) -> Option<(ItemId, Item, Vec<Pt>)> {
    let (id, prim) = ecad_model::derive::route_on_page(doc, c, page)?;
    let item = doc.items.get(&id)?.clone();
    Some((id, item, path_vertices(&prim)?))
}

/// The conductor ends on `page` that are loose and drawn within `tol` of `at`.
pub fn loose_ends_at(doc: &Document, page: PageId, at: Pt, tol: f64) -> Vec<(ConductorId, End)> {
    let mut out = Vec::new();
    for (_, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        let Some(cond) = doc.conductors.get(&c) else { continue };
        let Some((a, b)) = ecad_model::derive::route_ends_on(doc, c, page) else { continue };
        for (end, p) in [(End::A, a), (End::B, b)] {
            if cond.end(end).is_none() && p.near(at, tol) && !out.contains(&(c, end)) {
                out.push((c, end));
            }
        }
    }
    out
}

/// Plan a tagless component of starter `s` at `placement` on `page`, with properties `props`
/// (`kind`, and `name` for a continuation); loose ends lying on its connection point attach.
///
/// # Errors
/// The starter definition cannot be built (never expected).
pub fn plan_tagless(
    doc: &Document,
    ids: &mut IdAllocator,
    page: PageId,
    style: Style,
    s: &Starter,
    placement: Placement,
    props: ecad_model::Properties,
) -> Result<Tagless, String> {
    let mut ops = Vec::new();
    let definition = match symbols::embedded(doc, s) {
        Some(d) => d,
        None => {
            let d: DefinitionId = ids.fresh();
            ops.push(Op::EmbedDefinition { id: d, definition: s.definition()?.clone() });
            d
        }
    };
    let (component, terminal, item, representation): (ComponentId, TerminalId, ItemId, RepresentationId) =
        (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    ops.push(Op::CreateComponent {
        id: component,
        component: Component { properties: props, definition: Some(definition) },
    });
    ops.push(Op::AddTerminal { id: terminal, terminal: Terminal { component, label: None, key: Some("1".into()) } });
    ops.push(Op::AddItem {
        id: item,
        item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition, placement }) },
    });
    ops.push(Op::LinkRepresentation {
        id: representation,
        rep: Representation {
            subject: Subject::Component(component),
            page,
            form: Form::Symbol,
            role: "main".into(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    let attached = loose_ends_at(doc, page, placement.origin, ON_WIRE);
    for (c, end) in &attached {
        ops.push(Op::AttachEnd { conductor: *c, end: *end, before: None, after: Some(terminal) });
    }
    Ok(Tagless { ops, component, terminal, item, representation, definition, attached })
}

/// The nearest point to `p` on segment `a`-`b`, and its distance.
fn on_segment(p: Pt, a: Pt, b: Pt) -> (Pt, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = Pt::new(a.x + t * dx, a.y + t * dy);
    (q, q.dist(p))
}

/// A wire found under a point: the conductor, the segment index and the point on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnWire {
    pub conductor: ConductorId,
    pub segment: usize,
    pub at: Pt,
}

/// The wire on `page` whose route passes within `tol` of `p` (the nearest; never `except`), with
/// the point snapped to a route vertex within `tol`.
pub fn wire_at(doc: &Document, page: PageId, p: Pt, tol: f64, except: Option<ConductorId>) -> Option<OnWire> {
    let mut best: Option<(f64, OnWire)> = None;
    for (_, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if Some(c) == except || r.form != Form::Drawn {
            continue;
        }
        let Some(ItemKind::Primitive(prim)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) else {
            continue;
        };
        let Some(v) = path_vertices(prim) else { continue };
        let (first, last) = (v.first().copied(), v.last().copied());
        for (i, w) in v.windows(2).enumerate() {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
            let (q, d) = on_segment(p, *a, *b);
            // Hostile geometry (an overflowing segment) gives NaN: never a wire there.
            if !(d <= tol && q.is_finite()) {
                continue;
            }
            // A wire's interior wins over another wire's end at the same point.
            let at_end = first.is_some_and(|f| f.near(q, tol)) || last.is_some_and(|l| l.near(q, tol));
            let d = if at_end { d + tol } else { d };
            if best.is_none_or(|(bd, _)| d < bd) {
                let at = v.iter().copied().find(|x| x.near(q, tol)).unwrap_or(q);
                best = Some((d, OnWire { conductor: c, segment: i, at }));
            }
        }
    }
    best.map(|(_, w)| w)
}

/// A planned junction on a wire (design 022 §3.1).
#[derive(Debug, Clone)]
pub struct Junction {
    pub tagless: Tagless,
    /// The new second half of the wire.
    pub second: ConductorId,
    pub at: Pt,
}

/// Plan a junction where `on` lies on its wire: the junction, the wire split at the point (it
/// keeps its ID up to the point, a copy of it continues), and loose ends at the point attached.
///
/// # Errors
/// Design 022 §3.1's refusals: a cable core, a wire drawn on more than one page, a junction
/// already there, the point at a route end.
pub fn plan_junction(
    doc: &Document,
    ids: &mut IdAllocator,
    page: PageId,
    style: Style,
    on: OnWire,
) -> Result<Junction, String> {
    let c = on.conductor;
    let cond = doc.conductors.get(&c).ok_or("No such wire.")?;
    if let Some(k) = doc.cables_of(c).first() {
        let cable = crate::references::cable_words(doc, *k);
        let core = doc
            .cables
            .get(k)
            .and_then(|x| x.members.iter().find(|m| m.conductor == c))
            .and_then(|m| m.core.clone())
            .map_or_else(String::new, |n| format!("'s core {n}"));
        return Err(format!("{cable}{core} can't take a junction: a cable core runs end to end."));
    }
    let pages: std::collections::BTreeSet<PageId> =
        doc.representations_of(Subject::Conductor(c)).map(|(_, r)| r.page).collect();
    if pages.len() > 1 {
        return Err(format!(
            "This wire is drawn on {} pages; add the junction where it is a single wire.",
            pages.len()
        ));
    }
    let (item_id, item, v) = route_vertices(doc, c, page).ok_or("The wire has no route on this page.")?;
    for (_, r) in doc.representations_on(page) {
        let Subject::Component(j) = r.subject else { continue };
        if kind_of(doc, j) != Some(JUNCTION) {
            continue;
        }
        let here = doc
            .terminals_of(j)
            .any(|(t, _)| ecad_model::derive::terminal_position(doc, page, t).is_some_and(|p| p.near(on.at, ON_WIRE)));
        if here {
            return Err("There is a junction here already.".into());
        }
    }
    let (first, last) = (v.first().copied().ok_or("empty route")?, v.last().copied().ok_or("empty route")?);
    if on.at.near(first, ON_WIRE) || on.at.near(last, ON_WIRE) {
        return Err("Click on a wire, not its end: an end can attach to a connection point.".into());
    }
    let starter = junction_starter().ok_or("the junction symbol is missing")?;
    let mut props = ecad_model::Properties::new();
    props.insert(KIND.into(), JUNCTION.into());
    let placement = Placement { origin: on.at, rotation: 0.0, mirror: false, scale: 1.0 };
    let mut tagless = plan_tagless(doc, ids, page, style, starter, placement, props)?;
    let j = tagless.terminal;
    // Split: C keeps its vertices up to the point; C2 runs from the point to C's old end B.
    let seg = on.segment;
    let mut keep: Vec<Pt> = v.get(..=seg).unwrap_or_default().to_vec();
    let mut rest: Vec<Pt> = v.get(seg + 1..).unwrap_or_default().to_vec();
    if keep.last().is_some_and(|p| p.near(on.at, 1e-9)) {
        keep.pop();
    }
    if rest.first().is_some_and(|p| p.near(on.at, 1e-9)) {
        rest.remove(0);
    }
    keep.push(on.at);
    rest.insert(0, on.at);
    let path = |pts: Vec<Pt>| match pts.as_slice() {
        [a, b] => Primitive::Line { a: *a, b: *b },
        _ => Primitive::Polyline { vertices: pts, closed: false },
    };
    let mut shortened = item.clone();
    shortened.kind = ItemKind::Primitive(path(keep));
    tagless.ops.push(Op::ModifyItem { id: item_id, before: item.clone(), after: shortened });
    let old_b = cond.end(End::B);
    tagless.ops.push(Op::AttachEnd { conductor: c, end: End::B, before: old_b, after: Some(j) });
    let (c2, i2, r2): (ConductorId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    tagless.ops.push(Op::CreateConductor {
        id: c2,
        conductor: Conductor { properties: cond.properties.clone(), ends: [Some(j), old_b] },
    });
    let mut second = item;
    second.kind = ItemKind::Primitive(path(rest));
    tagless.ops.push(Op::AddItem { id: i2, item: second });
    let role = doc
        .representations_of(Subject::Conductor(c))
        .find(|(_, r)| r.page == page)
        .map_or_else(|| "route".to_string(), |(_, r)| r.role.clone());
    tagless.ops.push(Op::LinkRepresentation {
        id: r2,
        rep: Representation {
            subject: Subject::Conductor(c2),
            page,
            form: Form::Drawn,
            role,
            items: vec![i2],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(Junction { tagless, second: c2, at: on.at })
}

// ---------------------------------------------------------------- words (§7, §9)

/// Most connection points a net line names before `+n`.
pub const MAX_POINTS_SHOWN: usize = 6;

/// A connection point in words: `-K01:A1`; a junction `junction`, a continuation `→24V`.
pub fn point_words(doc: &Document, t: TerminalId) -> String {
    let Some(term) = doc.terminals.get(&t) else { return "?".into() };
    let c = term.component;
    match kind_of(doc, c) {
        Some(JUNCTION) => return "junction".into(),
        Some(CONTINUATION) => {
            let name = doc.components.get(&c).and_then(|x| x.properties.get(NAME)).cloned().unwrap_or_default();
            return format!("\u{2192}{name}");
        }
        _ => {}
    }
    let label = term.label.clone().or_else(|| term.key.clone()).unwrap_or_else(|| "?".into());
    match crate::devices::tag_of(doc, c) {
        Some(tag) => format!("{tag}:{label}"),
        None => format!("{}\u{2026}:{label}", c.hex().get(..8).unwrap_or_default()),
    }
}

/// The device connection points of a net (not junctions or continuations), in words, sorted.
pub fn net_points(doc: &Document, net: &ecad_model::NetData) -> Vec<String> {
    let mut v: Vec<String> = net
        .terminals
        .iter()
        .filter(|t| {
            doc.terminals.get(t).is_none_or(|x| !matches!(kind_of(doc, x.component), Some(JUNCTION | CONTINUATION)))
        })
        .map(|t| point_words(doc, *t))
        .collect();
    v.sort();
    v
}

/// `a, b, c +n` of at most `max` words.
pub fn first_words(words: &[String], max: usize) -> String {
    let mut s = words.iter().take(max).cloned().collect::<Vec<_>>().join(", ");
    if words.len() > max {
        s.push_str(&format!(" +{}", words.len() - max));
    }
    s
}

/// The net line of a wire's hover lens (design 022 §7): `Net 24V-04 · 3 wires · -S02:14, …`.
pub fn net_line(doc: &Document, net: &ecad_model::NetData) -> String {
    let number = ecad_model::wiring::net_number(doc, net).unwrap_or_else(|| "?".into());
    let n = net.conductors.len();
    let wires = if n == 1 { "1 wire".to_string() } else { format!("{n} wires") };
    let points = net_points(doc, net);
    if points.is_empty() {
        format!("Net {number} \u{b7} {wires}")
    } else {
        format!("Net {number} \u{b7} {wires} \u{b7} {}", first_words(&points, MAX_POINTS_SHOWN))
    }
}

/// The lines brief 022 adds to a wire's hover lens: number (fixed or not), potential, label
/// values (stored, or the potential's default), and the net.
pub fn hover_lines(doc: &Document, w: ConductorId) -> Vec<String> {
    let mut lines = Vec::new();
    let conn = doc.connectivity();
    let Some(net) = conn.net_of_conductor(w).and_then(|id| conn.net(id)) else { return lines };
    let Some(cond) = doc.conductors.get(&w) else { return lines };
    let number = ecad_model::wiring::net_number(doc, net).unwrap_or_else(|| "? (no number yet)".into());
    let fixed = if ecad_model::wiring::net_fixed(net) { " (fixed)" } else { "" };
    lines.push(format!("Wire number {number}{fixed}"));
    let pot = ecad_model::wiring::net_potential(doc, net);
    if let Some(p) = &pot.name {
        let from = if pot.from_continuation { " (continuation)" } else { "" };
        lines.push(format!("Potential {p}{from}"));
    }
    let shown = |key: &str, default: &Option<String>| match (cond.properties.get(key), default, &pot.name) {
        (Some(v), _, _) => Some(v.clone()),
        (None, Some(d), Some(p)) => Some(format!("{d} ({p})")),
        _ => None,
    };
    let mut values = Vec::new();
    if let Some(v) = shown(ecad_model::wiring::CROSS_SECTION, &pot.cross_section) {
        values.push(format!("Cross-section {v}"));
    }
    if let Some(v) = shown(ecad_model::wiring::COLOUR, &pot.colour) {
        values.push(format!("Colour {v}"));
    }
    if !values.is_empty() {
        lines.push(values.join(" \u{b7} "));
    }
    lines.push(net_line(doc, net));
    lines
}
