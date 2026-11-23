//! The contact mirror table: rows, their states, the parts drawn, the table box and its placement.

use crate::styles::text_style::fonts::TextMetrics;
use ecad_model::objects::devices::PoleKind;
use ecad_model::{DeviceId, Document, Form, LengthUnit, PageId, Pt, RepresentationId, Subject};
use std::collections::{BTreeMap, BTreeSet};

use super::classify::{Device, DrawnPole, anchor, bounds, connection_point_points, device};
use super::parse::{OfferedPole, Placement, offer_of, placement_of};
use super::places::Places;

/// The text style of a contact's or a connector member's reference.
const REF: crate::styles::text_style::TextKind = crate::styles::text_style::TextKind::Reference;

/// The height of reference text in drawing units: the `Reference` text style's.
fn ref_height(doc: &Document) -> f64 {
    crate::styles::text_style::text_style(doc, REF).height
}

/// `ecad.references`: ACI 7 (ByLayer on `ECAD-TAGS`). Its text is reference text: the `Reference` text
/// style, never a size of its own.
/// `ecad.xref-spare`: ACI 8.
pub(crate) const SPARE_ACI: u8 = 8;
/// `ecad.orphan`: ACI 1, for rows drawn beyond the offer.
pub(crate) const OVER_ACI: u8 = crate::references::cables::ORPHAN_ACI;
/// `ecad.mirror-row`: the table's row pitch, millimetres.
pub const ROW_PITCH_MM: f64 = 4.0;
/// The gap before the first auxiliary row after power rows, millimetres.
pub(crate) const GROUP_GAP_MM: f64 = 2.0;
/// What a spare row shows for its place.
pub const SPARE_PLACE: &str = "—";

// ---------------------------------------------------------------- rows

/// Whether a row's pole is drawn, offered and not drawn, or drawn beyond the offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum State {
    Drawn,
    Spare,
    Over,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::Drawn => "drawn",
            State::Spare => "spare",
            State::Over => "over",
        }
    }
}

/// One row of a contact table.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub labels: [String; 2],
    pub kind: PoleKind,
    pub power: bool,
    pub state: State,
    /// The contact that draws it.
    pub rep: Option<RepresentationId>,
    pub page: Option<PageId>,
    pub at: Option<Pt>,
    /// Its place, or [`SPARE_PLACE`].
    pub place: String,
}

impl Row {
    /// `13/14`.
    pub(crate) fn label_pair(&self) -> String {
        format!("{}/{}", self.labels[0], self.labels[1])
    }

    /// `power` or `aux`.
    pub(crate) fn group_word(&self) -> &'static str {
        if self.power { "power" } else { "aux" }
    }
}

/// Natural order of labels: `2` before `13`, `A2` before `A10`.
pub fn natural_key(s: &str) -> Vec<(u8, u128, String)> {
    // One ordering for labels: the model's.
    ecad_model::connectivity::jumper_bars::natural_key(s)
}

