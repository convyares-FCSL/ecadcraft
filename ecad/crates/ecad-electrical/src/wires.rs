//! Connectivity on the page (brief 022, design 022 §3, §7, design 072 §4.3): conductor labels,
//! continuation texts, drawing routes (lines, joins and T's) and placing continuations.
//!
//! # Semantics
//! - Labels and continuation texts are derived on every projection of a page, never stored
//!   (like 018's tag texts): only the page on screen is projected.
//! - A person draws routes ([`ecad_model::Route`]); each is drawn by one polyline item whose
//!   vertices are the route's points. A T is a route end on another route (`RouteEnd::On`):
//!   there are no junction components. The wires come from the commit's rewrite pass
//!   (`ecad_model::wire_order::rewrite`); the planners here write routes.
//! - A continuation is a tagless component made from the starter definition `continuation`
//!   (FP4), with one connection point keyed `1`.
//! - Labels use the project's label height and colour style (wiring settings) and are placed so
//!   none touches another label or another wire (`labels_on_page_with`).
//!
//! # Update map of the labels (design 032 §8.1)
//! | Change | Labels |
//! | --- | --- |
//! | `AddRoute`, `ModifyRoute`, `RemoveRoute` (incl. a merge's) | that page's labels, when it is projected |
//! | `CreateWire`, `DeleteWire` (nets join or split) | with the `net` rule, the pages where those nets have routes: their routes' pages are touched, marked stale, the page on screen re-projects |
//! | a wire number, potential, cross-section or colour | the pages of the conductors it was written on (the numbering pass's included) |
//! | wiring settings (label rule, shortest labelled wire, colour style, label height, potentials) | every page, when it is projected next |
//! | lane T's wire-label text style or show setting (031) | as lane T's map says; here the pages of the conductors concerned |
//! | a cable switched to fan or traditional view | that cable's members' labels hide or show on the page on screen (display only, nothing recorded) |
//! | anything else | nothing |
//!
//! # Update map of the looks-connected marks (design 032 §8.2)
//! Route ops on P and cable reference ops on P change P's marks,
//! computed when P is projected ([`looks_connected_on_page`]); anything else changes nothing.
//!
//! This module reads the document and plans ops; it commits nothing itself.

use crate::CmdError;
use ecad_model::Noun;
use std::collections::BTreeMap;

use std::collections::BTreeSet;

use ecad_model::connectivity::{CONTINUATION, KIND, NAME};
use ecad_model::{
    ConductorId, DefinitionId, Device, DeviceId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, NetId, Op, PageId, Placement, Primitive,
    Pt, Representation, RepresentationId, Route, RouteEnd, RouteId, Style, Subject, SymbolInstance, Terminal, TerminalId, Wire, WireId,
};

use crate::devices::{DerivedText, TextOf};
use crate::symbols::{self, Starter};
use crate::text_style::fonts::TextMetrics;

/// The reserved layer of conductor labels (derived, plotted), like 009's `ECAD-CABLES`.
pub const LABEL_LAYER: &str = "ECAD-WIRE-LABELS";
/// The smallest clearance from the line to the label, millimetres; it grows with the label height
/// ([`gap`]).
pub const LABEL_GAP: f64 = 0.8;
/// The least space kept between two labels, and between a label and another wire, millimetres.
pub const LABEL_CLEARANCE: f64 = 0.6;

/// Brief 040: the layer of a two-colour wire's dashes (derived, plotted only in colour).
pub const DASH_LAYER: &str = "ECAD-WIRE-COLOUR";

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

/// Why a conductor label alone cannot be edited.
pub const LABEL_DERIVED: &str = "The wire label is derived; change the number with Fix Wire Number, or the wire's cross-section and colour.";

/// Why a T's dot alone cannot be edited (design 072 §4.4).
pub const TEE_DERIVED: &str = "A T's dot is drawn from its lines; move or erase the line instead.";

/// The radii (mm) of a T's dot: concentric circles, so it reads as filled at any lineweight
/// setting (the old junction glyph's look, design 022 §3.1).
pub const TEE_DOT_RADII: [f64; 3] = [0.5, 0.33, 0.17];

/// The continuation name rule, in words (design 022 §3.2).
pub const NAME_RULE: &str = "A continuation name is 1 to 16 letters, digits or + - / . _";

/// The starter of a continuation.
pub fn continuation_starter() -> &'static Starter {
    symbols::starter(symbols::StarterKey::Continuation)
}

/// What a tagless component is (`continuation`), from its `kind` property.
pub fn kind_of(doc: &Document, c: DeviceId) -> Option<&str> {
    doc.devices.get(&c)?.properties.get(KIND).map(String::as_str)
}

/// A continuation name as typed, checked: trimmed, 1-16 letters, digits or `+ - / . _`.
///
/// # Errors
/// [`NAME_RULE`].
pub fn valid_name(text: &str) -> Result<String, CmdError> {
    let t = text.trim();
    let ok =
        !t.is_empty() && t.chars().count() <= MAX_NAME_CHARS && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '+' | '-' | '/' | '.' | '_'));
    if ok { Ok(t.to_string()) } else { Err(CmdError::refused(NAME_RULE)) }
}

// ---------------------------------------------------------------- labels (§7)

