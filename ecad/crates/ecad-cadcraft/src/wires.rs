//! Connectivity on the page (brief 022, design 022 §3, §7): conductor labels, continuation texts,
//! and placing junctions and continuations with existing ops.
//!
//! # Semantics
//! - Labels and continuation texts are derived on every projection of a page, never stored
//!   (like 018's tag texts): only the page on screen is projected.
//! - A junction or continuation is a tagless component made from the starter definition
//!   `junction` or `continuation` (FP4), with one connection point keyed `1`.
//! - Labels use the project's label height and colour style (wiring settings) and are placed so
//!   none touches another label or another wire ([`labels_on_page_with`]).
//!
//! # Update map of the labels (design 032 §8.1)
//! | Change | Labels |
//! | --- | --- |
//! | a route added, moved or removed (incl. a merge's `ModifyItem` and a fold's removal) | that page's labels, when it is projected |
//! | `AttachEnd`, `CreateConductor`, `DeleteConductor` (nets join or split) | with the `net` rule, the pages where those nets have routes: their routes' pages are touched, marked stale, the page on screen re-projects |
//! | a wire number, potential, cross-section or colour | the pages of the conductors it was written on (the numbering pass's included) |
//! | wiring settings (label rule, shortest labelled wire, colour style, label height, potentials) | every page, when it is projected next |
//! | lane T's wire-label text style or show setting (031) | as lane T's map says; here the pages of the conductors concerned |
//! | a cable switched to fan or traditional view | that cable's members' labels hide or show on the page on screen (display only, nothing recorded) |
//! | anything else | nothing |
//!
//! # Update map of the looks-connected marks (design 032 §8.2)
//! Route ops, `AttachEnd` of an end routed on P, and cable reference ops on P change P's marks,
//! computed when P is projected ([`looks_connected_on_page`]); anything else changes nothing.
//!
//! This module reads the document and plans ops; it commits nothing itself.

use std::collections::BTreeMap;

use cadcraft_engine::doc::{Common, Entity, EntityKind, HAlign, Handle, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::connectivity::{CONTINUATION, JUNCTION, KIND, NAME, name_key};
use ecad_model::{
    Component, ComponentId, Conductor, ConductorId, DefinitionId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Op, PageId, Placement,
    Primitive, Pt, Representation, RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalId,
};

use crate::devices::{DerivedText, TextOf};
use crate::symbols::{self, Starter};

/// The reserved layer of conductor labels (derived, plotted), like 009's `ECAD-CABLES`.
pub const LABEL_LAYER: &str = "ECAD-WIRE-LABELS";
/// The smallest clearance from the line to the label, millimetres; it grows with the label height
/// ([`gap`]).
pub const LABEL_GAP: f64 = 0.8;
/// The least space kept between two labels, and between a label and another wire, millimetres.
pub const LABEL_CLEARANCE: f64 = 0.6;

/// Brief 040: the layer of a two-colour wire's dashes (derived, plotted only in colour).
pub const DASH_LAYER: &str = "ECAD-WIRE-COLOUR";

/// Brief 040: draw the page's wires and cores in their colour codes (spec §4): the route entity
/// takes the first colour, and a two-colour code adds the second colour's 2 mm on / 2 mm off
/// dashes over it on [`DASH_LAYER`], mapped to the conductor like its label (picking a dash
/// picks the wire, [`crate::look::pick_routes`]). Colours are those of the sheet the page is
/// projected for ([`crate::look::projection_sheet`]). An unknown code leaves the route in ink.
/// Derived on every projection of the page, never stored.
pub(crate) fn colour_routes(
    doc: &Document, page: PageId, d: &mut cadcraft_engine::doc::Drawing, ents: &mut Vec<Entity>, map: &mut crate::project::HandleMap,
    prev: &crate::project::HandleMap,
) {
    use cadcraft_engine::doc::color::Color;
    let routes = crate::look::coloured_routes(doc, page, Some(map), crate::look::projection_sheet());
    if routes.is_empty() {
        return;
    }
    if d.layer(DASH_LAYER).is_none() {
        d.layers.push(cadcraft_engine::doc::Layer::new(DASH_LAYER));
    }
    let index: std::collections::HashMap<Handle, usize> = ents.iter().enumerate().map(|(i, e)| (e.handle, i)).collect();
    // The page's dash budget: past it a two-colour wire keeps its first colour only (Checks says).
    let budget = crate::look::dash_budget(doc, page);
    let mut dashes = Vec::new();
    for r in routes {
        let Some(route) = r.handle else { continue };
        let Some(e) = index.get(&route).and_then(|i| ents.get_mut(*i)) else { continue };
        e.common.color = Color::True(crate::look::rgb(r.colours.solid));
        map.insert_coloured(route);
        if !budget.dashed.contains(&r.rep) {
            continue;
        }
        let common = e.common.clone();
        for (i, mut dash) in route_dashes(&r, &common).into_iter().enumerate() {
            let Ok(i) = u32::try_from(i) else { break };
            dash.handle = prev.dash(r.rep, i).unwrap_or_else(|| d.new_handle());
            map.insert_dash(r.rep, r.conductor, i, dash.handle, route);
            dashes.push(dash);
        }
    }
    ents.extend(dashes);
}

/// Brief 050b (design 050 §5.5): draw `page`'s wire routes in their category's line type and
/// weight where these differ from the built-in (continuous, 0.25 mm), on screen and in print (a
/// mono print keeps them: they are not colour). `item_handle` finds a route item's entity.
pub(crate) fn style_routes(doc: &Document, page: PageId, ents: &mut [Entity], item_handle: impl Fn(ItemId) -> Option<Handle>) {
    let looks = crate::styles::wire::WireLooks::new(doc);
    let index: std::collections::HashMap<Handle, usize> = ents.iter().enumerate().map(|(i, e)| (e.handle, i)).collect();
    for (_, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != Form::Drawn {
            continue;
        }
        let look = looks.look(doc, c);
        let weight = (look.line_weight * 100.0).round().clamp(0.0, 211.0) as u16;
        if look.line_type == "continuous" && weight == crate::look::WIRE_WEIGHT {
            continue;
        }
        let Some(e) = r.items.first().and_then(|i| item_handle(*i)).and_then(|h| index.get(&h)).and_then(|i| ents.get_mut(*i)) else { continue };
        if look.line_type != "continuous" {
            e.common.linetype = crate::styles::wire::linetype(look.line_type).into();
        }
        e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(weight);
    }
}

/// The dash entities of a two-colour route (handles unset): the second colour's 2 mm on / 2 mm
/// off lines on [`DASH_LAYER`], with the route entity's `common` otherwise (its lineweight).
/// None for a one-colour route. Also what the page check expects ([`crate::verify`]).
pub(crate) fn route_dashes(r: &crate::look::ColouredRoute, route: &Common) -> Vec<Entity> {
    use cadcraft_engine::doc::color::Color;
    let Some(second) = r.colours.dashes else { return Vec::new() };
    let (on, off) = (f64::from(ecad_tokens::wire::DASH_ON_MM), f64::from(ecad_tokens::wire::DASH_OFF_MM));
    let common = Common { layer: DASH_LAYER.into(), color: Color::True(crate::look::rgb(second)), ..route.clone() };
    crate::look::dash_segments(&r.points, on, off)
        .into_iter()
        .map(|(a, b)| Entity {
            handle: Handle::default(),
            common: common.clone(),
            kind: EntityKind::Line(cadcraft_engine::doc::Line { a: Vec3::new(a.x, a.y, 0.0), b: Vec3::new(b.x, b.y, 0.0) }),
        })
        .collect()
}

/// The clearance between a label of height `h` and its line.
pub fn gap(h: f64) -> f64 {
    (0.5 * h).max(LABEL_GAP)
}
/// `ecad.wire-label`: ACI 7, ByLayer on the label layer.
pub const LABEL_ACI: u8 = 7;
/// How near a wire a point must be to lie on it (model units, mm).
pub const ON_WIRE: f64 = 0.5;
/// Most characters of a continuation name.
pub const MAX_NAME_CHARS: usize = 16;
/// Other places a continuation lists before `+n`.
pub const MAX_PLACES: usize = 3;

/// Why a conductor label alone cannot be edited.
pub const LABEL_DERIVED: &str = "The wire label is derived; change the number with Fix Wire Number, or the wire's cross-section and colour.";

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
    let ok =
        !t.is_empty() && t.chars().count() <= MAX_NAME_CHARS && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '+' | '-' | '/' | '.' | '_'));
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
    /// Text height (the project's wiring setting).
    pub height: f64,
    pub text: String,
    /// Drawn in `ecad.orphan`: the net is in a number error.
    pub error: bool,
    /// Drawn in `ecad.provisional`: no number yet, provisional tags on (brief 026).
    pub provisional: bool,
    /// Brief 032 fix round: its text width as the project's font draws it (lane T's
    /// `text_style::width_in`), which placement keeps clear.
    pub width: f64,
    /// Brief 050a: the conductor's own wire label style (its `text.wire_label.style`), else the
    /// project's.
    pub own: Option<crate::text_style::OwnStyle>,
}