/// The rows of device `dev`: offered poles matched to drawn poles by labels,
/// then by kind in order; unmatched offered poles are spare, unmatched drawn poles over. With no
/// offer every drawn pole is a plain row. Power rows first, each group in natural label order.
pub fn rows(places: &mut Places<'_>, dev: &Device, offer: Option<&[OfferedPole]>) -> Vec<Row> {
    let drawn_row = |places: &mut Places<'_>, p: &DrawnPole, state: State| Row {
        labels: p.labels.clone(),
        kind: p.kind,
        power: p.power,
        state,
        rep: Some(p.rep),
        page: Some(p.page),
        at: p.at,
        place: places.text(p.page, p.at),
    };
    let mut out: Vec<Row> = Vec::new();
    match offer {
        None => out.extend(dev.poles.iter().map(|p| drawn_row(places, p, State::Drawn))),
        Some(offer) => {
            let same = |o: &OfferedPole, p: &DrawnPole| o.kind == p.kind && o.power == p.power;
            let mut taken = vec![false; dev.poles.len()];
            let mut matched = vec![false; offer.len()];
            for (i, o) in offer.iter().enumerate() {
                if let Some(j) = dev.poles.iter().enumerate().position(|(j, p)| !taken[j] && same(o, p) && p.labels == o.labels) {
                    taken[j] = true;
                    matched[i] = true;
                }
            }
            for (i, o) in offer.iter().enumerate() {
                if matched[i] {
                    continue;
                }
                if let Some(j) = dev.poles.iter().enumerate().position(|(j, p)| !taken[j] && same(o, p)) {
                    taken[j] = true;
                    matched[i] = true;
                }
            }
            for (j, p) in dev.poles.iter().enumerate() {
                out.push(drawn_row(places, p, if taken[j] { State::Drawn } else { State::Over }));
            }
            for (i, o) in offer.iter().enumerate().filter(|(i, _)| !matched[*i]) {
                let _ = i;
                out.push(Row {
                    labels: o.labels.clone(),
                    kind: o.kind,
                    power: o.power,
                    state: State::Spare,
                    rep: None,
                    page: None,
                    at: None,
                    place: SPARE_PLACE.into(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        (!a.power, natural_key(&a.labels[0]), natural_key(&a.labels[1]), a.state, a.rep).cmp(&(
            !b.power,
            natural_key(&b.labels[0]),
            natural_key(&b.labels[1]),
            b.state,
            b.rep,
        ))
    });
    out
}

/// Per kind (power NO, power NC, auxiliary NO, auxiliary NC): drawn and offered.
pub fn counts(dev: &Device, offer: &[OfferedPole]) -> BTreeMap<(bool, PoleKind), (usize, usize)> {
    let mut m: BTreeMap<(bool, PoleKind), (usize, usize)> = BTreeMap::new();
    for p in &dev.poles {
        m.entry((p.power, p.kind)).or_default().0 += 1;
    }
    for o in offer {
        m.entry((o.power, o.kind)).or_default().1 += 1;
    }
    m
}

/// `auxiliary NO`, `power NC`…
pub fn kind_words(power: bool, kind: PoleKind) -> String {
    format!("{} {}", if power { "power" } else { "auxiliary" }, kind.upper())
}

/// The labels the next contact of kind `make` joining `c` takes from its offer: the first offered auxiliary pole of
/// that kind whose labels are free. `None` when `c` has no readable offer or no such pole is free.
pub fn offer_labels(doc: &Document, c: DeviceId, make: bool, used: &BTreeSet<String>) -> Option<Vec<String>> {
    let (_, offer) = offer_of(doc, c)?;
    let kind = if make { PoleKind::No } else { PoleKind::Nc };
    offer.iter().find(|o| !o.power && o.kind == kind && !used.contains(&o.labels[0]) && !used.contains(&o.labels[1])).map(|o| o.labels.to_vec())
}

/// The echo after a join that went beyond the offer, if it did.
pub fn over_line(doc: &Document, c: DeviceId, labels: &[String], make: bool) -> Option<String> {
    let (text, offer) = offer_of(doc, c)?;
    let dev = device(doc, c);
    let kind = if make { PoleKind::No } else { PoleKind::Nc };
    let (drawn, offered) = counts(&dev, &offer).get(&(false, kind)).copied().unwrap_or_default();
    (drawn > offered).then(|| {
        format!(
            "Added {} contact {} to {}; its part offers {offered} {} ({text}), now {drawn} drawn (see Checks).",
            kind.upper(),
            labels.join("/"),
            crate::devices::tags::tag_words(doc, c),
            kind_words(false, kind),
        )
    })
}

// ---------------------------------------------------------------- drawing

/// Where a reference text sits across its point (the drawing maps it to CADCraft's alignment,
/// `draw::reference`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Where a reference text sits up and down its point (as [`HAlign`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// A drawn part of a reference: a line or a text, in drawing units.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line(Pt, Pt),
    /// `rotation` in radians: 0 for every text but a continuation's reference.
    /// `kind` is the text style it is drawn in: its height is that style's, and the
    /// engine applies the style's font, weight and colour.
    Text {
        kind: crate::styles::text_style::TextKind,
        at: Pt,
        value: String,
        height: f64,
        halign: HAlign,
        valign: VAlign,
        rotation: f64,
    },
}

/// One part of a contact's text or a head's table.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// The contact (its text) or the head (its table).
    pub rep: RepresentationId,
    /// Its index among that representation's parts (handles are kept by it).
    pub index: u32,
    pub shape: Shape,
    /// A colour of its own (spare, over), else ByLayer.
    pub aci: Option<u8>,
    /// The table row it belongs to.
    pub row: Option<usize>,
    /// The table's top bar (drawn at the wire lineweight).
    pub bar: bool,
}

pub(super) fn unit(doc: &Document) -> f64 {
    match doc.settings().length_unit {
        LengthUnit::Millimetre => 1.0,
        LengthUnit::Inch => 1.0 / 25.4,
    }
}

/// The text under contact `rep` of device `dev`: its head's place, where it is
/// drawn, and how it is aligned. `None` when the device has no head or the contact cannot be
/// drawn.
pub fn contact_text(doc: &Document, places: &mut Places<'_>, dev: &Device, rep: RepresentationId) -> Option<Part> {
    let head = dev.head?;
    if !dev.children.contains(&rep) {
        return None;
    }
    let h = doc.representations().get(&head)?;
    let value = places.text(h.page, anchor(doc, h));
    let r = doc.representations().get(&rep)?;
    let k = unit(doc);
    let pts = connection_point_points(doc, dev.component, r);
    let vertical = match crate::devices::tags::rep_definition(doc, r) {
        Some((_, pl)) if r.form == Form::Symbol => crate::devices::texts::vertical(&pl),
        _ => {
            let xs: Vec<f64> = pts.iter().map(|(_, p)| p.x).collect();
            xs.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-6)
        }
    };
    let (at, halign) = if vertical {
        let x = pts.iter().map(|(_, p)| p.x).fold(f64::NEG_INFINITY, f64::max);
        let y = pts.iter().map(|(_, p)| p.y).fold(f64::INFINITY, f64::min);
        (Pt::new(x + 1.0 * k, y - 1.0 * k), HAlign::Left)
    } else {
        let (lo, hi) = match (r.form, crate::devices::tags::rep_definition(doc, r)) {
            (Form::Symbol, Some((def, pl))) => crate::devices::texts::placed_box(def, &pl)?,
            _ => bounds(&pts.iter().map(|(_, p)| *p).collect::<Vec<_>>())?,
        };
        (Pt::new((lo.x + hi.x) / 2.0, lo.y - 1.5 * k), HAlign::Center)
    };
    at.is_finite().then_some(Part {
        rep,
        index: 0,
        shape: Shape::Text { kind: REF, at, value, height: ref_height(doc), halign, valign: VAlign::Top, rotation: 0.0 },
        aci: None,
        row: None,
        bar: false,
    })
}

