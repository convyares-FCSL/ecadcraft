//! Derived texts.

use super::*;

/// What a derived text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextOf {
    Tag,
    ConnectionPoint(ConnectionPointId),
    /// A continuation's name.
    Name,
    /// A continuation's other places.
    Places,
    /// A device's description under its tag (hidden by default).
    Description,
    /// A device's part number under its tag (hidden by default).
    PartNumber,
    /// Another device text a project symbol's text slot places: its
    /// index in the definition's `texts`.
    Slot(u8),
    /// A PLC card body's caption, its rack and slot.
    Caption,
    /// A channel cell's channel number, address and description
    /// (written turned 90° when `rotated`).
    ChannelNo,
    Address,
    ChannelText {
        rotated: bool,
    },
}

/// One derived text on a page: a device's tag beside one of its symbols, or a terminal label.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedText {
    pub rep: RepresentationId,
    pub of: TextOf,
    pub value: String,
    pub at: Pt,
    pub height: f64,
    pub halign: HAlign,
    pub valign: VAlign,
    /// A colour of its own (ACI), else ByLayer.
    pub aci: Option<u8>,
    /// The object's own text style (its `text.<kind>.style`), drawn instead of the
    /// kind's project style; `None` for the kind's.
    pub own: Option<crate::styles::text_style::OwnStyle>,
}

/// The page box of a placed definition.
pub(crate) fn placed_box(def: &Definition, pl: &Placement) -> Option<(Pt, Pt)> {
    let (lo, hi) = symbols::graphics::bounds(def);
    let corners = [lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)].map(|q| pl.apply(q));
    let (mut a, mut b) = (corners[0], corners[0]);
    for q in corners {
        a = Pt::new(a.x.min(q.x), a.y.min(q.y));
        b = Pt::new(b.x.max(q.x), b.y.max(q.y));
    }
    (a.is_finite() && b.is_finite()).then_some((a, b))
}

/// Whether a placement's terminal axis is vertical (rotation near 0° or 180°).
pub(crate) fn vertical(pl: &Placement) -> bool {
    let q = pl.rotation.0.rem_euclid(std::f64::consts::PI);
    q < std::f64::consts::FRAC_PI_4 || q > 3.0 * std::f64::consts::FRAC_PI_4
}

/// The sizes of a symbol's tag and connection point labels in a project:
/// heights from the style table, the gap between the box and the tag as `text.tag.gap` × the
/// tag height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TagSizes {
    pub tag: f64,
    pub label: f64,
    pub gap: f64,
}

impl TagSizes {
    /// The sizes of `doc`.
    pub fn of(doc: &Document) -> TagSizes {
        use crate::styles::text_style::text_style;
        use ecad_model::look::TextKind;
        let tag = text_style(doc, TextKind::Tag).height;
        let factor = ecad_model::registry::resolve(doc, &crate::defaults::text::TAG_GAP, ecad_model::registry::Scope::project()).number();
        TagSizes { tag, label: text_style(doc, TextKind::PointLabel).height, gap: factor * tag }
    }
}

/// The kind of text a derived text is.
pub fn kind_of(of: TextOf) -> ecad_model::look::TextKind {
    use ecad_model::look::TextKind;
    match of {
        TextOf::Tag => TextKind::Tag,
        TextOf::ConnectionPoint(_) => TextKind::PointLabel,
        TextOf::Name => TextKind::ContinuationName,
        TextOf::Places => TextKind::Reference,
        TextOf::Description => TextKind::Description,
        // The card's caption is a name-block line; channel functions
        // and addresses have text styles of their own.
        TextOf::Caption => TextKind::PartNumber,
        TextOf::ChannelText { .. } => TextKind::ChannelText,
        TextOf::Address => TextKind::Address,
        TextOf::ChannelNo => TextKind::PointLabel,
        TextOf::PartNumber => TextKind::PartNumber,
        TextOf::Slot(_) => TextKind::Description,
    }
}