/// The vertices of a route path.
pub(crate) fn path_vertices(p: &Primitive) -> Option<Vec<Pt>> {
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

/// The reading angle of segment `a`-`b`, within (−90°, 90°] so the text is never upside down.
fn reading_angle(a: Pt, b: Pt) -> f64 {
    let mut theta = (b.y - a.y).atan2(b.x - a.x);
    let half = std::f64::consts::FRAC_PI_2;
    let eps = 1e-9;
    if theta > half + eps {
        theta -= std::f64::consts::PI;
    } else if theta <= -half + eps {
        theta += std::f64::consts::PI;
    }
    theta
}

/// Where a label of segment `a`-`b` goes first (design 022 §7): its baseline centre and rotation,
/// above the line in the text's own frame (left of a vertical line), at the clearance [`gap`] of
/// a label of height `h`.
pub fn label_place(a: Pt, b: Pt, h: f64) -> (Pt, f64) {
    let theta = reading_angle(a, b);
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let n = Pt::new(-theta.sin(), theta.cos());
    let g = gap(h);
    (Pt::new(mid.x + n.x * g, mid.y + n.y * g), theta)
}

/// An axis-aligned box `(lo, hi)`.
type Bbox = (Pt, Pt);

/// The box round a label's text: baseline centre `at`, angle `theta`, width `w`, height `h`.
fn text_box(at: Pt, theta: f64, w: f64, h: f64) -> Bbox {
    let (d, n) = (Pt::new(theta.cos(), theta.sin()), Pt::new(-theta.sin(), theta.cos()));
    let corners =
        [(-w / 2.0, 0.0), (w / 2.0, 0.0), (w / 2.0, h), (-w / 2.0, h)].map(|(u, v)| Pt::new(at.x + d.x * u + n.x * v, at.y + d.y * u + n.y * v));
    let (mut lo, mut hi) = (corners[0], corners[0]);
    for c in corners {
        lo = Pt::new(lo.x.min(c.x), lo.y.min(c.y));
        hi = Pt::new(hi.x.max(c.x), hi.y.max(c.y));
    }
    (lo, hi)
}

/// The box round label `l`'s text, as placement sees it (tests check that labels never touch).
pub fn label_box(l: &WireLabel) -> (Pt, Pt) {
    let w = if l.width.is_finite() && l.width > 0.0 { l.width } else { l.text.chars().count() as f64 * l.height * 0.8 };
    text_box(l.at, l.rotation, w, l.height)
}

fn boxes_meet(a: &Bbox, b: &Bbox, clearance: f64) -> bool {
    a.0.x < b.1.x + clearance && b.0.x < a.1.x + clearance && a.0.y < b.1.y + clearance && b.0.y < a.1.y + clearance
}

/// Whether segment `p`-`q` comes within `clearance` of box `b` (its own box test, then the
/// segment sampled: labels are short, so a few samples are enough).
fn segment_meets(b: &Bbox, p: Pt, q: Pt, clearance: f64) -> bool {
    let grown = (Pt::new(b.0.x - clearance, b.0.y - clearance), Pt::new(b.1.x + clearance, b.1.y + clearance));
    let seg = (Pt::new(p.x.min(q.x), p.y.min(q.y)), Pt::new(p.x.max(q.x), p.y.max(q.y)));
    if !boxes_meet(&grown, &seg, 0.0) {
        return false;
    }
    let len = p.dist(q);
    let steps = ((len / 0.5).ceil() as usize).clamp(1, 2_000);
    (0..=steps).any(|i| {
        let t = i as f64 / steps as f64;
        let x = Pt::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
        x.x >= grown.0.x && x.x <= grown.1.x && x.y >= grown.0.y && x.y <= grown.1.y
    })
}

/// The places tried for a label of width `w` on segment `a`-`b`, nearest first (owner,
/// 2026-11-09: labels must not touch): above the line at its middle, then below it, then moved
/// along the segment, then just past either end of it (a label longer than a short segment), then
/// one text height further from the line.
fn candidates(a: Pt, b: Pt, w: f64, h: f64) -> Vec<Pt> {
    let theta = reading_angle(a, b);
    let (d, n) = (Pt::new(theta.cos(), theta.sin()), Pt::new(-theta.sin(), theta.cos()));
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let len = a.dist(b);
    let g = gap(h);
    let past = len / 2.0 + w / 2.0 + LABEL_CLEARANCE;
    let shifts = [0.0, 0.25 * len, -0.25 * len, 0.4 * len, -0.4 * len, past, -past];
    let mut out = Vec::new();
    for ring in [0.0, h + LABEL_CLEARANCE] {
        for shift in shifts {
            for above in [true, false] {
                // Below the line the baseline is a text height and a clearance under it.
                let off = if above { g + ring } else { -(g + h + ring) };
                out.push(Pt::new(mid.x + d.x * shift + n.x * off, mid.y + d.y * shift + n.y * off));
            }
        }
    }
    out
}

/// Every conductor label on `page`: one per route representation, on its longest segment.
pub fn labels_on_page(doc: &Document, page: PageId) -> Vec<WireLabel> {
    labels_on_page_with(doc, page, &std::collections::BTreeSet::new())
}

/// [`labels_on_page`] without the routes whose items are `hidden` (a cable drawn as a fan: its
/// members' routes and their labels are not shown, brief 005 and owner 2026-11-09).
///
/// # Semantics
/// Labels are placed in route order. Each takes the first place ([`candidates`]) where its box
/// keeps [`LABEL_CLEARANCE`] from the labels already placed and from every other wire on the
/// page; failing that, the first place clear of labels; failing that, the first place.
pub fn labels_on_page_with(doc: &Document, page: PageId, hidden: &std::collections::BTreeSet<ItemId>) -> Vec<WireLabel> {
    // Brief 032 §4.3: the wire label height, weight and width through lane T's text styles.
    let style = crate::text_style::text_style(doc, crate::text_style::TextKind::WireLabel);
    let fonts = crate::text_style::Fonts::resolve(doc);
    let h = style.height;
    // Every route on the page: (representation, conductor, vertices).
    let mut routes: Vec<(RepresentationId, ConductorId, Vec<Pt>)> = Vec::new();
    for (rid, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != Form::Drawn || r.items.first().is_some_and(|i| hidden.contains(i)) {
            continue;
        }
        let Some(ItemKind::Primitive(p)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) else {
            continue;
        };
        if let Some(v) = path_vertices(p) {
            routes.push((rid, c, v));
        }
    }
    // Brief 032 §4.2: which routes get a label (the project's rule; a wire whose label lane T's
    // getter hides is left out, so its net's next route takes the label).
    let conn = doc.connectivity();
    let mut routes_in: Vec<LabelCandidate> = Vec::new();
    let mut texts: Vec<(usize, ecad_model::wiring::Label)> = Vec::new();
    // Brief 050b (design 050 §5.5): each wire's category gives its label's cross-section and
    // colour and its label rule.
    let looks = crate::styles::wire::WireLooks::new(doc);
    let mut rules: Vec<String> = Vec::new();
    for (i, (rid, c, v)) in routes.iter().enumerate() {
        let Some((a, b)) = longest_segment(v) else { continue };
        let look = looks.look(doc, *c);
        if look.label_rule == "none" {
            continue;
        }
        let Some(label) = ecad_model::wiring::label_with(doc, *c, look.cross_section.map(|x| x.0), look.colour.map(|x| x.0)) else { continue };
        let target = crate::text_style::TextTarget::conductor(*c, Some(page));
        if crate::text_style::text_shown(doc, target, crate::text_style::TextKind::WireLabel) == crate::text_style::Shown::Off {
            continue;
        }
        routes_in.push(LabelCandidate { rep: *rid, conductor: *c, net: conn.net_of_conductor(*c), text: label.text.clone(), longest: a.dist(b) });
        texts.push((i, label));
        rules.push(look.label_rule);
    }
    // One pass per rule over the candidates that follow it (the project's rule for most).
    let mut chosen_ix: Vec<usize> = Vec::new();
    let project_rule = label_rule(doc);
    for rule in rules.iter().collect::<std::collections::BTreeSet<_>>() {
        let keep: Vec<usize> = (0..routes_in.len()).filter(|k| rules.get(*k) == Some(rule)).collect();
        let subset: Vec<LabelCandidate> = keep.iter().filter_map(|k| routes_in.get(*k).cloned()).collect();
        let r = match rule.as_str() {
            ecad_model::wiring::LABEL_RULE_EACH => LabelRule::Each(label_min_length(doc)),
            ecad_model::wiring::LABEL_RULE_NET => LabelRule::Net,
            _ => project_rule,
        };
        chosen_ix.extend(labelled(&subset, r).into_iter().filter_map(|j| keep.get(j).copied()));
    }
    chosen_ix.sort_unstable();
    let chosen: Vec<(usize, ecad_model::wiring::Label)> = chosen_ix.into_iter().filter_map(|k| texts.get(k).cloned()).collect();
    let mut placed: Vec<Bbox> = Vec::new();
    let mut out = Vec::new();
    for (i, label) in chosen {
        let Some((rid, c, v)) = routes.get(i) else { continue };
        // Brief 050a: a conductor's own wire label style (height, weight, colour, font).
        let own = doc.conductors.get(c).and_then(|x| {
            let at = crate::defaults::Scope { conductor: Some(*c), ..crate::defaults::Scope::project() };
            crate::text_style::own_style(doc, crate::text_style::TextKind::WireLabel, at, &x.properties)
        });
        let h = own.as_ref().map_or(h, |o| o.style.height);
        let Some((a, b)) = longest_segment(v) else { continue };
        let theta = reading_angle(a, b);
        if !theta.is_finite() {
            continue;
        }
        let w = match &own {
            Some(o) => {
                let f = o.family.as_ref().map_or_else(|| fonts.clone(), |fam| crate::text_style::Fonts::for_family(doc, fam.clone()));
                crate::text_style::width_in(&f, o.style, &label.text)
            }
            None => crate::text_style::width_in(&fonts, style, &label.text),
        };
        let w = if w.is_finite() && w > 0.0 { w } else { label.text.chars().count() as f64 * h * 0.8 };
        let others = || routes.iter().filter(|(r, _, _)| r != rid).flat_map(|(_, _, v)| v.windows(2));
        let tried: Vec<(Pt, Bbox)> = candidates(a, b, w, h).into_iter().filter(|p| p.is_finite()).map(|p| (p, text_box(p, theta, w, h))).collect();
        let clear_of_labels = |bx: &Bbox| placed.iter().all(|q| !boxes_meet(bx, q, LABEL_CLEARANCE));
        let clear_of_wires = |bx: &Bbox| {
            others().all(|s| match (s.first(), s.get(1)) {
                (Some(p), Some(q)) => !segment_meets(bx, *p, *q, LABEL_CLEARANCE / 2.0),
                _ => true,
            })
        };
        let pick = tried
            .iter()
            .find(|(_, bx)| clear_of_labels(bx) && clear_of_wires(bx))
            .or_else(|| tried.iter().find(|(_, bx)| clear_of_labels(bx)))
            .or_else(|| tried.first());
        let Some((at, bx)) = pick else { continue };
        placed.push(*bx);
        out.push(WireLabel {
            conductor: *c,
            rep: *rid,
            at: *at,
            rotation: theta,
            height: h,
            text: label.text,
            error: label.error,
            provisional: label.provisional,
            width: w,
            own,
        });
    }
    out
}

/// A label as an entity on [`LABEL_LAYER`].
///
/// Its style (the wire-label kind's font, weight, and bold lineweight in the stroke font) comes
/// from lane T's resolved `look` of the page ([`crate::text_style::Look::apply`]).
pub fn label_entity(look: &crate::text_style::Look, l: &WireLabel, handle: Handle) -> Entity {
    let p = Vec3::new(l.at.x, l.at.y, 0.0);
    let color = if l.error {
        cadcraft_engine::doc::color::Color::Index(crate::references::ORPHAN_ACI)
    } else if l.provisional {
        cadcraft_engine::doc::color::Color::Index(crate::devices::PROVISIONAL_ACI)
    } else {
        cadcraft_engine::doc::color::Color::ByLayer
    };
    let mut e = Entity {
        handle,
        common: Common { layer: LABEL_LAYER.into(), color, ..Common::default() },
        kind: EntityKind::Text(Text {
            insert: p,
            align_pt: Some(p),
            height: l.height,
            value: l.text.clone(),
            rotation: l.rotation,
            width_factor: 1.0,
            oblique: 0.0,
            style: crate::text_style::STYLE_REGULAR.into(),
            halign: HAlign::Center,
            valign: VAlign::Bottom,
        }),
    };
    match &l.own {
        Some(o) => look.apply_own(&mut e, crate::text_style::TextKind::WireLabel, o),
        None => look.apply(&mut e, crate::text_style::TextKind::WireLabel),
    }
    // Brief 050a: an error or provisional label keeps its own colour over the style's.
    if l.error || l.provisional {
        e.common.color = color;
    }
    e
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
    let mut others: Vec<(((usize, u32, u32), String), ComponentId)> =
        doc.continuations_named(&key).into_iter().filter(|o| *o != c).filter_map(|o| place_of(doc, o).map(|p| (p, o))).collect();
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
pub fn continuation_texts(doc: &Document, c: ComponentId, rep: RepresentationId, pl: &Placement) -> Option<Vec<DerivedText>> {
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
        own: None,
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
            own: None,
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
    doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, s: &Starter, placement: Placement, props: ecad_model::Properties,
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
    ops.push(Op::CreateComponent { id: component, component: Component { properties: props, definition: Some(definition) } });
    ops.push(Op::AddTerminal { id: terminal, terminal: Terminal { component, label: None, key: Some("1".into()) } });
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition, placement }) } });
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
pub fn plan_junction(doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, on: OnWire) -> Result<Junction, String> {
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
    let pages: std::collections::BTreeSet<PageId> = doc.representations_of(Subject::Conductor(c)).map(|(_, r)| r.page).collect();
    if pages.len() > 1 {
        return Err(format!("This wire is drawn on {} pages; add the junction where it is a single wire.", pages.len()));
    }
    let (item_id, item, v) = route_vertices(doc, c, page).ok_or("The wire has no route on this page.")?;
    for (_, r) in doc.representations_on(page) {
        let Subject::Component(j) = r.subject else { continue };
        if kind_of(doc, j) != Some(JUNCTION) {
            continue;
        }
        let here = doc.terminals_of(j).any(|(t, _)| ecad_model::derive::terminal_position(doc, page, t).is_some_and(|p| p.near(on.at, ON_WIRE)));
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
    tagless.ops.push(Op::CreateConductor { id: c2, conductor: Conductor { properties: cond.properties.clone(), ends: [Some(j), old_b] } });
    let mut second = item;
    second.kind = ItemKind::Primitive(path(rest));
    tagless.ops.push(Op::AddItem { id: i2, item: second });
    let role =
        doc.representations_of(Subject::Conductor(c)).find(|(_, r)| r.page == page).map_or_else(|| "route".to_string(), |(_, r)| r.role.clone());
    tagless.ops.push(Op::LinkRepresentation {
        id: r2,
        rep: Representation { subject: Subject::Conductor(c2), page, form: Form::Drawn, role, items: vec![i2], connection_points: BTreeMap::new() },
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
        .filter(|t| doc.terminals.get(t).is_none_or(|x| !matches!(kind_of(doc, x.component), Some(JUNCTION | CONTINUATION))))
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
    if !doc.conductors.contains_key(&w) {
        return lines;
    }
    let number = ecad_model::wiring::net_number(doc, net).unwrap_or_else(|| "? (no number yet)".into());
    let fixed = if ecad_model::wiring::net_fixed(net) { " (fixed)" } else { "" };
    lines.push(format!("Wire number {number}{fixed}"));
    let pot = ecad_model::wiring::net_potential(doc, net);
    if let Some(p) = &pot.name {
        let from = if pot.from_continuation { " (continuation)" } else { "" };
        lines.push(format!("Potential {p}{from}"));
    }
    // Brief 050b: the wire's own values, else its category's (named).
    let look = crate::styles::wire::wire_look(doc, w);
    let w = &doc.settings.wiring;
    let cat = look.category.as_ref().map(|id| crate::styles::name(doc, "wire", id));
    let shown = |key: &str, v: &Option<(String, crate::defaults::Source)>| {
        let text = |v: &str| if key == ecad_model::wiring::COLOUR { w.colour(v) } else { v.to_string() };
        match (v, &cat) {
            (Some((v, crate::defaults::Source::Object)), _) => Some(text(v)),
            (Some((v, _)), Some(n)) => Some(format!("{} ({n})", text(v))),
            (Some((v, _)), None) => Some(text(v)),
            (None, _) => None,
        }
    };
    let mut values = Vec::new();
    if let Some(v) = shown(ecad_model::wiring::CROSS_SECTION, &look.cross_section) {
        values.push(format!("Cross-section {v}"));
    }
    if let Some(v) = shown(ecad_model::wiring::COLOUR, &look.colour) {
        values.push(format!("Colour {v}"));
    }
    if !values.is_empty() {
        lines.push(values.join(" \u{b7} "));
    }
    lines.push(net_line(doc, net));
    lines
}

// ---------------------------------------------------------------- joining (brief 032 §2)

/// The way out every join refusal names (design 032 §2.2).
pub const JOIN_WAY_OUT: &str = "Draw it with join off (join: false), or end it on a connection point.";

/// What one end of a new wire meets (design 032 §2.1).
#[derive(Debug, Clone)]
enum Meets {
    /// Nothing there: a deliberate open end.
    Open,
    /// A connection point there.
    Point(TerminalId),
    /// The one open end of a wire that takes the new one (§2.2), with that wire's route.
    Merge { conductor: ConductorId, end: End, route: (ItemId, Item, Vec<Pt>) },
    /// A new junction there: its ops, its terminal and component, how many other wire ends it
    /// joins, the wire it splits (if any) and the conductors whose ends it attaches.
    Junction { ops: Vec<Op>, terminal: TerminalId, component: ComponentId, others: usize, split: Option<ConductorId>, touched: Vec<ConductorId> },
}

/// A planned `wire` (design 032 §2.3): its ops and what the result reports.
#[derive(Debug, Clone)]
pub struct WirePlan {
    pub ops: Vec<Op>,
    /// The wire the drawn route (its first piece) now belongs to.
    pub conductor: ConductorId,
    /// That wire's route item.
    pub item: ItemId,
    /// False when the route was merged into an existing wire.
    pub created: bool,
    /// The ends that merged or met other wire ends at a junction: (end, the other wire, how, how
    /// many other wires met there).
    pub joined: Vec<(End, ConductorId, &'static str, usize)>,
    /// Wires folded into the survivor and deleted.
    pub removed: Vec<ConductorId>,
    /// Ends attached to a connection point that was already there.
    pub attached: Vec<(End, TerminalId)>,
    /// Junctions placed at the ends: (end, junction, point).
    pub junctions: Vec<(End, ComponentId, Pt)>,
    /// Reverse T: (point, junction, the new wire piece that starts there).
    pub splits: Vec<(Pt, ComponentId, ConductorId)>,
}

/// The other end of a wire.
fn other_end(e: End) -> End {
    match e {
        End::A => End::B,
        End::B => End::A,
    }
}

/// Whether `a`-`b`-`c` runs straight on through `b` (one line, same direction).
fn straight(a: Pt, b: Pt, c: Pt) -> bool {
    let (u, v) = (Pt::new(b.x - a.x, b.y - a.y), Pt::new(c.x - b.x, c.y - b.y));
    let (lu, lv) = (u.x.hypot(u.y), v.x.hypot(v.y));
    let cross = u.x * v.y - u.y * v.x;
    let dot = u.x * v.x + u.y * v.y;
    lu > 0.0 && lv > 0.0 && cross.abs() <= 1e-9 * lu * lv && dot > 0.0
}

/// `a` (ending at a point P) followed by `b` (starting at P): P once, dropped when the two
/// segments meeting there are one straight line (design 032 §2.2).
fn joined(mut a: Vec<Pt>, b: &[Pt]) -> Vec<Pt> {
    let Some((p, rest)) = b.split_first() else { return a };
    if a.is_empty() {
        a.push(*p);
    }
    let at = a.len().saturating_sub(1);
    a.extend_from_slice(rest);
    if let (Some(prev), Some(mid), Some(next)) = (at.checked_sub(1).and_then(|i| a.get(i)), a.get(at), a.get(at + 1))
        && straight(*prev, *mid, *next)
    {
        a.remove(at);
    }
    a
}

fn reversed(v: &[Pt]) -> Vec<Pt> {
    v.iter().rev().copied().collect()
}

/// The route of wire C (vertices `cv`, open end `open`) with `add` (starting at that end) joined
/// on, so C keeps its end letters.
fn merged_route(cv: &[Pt], open: End, add: &[Pt]) -> Vec<Pt> {
    match open {
        End::B => joined(cv.to_vec(), add),
        End::A => joined(reversed(add), cv),
    }
}

/// A route path as a primitive (a line for two points).
fn path_of(pts: Vec<Pt>) -> Primitive {
    match pts.as_slice() {
        [a, b] => Primitive::Line { a: *a, b: *b },
        _ => Primitive::Polyline { vertices: pts, closed: false },
    }
}

/// Whether a wire end at `at` is quiet (a cable reference end, brief 009): never joined.
fn is_quiet(quiet: &[Pt], at: Pt) -> bool {
    quiet.iter().any(|q| q.near(at, ON_WIRE))
}

/// The wire whose interior (not an end) passes within [`ON_WIRE`] of `at`, the nearest.
pub fn interior_wire_at(doc: &Document, page: PageId, at: Pt) -> Option<OnWire> {
    wire_at(doc, page, at, ON_WIRE, None).filter(|on| {
        route_vertices(doc, on.conductor, page)
            .is_some_and(|(_, _, v)| !v.first().is_some_and(|f| f.near(on.at, ON_WIRE)) && !v.last().is_some_and(|l| l.near(on.at, ON_WIRE)))
    })
}

/// Whether wire `c` can take a new wire end to end (design 032 §2.2): one route, on `page`, not a
/// cable member.
pub fn mergeable(doc: &Document, c: ConductorId, page: PageId) -> bool {
    let mut reps = doc.representations_of(Subject::Conductor(c));
    let one_here = reps.next().is_some_and(|(_, r)| r.page == page && r.form == Form::Drawn) && reps.next().is_none();
    one_here && doc.cables_of(c).is_empty()
}

/// The words of a wire in a refusal: its number, else `that wire`.
fn wire_words(doc: &Document, c: ConductorId) -> String {
    let conn = doc.connectivity();
    conn.net_of_conductor(c).and_then(|n| conn.net(n)).and_then(|n| ecad_model::wiring::net_number(doc, n)).unwrap_or_else(|| "that wire".into())
}

/// Refuse merging wire `c` with a new wire of potential `potential` (design 032 §2.2).
fn merge_potential(doc: &Document, c: ConductorId, potential: Option<&str>) -> Result<(), String> {
    let conn = doc.connectivity();
    let Some(net) = conn.net_of_conductor(c).and_then(|n| conn.net(n)) else { return Ok(()) };
    let theirs = ecad_model::wiring::net_potential(doc, net).name;
    if let (Some(x), Some(y)) = (theirs.as_deref(), potential)
        && !x.eq_ignore_ascii_case(y)
    {
        return Err(format!("That wire carries {x} and this one {y}; joining them would short them. {JOIN_WAY_OUT}"));
    }
    Ok(())
}

/// Refuse folding wires `a` and `b` into one (design 032 §2.2): potentials, fixed numbers, stored
/// cross-sections and colours that disagree.
pub fn fold_sense(doc: &Document, a: ConductorId, b: ConductorId) -> Result<(), String> {
    let conn = doc.connectivity();
    if let (Some(na), Some(nb)) = (conn.net_of_conductor(a), conn.net_of_conductor(b))
        && na != nb
        && let (Some(x), Some(y)) = (conn.net(na), conn.net(nb))
    {
        let (px, py) = (ecad_model::wiring::net_potential(doc, x).name, ecad_model::wiring::net_potential(doc, y).name);
        if let (Some(px), Some(py)) = (&px, &py)
            && !px.eq_ignore_ascii_case(py)
        {
            return Err(format!("That wire carries {px} and this one {py}; joining them would short them. {JOIN_WAY_OUT}"));
        }
        if ecad_model::wiring::net_fixed(x) && ecad_model::wiring::net_fixed(y) {
            let (fx, fy) = (ecad_model::wiring::net_number(doc, x).unwrap_or_default(), ecad_model::wiring::net_number(doc, y).unwrap_or_default());
            if fx != fy {
                return Err(format!("Both wires have fixed numbers of their own ({fx} and {fy}). {JOIN_WAY_OUT}"));
            }
        }
    }
    let (Some(ca), Some(cb)) = (doc.conductors.get(&a), doc.conductors.get(&b)) else { return Ok(()) };
    for (key, words) in [(ecad_model::wiring::CROSS_SECTION, "cross-sections"), (ecad_model::wiring::COLOUR, "colours")] {
        if let (Some(x), Some(y)) = (ca.properties.get(key), cb.properties.get(key))
            && x != y
        {
            return Err(format!("The two wires have different {words} ({x} and {y}). {JOIN_WAY_OUT}"));
        }
    }
    Ok(())
}

/// The ops that carry folded wire `o`'s own properties onto survivor `s` (overseer ruling on
/// design 032 §2.2: never lose them silently). Net-wide numbering keys are left to the numbering
/// pass. A value both wires store differently is refused by [`fold_sense`] (cross-section,
/// colour) or here (any other key), with words.
fn fold_properties(doc: &Document, s: ConductorId, o: ConductorId) -> Result<Vec<Op>, String> {
    let (Some(sc), Some(oc)) = (doc.conductors.get(&s), doc.conductors.get(&o)) else { return Ok(Vec::new()) };
    let mut ops = Vec::new();
    for (k, v) in &oc.properties {
        if k == ecad_model::wiring::WIRE_NUMBER_KEY || k == ecad_model::connectivity::WIRE_NUMBER_FIXED {
            continue;
        }
        match sc.properties.get(k) {
            Some(x) if x == v => {}
            Some(x) => return Err(format!("The two wires have different values of {k} ({x} and {v}). {JOIN_WAY_OUT}")),
            None => ops.push(Op::SetProperty { object: ecad_model::ObjectRef::Conductor(s), key: k.clone(), before: None, after: Some(v.clone()) }),
        }
    }
    Ok(ops)
}

/// Whether a wire's net has a fixed number.
pub fn has_fixed_number(doc: &Document, c: ConductorId) -> bool {
    let conn = doc.connectivity();
    conn.net_of_conductor(c).and_then(|n| conn.net(n)).is_some_and(ecad_model::wiring::net_fixed)
}

/// A junction made at `at` (no wire split): it attaches the open ends there.
fn junction_here(doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, at: Pt) -> Result<Tagless, String> {
    let starter = junction_starter().ok_or("the junction symbol is missing")?;
    let mut props = ecad_model::Properties::new();
    props.insert(KIND.into(), JUNCTION.into());
    plan_tagless(doc, ids, page, style, starter, Placement { origin: at, rotation: 0.0, mirror: false, scale: 1.0 }, props)
}

/// The point of end `e` of `c`'s route on `page`.
fn end_point(doc: &Document, c: ConductorId, e: End, page: PageId) -> Option<Pt> {
    ecad_model::derive::route_ends_on(doc, c, page).map(|(a, b)| if e == End::A { a } else { b })
}

/// The open ends of other wires at `at` on `page` that a new wire may join (not quiet ones).
pub fn joinable_ends_at(doc: &Document, page: PageId, at: Pt, quiet: &[Pt]) -> Vec<(ConductorId, End)> {
    loose_ends_at(doc, page, at, ON_WIRE).into_iter().filter(|(c, e)| end_point(doc, *c, *e, page).is_some_and(|p| !is_quiet(quiet, p))).collect()
}

/// What the end of a new wire at `at` meets (design 032 §2.1, step 2), and the point it snaps to.
fn meet_at(doc: &Document, ids: &mut IdAllocator, page: PageId, style: &Style, at: Pt, quiet: &[Pt]) -> Result<(Meets, Pt), String> {
    let loose = joinable_ends_at(doc, page, at, quiet);
    let through = interior_wire_at(doc, page, at);
    match (loose.as_slice(), through) {
        ([], None) => Ok((Meets::Open, at)),
        ([(c, e)], None) if mergeable(doc, *c, page) => {
            let route = route_vertices(doc, *c, page).ok_or("The wire has no route on this page.")?;
            let p = end_point(doc, *c, *e, page).unwrap_or(at);
            Ok((Meets::Merge { conductor: *c, end: *e, route }, p))
        }
        (_, Some(on)) => {
            let j = plan_junction(doc, ids, page, style.clone(), on).map_err(|why| format!("{why} {JOIN_WAY_OUT}"))?;
            let touched = j.tagless.attached.iter().map(|(c, _)| *c).collect();
            let meets = Meets::Junction {
                ops: j.tagless.ops,
                terminal: j.tagless.terminal,
                component: j.tagless.component,
                others: j.tagless.attached.len() + 1,
                split: Some(on.conductor),
                touched,
            };
            Ok((meets, j.at))
        }
        ([(c, e), ..], None) => {
            let p = end_point(doc, *c, *e, page).unwrap_or(at);
            let t = junction_here(doc, ids, page, style.clone(), p)?;
            let touched = t.attached.iter().map(|(c, _)| *c).collect();
            Ok((Meets::Junction { ops: t.ops, terminal: t.terminal, component: t.component, others: t.attached.len(), split: None, touched }, p))
        }
    }
}

/// The open ends of other wires on `page` that lie on the interior of path `v` (design 032 §2.1
/// step 3), each as (segment index, distance along it, point), in order along the path, one per
/// point.
fn ends_on_path(doc: &Document, page: PageId, v: &[Pt], quiet: &[Pt]) -> Vec<(usize, f64, Pt)> {
    let (Some(first), Some(last)) = (v.first().copied(), v.last().copied()) else { return Vec::new() };
    // A grid of the new wire's segments (overseer ruling: no segment-by-segment test of every end
    // on the page): an end is tested only against the segments in its cell.
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    let mut long: Vec<usize> = Vec::new();
    for (i, w) in v.windows(2).enumerate() {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let (Some(lo), Some(hi)) = (
            looks_cell(Pt::new(a.x.min(b.x) - ON_WIRE, a.y.min(b.y) - ON_WIRE)),
            looks_cell(Pt::new(a.x.max(b.x) + ON_WIRE, a.y.max(b.y) + ON_WIRE)),
        ) else {
            continue;
        };
        let n = (hi.0.saturating_sub(lo.0).saturating_add(1)).saturating_mul(hi.1.saturating_sub(lo.1).saturating_add(1));
        if usize::try_from(n).map_or(true, |n| n > LOOKS_MAX_CELLS) {
            long.push(i);
            continue;
        }
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                grid.entry((x, y)).or_default().push(i);
            }
        }
    }
    let mut out: Vec<(usize, f64, Pt)> = Vec::new();
    for (_, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != Form::Drawn {
            continue;
        }
        let Some(cond) = doc.conductors.get(&c) else { continue };
        let Some((a, b)) = ecad_model::derive::route_ends_on(doc, c, page) else { continue };
        for (end, q) in [(End::A, a), (End::B, b)] {
            if cond.end(end).is_some() || !q.is_finite() || q.near(first, ON_WIRE) || q.near(last, ON_WIRE) || is_quiet(quiet, q) {
                continue;
            }
            if out.iter().any(|(_, _, x)| x.near(q, ON_WIRE)) {
                continue;
            }
            let Some(cell) = looks_cell(q) else { continue };
            let mut segs: Vec<usize> = grid.get(&cell).into_iter().flatten().chain(long.iter()).copied().collect();
            segs.sort_unstable();
            segs.dedup();
            let hit = segs.into_iter().find_map(|i| {
                let (s, e) = (*v.get(i)?, *v.get(i + 1)?);
                let (on, d) = on_segment(q, s, e);
                (d <= ON_WIRE && on.is_finite()).then(|| (i, s.dist(on)))
            });
            if let Some((i, along)) = hit {
                out.push((i, along, q));
            }
        }
    }
    out.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)));
    out
}