/// A head's table: where it is, its rows and its parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub head: RepresentationId,
    pub page: PageId,
    /// The top centre of its bar.
    pub top: Pt,
    pub placement: Placement,
    pub rows: Vec<Row>,
    pub parts: Vec<Part>,
    /// Which step placed it.
    pub shown: TableShown,
    /// The y of its box's bottom (drawing units).
    pub bottom: f64,
}

/// Where a contact table went: where its placement puts it, right of its head
/// (no room below), or lifted above the title block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableShown {
    AsPlaced,
    Right,
    Lifted,
}

impl TableShown {
    /// The `contacts` reply's word.
    pub fn word(self) -> &'static str {
        match self {
            TableShown::AsPlaced => "as placed",
            TableShown::Right => "right",
            TableShown::Lifted => "lifted",
        }
    }
}

/// The box of a table at `top` with `rows`: from −10 mm to 12 mm plus the widest
/// place text across, 3 mm plus the rows and the power/auxiliary gap down. (lo, hi), drawing units.
pub fn table_box(doc: &Document, top: Pt, rows: &[Row], m: &dyn TextMetrics) -> (Pt, Pt) {
    let k = unit(doc);
    // The widest place text in the font and size it is drawn with (drawing units).
    let widest = rows
        .iter()
        .map(|r| crate::styles::text_style::fonts::text_width(doc, crate::styles::text_style::TextKind::Table, &r.place, m))
        .fold(0.0, f64::max);
    let gap = rows.windows(2).any(|w| w.first().is_some_and(|a| a.power) && w.get(1).is_some_and(|b| !b.power));
    let height = 3.0 + ROW_PITCH_MM * rows.len() as f64 + if gap { GROUP_GAP_MM } else { 0.0 };
    (Pt::new(top.x - 10.0 * k, top.y - height * k), Pt::new(top.x + 12.0 * k + widest, top.y))
}