/// One conductor label on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct WireLabel {
    pub conductor: WireId,
    /// The representation of the route it labels.
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
pub fn path_vertices(p: &Primitive) -> Option<Vec<Pt>> {
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

/// Every conductor label on `page` (one per route, of the wire it stands for ([`wire_of_route`]),
/// on its longest segment) but
/// those of the routes whose items are `hidden` (a cable drawn as a fan: its members' routes and
/// their labels are not shown, brief 005 and owner 2026-11-09). `m` measures the labels (the C
/// wrappers `labels_on_page` and `labels_on_page_with` pass CADCraft's fonts).
///
/// # Semantics
/// Labels are placed in route order. Each takes the first place (`candidates`) where its box
/// keeps [`LABEL_CLEARANCE`] from the labels already placed and from every other wire on the
/// page; failing that, the first place clear of labels; failing that, the first place.
pub fn labels_measured(doc: &Document, page: PageId, hidden: &std::collections::BTreeSet<ItemId>, m: &dyn TextMetrics) -> Vec<WireLabel> {
    // Brief 032 §4.3: the wire label height, weight and width through lane T's text styles.
    let style = crate::text_style::text_style(doc, crate::text_style::TextKind::WireLabel);
    let fonts = crate::text_style::fonts::Fonts::resolve_with(doc, m);
    let h = style.height;
    // Every route on the page (through the page index): (representation, wire, points).
    let mut routes: Vec<(RepresentationId, WireId, Vec<Pt>)> = Vec::new();
    for r in doc.routes_on(page) {
        let (Some(route), Some((rid, item, _))) = (doc.routes.get(&r), route_item(doc, r)) else { continue };
        if hidden.contains(&item) {
            continue;
        }
        if let Some(c) = wire_of_route(doc, r) {
            routes.push((rid, c, route.points.clone()));
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
        let Some(label) = ecad_model::wiring::label_with(doc, *c, look.cross_section.map(|x| x.0), look.colour.map(|x| x.0.to_string())) else {
            continue;
        };
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
        let own = doc.wires.get(c).and_then(|x| {
            let at = crate::defaults::Scope { conductor: Some(*c), ..crate::defaults::Scope::project() };
            crate::text_style::own_style(doc, crate::text_style::TextKind::WireLabel, at, &x.display)
        });
        let h = own.as_ref().map_or(h, |o| o.style.height);
        let Some((a, b)) = longest_segment(v) else { continue };
        let theta = reading_angle(a, b);
        if !theta.is_finite() {
            continue;
        }
        let w = match &own {
            Some(o) => {
                let f = o.family.as_ref().map_or_else(|| fonts.clone(), |fam| crate::text_style::fonts::Fonts::for_family_with(doc, fam.clone(), m));
                crate::text_style::fonts::width_in(&f, o.style, &label.text, m)
            }
            None => crate::text_style::fonts::width_in(&fonts, style, &label.text, m),
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

// ---------------------------------------------------------------- continuation texts (§3.2)

/// The text beside a continuation symbol (design 022 §3.2): its name; `None` when `c` is not a
/// continuation. Its other ends' places are reference parts since brief 049
/// ([`crate::continuations`]).
pub fn continuation_texts(doc: &Document, c: DeviceId, rep: RepresentationId, pl: &Placement) -> Option<Vec<DerivedText>> {
    if kind_of(doc, c) != Some(CONTINUATION) {
        return None;
    }
    let mut out = Vec::new();
    let name = doc.devices.get(&c)?.properties.get(NAME).cloned().unwrap_or_default();
    let at = pl.apply(Pt::new(2.5, -4.0));
    if !at.is_finite() {
        return Some(out);
    }
    out.push(DerivedText {
        rep,
        of: TextOf::Name,
        value: name,
        at,
        height: crate::text_style::text_style(doc, crate::devices::kind_of(TextOf::Name)).height,
        halign: ecad_model::TextHAlign::Left,
        valign: ecad_model::TextVAlign::Bottom,
        aci: None,
        own: None,
    });
    Some(out)
}

// ---------------------------------------------------------------- routes (design 072 §4.3)

/// A route's drawing: its representation, its item's ID and the item.
pub fn route_item(doc: &Document, r: RouteId) -> Option<(RepresentationId, ItemId, &Item)> {
    doc.representations_of(Subject::Route(r)).find_map(|(rid, rep)| {
        let id = *rep.items.first()?;
        Some((rid, id, doc.items.get(&id)?))
    })
}

/// The T's on `page` (design 072 §4.4): each route end that lands on another route
/// (`RouteEnd::On`), as (route, end index, point). Derived from the routes, never stored; a page
/// is drawn again when a route op touches it, so the dots follow AddRoute, ModifyRoute and
/// RemoveRoute of that page's routes and nothing else.
pub fn tees_on_page(doc: &Document, page: PageId) -> Vec<(RouteId, u8, Pt)> {
    let mut out = Vec::new();
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (i, end) in (0u8..).zip(route.ends) {
            if let RouteEnd::On { at, .. } = end
                && at.is_finite()
            {
                out.push((r, i, at));
            }
        }
    }
    out
}

/// Route `r`'s item, the item and the route's points.
pub fn route_vertices(doc: &Document, r: RouteId) -> Option<(ItemId, Item, Vec<Pt>)> {
    let route = doc.routes.get(&r)?;
    let (_, id, item) = route_item(doc, r)?;
    Some((id, item.clone(), route.points.clone()))
}

/// The polyline item drawing a route through `points` on `page`.
pub fn route_drawing(page: PageId, style: Style, points: Vec<Pt>) -> Item {
    Item { page, style, kind: ItemKind::Primitive(Primitive::Polyline { vertices: points, closed: false }) }
}

/// The ops that draw a new route on `page` through `points` with `ends` (design 072 §4.3): the
/// route, its item and the representation linking them. Returns the route's and the item's IDs.
pub fn add_route(ids: &mut IdAllocator, page: PageId, style: Style, points: Vec<Pt>, ends: [RouteEnd; 2]) -> (RouteId, ItemId, Vec<Op>) {
    let (r, i, rep): (RouteId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let ops = vec![
        Op::AddRoute { id: r, route: Route::new(page, points.clone(), ends) },
        Op::AddItem { id: i, item: route_drawing(page, style, points) },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Route(r),
                page,
                form: Form::Drawn,
                role: "route".into(),
                items: vec![i],
                connection_points: BTreeMap::new(),
            },
        },
    ];
    (r, i, ops)
}

/// The ops that remove route `r` with its drawing. Routes landing on it must be changed first
/// ([`RouteEdits::free_tees`] or [`RouteEdits::retarget_tees`]).
pub fn remove_route_ops(doc: &Document, r: RouteId) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut items = Vec::new();
    for (rid, rep) in doc.representations_of(Subject::Route(r)) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
        items.extend(rep.items.iter().copied());
    }
    for i in items {
        if let Some(item) = doc.items.get(&i) {
            ops.push(Op::RemoveItem { id: i, item: item.clone() });
        }
    }
    if let Some(route) = doc.routes.get(&r) {
        ops.push(Op::RemoveRoute { id: r, route: route.clone() });
    }
    ops
}

/// Changes to existing routes gathered while planning, so that a route changed more than once is
/// one `ModifyRoute`, with one `ModifyItem` when its points move (its drawing follows it).
#[derive(Debug, Clone)]
pub struct RouteEdits<'a> {
    doc: &'a Document,
    after: BTreeMap<RouteId, Route>,
}

impl<'a> RouteEdits<'a> {
    pub fn new(doc: &'a Document) -> Self {
        RouteEdits { doc, after: BTreeMap::new() }
    }

    /// Route `r` as changed so far, to change further; `None` when there is no such route.
    pub fn get(&mut self, r: RouteId) -> Option<&mut Route> {
        if !self.after.contains_key(&r) {
            let route = self.doc.routes.get(&r)?.clone();
            self.after.insert(r, route);
        }
        self.after.get_mut(&r)
    }

    /// End `e` of route `r` lands on `to`.
    pub fn set_end(&mut self, r: RouteId, e: End, to: RouteEnd) {
        if let Some(x) = self.get(r).and_then(|route| route.ends.get_mut(end_index(e))) {
            *x = to;
        }
    }

    /// Every route end on connection point `from` (found through the index) lands on `to`
    /// instead: another point, or `Free` when the point goes.
    pub fn repoint(&mut self, from: TerminalId, to: RouteEnd) {
        let routes: Vec<RouteId> = self.doc.routes_at(from).collect();
        for r in routes {
            if let Some(route) = self.get(r) {
                for e in route.ends.iter_mut().filter(|e| **e == RouteEnd::Point(from)) {
                    *e = to;
                }
            }
        }
    }

    /// The routes landing on route `from` land on route `to` at the same places.
    pub fn retarget_tees(&mut self, from: RouteId, to: RouteId) {
        let tees: Vec<RouteId> = self.doc.tees_of(from).collect();
        for t in tees {
            if let Some(route) = self.get(t) {
                for e in route.ends.iter_mut() {
                    if let RouteEnd::On { route: x, at } = *e
                        && x == from
                    {
                        *e = RouteEnd::On { route: to, at };
                    }
                }
            }
        }
    }

    /// The routes landing on route `r` end free where they are (before `r` goes).
    pub fn free_tees(&mut self, r: RouteId) {
        let tees: Vec<RouteId> = self.doc.tees_of(r).collect();
        for t in tees {
            if let Some(route) = self.get(t) {
                for e in route.ends.iter_mut().filter(|e| e.route() == Some(r)) {
                    *e = RouteEnd::Free;
                }
            }
        }
    }

    /// Route `r` is removed by other ops: no change of it is planned.
    pub fn forget(&mut self, r: RouteId) {
        self.after.remove(&r);
    }

    /// The ops: a `ModifyRoute` per changed route, with a `ModifyItem` of its drawing when its
    /// points changed.
    pub fn ops(self) -> Vec<Op> {
        let mut ops = Vec::new();
        for (r, after) in self.after {
            let Some(before) = self.doc.routes.get(&r) else { continue };
            if *before == after {
                continue;
            }
            if before.points != after.points
                && let Some((_, i, item)) = route_item(self.doc, r)
            {
                let moved = Item { kind: ItemKind::Primitive(Primitive::Polyline { vertices: after.points.clone(), closed: false }), ..item.clone() };
                ops.push(Op::ModifyItem { id: i, before: item.clone(), after: moved });
            }
            ops.push(Op::ModifyRoute { id: r, before: before.clone(), after });
        }
        ops
    }
}