/// The inputs of [`plan_wire`] besides the document and IDs.
#[derive(Debug, Clone)]
pub struct WireInput<'a> {
    pub page: PageId,
    pub style: Style,
    pub points: Vec<Pt>,
    pub potential: Option<&'a str>,
    /// Join ends that meet other wires (design 032 §2.1 steps 2 and 3).
    pub join: bool,
    /// The project tolerance (an end on a connection point).
    pub tol: f64,
    /// The page's quiet ends (cable reference ends): never joined.
    pub quiet: &'a [Pt],
    /// Most points a route may have, and most wire ends one wire may cross.
    pub max_points: usize,
}

/// Plan a new wire (design 032 §2): ends on connection points attach; with `join`, ends meeting
/// other wires merge or get a junction, and open ends of other wires lying on it split it with a
/// junction (the reverse T).
///
/// # Errors
/// Design 032 §2.2's refusals (potentials, fixed numbers, a loop, too many points or splits), and
/// a junction that cannot be placed; nothing is planned then.
pub fn plan_wire(doc: &Document, ids: &mut IdAllocator, input: WireInput<'_>) -> Result<WirePlan, String> {
    let WireInput { page, style, points: mut pts, potential, join, tol, quiet, max_points } = input;
    let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else { return Err("a wire needs two points".into()) };
    let mut shown = crate::translate::ShownOnce::new(doc, page);
    let loop_refusal =
        |c: ConductorId| format!("This wire meets wire {} at two places, which would close it into a loop. {JOIN_WAY_OUT}", wire_words(doc, c));
    // 1. The ends.
    let mut meets: Vec<Meets> = Vec::with_capacity(2);
    for (i, at) in [first, last].into_iter().enumerate() {
        let (m, p) = if let Some(t) = shown.terminal_at(at, tol) {
            (Meets::Point(t), at)
        } else if join {
            meet_at(doc, ids, page, &style, at, quiet)?
        } else {
            (Meets::Open, at)
        };
        let slot = if i == 0 { pts.first_mut() } else { pts.last_mut() };
        if let Some(s) = slot {
            *s = p;
        }
        meets.push(m);
    }
    let ma = meets.first().cloned().unwrap_or(Meets::Open);
    let mb = meets.get(1).cloned().unwrap_or(Meets::Open);
    if let (Meets::Merge { conductor: a, .. }, Meets::Merge { conductor: b, .. }) = (&ma, &mb)
        && a == b
    {
        return Err(format!("Both ends land on the ends of one wire: that would close it into a loop. {JOIN_WAY_OUT}"));
    }
    // Conductors this plan splits or merges: each at most once, and no other end of the plan
    // attaches to them (else the ops would contradict each other).
    let mut exclusive: Vec<ConductorId> = Vec::new();
    let mut touched: Vec<ConductorId> = Vec::new();
    for m in [&ma, &mb] {
        match m {
            Meets::Merge { conductor, .. } => exclusive.push(*conductor),
            Meets::Junction { split, touched: t, .. } => {
                exclusive.extend(split.iter().copied());
                touched.extend(t.iter().copied());
            }
            _ => {}
        }
    }
    // 2. The reverse T: open ends lying on the new wire's interior split it.
    let mut splits: Vec<(Pt, Tagless)> = Vec::new();
    let mut pieces: Vec<Vec<Pt>> = vec![pts.clone()];
    if join {
        let on_path = ends_on_path(doc, page, &pts, quiet);
        if on_path.len() > max_points {
            return Err(format!("This wire crosses more than {max_points} wire ends; draw it in parts."));
        }
        if !on_path.is_empty() {
            let mut current: Vec<Pt> = Vec::new();
            pieces.clear();
            let mut k = 0usize;
            for (i, w) in pts.windows(2).enumerate() {
                let (Some(s), Some(e)) = (w.first().copied(), w.get(1).copied()) else { continue };
                if current.is_empty() {
                    current.push(s);
                }
                while let Some((seg, _, q)) = on_path.get(k).copied()
                    && seg == i
                {
                    k += 1;
                    if shown.terminal_at(q, tol).is_some() {
                        continue;
                    }
                    let t = junction_here(doc, ids, page, style.clone(), q)?;
                    touched.extend(t.attached.iter().map(|(c, _)| *c));
                    if current.last().is_none_or(|l| !l.near(q, 1e-9)) {
                        current.push(q);
                    }
                    pieces.push(std::mem::take(&mut current));
                    current.push(q);
                    splits.push((q, t));
                }
                if current.last().is_none_or(|l| !l.near(e, 1e-9)) {
                    current.push(e);
                }
            }
            pieces.push(current);
            if pieces.iter().any(|p| p.len() < 2) || pieces.len() != splits.len() + 1 {
                return Err("This wire would split where it starts or ends; draw it in parts.".into());
            }
        }
    }
    for c in &touched {
        if exclusive.contains(c) {
            return Err(loop_refusal(*c));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for c in &exclusive {
        if !seen.insert(*c) {
            return Err(loop_refusal(*c));
        }
    }
    let mut plan = WirePlan {
        ops: Vec::new(),
        conductor: ConductorId::from(0u128),
        item: ItemId::from(0u128),
        created: true,
        joined: Vec::new(),
        removed: Vec::new(),
        attached: Vec::new(),
        junctions: Vec::new(),
        splits: Vec::new(),
    };
    // The ops of the junctions come first: the ends attach to their connection points.
    let mut ops: Vec<Op> = Vec::new();
    for m in [&ma, &mb] {
        if let Meets::Junction { ops: j, .. } = m {
            ops.extend(j.iter().cloned());
        }
    }
    for (_, t) in &splits {
        ops.extend(t.ops.iter().cloned());
    }
    for (end, m) in [(End::A, &ma), (End::B, &mb)] {
        match m {
            Meets::Point(t) => plan.attached.push((end, *t)),
            Meets::Junction { component, others, touched: t, .. } => {
                let at = if end == End::A { pts.first() } else { pts.last() };
                plan.junctions.push((end, *component, at.copied().unwrap_or(first)));
                if let Some(c) = t.first()
                    && *others >= 2
                {
                    plan.joined.push((end, *c, "junction", *others));
                }
            }
            Meets::Merge { conductor, .. } => plan.joined.push((end, *conductor, "merged", 1)),
            Meets::Open => {}
        }
    }
    // What each end of the whole wire attaches to (None: open, or merged).
    let end_terminal = |m: &Meets| match m {
        Meets::Point(t) => Some(*t),
        Meets::Junction { terminal, .. } => Some(*terminal),
        _ => None,
    };
    let n = pieces.len();
    let mut conductor = Conductor::default();
    if let Some(v) = potential {
        conductor.properties.insert(ecad_model::wiring::POTENTIAL_KEY.into(), v.to_string());
    }
    // The potential the drawn wire carries goes on the wire that takes it, when it has none.
    let give_potential = |c: ConductorId, ops: &mut Vec<Op>| {
        let Some(v) = potential else { return };
        if doc.conductors.get(&c).is_some_and(|x| !x.properties.contains_key(ecad_model::wiring::POTENTIAL_KEY)) {
            ops.push(Op::SetProperty {
                object: ecad_model::ObjectRef::Conductor(c),
                key: ecad_model::wiring::POTENTIAL_KEY.into(),
                before: None,
                after: Some(v.to_string()),
            });
        }
    };
    let too_long = || format!("The joined wire would have more than {max_points} points.");
    let mut attach: Vec<Op> = Vec::new();
    let mut tail: Vec<Op> = Vec::new();
    let mut owner: Option<(ConductorId, ItemId)> = None;
    if let (Meets::Merge { conductor: c1, end: e1, route: r1 }, Meets::Merge { conductor: c2, end: e2, route: r2 }, 1) = (&ma, &mb, n) {
        // Both ends meet open ends of two wires: one wire (§2.2).
        merge_potential(doc, *c1, potential)?;
        merge_potential(doc, *c2, potential)?;
        fold_sense(doc, *c1, *c2)?;
        // The survivor: the one with a fixed number, else the wire at end A.
        let c1_survives = has_fixed_number(doc, *c1) || !has_fixed_number(doc, *c2);
        let (s, es, rs, o, eo, ro, w) =
            if c1_survives { (*c1, *e1, r1, *c2, *e2, r2, pts.clone()) } else { (*c2, *e2, r2, *c1, *e1, r1, reversed(&pts)) };
        let o_from_end = if eo == End::A { ro.2.clone() } else { reversed(&ro.2) };
        let route = merged_route(&rs.2, es, &joined(w, &o_from_end));
        if route.len() > max_points {
            return Err(too_long());
        }
        let mut after = rs.1.clone();
        after.kind = ItemKind::Primitive(path_of(route));
        ops.push(Op::ModifyItem { id: rs.0, before: rs.1.clone(), after });
        let folded = fold_properties(doc, s, o)?;
        let carries_potential = folded.iter().any(|op| matches!(op, Op::SetProperty { key, .. } if key == ecad_model::wiring::POTENTIAL_KEY));
        ops.extend(folded);
        if !carries_potential {
            give_potential(s, &mut ops);
        }
        let oc = doc.conductors.get(&o).ok_or("No such wire.")?;
        let far = other_end(eo);
        let mut gone = oc.clone();
        if let Some(t) = oc.end(far) {
            tail.push(Op::AttachEnd { conductor: o, end: far, before: Some(t), after: None });
            tail.push(Op::AttachEnd { conductor: s, end: es, before: None, after: Some(t) });
            gone.set_end(far, None);
        }
        for (rid, rep) in doc.representations_of(Subject::Conductor(o)) {
            tail.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
        }
        tail.push(Op::RemoveItem { id: ro.0, item: ro.1.clone() });
        tail.push(Op::DeleteConductor { id: o, conductor: gone });
        plan.removed.push(o);
        plan.created = false;
        owner = Some((s, rs.0));
    } else {
        // The terminal between pieces k and k+1.
        let split_terminal = |k: usize| splits.get(k).map(|(_, t)| t.terminal);
        for (k, piece) in pieces.iter().enumerate() {
            let ta = if k == 0 { end_terminal(&ma) } else { split_terminal(k - 1) };
            let tb = if k + 1 == n { end_terminal(&mb) } else { split_terminal(k) };
            // A piece at a merging end joins that wire: its route grows, and the piece's other
            // end's attachment goes on that wire's end.
            let merge = match (k == 0, k + 1 == n, &ma, &mb) {
                (true, _, Meets::Merge { conductor, end, route }, _) => Some((*conductor, *end, route, piece.clone(), tb)),
                (_, true, _, Meets::Merge { conductor, end, route }) => Some((*conductor, *end, route, reversed(piece), ta)),
                _ => None,
            };
            if let Some((c, e, route, from_end, far)) = merge {
                merge_potential(doc, c, potential)?;
                let r = merged_route(&route.2, e, &from_end);
                if r.len() > max_points {
                    return Err(too_long());
                }
                let mut after = route.1.clone();
                after.kind = ItemKind::Primitive(path_of(r));
                ops.push(Op::ModifyItem { id: route.0, before: route.1.clone(), after });
                give_potential(c, &mut ops);
                if let Some(t) = far {
                    attach.push(Op::AttachEnd { conductor: c, end: e, before: None, after: Some(t) });
                }
                if k == 0 {
                    owner = Some((c, route.0));
                    plan.created = false;
                }
                continue;
            }
            let (w, i, r): (ConductorId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
            ops.push(Op::CreateConductor { id: w, conductor: conductor.clone() });
            let item = Item { page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Polyline { vertices: piece.clone(), closed: false }) };
            ops.push(Op::AddItem { id: i, item });
            let rep = Representation {
                subject: Subject::Conductor(w),
                page,
                form: Form::Drawn,
                role: "route".into(),
                items: vec![i],
                connection_points: BTreeMap::new(),
            };
            ops.push(Op::LinkRepresentation { id: r, rep });
            for (end, t) in [(End::A, ta), (End::B, tb)] {
                if let Some(t) = t {
                    attach.push(Op::AttachEnd { conductor: w, end, before: None, after: Some(t) });
                }
            }
            if k == 0 {
                owner = Some((w, i));
            }
            if let Some((q, t)) = k.checked_sub(1).and_then(|j| splits.get(j)) {
                plan.splits.push((*q, t.component, w));
            }
        }
    }
    ops.extend(attach);
    ops.extend(tail);
    let (c, i) = owner.ok_or("the wire has no route")?;
    plan.conductor = c;
    plan.item = i;
    plan.ops = ops;
    Ok(plan)
}

/// A planned merge of two open ends that meet (`joinwires`, design 032 §2.5).
#[derive(Debug, Clone)]
pub struct EndJoin {
    pub ops: Vec<Op>,
    pub survivor: ConductorId,
    pub removed: ConductorId,
}

/// Plan merging the open ends `a` and `b` of two wires that meet on `page` into one wire (design
/// 032 §2.5): the wire with a fixed number survives, else the lower conductor ID; the other's far
/// attachment moves onto the survivor, and the other is deleted.
///
/// # Errors
/// One wire's two ends (a loop), a wire that cannot be merged (a cable core, drawn on two pages:
/// `Ok(None)`, the caller places a junction instead), and §2.2's refusals.
pub fn plan_end_join(
    doc: &Document, page: PageId, a: (ConductorId, End), b: (ConductorId, End), max_points: usize,
) -> Result<Option<EndJoin>, String> {
    if a.0 == b.0 {
        return Err(format!("Both ends belong to one wire: joining them would close it into a loop. {JOIN_WAY_OUT}"));
    }
    if !mergeable(doc, a.0, page) || !mergeable(doc, b.0, page) {
        return Ok(None);
    }
    fold_sense(doc, a.0, b.0)?;
    let a_survives = match (has_fixed_number(doc, a.0), has_fixed_number(doc, b.0)) {
        (true, false) => true,
        (false, true) => false,
        _ => a.0 < b.0,
    };
    let ((s, es), (o, eo)) = if a_survives { (a, b) } else { (b, a) };
    let rs = route_vertices(doc, s, page).ok_or("The wire has no route on this page.")?;
    let ro = route_vertices(doc, o, page).ok_or("The wire has no route on this page.")?;
    let o_from_end = if eo == End::A { ro.2.clone() } else { reversed(&ro.2) };
    // The other wire's open end lies on the survivor's (within ON_WIRE): it starts there.
    let mut add = o_from_end;
    if let (Some(first), Some(at)) = (add.first_mut(), end_point(doc, s, es, page)) {
        *first = at;
    }
    let route = merged_route(&rs.2, es, &add);
    if route.len() > max_points {
        return Err(format!("The joined wire would have more than {max_points} points."));
    }
    let mut ops = Vec::new();
    let mut after = rs.1.clone();
    after.kind = ItemKind::Primitive(path_of(route));
    ops.push(Op::ModifyItem { id: rs.0, before: rs.1.clone(), after });
    ops.extend(fold_properties(doc, s, o)?);
    let oc = doc.conductors.get(&o).ok_or("No such wire.")?;
    let far = other_end(eo);
    let mut gone = oc.clone();
    if let Some(t) = oc.end(far) {
        ops.push(Op::AttachEnd { conductor: o, end: far, before: Some(t), after: None });
        ops.push(Op::AttachEnd { conductor: s, end: es, before: None, after: Some(t) });
        gone.set_end(far, None);
    }
    for (rid, rep) in doc.representations_of(Subject::Conductor(o)) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
    }
    ops.push(Op::RemoveItem { id: ro.0, item: ro.1.clone() });
    ops.push(Op::DeleteConductor { id: o, conductor: gone });
    Ok(Some(EndJoin { ops, survivor: s, removed: o }))
}

/// The nearest open end of a wire on `page` within `tol` of `p` that a new wire may join (not a
/// quiet one): the Wire tool snaps a click there (design 032 §2.4).
pub fn open_end_near(doc: &Document, page: PageId, p: Pt, tol: f64, quiet: &[Pt]) -> Option<Pt> {
    let mut best: Option<(f64, Pt)> = None;
    for (_, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != Form::Drawn {
            continue;
        }
        let Some(cond) = doc.conductors.get(&c) else { continue };
        let Some((a, b)) = ecad_model::derive::route_ends_on(doc, c, page) else { continue };
        for (end, q) in [(End::A, a), (End::B, b)] {
            let d = q.dist(p);
            if cond.end(end).is_none() && d <= tol && !is_quiet(quiet, q) && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, q));
            }
        }
    }
    best.map(|(_, q)| q)
}