/// The page's free area for tables: the frame's drawing area above the title
/// block, the registry's `print.contact_table_clearance` clear of its top line. (lo, hi),
/// drawing units.
pub(crate) fn free_area(doc: &Document, page: PageId) -> (Pt, Pt) {
    let g = ecad_model::document::sheet::Grid::of(doc, page);
    let k = unit(doc);
    let gap = crate::defaults::resolve(doc, &crate::defaults::print::CONTACT_TABLE_CLEARANCE, crate::defaults::Scope::project()).length(doc);
    let lo = Pt::new(g.frame_left(), g.frame_bottom() + ecad_model::document::sheet::keep_out(doc, page) * k + gap);
    let hi = Pt::new(g.frame_left() + g.area_width(), g.frame_top());
    (lo, hi)
}

/// Whether the box `b` lies in the free area `free`.
fn inside(b: (Pt, Pt), free: (Pt, Pt)) -> bool {
    b.0.x >= free.0.x - 1e-9 && b.0.y >= free.0.y - 1e-9 && b.1.x <= free.1.x + 1e-9 && b.1.y <= free.1.y + 1e-9
}

/// Whether the box `b` enters the title block of `page` (the user-placed check).
pub(crate) fn enters_title_block(doc: &Document, page: PageId, b: (Pt, Pt)) -> bool {
    let g = ecad_model::document::sheet::Grid::of(doc, page);
    let line = g.frame_bottom() + ecad_model::document::sheet::keep_out(doc, page) * unit(doc);
    let (left, right) = (g.frame_left(), g.frame_left() + g.area_width());
    b.0.y < line && b.1.y > g.frame_bottom() && b.1.x > left && b.0.x < right
}

/// Where an automatic table goes, from where `placement` puts it (`top`): there
/// when its box lies in the free area; else right of the head (`auto`, `below`) when that fits;
/// else lifted onto the free area's bottom and pulled left inside the frame. A table taller than
/// the free area starts at its top. A table the user placed (`at`) is never moved.
pub fn settle(
    doc: &Document, c: DeviceId, head: RepresentationId, placement: Placement, top: Pt, rows: &[Row], m: &dyn TextMetrics,
) -> (Pt, TableShown) {
    if matches!(placement, Placement::At(_) | Placement::Off) {
        return (top, TableShown::AsPlaced);
    }
    let Some(page) = doc.representations().get(&head).map(|r| r.page) else { return (top, TableShown::AsPlaced) };
    let free = free_area(doc, page);
    if inside(table_box(doc, top, rows, m), free) {
        return (top, TableShown::AsPlaced);
    }
    if matches!(placement, Placement::Auto | Placement::Below)
        && let Some(right) = table_top(doc, c, head, Placement::Right)
        && inside(table_box(doc, right, rows, m), free)
    {
        return (right, TableShown::Right);
    }
    let (lo, hi) = table_box(doc, top, rows, m);
    let mut at = top;
    if lo.y < free.0.y {
        at.y += free.0.y - lo.y;
    }
    if at.y > free.1.y {
        at.y = free.1.y;
    }
    if hi.x > free.1.x {
        at.x -= hi.x - free.1.x;
    }
    if at.x - (top.x - lo.x) < free.0.x {
        at.x = free.0.x + (top.x - lo.x);
    }
    if at == top || !at.is_finite() {
        return (top, TableShown::AsPlaced);
    }
    (at, TableShown::Lifted)
}

