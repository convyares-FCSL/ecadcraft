//! Wire labels (§7) and continuation texts (§3.2).

use super::*;

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
    pub own: Option<crate::styles::text_style::OwnStyle>,
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
    let style = crate::styles::text_style::text_style(doc, crate::styles::text_style::TextKind::WireLabel);
    let fonts = crate::styles::text_style::fonts::Fonts::resolve_with(doc, m);
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
    let mut texts: Vec<(usize, ecad_model::connectivity::wiring::Label)> = Vec::new();
    // Brief 050b (design 050 §5.5): each wire's category gives its label's cross-section and
    // colour and its label rule.
    let draws = crate::styles::wire::WireDraws::new(doc);
    let mut rules: Vec<String> = Vec::new();
    for (i, (rid, c, v)) in routes.iter().enumerate() {
        let Some((a, b)) = longest_segment(v) else { continue };
        let draw = draws.draw(doc, *c);
        if draw.label_rule == "none" {
            continue;
        }
        let Some(label) =
            ecad_model::connectivity::wiring::label_with(doc, *c, draw.cross_section.map(|x| x.0), draw.colour.map(|x| x.0.to_string()))
        else {
            continue;
        };
        let target = crate::styles::text_style::TextTarget::conductor(*c, Some(page));
        if crate::styles::text_style::text_shown(doc, target, crate::styles::text_style::TextKind::WireLabel) == crate::styles::text_style::Shown::Off
        {
            continue;
        }
        routes_in.push(LabelCandidate { rep: *rid, conductor: *c, net: conn.net_of_conductor(*c), text: label.text.clone(), longest: a.dist(b) });
        texts.push((i, label));
        rules.push(draw.label_rule);
    }
    // One pass per rule over the candidates that follow it (the project's rule for most).
    let mut chosen_ix: Vec<usize> = Vec::new();
    let project_rule = label_rule(doc);
    for rule in rules.iter().collect::<std::collections::BTreeSet<_>>() {
        let keep: Vec<usize> = (0..routes_in.len()).filter(|k| rules.get(*k) == Some(rule)).collect();
        let subset: Vec<LabelCandidate> = keep.iter().filter_map(|k| routes_in.get(*k).cloned()).collect();
        let r = match rule.as_str() {
            ecad_model::connectivity::wiring::LABEL_RULE_EACH => LabelRule::Each(label_min_length(doc)),
            ecad_model::connectivity::wiring::LABEL_RULE_NET => LabelRule::Net,
            _ => project_rule,
        };
        chosen_ix.extend(labelled(&subset, r).into_iter().filter_map(|j| keep.get(j).copied()));
    }
    chosen_ix.sort_unstable();
    let chosen: Vec<(usize, ecad_model::connectivity::wiring::Label)> = chosen_ix.into_iter().filter_map(|k| texts.get(k).cloned()).collect();
    let mut placed: Vec<Bbox> = Vec::new();
    let mut out = Vec::new();
    for (i, label) in chosen {
        let Some((rid, c, v)) = routes.get(i) else { continue };
        // Brief 050a: a conductor's own wire label style (height, weight, colour, font).
        let own = doc.wires.get(c).and_then(|x| {
            let at = crate::defaults::Scope { conductor: Some(*c), ..crate::defaults::Scope::project() };
            crate::styles::text_style::own_style(doc, crate::styles::text_style::TextKind::WireLabel, at, &x.display)
        });
        let h = own.as_ref().map_or(h, |o| o.style.height);
        let Some((a, b)) = longest_segment(v) else { continue };
        let theta = reading_angle(a, b);
        if !theta.is_finite() {
            continue;
        }
        let w = match &own {
            Some(o) => {
                let f = o
                    .family
                    .as_ref()
                    .map_or_else(|| fonts.clone(), |fam| crate::styles::text_style::fonts::Fonts::for_family_with(doc, fam.clone(), m));
                crate::styles::text_style::fonts::width_in(&f, o.style, &label.text, m)
            }
            None => crate::styles::text_style::fonts::width_in(&fonts, style, &label.text, m),
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

/// The text beside a continuation symbol (design 022 §3.2): its name; `None` when `a` is not a
/// continuation. Its other ends' places are reference parts since brief 049
/// ([`crate::continuations`]).
pub fn continuation_texts(doc: &Document, a: AidId, rep: RepresentationId, pl: &Placement) -> Option<Vec<DerivedText>> {
    let (name, _) = crate::continuations::continuation(doc, a)?;
    let name = name.unwrap_or_default().to_string();
    let mut out = Vec::new();
    let at = pl.apply(Pt::new(2.5, -4.0));
    if !at.is_finite() {
        return Some(out);
    }
    out.push(DerivedText {
        rep,
        of: TextOf::Name,
        value: name,
        at,
        height: crate::styles::text_style::text_style(doc, crate::devices::texts::kind_of(TextOf::Name)).height,
        halign: ecad_model::TextHAlign::Left,
        valign: ecad_model::TextVAlign::Bottom,
        aci: None,
        own: None,
    });
    Some(out)
}