/// What the Wire tool shows at an end point `at` (design 032 §2.4): the point, the word (`join`,
/// `junction`, or the first words of a refusal such as `0V ≠ 24V`) and whether it is refused;
/// `None` when the end would stay open.
pub fn join_preview(doc: &Document, page: PageId, at: Pt, quiet: &[Pt], potential: Option<&str>) -> Option<(Pt, String, bool)> {
    let loose = joinable_ends_at(doc, page, at, quiet);
    let through = interior_wire_at(doc, page, at);
    match (loose.as_slice(), through) {
        ([], None) => None,
        ([(c, e)], None) if mergeable(doc, *c, page) => {
            let p = end_point(doc, *c, *e, page).unwrap_or(at);
            let conn = doc.connectivity();
            let theirs = conn.net_of_conductor(*c).and_then(|n| conn.net(n)).and_then(|n| ecad_model::wiring::net_potential(doc, n).name);
            match (theirs, potential) {
                (Some(x), Some(y)) if !x.eq_ignore_ascii_case(y) => Some((p, format!("{x} \u{2260} {y}"), true)),
                _ => Some((p, "join".into(), false)),
            }
        }
        (_, Some(on)) => Some((on.at, "junction".into(), false)),
        ([(c, e), ..], None) => Some((end_point(doc, *c, *e, page).unwrap_or(at), "junction".into(), false)),
    }
}