/// The straight vertical run of a conductor route leaving `from` (one of its ends): the y of the
/// run's far end, when it runs down.
fn run_down(path: &[Pt], from: Pt) -> Option<f64> {
    let (first, last) = (*path.first()?, *path.last()?);
    let pts: Vec<Pt> = if first.dist(from) <= last.dist(from) { path.to_vec() } else { path.iter().rev().copied().collect() };
    let start = *pts.first()?;
    let mut end = start;
    for p in pts.iter().skip(1) {
        if (p.x - start.x).abs() > 1e-6 {
            break;
        }
        end = *p;
    }
    (end.y < start.y - 1e-6).then_some(end.y)
}

/// Where the table of head `head` of device `c` goes, for `placement`; `None` for `off` or a
/// head that cannot be drawn.
pub fn table_top(doc: &Document, c: DeviceId, head: RepresentationId, placement: Placement) -> Option<Pt> {
    let r = doc.representations().get(&head)?;
    let k = unit(doc);
    let origin = anchor(doc, r)?;
    let pts = connection_point_points(doc, c, r);
    let lowest = pts.iter().min_by(|a, b| a.1.y.total_cmp(&b.1.y).then(a.0.cmp(&b.0))).copied();
    let below = || lowest.map_or(Pt::new(origin.x, origin.y - 4.0 * k), |(_, p)| Pt::new(origin.x, p.y - 4.0 * k));
    let top = match placement {
        Placement::Off => return None,
        Placement::At(d) => Pt::new(origin.x + d.x * k, origin.y + d.y * k),
        Placement::Below => below(),
        Placement::Right => {
            let hi = match crate::devices::tags::rep_definition(doc, r) {
                Some((def, pl)) if r.form == Form::Symbol => crate::devices::texts::placed_box(def, &pl).map(|(_, hi)| hi),
                _ => bounds(&pts.iter().map(|(_, p)| *p).collect::<Vec<_>>()).map(|(_, hi)| hi),
            };
            let hi = hi?;
            Pt::new(hi.x + 14.0 * k, hi.y)
        }
        Placement::Auto => {
            let run = lowest.and_then(|(t, p)| {
                doc.routes_at(t).filter_map(|x| doc.routes().get(&x)).filter(|x| x.page == r.page).find_map(|x| run_down(&x.points, p))
            });
            match run {
                Some(y) => Pt::new(origin.x, y - 4.0 * k),
                None => below(),
            }
        }
    };
    top.is_finite().then_some(top)
}

/// The table under device `dev`'s head, or `None`: no head, `off`, or neither
/// children nor an offer.
pub fn table_measured(doc: &Document, places: &mut Places<'_>, dev: &Device, m: &dyn TextMetrics) -> Option<Table> {
    let head = dev.head?;
    let offer = offer_of(doc, dev.component);
    if dev.children.is_empty() && offer.is_none() {
        return None;
    }
    let (placement, _) = placement_of(doc, dev.component);
    let top = table_top(doc, dev.component, head, placement)?;
    let page = doc.representations().get(&head)?.page;
    let rows = rows(places, dev, offer.as_ref().map(|(_, o)| o.as_slice()));
    let (top, shown) = settle(doc, dev.component, head, placement, top, &rows, m);
    let parts = table_parts(doc, head, top, &rows);
    let bottom = table_box(doc, top, &rows, m).0.y;
    Some(Table { head, page, top, placement, rows, parts, shown, bottom })
}

