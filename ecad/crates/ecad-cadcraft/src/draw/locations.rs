//! The engine half of [`crate::locations`]: box labels as text entities, box outlines in their
//! location style, and the box line types in a drawing.

use ecad_model::{Document, ItemId, PageId, Pt, RepresentationId, Subject};

use crate::draw::text_style::LookApply as _;
use crate::locations::{LAYER, LINETYPE, boxes_on};

/// A box label as an entity on [`LAYER`], top-left aligned, in the location label style of `look`
/// (design 031 §3.2).
pub fn label_entity(look: &crate::text_style::Look, value: &str, at: Pt, handle: cadcraft_engine::doc::Handle) -> cadcraft_engine::doc::Entity {
    use crate::text_style::TextKind;
    use cadcraft_engine::doc::{HAlign, VAlign};
    let h = look.style(TextKind::LocationLabel).height;
    let mut e = crate::devices::text_entity_with(value, at, h, HAlign::Left, VAlign::Top, handle, LAYER);
    look.apply(&mut e, TextKind::LocationLabel);
    e
}

/// Brief 050a (design 050 §4.2): a box label in its box's label style when that is not the
/// location text kind's own style: the style's height, weight and colour.
pub fn styled_label_entity(
    doc: &Document, look: &crate::text_style::Look, rep: RepresentationId, value: &str, at: Pt, handle: cadcraft_engine::doc::Handle,
) -> cadcraft_engine::doc::Entity {
    use crate::text_style::TextKind;
    let mut e = label_entity(look, value, at, handle);
    let Some(c) = doc.representations.get(&rep).and_then(|r| match r.subject {
        Subject::Component(c) => Some(c),
        _ => None,
    }) else {
        return e;
    };
    let label = crate::styles::location::box_look(doc, c).label_style;
    if label == crate::text_style::style_of(doc, TextKind::LocationLabel, crate::defaults::Scope::project()) {
        return e;
    }
    let f = |k: &str| crate::styles::field_of(doc, "text", &label, k, None);
    if let (Some(h), cadcraft_engine::doc::EntityKind::Text(t)) = (f("height"), &mut e.kind) {
        t.height = h.length(doc);
        t.style = match f("weight").map(|r| r.word()) {
            Some("bold") => crate::text_style::STYLE_BOLD.into(),
            _ => crate::text_style::STYLE_REGULAR.into(),
        };
    }
    if let Some(rgb) = f("colour").and_then(|r| crate::look::colour_rgb(&r.text(), crate::look::projection_sheet())) {
        e.common.color = cadcraft_engine::doc::color::Color::True(rgb);
    }
    e
}

/// Brief 050a (design 050 §4.2): draw the outlines of `page`'s boxes in their location style
/// (line type, weight and colour) instead of the stored item style. `item_handle` finds a box
/// item's entity among `ents`.
pub fn style_boxes(
    doc: &Document, page: PageId, ents: &mut [cadcraft_engine::doc::Entity], item_handle: impl Fn(ItemId) -> Option<cadcraft_engine::doc::Handle>,
) {
    let boxes = boxes_on(doc, page);
    if boxes.is_empty() {
        return;
    }
    let index: std::collections::HashMap<cadcraft_engine::doc::Handle, usize> = ents.iter().enumerate().map(|(i, e)| (e.handle, i)).collect();
    for b in boxes {
        let look = crate::styles::location::box_look(doc, b.component);
        let Some(e) = item_handle(b.item).and_then(|h| index.get(&h)).and_then(|i| ents.get_mut(*i)) else { continue };
        e.common.linetype = look.linetype.into();
        e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(u16::try_from(look.lineweight).unwrap_or(25));
        if let Some(rgb) = crate::look::colour_rgb(&look.colour, crate::look::projection_sheet()) {
            e.common.color = cadcraft_engine::doc::color::Color::True(rgb);
        }
    }
}

/// Load [`LINETYPE`] into `d` from CADCraft's standard linetypes when it is missing, so box
/// outlines draw and plot dashed.
pub fn add_linetype(d: &mut cadcraft_engine::doc::Drawing) {
    // Brief 050a: a location style may draw continuous or dash-dot as well.
    for name in [LINETYPE, "DASHDOT"] {
        if d.linetype(name).is_some() {
            continue;
        }
        if let Some(lt) = cadcraft_engine::doc::library::standard_linetypes_for(d).into_iter().find(|l| l.name == name) {
            d.linetypes.push(lt);
        }
    }
}