/// The cell size of [`looks_connected_on_page`]'s grid, millimetres.
const LOOKS_CELL_MM: f64 = 10.0;
/// Most cells one segment enters in that grid (a hostile huge segment costs no more than a page);
/// a longer segment is checked against every end instead.
const LOOKS_MAX_CELLS: usize = 4_096;

fn looks_cell(p: Pt) -> Option<(i64, i64)> {
    p.is_finite().then(|| ((p.x / LOOKS_CELL_MM).floor() as i64, (p.y / LOOKS_CELL_MM).floor() as i64))
}

/// The open wire ends on `page` that look connected (design 032 §2.6): lying on another wire's
/// end or interior, not quiet (a cable reference end). One point per place, in route order.
///
/// # Semantics
/// Computed during the page's projection from that page's routes only, with a grid of their
/// segments, so the cost follows the page, never the project (update map §8.2).
pub fn looks_connected_on_page(doc: &Document, page: PageId, quiet: &[Pt]) -> Vec<Pt> {
    // Every route on the page: (conductor, vertices).
    let mut routes: Vec<(ConductorId, Vec<Pt>)> = Vec::new();
    for (_, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != Form::Drawn {
            continue;
        }
        if let Some(v) = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
            ItemKind::Primitive(p) => path_vertices(p),
            _ => None,
        }) {
            routes.push((c, v));
        }
    }
    let mut grid: std::collections::HashMap<(i64, i64), Vec<(usize, usize)>> = std::collections::HashMap::new();
    let mut long: Vec<(usize, usize)> = Vec::new();
    for (r, (_, v)) in routes.iter().enumerate() {
        for (s, w) in v.windows(2).enumerate() {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
            let (Some(lo), Some(hi)) = (
                looks_cell(Pt::new(a.x.min(b.x) - ON_WIRE, a.y.min(b.y) - ON_WIRE)),
                looks_cell(Pt::new(a.x.max(b.x) + ON_WIRE, a.y.max(b.y) + ON_WIRE)),
            ) else {
                continue;
            };
            let n = (hi.0.saturating_sub(lo.0).saturating_add(1)).saturating_mul(hi.1.saturating_sub(lo.1).saturating_add(1));
            if usize::try_from(n).map_or(true, |n| n > LOOKS_MAX_CELLS) {
                long.push((r, s));
                continue;
            }
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    grid.entry((x, y)).or_default().push((r, s));
                }
            }
        }
    }
    let mut out: Vec<Pt> = Vec::new();
    for (r, (c, v)) in routes.iter().enumerate() {
        let Some(cond) = doc.conductors.get(c) else { continue };
        for (end, at) in [(End::A, v.first()), (End::B, v.last())] {
            let Some(at) = at.copied() else { continue };
            if cond.end(end).is_some() || is_quiet(quiet, at) || out.iter().any(|q| q.near(at, ON_WIRE)) {
                continue;
            }
            let Some(cell) = looks_cell(at) else { continue };
            let near = grid.get(&cell).into_iter().flatten().chain(long.iter()).any(|(o, s)| {
                if *o == r {
                    return false;
                }
                let Some((_, ov)) = routes.get(*o) else { return false };
                let (Some(a), Some(b)) = (ov.get(*s), ov.get(s + 1)) else { return false };
                let (q, d) = on_segment(at, *a, *b);
                d <= ON_WIRE && q.is_finite()
            });
            if near {
                out.push(at);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- the label rule (brief 032 §4)

/// Which routes on a page get a label (design 032 §4.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LabelRule {
    /// One label per net (and label text) per page.
    Net,
    /// Every wire whose longest segment is at least this long, millimetres.
    Each(f64),
}

/// One route as the label rule sees it.
#[derive(Debug, Clone)]
pub struct LabelCandidate {
    pub rep: RepresentationId,
    pub conductor: ConductorId,
    /// The route's net, if it has one.
    pub net: Option<ecad_model::NetId>,
    pub text: String,
    /// Length of its longest segment.
    pub longest: f64,
}

/// The candidates (by index) that get a label under `rule` (design 032 §4.2): with
/// [`LabelRule::Net`] one per (net, label text), on the route whose longest segment is longest
/// (ties: lower conductor ID, then lower representation ID); with [`LabelRule::Each`] every route
/// long enough. In both, every net drawn on the page is labelled at least once. The result keeps
/// the candidates' order.
pub fn labelled(candidates: &[LabelCandidate], rule: LabelRule) -> Vec<usize> {
    // The better of two candidates for one label.
    let better =
        |a: &LabelCandidate, b: &LabelCandidate| a.longest > b.longest || (a.longest == b.longest && (a.conductor, a.rep) < (b.conductor, b.rep));
    let mut chosen = vec![false; candidates.len()];
    match rule {
        LabelRule::Net => {
            let mut best: BTreeMap<(Option<ecad_model::NetId>, Option<ConductorId>, &str), usize> = BTreeMap::new();
            for (i, c) in candidates.iter().enumerate() {
                // A wire without a net is its own group.
                let key = (c.net, if c.net.is_none() { Some(c.conductor) } else { None }, c.text.as_str());
                match best.get(&key).and_then(|j| candidates.get(*j)) {
                    Some(b) if !better(c, b) => {}
                    _ => {
                        best.insert(key, i);
                    }
                }
            }
            for i in best.into_values() {
                if let Some(x) = chosen.get_mut(i) {
                    *x = true;
                }
            }
        }
        LabelRule::Each(min) => {
            for (i, c) in candidates.iter().enumerate() {
                if c.longest >= min
                    && let Some(x) = chosen.get_mut(i)
                {
                    *x = true;
                }
            }
        }
    }
    // Every net drawn on the page is labelled at least once: on its best route.
    let mut nets: BTreeMap<(Option<ecad_model::NetId>, Option<ConductorId>), (bool, usize)> = BTreeMap::new();
    for (i, c) in candidates.iter().enumerate() {
        let key = (c.net, if c.net.is_none() { Some(c.conductor) } else { None });
        let on = chosen.get(i).copied().unwrap_or(false);
        let e = nets.entry(key).or_insert((on, i));
        e.0 |= on;
        if candidates.get(e.1).is_some_and(|b| better(c, b)) {
            e.1 = i;
        }
    }
    for (labelled, best) in nets.into_values() {
        if !labelled && let Some(x) = chosen.get_mut(best) {
            *x = true;
        }
    }
    chosen.iter().enumerate().filter(|(_, on)| **on).map(|(i, _)| i).collect()
}

/// The project's label rule word, `net` or `each` (the registered `wiring.label_rule`; a stored
/// value it does not know resolves as the default).
pub fn label_rule_word(doc: &Document) -> &'static str {
    crate::defaults::resolve(doc, &crate::defaults::wires::WIRING_LABEL_RULE, crate::defaults::Scope::project()).word()
}

/// The project's shortest labelled wire, millimetres (the registered `wiring.label_min_length`;
/// out of range resolves as the default).
pub fn label_min_length(doc: &Document) -> f64 {
    crate::defaults::resolve(doc, &crate::defaults::wires::WIRING_LABEL_MIN_LENGTH, crate::defaults::Scope::project()).number()
}

/// The project's label rule (design 032 §4.1).
pub fn label_rule(doc: &Document) -> LabelRule {
    if label_rule_word(doc) == ecad_model::wiring::LABEL_RULE_EACH { LabelRule::Each(label_min_length(doc)) } else { LabelRule::Net }
}

/// The quiet wire ends on `page` (cable reference ends, briefs 009 and 010): the ends a new wire
/// never joins and that show no loose-end square. Derived from the document and the cables shown
/// as fans (`fan`), never from what is on screen (review of #65, finding 5). Only a page with a
/// cable reference has any, so other pages cost nothing.
pub fn quiet_ends_of(doc: &Document, page: PageId, fan: &std::collections::BTreeSet<ecad_model::CableId>, tol: f64) -> Vec<Pt> {
    if crate::references::markers_on_page(doc, page).is_empty() {
        return Vec::new();
    }
    crate::views::drawings_on_page(doc, page, fan, tol).into_values().flat_map(|d| d.quiet).collect()
}