/// The lines and texts of a table at `top`.
pub fn table_parts(doc: &Document, head: RepresentationId, top: Pt, rows: &[Row]) -> Vec<Part> {
    let k = unit(doc);
    let at = |x: f64, y: f64| Pt::new(top.x + x * k, top.y + y * k);
    let mut parts = Vec::new();
    let mut push = |shape: Shape, aci: Option<u8>, row: Option<usize>, bar: bool| {
        let index = u32::try_from(parts.len()).unwrap_or(u32::MAX);
        parts.push(Part { rep: head, index, shape, aci, row, bar });
    };
    push(Shape::Line(at(-10.0, 0.0), at(10.0, 0.0)), None, None, true);
    // Contact-table rows are table text.
    let h = crate::styles::text_style::text_style(doc, crate::styles::text_style::TextKind::Table).height;
    let mut y = -3.0;
    let mut last_power = rows.first().map(|r| r.power);
    for (i, r) in rows.iter().enumerate() {
        if last_power == Some(true) && !r.power {
            y -= GROUP_GAP_MM;
        }
        last_power = Some(r.power);
        let aci = match r.state {
            State::Drawn => None,
            State::Spare => Some(SPARE_ACI),
            State::Over => Some(OVER_ACI),
        };
        let text = |x: f64, value: &str, halign: HAlign| Shape::Text {
            kind: crate::styles::text_style::TextKind::Table,
            at: at(x, y),
            value: value.to_string(),
            height: h,
            halign,
            valign: VAlign::Middle,
            rotation: 0.0,
        };
        push(text(-5.0, &r.labels[0], HAlign::Right), aci, Some(i), false);
        let line = |a: (f64, f64), b: (f64, f64)| Shape::Line(at(a.0, y + a.1), at(b.0, y + b.1));
        push(line((-4.0, 0.0), (-1.5, 0.0)), aci, Some(i), false);
        match r.kind {
            PoleKind::No => push(line((-1.5, 0.0), (1.5, 1.5)), aci, Some(i), false),
            PoleKind::Nc => {
                push(line((-1.5, 0.0), (2.0, -1.2)), aci, Some(i), false);
                push(line((1.5, 0.0), (1.5, -1.2)), aci, Some(i), false);
            }
        }
        push(line((1.5, 0.0), (4.0, 0.0)), aci, Some(i), false);
        push(text(5.0, &r.labels[1], HAlign::Left), aci, Some(i), false);
        push(text(12.0, &r.place, HAlign::Left), aci, Some(i), false);
        y -= ROW_PITCH_MM;
    }
    parts
}

/// Most places one connector reference lists before `+n` (hover lists all).
pub(crate) const MAX_CONNECTOR_PLACES: usize = 3;

/// The reference texts of connector `c`: under each member with targets, their
/// places (`/102.C4`), at most [`MAX_CONNECTOR_PLACES`] then `+n`. O(the connector's symbols).
pub(crate) fn connector_parts(doc: &Document, places: &mut Places<'_>, c: DeviceId) -> Vec<(PageId, Part)> {
    let list = crate::connectors::ref_classes(doc, c);
    let edges = crate::connectors::ref_edges(doc, c, &list);
    let mut targets: BTreeMap<RepresentationId, Vec<RepresentationId>> = BTreeMap::new();
    for (s, t) in edges {
        targets.entry(s).or_default().push(t);
    }
    let k = unit(doc);
    let mut out = Vec::new();
    for (rid, ts) in targets {
        let Some(r) = doc.representations().get(&rid) else { continue };
        let mut texts: Vec<String> = ts.iter().filter_map(|t| doc.representations().get(t)).map(|t| places.text(t.page, anchor(doc, t))).collect();
        texts.sort();
        texts.dedup();
        let more = texts.len().saturating_sub(MAX_CONNECTOR_PLACES);
        texts.truncate(MAX_CONNECTOR_PLACES);
        let mut value = texts.join(" ");
        if more > 0 {
            value.push_str(&format!(" +{more}"));
        }
        let Some((lo, hi)) = (match (r.form, crate::devices::tags::rep_definition(doc, r)) {
            (Form::Symbol, Some((def, pl))) => crate::devices::texts::placed_box(def, &pl),
            _ => None,
        }) else {
            continue;
        };
        let at = Pt::new((lo.x + hi.x) / 2.0, lo.y - 1.5 * k);
        if !at.is_finite() {
            continue;
        }
        out.push((
            r.page,
            Part {
                rep: rid,
                index: 0,
                shape: Shape::Text { kind: REF, at, value, height: ref_height(doc), halign: HAlign::Center, valign: VAlign::Top, rotation: 0.0 },
                aci: None,
                row: None,
                bar: false,
            },
        ));
    }
    out
}