/// The texts of one symbol representation at `pl` (also used by the placing preview): its
/// device's tag and the labels `labels` at their page positions, at `sizes`.
pub fn texts_for(
    def: &Definition, pl: &Placement, tag: Option<&str>, labels: &[(TextOf, String, Pt)], sizes: TagSizes,
) -> Vec<(TextOf, String, Pt, f64, HAlign, VAlign)> {
    let Some((lo, hi)) = placed_box(def, pl) else { return Vec::new() };
    let centre = Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    let vert = vertical(pl);
    let mut out = Vec::new();
    if let Some(t) = tag {
        let (at, h, v) = if vert {
            (Pt::new(lo.x - sizes.gap, centre.y), HAlign::Right, VAlign::Middle)
        } else {
            (Pt::new(centre.x, hi.y + sizes.gap), HAlign::Center, VAlign::Bottom)
        };
        out.push((TextOf::Tag, t.to_string(), at, sizes.tag, h, v));
    }
    for (of, label, at) in labels {
        if !at.is_finite() {
            continue;
        }
        let (p, h, v) = if vert {
            let dy = if at.y > centre.y { -2.5 } else { 2.5 };
            (Pt::new(at.x + 1.0, at.y + dy), HAlign::Left, VAlign::Middle)
        } else {
            let dx = if at.x > centre.x { -2.5 } else { 2.5 };
            (Pt::new(at.x + dx, at.y + 1.0), HAlign::Center, VAlign::Bottom)
        };
        out.push((*of, label.clone(), p, sizes.label, h, v));
    }
    out
}

/// Every derived text shown on `page` (what the page draws and prints): `all_texts_on_page`'s
/// shown ones.
///
/// `m` measures the texts (the C wrapper `texts_on_page` passes CADCraft's fonts).
pub fn texts_on_page_measured(doc: &Document, page: PageId, m: &dyn TextLook) -> Vec<DerivedText> {
    all_texts_measured(doc, page, None, m).into_iter().filter(|t| t.shown).map(|t| t.text).collect()
}

/// A derived text and whether it is shown: hidden texts are still data, listed by
/// `pagetexts` and drawn dimmed by Show Hidden Text.
#[derive(Debug, Clone, PartialEq)]
pub struct PageText {
    pub text: DerivedText,
    pub shown: bool,
    /// Its place is stored (Move Text).
    pub moved: bool,
    /// Its tag block found no free candidate.
    pub overlaps: bool,
}

