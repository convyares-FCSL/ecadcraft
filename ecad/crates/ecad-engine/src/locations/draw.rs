//! The engine half of [`crate::locations`]: box labels as text entities, box outlines in their
//! location style, and the box line types in a drawing.

use ecad_model::{Document, ItemId, PageId, Pt, RepresentationId, Subject};

use crate::styles::draw::DrawApply as _;
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
        Subject::Aid(a) => Some(a),
        _ => None,
    }) else {
        return e;
    };
    let label = ecad_electrical::styles::location::box_draw(doc, c).label_style;
    if label == crate::text_style::style_of(doc, TextKind::LocationLabel, ecad_electrical::defaults::Scope::project()) {
        return e;
    }
    // Brief 066d: the label style as a whole (height, weight, font, colour), never a literal
    // style name; a font of its own is installed by the page (`install_family`).
    let own = crate::text_style::own_style_of(doc, &label);
    if let cadcraft_engine::doc::EntityKind::Text(t) = &mut e.kind {
        t.height = own.style.height;
    }
    look.apply_own(&mut e, TextKind::LocationLabel, &own);
    e
}

/// The font family a box's label style draws in when it is not the project font (brief 066d), for
/// the page to install (`install_family`).
pub fn label_family(doc: &Document, rep: RepresentationId) -> Option<String> {
    let c = doc.representations.get(&rep).and_then(|r| match r.subject {
        Subject::Aid(a) => Some(a),
        _ => None,
    })?;
    let label = ecad_electrical::styles::location::box_draw(doc, c).label_style;
    crate::text_style::own_style_of(doc, &label).family
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
        let look = ecad_electrical::styles::location::box_draw(doc, b.aid);
        let Some(e) = item_handle(b.item).and_then(|h| index.get(&h)).and_then(|i| ents.get_mut(*i)) else { continue };
        e.common.linetype = look.linetype.into();
        e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(u16::try_from(look.lineweight).unwrap_or(25));
        if let Some(rgb) = crate::cables::draw::colour_rgb(&look.colour, crate::cables::draw::projection_sheet()) {
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