/// Every reference part drawn on `page`: the texts under its contacts and the tables under its
/// heads. O(the page's component representations and their groups).
pub fn parts_on_page_measured(doc: &Document, page: PageId, m: &dyn TextMetrics) -> Vec<Part> {
    let mut by_component: BTreeMap<DeviceId, Vec<RepresentationId>> = BTreeMap::new();
    for (rid, r) in doc.representations_on(page) {
        if let Subject::Device(c) = r.subject {
            by_component.entry(c).or_default().push(rid);
        }
    }
    let mut places = Places::new(doc);
    let mut out = Vec::new();
    let mut tables: Vec<Table> = Vec::new();
    for (c, here) in by_component {
        // A connector's half and pin references.
        if crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating) {
            out.extend(connector_parts(doc, &mut places, c).into_iter().filter(|(_, p)| here.contains(&p.rep)).map(|(_, p)| p));
            continue;
        }
        let dev = device(doc, c);
        if dev.head.is_none() {
            continue;
        }
        for rid in &here {
            if let Some(p) = contact_text(doc, &mut places, &dev, *rid) {
                out.push(p);
            }
        }
        if dev.head.is_some_and(|h| here.contains(&h))
            && let Some(t) = table_measured(doc, &mut places, &dev, m)
        {
            tables.push(t);
        }
    }
    // Pack re-run, bug 4: tables of heads drawn close together clear each other. This depends
    // only on the page's tables as each device derives it, and a commit that changes one of them
    // redraws the page ([`Watch`]), so the update map is unchanged.
    spread(doc, &mut tables, m);
    out.extend(tables.into_iter().flat_map(|t| t.parts));
    // Continuation arrows' other ends, brackets and captions.
    out.extend(crate::continuations::parts_on_page_measured(doc, &mut places, page, m));
    out
}

/// The most steps one table takes to clear the tables before it (hostile pages stay bounded).
const MAX_SPREAD_STEPS: usize = 64;
/// The gap kept between two tables spread apart, millimetres.
const SPREAD_GAP_MM: f64 = 2.0;

/// Whether boxes `a` and `b` overlap (touching is not overlapping).
fn boxes_overlap(a: (Pt, Pt), b: (Pt, Pt)) -> bool {
    a.0.x < b.1.x - 1e-9 && b.0.x < a.1.x - 1e-9 && a.0.y < b.1.y - 1e-9 && b.0.y < a.1.y - 1e-9
}