/// The ops that let go of connection points `points` (being removed or replaced): every route
/// end on one of them ends free where it is, so the commit's rewrite takes their wires away.
pub fn free_points_ops(doc: &Document, points: impl IntoIterator<Item = TerminalId>) -> Vec<Op> {
    let mut edits = RouteEdits::new(doc);
    for t in points {
        edits.repoint(t, RouteEnd::Free);
    }
    edits.ops()
}

/// The ops moving every route end from one connection point to another (`map`: from → to), in
/// one `ModifyRoute` per route.
pub fn repoint_ops(doc: &Document, map: &BTreeMap<TerminalId, TerminalId>) -> Vec<Op> {
    let mut edits = RouteEdits::new(doc);
    for (from, to) in map {
        edits.repoint(*from, RouteEnd::Point(*to));
    }
    edits.ops()
}

/// The free route ends on `page` drawn within `tol` of `at`.
pub fn loose_ends_at(doc: &Document, page: PageId, at: Pt, tol: f64) -> Vec<(RouteId, End)> {
    let mut out = Vec::new();
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (end, e) in End::BOTH.into_iter().zip(route.ends) {
            if e == RouteEnd::Free && end_point(doc, r, end).is_some_and(|p| p.near(at, tol)) {
                out.push((r, end));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- the wires a route carries

/// The wire between connection points `a` and `b`, if there is one (through the index).
pub fn wire_between(doc: &Document, a: TerminalId, b: TerminalId) -> Option<WireId> {
    doc.ends_at(a).into_iter().map(|(w, _)| w).find(|w| doc.wires.get(w).is_some_and(|x| x.ends.contains(&a) && x.ends.contains(&b)))
}

/// A connection point route `r` reaches over routes, the nearest first: its own ends, then the
/// routes it lands on and that land on it; never a named point. The cost follows the routes
/// joined to it.
fn point_of_route(doc: &Document, r: RouteId) -> Option<TerminalId> {
    let mut seen = BTreeSet::new();
    let mut todo = std::collections::VecDeque::from([r]);
    while let Some(x) = todo.pop_front() {
        if !seen.insert(x) {
            continue;
        }
        let Some(route) = doc.routes.get(&x) else { continue };
        // A named point (a cable reference's, F4) is an aid: no wire ends there.
        if let Some(t) = route.points_landed().find(|t| doc.named_points(*t).is_none()) {
            return Some(t);
        }
        todo.extend(route.ends.iter().filter_map(|e| e.route()));
        todo.extend(doc.tees_of(x));
    }
    None
}

/// The wire route `r` stands for when a person clicks it: the wire between its two connection
/// points when it runs from one to the other, else the first (in net order) of the wires at the
/// nearest connection point it reaches. `None` for a route on no net with wires.
pub fn wire_of_route(doc: &Document, r: RouteId) -> Option<WireId> {
    let route = doc.routes.get(&r)?;
    if let [RouteEnd::Point(a), RouteEnd::Point(b)] = route.ends
        && let Some(w) = wire_between(doc, a, b)
    {
        return Some(w);
    }
    let t = point_of_route(doc, r)?;
    doc.ends_at(t).into_iter().map(|(w, _)| w).filter_map(|w| doc.wires.get(&w).map(|x| (x.net_order, w))).min().map(|(_, w)| w)
}

/// The wires route `r` carries: the wire between its two connection points when it runs from one
/// to the other, else every wire of its net, in net order.
pub fn wires_of_route(doc: &Document, r: RouteId) -> Vec<WireId> {
    let Some(route) = doc.routes.get(&r) else { return Vec::new() };
    if let [RouteEnd::Point(a), RouteEnd::Point(b)] = route.ends
        && let Some(w) = wire_between(doc, a, b)
    {
        return vec![w];
    }
    let Some(net) = point_of_route(doc, r).and_then(|t| doc.ends_at(t).first().and_then(|(w, _)| doc.wires.get(w)).map(|w| w.net)) else {
        return Vec::new();
    };
    let mut v: Vec<(u16, WireId)> = doc.wires_of_net(net).filter_map(|w| doc.wires.get(&w).map(|x| (x.net_order, w))).collect();
    v.sort_unstable();
    v.into_iter().map(|(_, w)| w).collect()
}

/// The routes drawing wire `w`: those with an end on either of its connection points, in ID
/// order (through the index).
pub fn routes_of_wire(doc: &Document, w: WireId) -> Vec<RouteId> {
    let Some(wire) = doc.wires.get(&w) else { return Vec::new() };
    let mut v: Vec<RouteId> = wire.ends.iter().flat_map(|t| doc.routes_at(*t)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The route drawing wire `w` on `page`: the one running straight between its two points when
/// there is one, else the first of [`routes_of_wire`] there.
pub fn route_of_wire_on(doc: &Document, w: WireId, page: PageId) -> Option<RouteId> {
    let ends = doc.wires.get(&w)?.ends;
    let here: Vec<RouteId> = routes_of_wire(doc, w).into_iter().filter(|r| doc.routes.get(r).is_some_and(|x| x.page == page)).collect();
    let direct = here.iter().copied().find(|r| doc.routes.get(r).is_some_and(|x| ends.iter().all(|t| x.ends.contains(&RouteEnd::Point(*t)))));
    direct.or_else(|| here.first().copied())
}

/// Where connection point `t` is drawn as a route end: (page, point) of every route end landing
/// on it, in route ID order (through the index).
pub fn drawn_ends_at(doc: &Document, t: TerminalId) -> Vec<(PageId, Pt)> {
    let mut out = Vec::new();
    for r in doc.routes_at(t) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (p, e) in [route.points.first(), route.points.last()].into_iter().zip(route.ends) {
            if let Some(p) = p
                && e == RouteEnd::Point(t)
            {
                out.push((route.page, *p));
            }
        }
    }
    out
}

/// The pages wire `w` is drawn on (see [`routes_of_wire`]).
pub fn wire_pages(doc: &Document, w: WireId) -> BTreeSet<PageId> {
    routes_of_wire(doc, w).into_iter().filter_map(|r| doc.routes.get(&r).map(|x| x.page)).collect()
}

// ---------------------------------------------------------------- placing (§3)

/// A planned continuation (a tagless component).
#[derive(Debug, Clone)]
pub struct Tagless {
    pub ops: Vec<Op>,
    pub component: DeviceId,
    pub terminal: TerminalId,
    pub item: ItemId,
    pub representation: RepresentationId,
    pub definition: DefinitionId,
    /// Free route ends found at the point, now on it.
    pub attached: Vec<(RouteId, End)>,
}

/// Plan a tagless component of starter `s` at `placement` on `page`, with properties `props`
/// (`kind`, and `name` for a continuation); free route ends lying on its connection point land on
/// it.
///
/// # Errors
/// The starter definition cannot be built (never expected).
pub fn plan_tagless(
    doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, s: &Starter, placement: Placement, props: ecad_model::Properties,
) -> Result<Tagless, CmdError> {
    let mut ops = Vec::new();
    let definition = match symbols::embedded(doc, s) {
        Some(d) => d,
        None => {
            let d: DefinitionId = ids.fresh();
            ops.push(Op::EmbedDefinition { id: d, definition: s.definition()?.clone() });
            d
        }
    };
    let (component, terminal, item, representation): (DeviceId, TerminalId, ItemId, RepresentationId) =
        (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    ops.push(Op::CreateDevice {
        id: component,
        device: Device { properties: props, definition: Some(definition), device_type: s.device_type(), ..Device::default() },
    });
    ops.push(Op::AddTerminal {
        id: terminal,
        terminal: Terminal { component, label: None, key: Some(ecad_model::PointKey::def("1")), accepts: Default::default() },
    });
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: representation,
        rep: Representation {
            subject: Subject::Device(component),
            page,
            form: Form::Symbol,
            role: s.key.to_string(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    let attached = loose_ends_at(doc, page, placement.origin, ON_WIRE);
    let mut edits = RouteEdits::new(doc);
    for (r, end) in &attached {
        edits.set_end(*r, *end, RouteEnd::Point(terminal));
    }
    ops.extend(edits.ops());
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

/// A route found under a point: the route, the segment index and the point on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnRoute {
    pub route: RouteId,
    pub segment: usize,
    pub at: Pt,
}

/// The route on `page` passing within `tol` of `p` (the nearest; never `except`), with the point
/// snapped to a route vertex within `tol`. Reads the page's routes through the index.
pub fn route_at(doc: &Document, page: PageId, p: Pt, tol: f64, except: Option<RouteId>) -> Option<OnRoute> {
    let mut best: Option<(f64, OnRoute)> = None;
    for r in doc.routes_on(page) {
        if Some(r) == except {
            continue;
        }
        let Some(v) = doc.routes.get(&r).map(|x| &x.points) else { continue };
        let (first, last) = (v.first().copied(), v.last().copied());
        for (i, w) in v.windows(2).enumerate() {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
            let (q, d) = on_segment(p, *a, *b);
            // Hostile geometry (an overflowing segment) gives NaN: never a route there.
            if !(d <= tol && q.is_finite()) {
                continue;
            }
            // A route's interior wins over another route's end at the same point.
            let at_end = first.is_some_and(|f| f.near(q, tol)) || last.is_some_and(|l| l.near(q, tol));
            let d = if at_end { d + tol } else { d };
            if best.is_none_or(|(bd, _)| d < bd) {
                let at = v.iter().copied().find(|x| x.near(q, tol)).unwrap_or(q);
                best = Some((d, OnRoute { route: r, segment: i, at }));
            }
        }
    }
    best.map(|(_, w)| w)
}

/// The route `r` met at `at`: its segment there, when `at` lies within [`ON_WIRE`] of it.
pub fn on_route_of(doc: &Document, r: RouteId, at: Pt) -> Option<OnRoute> {
    let v = &doc.routes.get(&r)?.points;
    let mut best: Option<(f64, usize)> = None;
    for (i, w) in v.windows(2).enumerate() {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let (_, d) = on_segment(at, *a, *b);
        if d.is_finite() && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    let (d, segment) = best?;
    (d <= ON_WIRE).then_some(OnRoute { route: r, segment, at })
}

/// A route split in two at a point.
#[derive(Debug, Clone)]
pub struct Split {
    pub ops: Vec<Op>,
    /// The route, which keeps its first end and its points up to the split.
    pub route: RouteId,
    /// The new second part, from the split to the route's old last end.
    pub new: RouteId,
    /// The second part's item.
    pub new_item: ItemId,
}

/// Plan splitting route `on.route` at `on.at` (design 028 §3.1, design 072 §4.3): the route keeps
/// its first end and ends on `first` (an end and the point it is drawn to); a new route starts on
/// `second` and keeps the old last end. Routes landing on the second part land on the new route.
///
/// # Errors
/// No such route, or the point at one of its ends.
pub fn plan_split(doc: &Document, ids: &mut IdAllocator, on: &OnRoute, first: (RouteEnd, Pt), second: (RouteEnd, Pt)) -> Result<Split, CmdError> {
    let r = on.route;
    let route = doc.routes.get(&r).ok_or(CmdError::NotFound(Noun::Wire))?;
    let v = &route.points;
    let [a, b] = route.end_points().ok_or(CmdError::refused("empty route"))?;
    if on.at.near(a, ON_WIRE) || on.at.near(b, ON_WIRE) {
        return Err(CmdError::refused("That is the end of the wire. Click along the wire, away from its ends, or on empty space."));
    }
    let seg = on.segment;
    let mut keep: Vec<Pt> = v.get(..=seg).unwrap_or_default().to_vec();
    let mut rest: Vec<Pt> = v.get(seg + 1..).unwrap_or_default().to_vec();
    if keep.last().is_some_and(|p| p.near(on.at, 1e-9)) {
        keep.pop();
    }
    if rest.first().is_some_and(|p| p.near(on.at, 1e-9)) {
        rest.remove(0);
    }
    keep.push(first.1);
    rest.insert(0, second.1);
    let style = route_item(doc, r).map(|(_, _, i)| i.style.clone()).ok_or(CmdError::refused("The wire has no drawing on this page."))?;
    let (new, new_item, mut ops) = add_route(ids, route.page, style, rest, [second.0, route.ends[1]]);
    let split_at = route.along(on.at).map_or(0.0, |(s, _)| s);
    let mut edits = RouteEdits::new(doc);
    // The routes landing on the second part land on the new route.
    for t in doc.tees_of(r).collect::<Vec<_>>() {
        if let Some(x) = edits.get(t) {
            for e in x.ends.iter_mut() {
                if let RouteEnd::On { route: h, at } = *e
                    && h == r
                    && route.along(at).is_some_and(|(s, _)| s > split_at)
                {
                    *e = RouteEnd::On { route: new, at };
                }
            }
        }
    }
    if let Some(x) = edits.get(r) {
        x.points = keep;
        x.ends[1] = first.0;
    }
    ops.extend(edits.ops());
    Ok(Split { ops, route: r, new, new_item })
}

// ---------------------------------------------------------------- words (§7, §9)

/// Most connection points a net line names before `+n`.
pub const MAX_POINTS_SHOWN: usize = 6;

/// A connection point in words: `-K01:A1`; a continuation `→24V`.
pub fn point_words(doc: &Document, t: TerminalId) -> String {
    let Some(term) = doc.terminals.get(&t) else { return "?".into() };
    let c = term.component;
    if kind_of(doc, c) == Some(CONTINUATION) {
        let name = doc.devices.get(&c).and_then(|x| x.properties.get(NAME)).cloned().unwrap_or_default();
        return format!("\u{2192}{name}");
    }
    let label = term.label.clone().or_else(|| term.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| "?".into());
    match crate::devices::tag_of(doc, c) {
        Some(tag) => format!("{tag}:{label}"),
        None => format!("{}\u{2026}:{label}", c.hex().get(..8).unwrap_or_default()),
    }
}

/// The device connection points of a net (not continuations), in words, sorted.
pub fn net_points(doc: &Document, net: &ecad_model::NetData) -> Vec<String> {
    let mut v: Vec<String> = net
        .terminals
        .iter()
        .filter(|t| doc.terminals.get(t).is_none_or(|x| !matches!(kind_of(doc, x.component), Some(CONTINUATION))))
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
    let n = net.wires.len();
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
pub fn hover_lines(doc: &Document, w: WireId) -> Vec<String> {
    let mut lines = Vec::new();
    let conn = doc.connectivity();
    let Some(net) = conn.net_of_conductor(w).and_then(|id| conn.net(id)) else { return lines };
    if !doc.wires.contains_key(&w) {
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
    let cat = look.category.as_ref().map(|id| crate::styles::name(doc, ecad_model::StyleCatalogue::Wire, id));
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
    if let Some(v) = shown(ecad_model::wiring::COLOUR, &look.colour_text()) {
        values.push(format!("Colour {v}"));
    }
    if !values.is_empty() {
        lines.push(values.join(" \u{b7} "));
    }
    lines.push(net_line(doc, net));
    lines
}

// ---------------------------------------------------------------- joining (brief 032 §2, design 072 §4.3)

/// The way out every join refusal names (design 032 §2.2).
pub const JOIN_WAY_OUT: &str = "Draw it with join off (join: false), or end it on a connection point.";

/// What one end of a new route meets (design 032 §2.1, design 072 §4.3).
#[derive(Debug, Clone, PartialEq)]
enum Meets {
    /// Nothing there: a deliberate open end.
    Free,
    /// A connection point there.
    Point(TerminalId),
    /// The one free end of another route: the new line extends that route.
    Merge { route: RouteId, end: End },
    /// The middle of route `route` (a T), or its free end where the free ends of several routes
    /// meet; the free ends of `others` there land on `route` too.
    On { route: RouteId, others: Vec<(RouteId, End)> },
}

impl Meets {
    /// The other routes this end changes or lands on.
    fn routes(&self) -> Vec<RouteId> {
        match self {
            Meets::Free | Meets::Point(_) => Vec::new(),
            Meets::Merge { route, .. } => vec![*route],
            Meets::On { route, others } => std::iter::once(*route).chain(others.iter().map(|(r, _)| *r)).collect(),
        }
    }

    /// A merge that cannot be made lands on the route instead.
    fn not_merged(self) -> Meets {
        match self {
            Meets::Merge { route, .. } => Meets::On { route, others: Vec::new() },
            m => m,
        }
    }
}

/// A planned `wire` (design 032 §2.3, design 072 §4.3): its ops and what the result reports.
#[derive(Debug, Clone)]
pub struct WirePlan {
    pub ops: Vec<Op>,
    /// The route the drawn line now belongs to: a new one, or the one it extends.
    pub route: RouteId,
    /// That route's item.
    pub item: ItemId,
    /// False when the line was merged into an existing route.
    pub created: bool,
    /// The ends that met other routes: (end, the route met, `merged` or `on`).
    pub joined: Vec<(End, RouteId, &'static str)>,
    /// Routes merged into the survivor and removed.
    pub removed: Vec<RouteId>,
    /// Ends on a connection point.
    pub attached: Vec<(End, TerminalId)>,
    /// Reverse T: (point, the route whose free end now lands on the drawn line there).
    pub tees: Vec<(Pt, RouteId)>,
    /// The wire made between the two connection points to carry the tool's potential.
    pub wire: Option<WireId>,
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

/// The points of a route (`cv`, free end `open`) with `add` (starting at that end) joined on, so
/// the route keeps its direction.
fn merged_route(cv: &[Pt], open: End, add: &[Pt]) -> Vec<Pt> {
    match open {
        End::B => joined(cv.to_vec(), add),
        End::A => joined(reversed(add), cv),
    }
}

/// Whether a route end at `at` is quiet (a cable reference end, brief 009): never joined.
fn is_quiet(quiet: &[Pt], at: Pt) -> bool {
    quiet.iter().any(|q| q.near(at, ON_WIRE))
}

/// The route on `page` whose interior (not an end) passes within [`ON_WIRE`] of `at`, the nearest.
pub fn interior_route_at(doc: &Document, page: PageId, at: Pt) -> Option<OnRoute> {
    route_at(doc, page, at, ON_WIRE, None)
        .filter(|on| doc.routes.get(&on.route).and_then(Route::end_points).is_some_and(|[f, l]| !f.near(on.at, ON_WIRE) && !l.near(on.at, ON_WIRE)))
}

/// The net of the wires route `r` carries.
fn route_net_key(doc: &Document, r: RouteId) -> Option<ecad_model::NetKey> {
    wire_of_route(doc, r).and_then(|w| doc.connectivity().net_of_conductor(w))
}

/// The net of the wires route `r` carries.
fn route_net(doc: &Document, r: RouteId) -> Option<&ecad_model::NetData> {
    route_net_key(doc, r).and_then(|n| doc.connectivity().net(n))
}

/// Refuse joining route `r` with a new line of potential `potential` (design 032 §2.2).
fn merge_potential(doc: &Document, r: RouteId, potential: Option<&str>) -> Result<(), CmdError> {
    let Some(net) = route_net(doc, r) else { return Ok(()) };
    let theirs = ecad_model::wiring::net_potential(doc, net).name;
    if let (Some(x), Some(y)) = (theirs.as_deref(), potential)
        && !x.eq_ignore_ascii_case(y)
    {
        return Err(CmdError::refused(format!("That wire carries {x} and this one {y}; joining them would short them. {JOIN_WAY_OUT}")));
    }
    Ok(())
}

/// Refuse joining routes `a` and `b` into one (design 032 §2.2): their nets carry different
/// potentials, or both have fixed numbers that differ.
pub fn join_sense(doc: &Document, a: RouteId, b: RouteId) -> Result<(), CmdError> {
    let (Some(ka), Some(kb)) = (route_net_key(doc, a), route_net_key(doc, b)) else { return Ok(()) };
    let conn = doc.connectivity();
    let (Some(x), Some(y)) = (conn.net(ka), conn.net(kb)) else { return Ok(()) };
    if ka == kb {
        return Ok(());
    }
    let (px, py) = (ecad_model::wiring::net_potential(doc, x).name, ecad_model::wiring::net_potential(doc, y).name);
    if let (Some(px), Some(py)) = (&px, &py)
        && !px.eq_ignore_ascii_case(py)
    {
        return Err(CmdError::refused(format!("That wire carries {px} and this one {py}; joining them would short them. {JOIN_WAY_OUT}")));
    }
    if ecad_model::wiring::net_fixed(x) && ecad_model::wiring::net_fixed(y) {
        let (fx, fy) = (ecad_model::wiring::net_number(doc, x).unwrap_or_default(), ecad_model::wiring::net_number(doc, y).unwrap_or_default());
        if fx != fy {
            return Err(CmdError::refused(format!("Both wires have fixed numbers of their own ({fx} and {fy}). {JOIN_WAY_OUT}")));
        }
    }
    Ok(())
}

/// Whether a wire's net has a fixed number.
pub fn has_fixed_number(doc: &Document, c: WireId) -> bool {
    let conn = doc.connectivity();
    conn.net_of_conductor(c).and_then(|n| conn.net(n)).is_some_and(ecad_model::wiring::net_fixed)
}

/// The point at end `e` of route `r`.
pub fn end_point(doc: &Document, r: RouteId, e: End) -> Option<Pt> {
    let v = &doc.routes.get(&r)?.points;
    match e {
        End::A => v.first().copied(),
        End::B => v.last().copied(),
    }
}

/// The index of end `e` in a route's `ends`.
pub fn end_index(e: End) -> usize {
    match e {
        End::A => 0,
        End::B => 1,
    }
}

/// The free route ends at `at` on `page` that a new line may join (not quiet ones).
pub fn joinable_ends_at(doc: &Document, page: PageId, at: Pt, quiet: &[Pt]) -> Vec<(RouteId, End)> {
    loose_ends_at(doc, page, at, ON_WIRE).into_iter().filter(|(r, e)| end_point(doc, *r, *e).is_some_and(|p| !is_quiet(quiet, p))).collect()
}

/// What the end of a new line at `at` meets (design 032 §2.1, step 2), and the point it snaps to.
fn meet_at(doc: &Document, page: PageId, at: Pt, quiet: &[Pt]) -> (Meets, Pt) {
    let loose = joinable_ends_at(doc, page, at, quiet);
    if let Some(on) = interior_route_at(doc, page, at) {
        let others = loose.into_iter().filter(|(r, _)| *r != on.route).collect();
        return (Meets::On { route: on.route, others }, on.at);
    }
    match loose.as_slice() {
        [] => (Meets::Free, at),
        [(r, e)] => (Meets::Merge { route: *r, end: *e }, end_point(doc, *r, *e).unwrap_or(at)),
        [(r, e), rest @ ..] => {
            let others = rest.iter().filter(|(x, _)| x != r).copied().collect();
            (Meets::On { route: *r, others }, end_point(doc, *r, *e).unwrap_or(at))
        }
    }
}

/// The free ends of other routes on `page` that lie on the interior of path `v` (design 032 §2.1
/// step 3), each as (segment index, distance along it, point, route, end), in order along the
/// path.
fn ends_on_path(doc: &Document, page: PageId, v: &[Pt], quiet: &[Pt]) -> Vec<(usize, f64, Pt, RouteId, End)> {
    let (Some(first), Some(last)) = (v.first().copied(), v.last().copied()) else { return Vec::new() };
    // A grid of the new line's segments (overseer ruling: no segment-by-segment test of every end
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
    let mut out: Vec<(usize, f64, Pt, RouteId, End)> = Vec::new();
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (end, e) in End::BOTH.into_iter().zip(route.ends) {
            let Some(q) = end_point(doc, r, end) else { continue };
            if e != RouteEnd::Free || !q.is_finite() || q.near(first, ON_WIRE) || q.near(last, ON_WIRE) || is_quiet(quiet, q) {
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
                out.push((i, along, q, r, end));
            }
        }
    }
    out.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.3.cmp(&y.3)));
    out
}

/// The inputs of [`plan_wire`] besides the document and IDs.
#[derive(Debug, Clone)]
pub struct WireInput<'a> {
    pub page: PageId,
    pub style: Style,
    pub points: Vec<Pt>,
    pub potential: Option<&'a str>,
    /// Join ends that meet other routes (design 032 §2.1 steps 2 and 3).
    pub join: bool,
    /// The project tolerance (an end on a connection point).
    pub tol: f64,
    /// The page's quiet ends (cable reference ends): never joined.
    pub quiet: &'a [Pt],
    /// Most points a route may have, and most route ends one line may cross.
    pub max_points: usize,
}

/// Plan drawing a line (design 032 §2, design 072 §4.3): a route whose ends land on connection
/// points, and with `join` on other routes: an end on another route's free end extends that route
/// (two such ends make the two routes one), an end on a route's middle lands there (a T), and free
/// ends of other routes lying on the line land on it (the reverse T). The wires come from the
/// commit's rewrite pass; a line from one connection point straight to another also makes that
/// wire, to carry the tool's potential.
///
/// # Errors
/// Design 032 §2.2's refusals (potentials, fixed numbers, too many points or ends crossed);
/// nothing is planned then.
pub fn plan_wire(doc: &Document, ids: &mut IdAllocator, input: WireInput<'_>) -> Result<WirePlan, CmdError> {
    let WireInput { page, style, points: mut pts, potential, join, tol, quiet, max_points } = input;
    let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else { return Err(CmdError::refused("a wire needs two points")) };
    let mut shown = ShownOnce::new(doc, page);
    // 1. The ends.
    let mut meets: Vec<Meets> = Vec::with_capacity(2);
    for (i, at) in [first, last].into_iter().enumerate() {
        let (m, p) = if let Some(t) = shown.terminal_at(at, tol) {
            (Meets::Point(t), at)
        } else if join {
            meet_at(doc, page, at, quiet)
        } else {
            (Meets::Free, at)
        };
        let slot = if i == 0 { pts.first_mut() } else { pts.last_mut() };
        if let Some(s) = slot {
            *s = p;
        }
        meets.push(m);
    }
    let mut mb = meets.pop().unwrap_or(Meets::Free);
    let mut ma = meets.pop().unwrap_or(Meets::Free);
    // 2. The reverse T: free ends lying on the line's interior land on it.
    let tees: Vec<(Pt, RouteId, End)> = if join {
        ends_on_path(doc, page, &pts, quiet)
            .into_iter()
            .filter(|(_, _, q, _, _)| shown.terminal_at(*q, tol).is_none())
            .map(|(_, _, q, r, e)| (q, r, e))
            .collect()
    } else {
        Vec::new()
    };
    if tees.len() > max_points {
        return Err(CmdError::refused(format!("This wire crosses more than {max_points} wire ends; draw it in parts.")));
    }
    // A route merged into the line must not also land on it, or be landed on by its other end:
    // that end lands on the route instead.
    let tee_routes: BTreeSet<RouteId> = tees.iter().map(|(_, r, _)| *r).collect();
    let clash = |m: &Meets, other: &Meets| match m {
        Meets::Merge { route, .. } => tee_routes.contains(route) || other.routes().contains(route),
        _ => false,
    };
    let (clash_a, clash_b) = (clash(&ma, &mb), clash(&mb, &ma));
    if clash_a {
        ma = ma.not_merged();
    }
    if clash_b {
        mb = mb.not_merged();
    }
    if let (Meets::Merge { route: a, .. }, Meets::Merge { route: b, .. }) = (&ma, &mb) {
        let lands = |x: RouteId, y: RouteId| doc.routes.get(&x).is_some_and(|r| r.ends.iter().any(|e| e.route() == Some(y)));
        if lands(*a, *b) || lands(*b, *a) {
            mb = mb.not_merged();
        }
    }
    // Joining at an open end or landing on a line's middle (a T) joins the nets alike.
    for m in [&ma, &mb] {
        if let Meets::Merge { route, .. } | Meets::On { route, .. } = m {
            merge_potential(doc, *route, potential)?;
        }
    }
    if let (Meets::Merge { route: a, .. }, Meets::Merge { route: b, .. }) = (&ma, &mb) {
        join_sense(doc, *a, *b)?;
    }
    let (pa, pb) = (pts.first().copied().unwrap_or(first), pts.last().copied().unwrap_or(last));
    let end_of = |m: &Meets, at: Pt| match m {
        Meets::Point(t) => RouteEnd::Point(*t),
        Meets::On { route, .. } => RouteEnd::On { route: *route, at },
        Meets::Free | Meets::Merge { .. } => RouteEnd::Free,
    };
    let (ea, eb) = (end_of(&ma, pa), end_of(&mb, pb));
    let too_long = || CmdError::refused(format!("The joined wire would have more than {max_points} points."));
    let mut ops: Vec<Op> = Vec::new();
    let mut tail: Vec<Op> = Vec::new();
    let mut edits = RouteEdits::new(doc);
    let mut removed = Vec::new();
    let mut joined_ends = Vec::new();
    // 3. The route that holds the line.
    let (holder, item, created) = match (&ma, &mb) {
        (Meets::Merge { route: r1, end: e1 }, Meets::Merge { route: r2, end: e2 }) => {
            // Both ends meet free ends of two routes: one route (§2.2), the one at end A.
            let o = doc.routes.get(r2).ok_or(CmdError::NotFound(Noun::Wire))?;
            let o_from_end = if *e2 == End::A { o.points.clone() } else { reversed(&o.points) };
            let far = o.ends.get(end_index(e2.other())).copied().unwrap_or(RouteEnd::Free);
            let s = edits.get(*r1).ok_or(CmdError::NotFound(Noun::Wire))?;
            let points = merged_route(&s.points, *e1, &joined(pts.clone(), &o_from_end));
            if points.len() > max_points {
                return Err(too_long());
            }
            s.points = points;
            if let Some(x) = s.ends.get_mut(end_index(*e1)) {
                *x = far;
            }
            edits.retarget_tees(*r2, *r1);
            tail = remove_route_ops(doc, *r2);
            edits.forget(*r2);
            removed.push(*r2);
            joined_ends.push((End::A, *r1, "merged"));
            joined_ends.push((End::B, *r2, "merged"));
            let item = route_item(doc, *r1).map(|(_, i, _)| i).ok_or(CmdError::refused("The wire has no drawing on this page."))?;
            (*r1, item, false)
        }
        (Meets::Merge { route, end }, _) | (_, Meets::Merge { route, end }) => {
            let at_a = matches!(ma, Meets::Merge { .. });
            let (add, far) = if at_a { (pts.clone(), eb) } else { (reversed(&pts), ea) };
            let s = edits.get(*route).ok_or(CmdError::NotFound(Noun::Wire))?;
            let points = merged_route(&s.points, *end, &add);
            if points.len() > max_points {
                return Err(too_long());
            }
            s.points = points;
            if let Some(x) = s.ends.get_mut(end_index(*end)) {
                *x = far;
            }
            joined_ends.push((if at_a { End::A } else { End::B }, *route, "merged"));
            let item = route_item(doc, *route).map(|(_, i, _)| i).ok_or(CmdError::refused("The wire has no drawing on this page."))?;
            (*route, item, false)
        }
        _ => {
            if pts.len() > max_points {
                return Err(CmdError::refused(format!("A wire has at most {max_points} points.")));
            }
            let (r, i, add) = add_route(ids, page, style, pts.clone(), [ea, eb]);
            ops.extend(add);
            (r, i, true)
        }
    };
    // 4. What lands where.
    let mut attached = Vec::new();
    for (end, m, at) in [(End::A, &ma, pa), (End::B, &mb, pb)] {
        match m {
            Meets::Point(t) => attached.push((end, *t)),
            Meets::On { route, others } => {
                joined_ends.push((end, *route, "on"));
                for (r, e) in others {
                    edits.set_end(*r, *e, RouteEnd::On { route: *route, at });
                }
            }
            Meets::Free | Meets::Merge { .. } => {}
        }
    }
    let mut landed = Vec::new();
    for (q, r, e) in &tees {
        edits.set_end(*r, *e, RouteEnd::On { route: holder, at: *q });
        landed.push((*q, *r));
    }
    // 5. The tool's potential, on the wire between two connection points.
    let mut wire = None;
    if created
        && let (Some(v), Meets::Point(a), Meets::Point(b)) = (potential, &ma, &mb)
        && a != b
        && wire_between(doc, *a, *b).is_none()
    {
        let (id, net, conductor): (WireId, NetId, ConductorId) = (ids.fresh(), ids.fresh(), ids.fresh());
        let mut w = Wire::new(net, conductor, *a, *b);
        w.potential = ecad_model::text::Text::new(v).ok();
        ops.push(Op::CreateWire { id, wire: w });
        wire = Some(id);
    }
    ops.extend(edits.ops());
    ops.extend(tail);
    Ok(WirePlan { ops, route: holder, item, created, joined: joined_ends, removed, attached, tees: landed, wire })
}

/// A planned merge of two free route ends that meet (`joinwires`, design 032 §2.5).
#[derive(Debug, Clone)]
pub struct EndJoin {
    pub ops: Vec<Op>,
    pub survivor: RouteId,
    pub removed: RouteId,
}

/// Plan merging the free ends `a` and `b` of two routes that meet into one route (design 032
/// §2.5): the lower route ID survives and takes the other's points and far end; routes landing on
/// the other land on it; the other is removed. `Ok(None)` when one of them lands on the other (the
/// caller lands one end on the other route instead).
///
/// # Errors
/// One route's two ends; a route too long; §2.2's refusals.
pub fn plan_end_join(doc: &Document, a: (RouteId, End), b: (RouteId, End), max_points: usize) -> Result<Option<EndJoin>, CmdError> {
    if a.0 == b.0 {
        return Err(CmdError::refused(format!("Both ends belong to one wire: joining them would close it into a loop. {JOIN_WAY_OUT}")));
    }
    let lands = |x: RouteId, y: RouteId| doc.routes.get(&x).is_some_and(|r| r.ends.iter().any(|e| e.route() == Some(y)));
    if lands(a.0, b.0) || lands(b.0, a.0) {
        return Ok(None);
    }
    join_sense(doc, a.0, b.0)?;
    let ((s, es), (o, eo)) = if a.0 < b.0 { (a, b) } else { (b, a) };
    let ro = doc.routes.get(&o).ok_or(CmdError::NotFound(Noun::Wire))?;
    // The other route's free end lies on the survivor's (within ON_WIRE): it starts there.
    let mut add = if eo == End::A { ro.points.clone() } else { reversed(&ro.points) };
    if let (Some(first), Some(at)) = (add.first_mut(), end_point(doc, s, es)) {
        *first = at;
    }
    let far = ro.ends.get(end_index(eo.other())).copied().unwrap_or(RouteEnd::Free);
    let mut edits = RouteEdits::new(doc);
    let rs = edits.get(s).ok_or(CmdError::NotFound(Noun::Wire))?;
    let points = merged_route(&rs.points, es, &add);
    if points.len() > max_points {
        return Err(CmdError::refused(format!("The joined wire would have more than {max_points} points.")));
    }
    rs.points = points;
    if let Some(x) = rs.ends.get_mut(end_index(es)) {
        *x = far;
    }
    edits.retarget_tees(o, s);
    edits.forget(o);
    let mut ops = edits.ops();
    ops.extend(remove_route_ops(doc, o));
    Ok(Some(EndJoin { ops, survivor: s, removed: o }))
}

/// The nearest free route end on `page` within `tol` of `p` that a new line may join (not a quiet
/// one): the Wire tool snaps a click there (design 032 §2.4).
pub fn open_end_near(doc: &Document, page: PageId, p: Pt, tol: f64, quiet: &[Pt]) -> Option<Pt> {
    let mut best: Option<(f64, Pt)> = None;
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (end, e) in End::BOTH.into_iter().zip(route.ends) {
            let Some(q) = end_point(doc, r, end) else { continue };
            let d = q.dist(p);
            if e == RouteEnd::Free && d <= tol && !is_quiet(quiet, q) && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, q));
            }
        }
    }
    best.map(|(_, q)| q)
}

/// What the Wire tool shows at an end point `at` (design 032 §2.4): the point, the word (`join`,
/// `tee`, or the first words of a refusal such as `0V ≠ 24V`) and whether it is refused; `None`
/// when the end would stay open.
pub fn join_preview(doc: &Document, page: PageId, at: Pt, quiet: &[Pt], potential: Option<&str>) -> Option<(Pt, String, bool)> {
    let (m, p) = meet_at(doc, page, at, quiet);
    match m {
        Meets::Free | Meets::Point(_) => None,
        Meets::Merge { route, .. } => {
            let theirs = route_net(doc, route).and_then(|n| ecad_model::wiring::net_potential(doc, n).name);
            match (theirs, potential) {
                (Some(x), Some(y)) if !x.eq_ignore_ascii_case(y) => Some((p, format!("{x} \u{2260} {y}"), true)),
                _ => Some((p, "join".into(), false)),
            }
        }
        Meets::On { .. } => Some((p, "tee".into(), false)),
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

/// The free route ends on `page` that look connected (design 032 §2.6): lying on another route's
/// end or interior, not quiet (a cable reference end). One point per place, in route order.
///
/// # Semantics
/// Computed during the page's projection from that page's routes only (the page index), with a
/// grid of their segments, so the cost follows the page, never the project (update map §8.2).
pub fn looks_connected_on_page(doc: &Document, page: PageId, quiet: &[Pt]) -> Vec<Pt> {
    let routes: Vec<&Route> = doc.routes_on(page).filter_map(|r| doc.routes.get(&r)).collect();
    let mut grid: std::collections::HashMap<(i64, i64), Vec<(usize, usize)>> = std::collections::HashMap::new();
    let mut long: Vec<(usize, usize)> = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for (s, w) in route.points.windows(2).enumerate() {
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
    for (r, route) in routes.iter().enumerate() {
        for (at, e) in [route.points.first(), route.points.last()].into_iter().zip(route.ends) {
            let Some(at) = at.copied() else { continue };
            if e != RouteEnd::Free || is_quiet(quiet, at) || out.iter().any(|q| q.near(at, ON_WIRE)) {
                continue;
            }
            let Some(cell) = looks_cell(at) else { continue };
            let near = grid.get(&cell).into_iter().flatten().chain(long.iter()).any(|(o, s)| {
                if *o == r {
                    return false;
                }
                let Some(ov) = routes.get(*o).map(|x| &x.points) else { return false };
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
    pub conductor: WireId,
    /// The route's net, if it has one.
    pub net: Option<ecad_model::NetKey>,
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
            let mut best: BTreeMap<(Option<ecad_model::NetKey>, Option<WireId>, &str), usize> = BTreeMap::new();
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
    let mut nets: BTreeMap<(Option<ecad_model::NetKey>, Option<WireId>), (bool, usize)> = BTreeMap::new();
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

/// The first terminal of `shown` (as `ecad_model::derive::terminal_positions` lists them) within
/// `tol` of `at`.
pub fn first_near(shown: &[(TerminalId, RepresentationId, Pt)], at: Pt, tol: f64) -> Option<TerminalId> {
    shown.iter().find(|(_, _, p)| p.near(at, tol)).map(|(t, _, _)| *t)
}

/// The terminal at each of many points on one document (`translate::terminal_at`): the positions are worked out on
/// first use and kept, so an edit that moves many route ends lists them once, not once per end.
pub struct ShownOnce<'a> {
    doc: &'a Document,
    page: PageId,
    shown: Option<Vec<(TerminalId, RepresentationId, Pt)>>,
}

impl<'a> ShownOnce<'a> {
    pub fn new(doc: &'a Document, page: PageId) -> Self {
        Self { doc, page, shown: None }
    }

    pub fn terminal_at(&mut self, at: Pt, tol: f64) -> Option<TerminalId> {
        let (doc, page) = (self.doc, self.page);
        first_near(self.shown.get_or_insert_with(|| ecad_model::derive::terminal_positions(doc, page)), at, tol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::{Color, Layer, LayerId, Page};

    /// A document with one page and one layer, and the style to draw on it.
    fn page_doc() -> (Document, PageId, Style) {
        let mut doc = Document::default();
        let (page, layer) = (PageId::from(1u128), LayerId::from(2u128));
        for op in
            [Op::AddLayer { id: layer, layer: Layer { name: "0".into() } }, Op::AddPage { id: page, page: Page::titled("1").unwrap(), index: 0 }]
        {
            op.apply(&mut doc).expect("setup");
        }
        doc.rebuild_index();
        (doc, page, Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 })
    }

    fn draw(doc: &mut Document, ids: &mut IdAllocator, page: PageId, style: &Style, pts: &[(f64, f64)]) -> WirePlan {
        let input = WireInput {
            page,
            style: style.clone(),
            points: pts.iter().map(|(x, y)| Pt::new(*x, *y)).collect(),
            potential: None,
            join: true,
            tol: 0.1,
            quiet: &[],
            max_points: 100,
        };
        let plan = plan_wire(doc, ids, input).expect("plan");
        for op in &plan.ops {
            op.apply(doc).expect("apply");
        }
        doc.rebuild_index();
        plan
    }

    /// The item drawing route `r` has the route's points.
    fn drawn_as_routed(doc: &Document, r: RouteId) -> bool {
        matches!(route_vertices(doc, r), Some((_, Item { kind: ItemKind::Primitive(Primitive::Polyline { vertices, .. }), .. }, v)) if vertices == v)
    }

    #[test]
    fn a_line_is_a_route_drawn_by_one_polyline() {
        let (mut doc, page, style) = page_doc();
        let mut ids = IdAllocator::seeded(7);
        let plan = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
        assert!(plan.created);
        let route = doc.routes.get(&plan.route).expect("route");
        assert_eq!(route.ends, [RouteEnd::Free, RouteEnd::Free]);
        assert!(drawn_as_routed(&doc, plan.route));
        assert!(doc.wires.is_empty(), "no connection points: no wire");
    }

    #[test]
    fn a_line_from_a_free_end_extends_that_route() {
        let (mut doc, page, style) = page_doc();
        let mut ids = IdAllocator::seeded(7);
        let first = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
        let second = draw(&mut doc, &mut ids, page, &style, &[(10.0, 0.0), (20.0, 0.0)]);
        assert!(!second.created);
        assert_eq!(second.route, first.route);
        assert_eq!(doc.routes.len(), 1);
        assert_eq!(doc.routes.get(&first.route).map(|r| r.points.clone()), Some(vec![Pt::new(0.0, 0.0), Pt::new(20.0, 0.0)]), "one straight line");
        assert!(drawn_as_routed(&doc, first.route));
    }

    #[test]
    fn a_line_between_two_free_ends_makes_one_route_of_three() {
        let (mut doc, page, style) = page_doc();
        let mut ids = IdAllocator::seeded(7);
        let a = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
        let b = draw(&mut doc, &mut ids, page, &style, &[(20.0, 10.0), (20.0, 20.0)]);
        let c = draw(&mut doc, &mut ids, page, &style, &[(10.0, 0.0), (20.0, 0.0), (20.0, 10.0)]);
        assert_eq!(c.route, a.route);
        assert_eq!(c.removed, vec![b.route]);
        assert_eq!(doc.routes.len(), 1);
        assert_eq!(doc.routes.get(&a.route).map(|r| r.points.len()), Some(3));
        assert!(drawn_as_routed(&doc, a.route));
        assert!(route_item(&doc, b.route).is_none(), "the merged route's drawing goes with it");
    }

    #[test]
    fn a_line_ending_on_a_route_is_a_tee_and_a_free_end_on_a_new_line_lands_on_it() {
        let (mut doc, page, style) = page_doc();
        let mut ids = IdAllocator::seeded(7);
        let main = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (20.0, 0.0)]);
        let tee = draw(&mut doc, &mut ids, page, &style, &[(5.0, 10.0), (5.0, 0.0)]);
        assert_eq!(doc.routes.get(&tee.route).map(|r| r.ends[1]), Some(RouteEnd::On { route: main.route, at: Pt::new(5.0, 0.0) }));
        assert!(doc.devices.is_empty(), "no junction component");
        // Reverse T: a free end lying on a new line's middle lands on it.
        let stub = draw(&mut doc, &mut ids, page, &style, &[(40.0, 10.0), (40.0, 0.0)]);
        let across = draw(&mut doc, &mut ids, page, &style, &[(30.0, 0.0), (50.0, 0.0)]);
        assert_eq!(across.tees, vec![(Pt::new(40.0, 0.0), stub.route)]);
        assert_eq!(doc.routes.get(&stub.route).map(|r| r.ends[1]), Some(RouteEnd::On { route: across.route, at: Pt::new(40.0, 0.0) }));
    }

    #[test]
    fn joining_two_free_ends_merges_and_tees_follow_the_survivor() {
        let (mut doc, page, style) = page_doc();
        let mut ids = IdAllocator::seeded(7);
        let a = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
        let input = WireInput {
            page,
            style: style.clone(),
            points: vec![Pt::new(10.0, 0.0), Pt::new(20.0, 0.0)],
            potential: None,
            join: false,
            tol: 0.1,
            quiet: &[],
            max_points: 100,
        };
        let b = plan_wire(&doc, &mut ids, input).expect("plan");
        for op in &b.ops {
            op.apply(&mut doc).expect("apply");
        }
        doc.rebuild_index();
        let tee = draw(&mut doc, &mut ids, page, &style, &[(15.0, 10.0), (15.0, 0.0)]);
        let ends = [(a.route, End::B), (b.route, End::A)];
        let j = plan_end_join(&doc, ends[0], ends[1], 100).expect("join").expect("merge");
        for op in &j.ops {
            op.apply(&mut doc).expect("apply");
        }
        doc.rebuild_index();
        assert_eq!(doc.routes.len(), 2);
        let survivor = j.survivor;
        assert_eq!(doc.routes.get(&tee.route).and_then(|r| r.ends[1].route()), Some(survivor));
        assert!(drawn_as_routed(&doc, survivor));
    }
}
