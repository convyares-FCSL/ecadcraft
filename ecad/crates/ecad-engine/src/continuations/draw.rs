//! The engine half of [`crate::continuations`]: the reference parts laid out with CADCraft's fonts,
//! and an arrow's other-end text coloured in the sheet's ink.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{Document, PageId};

use crate::styles::draw::EngineMetrics;
use {ecad_electrical::references::devices::Part, ecad_electrical::references::devices::Places};

/// The parts drawn on `page`: each arrow's reference text and each caption run, laid out with
/// CADCraft's fonts ([`ecad_electrical::continuations::parts_on_page_measured`]).
pub fn parts_on_page(doc: &Document, places: &mut Places<'_>, page: PageId) -> Vec<Part> {
    ecad_electrical::continuations::parts_on_page_measured(doc, places, page, &EngineMetrics)
}

/// What the arrows named `names` show, by page, laid out with CADCraft's fonts
/// ([`ecad_electrical::continuations::shown_measured`]).
pub fn shown(doc: &Document, places: &mut Places<'_>, names: &BTreeSet<String>) -> BTreeMap<PageId, Vec<Part>> {
    ecad_electrical::continuations::shown_measured(doc, places, names, &EngineMetrics)
}

/// Colour a drawn continuation part (review of #85, 3; `ui-design-spec.md`, canvas colours): an
/// arrow's other-end text in Ink 2 of the sheet the page is projected for, as zone labels, pin
/// labels and references are; the bracket and caption stay ByLayer (ink). Other parts unchanged.
pub fn colour(doc: &Document, part: &Part, e: &mut cadcraft_engine::doc::Entity) {
    if part.index == ecad_electrical::continuations::REF_INDEX && ecad_electrical::continuations::arrow(doc, part.rep).is_some() {
        let ink2 = crate::cables::draw::projection_sheet().color(ecad_drawing_style::Meaning::Ink2);
        e.common.color = cadcraft_engine::doc::color::Color::True(crate::cables::draw::rgb(ink2));
    }
}

#[cfg(test)]
mod tests {
    use ecad_model::{Pt, RepresentationId};

    use {ecad_electrical::references::devices::HAlign, ecad_electrical::references::devices::Part, ecad_electrical::references::devices::Shape, ecad_electrical::references::devices::VAlign};

    /// FP3 (overseer ruling, 2026-11-13): an unrotated reference part draws byte-identically to
    /// the entity drawn before `rotation` existed.
    #[test]
    fn an_unrotated_part_draws_as_before() {
        let part = Part {
            rep: RepresentationId(7),
            index: 0,
            shape: Shape::Text {
                kind: ecad_electrical::styles::text_style::TextKind::Reference,
                at: Pt::new(1.0, 2.0),
                value: "100-B-3".into(),
                height: 2.5,
                halign: HAlign::Left,
                valign: VAlign::Top,
                rotation: 0.0},
            aci: None,
            row: None,
            bar: false};
        let h = cadcraft_engine::doc::Handle(0x42);
        let look = crate::styles::draw::Look::new(&ecad_model::Document::default());
        let mut before = crate::devices::draw::text_entity_with(
            "100-B-3",
            Pt::new(1.0, 2.0),
            2.5,
            cadcraft_engine::doc::HAlign::Left,
            cadcraft_engine::doc::VAlign::Top,
            h,
            ecad_electrical::devices::TAG_LAYER,
        );
        // Brief 066d: and in the Reference text style, as every reference text.
        crate::styles::draw::DrawApply::apply(&look.0, &mut before, ecad_electrical::styles::text_style::TextKind::Reference);
        assert_eq!(format!("{:?}", crate::project::device_reference_entity(&look, &part, h)), format!("{before:?}"));
    }
}