/// Spread one page's `tables` so that no two overlap (pack re-run, bug 4). A table the user placed
/// (`at`) never moves. The others are taken left to right (then top down): each keeps its place
/// when it clears the tables before it, else moves right past them; when that leaves the free
/// area, it goes under them instead, if that stays in the free area, else stays. Deterministic,
/// and each table moves only for tables before it.
pub fn spread(doc: &Document, tables: &mut [Table], m: &dyn TextMetrics) {
    let Some(page) = tables.first().map(|t| t.page) else { return };
    let free = free_area(doc, page);
    let gap = SPREAD_GAP_MM * unit(doc);
    let fixed = |t: &Table| matches!(t.placement, Placement::At(_) | Placement::Off);
    let mut taken: Vec<(Pt, Pt)> = tables.iter().filter(|t| fixed(t)).map(|t| table_box(doc, t.top, &t.rows, m)).collect();
    let mut order: Vec<usize> = (0..tables.len()).filter(|i| tables.get(*i).is_some_and(|t| !fixed(t))).collect();
    order.sort_by(|a, b| match (tables.get(*a), tables.get(*b)) {
        (Some(x), Some(y)) => x.top.x.total_cmp(&y.top.x).then(y.top.y.total_cmp(&x.top.y)).then(x.head.cmp(&y.head)),
        _ => a.cmp(b),
    });
    for i in order {
        let Some(t) = tables.get_mut(i) else { continue };
        let first = table_box(doc, t.top, &t.rows, m);
        let left = t.top.x - first.0.x;
        let height = t.top.y - first.0.y;
        // Right past the tables it meets.
        let mut right = t.top;
        for _ in 0..MAX_SPREAD_STEPS {
            let b = table_box(doc, right, &t.rows, m);
            let Some(edge) = taken.iter().filter(|o| boxes_overlap(b, **o)).map(|o| o.1.x).reduce(f64::max) else { break };
            right.x = edge + gap + left;
        }
        let mut top = right;
        if !inside(table_box(doc, right, &t.rows, m), free) && inside(first, free) {
            // Under the tables it meets, from its own place; else where it was (never off the
            // free area it started in).
            let mut down = t.top;
            for _ in 0..MAX_SPREAD_STEPS {
                let b = table_box(doc, down, &t.rows, m);
                let Some(edge) = taken.iter().filter(|o| boxes_overlap(b, **o)).map(|o| o.0.y).reduce(f64::min) else { break };
                down.y = edge - gap;
            }
            top = if inside(table_box(doc, down, &t.rows, m), free) { down } else { t.top };
        }
        let b = table_box(doc, top, &t.rows, m);
        if top != t.top && top.is_finite() && taken.iter().all(|o| !boxes_overlap(b, *o)) {
            t.top = top;
            t.parts = table_parts(doc, t.head, top, &t.rows);
            t.bottom = top.y - height;
        }
        taken.push(table_box(doc, t.top, &t.rows, m));
    }
}

/// Device `dev`'s table where its page draws it: `table`, then moved by [`spread`] with the other
/// tables on its page. O(devices on that page); for replies, not per commit.
pub(crate) fn drawn_table(doc: &Document, places: &mut Places<'_>, dev: &Device, m: &dyn TextMetrics) -> Option<Table> {
    let own = table_measured(doc, places, dev, m)?;
    let mut others: BTreeSet<DeviceId> = BTreeSet::new();
    for (_, r) in doc.representations_on(own.page) {
        if let Subject::Device(c) = r.subject
            && c != dev.component
        {
            others.insert(c);
        }
    }
    let mut tables: Vec<Table> = others
        .into_iter()
        .filter(|c| !crate::connectors::connector_of(doc, *c).is_some_and(|x| x.mating))
        .map(|c| device(doc, c))
        .filter(|d| d.head.is_some_and(|h| doc.representations().get(&h).is_some_and(|r| r.page == own.page)))
        .filter_map(|d| table_measured(doc, places, &d, m))
        .collect();
    let head = own.head;
    tables.push(own);
    spread(doc, &mut tables, m);
    tables.into_iter().find(|t| t.head == head)
}

/// What device `c` shows on each page (its contacts' texts and its table), for comparing before
/// and after a commit: page → parts.
pub(super) fn shown(doc: &Document, places: &mut Places<'_>, c: DeviceId, m: &dyn TextMetrics) -> BTreeMap<PageId, Vec<Part>> {
    let mut out: BTreeMap<PageId, Vec<Part>> = BTreeMap::new();
    // A connector's references.
    if crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating) {
        for (page, p) in connector_parts(doc, places, c) {
            out.entry(page).or_default().push(p);
        }
        return out;
    }
    let dev = device(doc, c);
    if dev.head.is_none() {
        return out;
    }
    for rid in &dev.children {
        if let (Some(p), Some(r)) = (contact_text(doc, places, &dev, *rid), doc.representations().get(rid)) {
            out.entry(r.page).or_default().push(p);
        }
    }
    if let Some(t) = table_measured(doc, places, &dev, m) {
        out.entry(t.page).or_default().extend(t.parts);
    }
    out
}