thread_local! {
    /// How many times [`all_texts_on_page`] ran per page on this thread: the update-map tests
    /// count it (a change elsewhere must not work a page's texts out again).
    static TEXT_PASSES: std::cell::RefCell<BTreeMap<PageId, u64>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// How many times this thread has worked out `page`'s derived texts (see `TEXT_PASSES`).
pub fn text_passes(page: PageId) -> u64 {
    TEXT_PASSES.with(|c| c.borrow().get(&page).copied().unwrap_or(0))
}

/// The run rule of a symbol, from its starter kind.
pub(crate) fn run_rule(doc: &Document, r: &Representation) -> crate::styles::text_style::RunRule {
    use crate::styles::text_style::RunRule;
    // A connector's pins show their half's tag once per run.
    if crate::symbols::reps::rep_type(doc, r) == ecad_model::PartRole::Pin {
        return RunRule::Strip;
    }
    match rep_starter(doc, r).map(|s| s.kind) {
        Some(Kind::TerminalBlock) => RunRule::Strip,
        Some(Kind::Channel(_)) => RunRule::Plc,
        _ => RunRule::None,
    }
}

/// Every derived text on `page`, shown or hidden: for each symbol representation of a component,
/// its device's tag block (the tag, then its description and part number when they have values)
/// and its terminals' labels; a continuation's name and other places. Visibility follows design
/// (symbol, device, page, project, built-in) and a tag set to once per run is shown on
/// the first symbol of its run only. Hostile values (non-finite placements) give nothing
/// for that representation. PLC addresses are read from the kept model `plc` when there is one,
/// and `m` measures the texts (the C wrappers
/// `all_texts_on_page` and `all_texts_on_page_with` pass CADCraft's fonts).
pub fn all_texts_measured(doc: &Document, page: PageId, plc: Option<&crate::plc::kept::Plc>, m: &dyn TextLook) -> Vec<PageText> {
    TEXT_PASSES.with(|c| *c.borrow_mut().entry(page).or_default() += 1);
    use crate::styles::text_style::{Block, Shown, TextTarget, text_shown, text_style};
    use ecad_model::look::TextKind;
    let mut by_rep: BTreeMap<RepresentationId, Vec<(TextOf, String, Pt)>> = BTreeMap::new();
    for (t, rid, at) in connection_point_positions(doc, page) {
        // A pin pair shows its pin name once, beside the socket point.
        if doc.representations().get(&rid).is_some_and(|r| crate::connectors::hides_label(doc, r, t)) {
            continue;
        }
        // A connector pin's label on the page is its pin name (`3`), beside its half's
        // tag; elsewhere it is named `-X10.S:3`.
        if crate::connectors::side_of(doc, t).is_some() {
            if let Some(label) = doc.connection_points().get(&t).and_then(|x| x.label.clone()) {
                by_rep.entry(rid).or_default().push((TextOf::ConnectionPoint(t), label, at));
            }
            continue;
        }
        // A strip level shows its shown label (`1.L`), not its position.
        if let Some(label) = crate::terminal_strips::terminal_blocks::shown_label(doc, t) {
            by_rep.entry(rid).or_default().push((TextOf::ConnectionPoint(t), label, at));
        }
    }
    let mut out = Vec::new();
    let sizes = TagSizes::of(doc);
    let pitch = ecad_model::registry::resolve(doc, &crate::defaults::text::LINE_PITCH, ecad_model::registry::Scope::project()).number();
    let firsts = crate::styles::text_style::run_firsts(doc, page);
    // Location prefixes read this page's boxes once.
    let mut boxes = crate::locations::BoxCache::default();
    // The racks' address blocks, worked out once per draw.
    let mut plc_blocks = std::collections::BTreeMap::new();
    // Tag blocks wait for placement, after every fixed text is known.
    struct Pending {
        block: Block,
        lines: Vec<(TextOf, String, f64, bool, Option<u8>)>,
    }
    let mut pending: Vec<Pending> = Vec::new();
    let mut fixed: Vec<crate::styles::text_style::TextBox> = Vec::new();
    // A device's own text style for a kind (its
    // `text.<kind>.style`), else `None` (the kind's project style).
    let own = |c: DeviceId, kind: TextKind| -> Option<crate::styles::text_style::OwnStyle> {
        let props = &doc.devices().get(&c)?.properties;
        crate::styles::text_style::own_style(doc, kind, ecad_model::registry::Scope::component(c), props)
    };
    let height_of = |c: DeviceId, kind: TextKind, project: f64| own(c, kind).map_or(project, |s| s.style.height);
    for (rid, r) in doc.representations_on(page) {
        if r.form != Form::Symbol {
            continue;
        }
        let Some((def, pl)) = rep_definition(doc, r) else { continue };
        if !pl.is_valid() {
            continue;
        }
        // A continuation (a drawing aid) shows its name and its other places
        // instead, in the project's styles for those kinds.
        if let Subject::Aid(a) = r.subject {
            let target = TextTarget { page: Some(page), component: None, representation: Some(rid), conductor: None, run: run_rule(doc, r) };
            for mut t in crate::wires::labels::continuation_texts(doc, a, rid, &pl).unwrap_or_default() {
                let kind = kind_of(t.of);
                t.height = text_style(doc, kind).height;
                let shown = match text_shown(doc, target, kind) {
                    Shown::On => true,
                    Shown::Off => false,
                    Shown::First => firsts.contains(&rid),
                };
                out.push(PageText { text: t, shown, moved: false, overlaps: false });
            }
            continue;
        }
        let Subject::Device(c) = r.subject else { continue };
        let labels = by_rep.remove(&rid).unwrap_or_default();
        let target = TextTarget { page: Some(page), component: Some(c), representation: Some(rid), conductor: None, run: run_rule(doc, r) };
        let shown = |kind: TextKind| match text_shown(doc, target, kind) {
            Shown::On => true,
            Shown::Off => false,
            Shown::First => firsts.contains(&rid),
        };
        // Connection point labels keep their fixed rule (or a stored place) and are obstacles.
        let labels_shown = shown(TextKind::PointLabel);
        let label_style = text_style(doc, TextKind::PointLabel);
        let props = doc.devices().get(&c).map(|x| &x.properties);
        for (of, value, at, height, halign, valign) in texts_for(def, &pl, None, &labels, sizes) {
            let height = height_of(c, kind_of(of), height);
            let (mut at, mut halign, mut valign, mut moved) = (at, halign, valign, false);
            if let TextOf::ConnectionPoint(t) = of
                && let Some((p, h, v)) = props
                    .and_then(|p| p.get(&point_label_key(doc, rid, t)))
                    .and_then(|s| crate::styles::text_style::parse_at(s))
                    .map(|(p, h, v)| (pl.apply(p), h, v))
                    .filter(|(p, _, _)| p.is_finite())
            {
                (at, halign, valign, moved) = (p, h, v, true);
            }
            if labels_shown {
                let w = m.width(doc, TextKind::PointLabel, ecad_model::look::TextStyle { height, ..label_style }, &value);
                fixed.push(crate::styles::text_style::text_box(w, height, at, halign, valign));
            }
            out.push(PageText {
                text: DerivedText { rep: rid, of, value, at, height, halign, valign, aci: None, own: own(c, kind_of(of)) },
                shown: labels_shown,
                moved,
                overlaps: false,
            });
        }
        // A framed PLC cell or body has its tag and caption in its
        // frame's name block, not beside it.
        let framed = crate::plc::draw::framed(doc, c, rid, r);
        // A card body's caption at its anchor, under the body.
        if !framed && let Some(text) = crate::plc::caption_text(doc, c, rid, r, &pl) {
            out.push(text);
        }
        // A channel cell's number, address and description.
        out.extend(crate::plc::channel_texts(doc, c, rid, r, &pl, plc, &mut plc_blocks));
        if framed {
            continue;
        }
        // A symbol with text slots places each device text at its
        // slot instead of in a tag block.
        if !def.texts.is_empty() {
            out.extend(slot_texts(doc, c, rid, r, def, &pl, sizes, &shown, &mut boxes, m, &mut fixed));
            continue;
        }
        // The tag block: the tag, then the description and part number.
        let Some((lo, hi)) = placed_box(def, &pl) else { continue };
        // A provisional tag is drawn in `ecad.provisional`.
        let prov = doc.devices().get(&c).is_some_and(ecad_model::Device::is_provisional);
        let mut lines: Vec<(TextOf, String, f64, bool, Option<u8>)> = Vec::new();
        // A connector symbol shows its half's tag and fields.
        let half = crate::connectors::rep_side(doc, r);
        let tag_text = match half {
            Some(side) => Some(crate::connectors::half_tag(doc, c, side)),
            None => crate::devices::tags::tag_shown_of(doc, c),
        };
        if let Some(t) = tag_text.map(|t| crate::locations::shown_tag(doc, c, r, &t, &mut boxes)) {
            lines.push((TextOf::Tag, t, height_of(c, TextKind::Tag, sizes.tag), shown(TextKind::Tag), prov.then_some(PROVISIONAL_ACI)));
        }
        for (of, key, kind) in
            [(TextOf::Description, "description", TextKind::Description), (TextOf::PartNumber, "part_number", TextKind::PartNumber)]
        {
            let value = match half {
                Some(side @ (crate::symbols::Side::Socket | crate::symbols::Side::Plug)) => crate::connectors::half_prop(doc, c, side, key),
                Some(crate::symbols::Side::Both) => None,
                None => doc.devices().get(&c).and_then(|d| d.get(key)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
            };
            if let Some(v) = value {
                lines.push((of, v, height_of(c, kind, text_style(doc, kind).height), shown(kind), None));
            }
        }
        if lines.is_empty() {
            continue;
        }
        // Shown lines first; the size is theirs (all of them when none is shown, for Show Hidden
        // Text).
        lines.sort_by_key(|l| !l.3);
        let any = lines.iter().any(|l| l.3);
        let sized: Vec<&(TextOf, String, f64, bool, Option<u8>)> = lines.iter().filter(|l| l.3 || !any).collect();
        let w = sized
            .iter()
            .map(|l| m.width(doc, kind_of(l.0), ecad_model::look::TextStyle { height: l.2, ..text_style(doc, kind_of(l.0)) }, &l.1))
            .fold(0.0, f64::max);
        // Line tops are `text.line_pitch` × each line's height apart.
        let h = sized.iter().map(|l| pitch * l.2).sum::<f64>() - sized.last().map_or(0.0, |l| (pitch - 1.0) * l.2);
        pending.push(Pending { block: Block { rep: rid, component: c, lo, hi, vertical: vertical(&pl), placement: pl, w, h }, lines });
    }
    let placed = crate::styles::text_style::place_blocks(
        doc,
        page,
        &pending.iter().filter(|p| p.lines.iter().any(|l| l.3)).map(|p| p.block.clone()).collect::<Vec<_>>(),
        &fixed,
    );
    for p in pending {
        let place = placed.get(&p.block.rep).copied().unwrap_or_else(|| {
            // Nothing shown: the first candidate, for Show Hidden Text only.
            let gap = sizes.gap;
            let (rect, halign) = crate::styles::text_style::candidates(p.block.lo, p.block.hi, p.block.vertical, p.block.w, p.block.h, gap)[0];
            crate::styles::text_style::BlockPlace { rect, halign, moved: false, overlaps: false }
        });
        let mut top = place.rect.1.y;
        for (of, value, height, shown, aci) in p.lines {
            let at = place.line_at(top);
            out.push(PageText {
                text: DerivedText {
                    rep: p.block.rep,
                    of,
                    value,
                    at,
                    height,
                    halign: place.halign,
                    valign: VAlign::Top,
                    aci,
                    own: own(p.block.component, kind_of(of)),
                },
                shown,
                moved: place.moved,
                overlaps: place.overlaps && shown,
            });
            top -= pitch * height;
        }
    }
    out
}

/// The texts a project symbol's slots place for representation `rid` of device `c`: the tag where its show rule says,
/// every other slot's text whenever the device has a value for it. Each is an obstacle for the page's tag blocks.
#[allow(clippy::too_many_arguments)]
fn slot_texts(
    doc: &Document, c: DeviceId, rid: RepresentationId, r: &Representation, def: &Definition, pl: &Placement, sizes: TagSizes,
    shown: &dyn Fn(ecad_model::look::TextKind) -> bool, boxes: &mut crate::locations::BoxCache, m: &dyn TextLook, fixed: &mut Vec<(Pt, Pt)>,
) -> Vec<PageText> {
    use crate::styles::text_style::text_style;
    use ecad_model::SlotText;
    use ecad_model::look::TextKind;
    let device = doc.devices().get(&c);
    let prop = |k: &str| device.and_then(|d| d.value(k)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let (h, v) = (|a: HAlign| a, |a: VAlign| a);
    let mut out = Vec::new();
    for (i, slot) in def.texts.iter().enumerate().take(usize::from(u8::MAX)) {
        let (of, kind, value, aci) = match &slot.text {
            SlotText::Tag => {
                let t = crate::devices::tags::tag_shown_of(doc, c).map(|t| crate::locations::shown_tag(doc, c, r, &t, boxes));
                let prov = doc.devices().get(&c).is_some_and(ecad_model::Device::is_provisional);
                (TextOf::Tag, TextKind::Tag, t, prov.then_some(PROVISIONAL_ACI))
            }
            SlotText::Description => (TextOf::Description, TextKind::Description, prop("description"), None),
            SlotText::PartNumber => (TextOf::PartNumber, TextKind::PartNumber, prop("part_number"), None),
            other @ (SlotText::Rating
            | SlotText::Maker
            | SlotText::ArticleNumber
            | SlotText::Function
            | SlotText::Location
            | SlotText::PointRating(_)) => (TextOf::Slot(u8::try_from(i).unwrap_or(u8::MAX)), TextKind::Description, prop(&other.word()), None),
        };
        let Some(value) = value else { continue };
        let style = text_style(doc, kind);
        // The device's own text style of this kind, font included.
        let own = doc
            .devices()
            .get(&c)
            .and_then(|x| crate::styles::text_style::own_style(doc, kind, ecad_model::registry::Scope::component(c), &x.properties));
        let height = match &own {
            Some(o) => o.style.height,
            None if of == TextOf::Tag => sizes.tag,
            None => style.height,
        };
        let on = if of == TextOf::Tag { shown(kind) } else { true };
        let at = pl.apply(slot.at);
        if !at.is_finite() {
            continue;
        }
        let (ha, va) = (h(slot.h), v(slot.v));
        if on {
            let w = m.width(doc, kind, ecad_model::look::TextStyle { height, ..style }, &value);
            fixed.push(crate::styles::text_style::text_box(w, height, at, ha, va));
        }
        out.push(PageText {
            text: DerivedText { rep: rid, of, value, at, height, halign: ha, valign: va, aci, own },
            shown: on,
            moved: false,
            overlaps: false,
        });
    }
    out
}

/// The key a connection point label's stored place is kept under:
/// `text.<rep>.point_label.<terminal key>.at`, the terminal key being its definition key, else its
/// label.
pub(crate) fn point_label_key(doc: &Document, rep: RepresentationId, t: ConnectionPointId) -> String {
    let term = doc.connection_points().get(&t);
    let key = term.and_then(|x| x.key.as_ref()).map(ToString::to_string).or_else(|| term.and_then(|x| x.label.clone())).unwrap_or_else(|| t.hex());
    format!("{}point_label.{key}.at", crate::styles::text_style::symbol_prefix(rep))
}
