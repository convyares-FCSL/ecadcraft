//! The engine half of [`crate::xref`]: reference texts aligned as CADCraft aligns text, the
//! tables laid out with CADCraft's fonts, and the refusal of an edit of a drawn mapping.

use ecad_model::{DeviceId, Document, PageId};
use serde_json::Value;

use crate::styles::draw::EngineMetrics;
use {
    ecad_electrical::references::devices::Device, ecad_electrical::references::devices::HAlign, ecad_electrical::references::devices::Part,
    ecad_electrical::references::devices::Places, ecad_electrical::references::devices::Table, ecad_electrical::references::devices::VAlign,
};

/// CADCraft's horizontal alignment of a reference text.
pub fn engine_halign(h: HAlign) -> cadcraft_engine::doc::HAlign {
    match h {
        HAlign::Left => cadcraft_engine::doc::HAlign::Left,
        HAlign::Center => cadcraft_engine::doc::HAlign::Center,
        HAlign::Right => cadcraft_engine::doc::HAlign::Right,
    }
}

/// CADCraft's vertical alignment of a reference text.
pub fn engine_valign(v: VAlign) -> cadcraft_engine::doc::VAlign {
    match v {
        VAlign::Baseline => cadcraft_engine::doc::VAlign::Baseline,
        VAlign::Bottom => cadcraft_engine::doc::VAlign::Bottom,
        VAlign::Middle => cadcraft_engine::doc::VAlign::Middle,
        VAlign::Top => cadcraft_engine::doc::VAlign::Top,
    }
}

/// The table under device `dev`'s head (design 025 §4.2), laid out with CADCraft's fonts
/// ([`ecad_electrical::references::devices::table_measured`]).
pub fn table(doc: &Document, places: &mut Places<'_>, dev: &Device) -> Option<Table> {
    ecad_electrical::references::devices::table_measured(doc, places, dev, &EngineMetrics)
}

/// Every reference part drawn on `page`: the texts under its contacts and the tables under its
/// heads, laid out with CADCraft's fonts ([`ecad_electrical::references::devices::parts_on_page_measured`]).
pub fn parts_on_page(doc: &Document, page: PageId) -> Vec<Part> {
    ecad_electrical::references::devices::parts_on_page_measured(doc, page, &EngineMetrics)
}

/// `contacts {component}`'s result (design 025 §5.4), its table laid out with CADCraft's fonts
/// ([`ecad_electrical::references::devices::contacts_json_measured`]).
///
/// # Errors
/// Unknown component.
pub fn contacts_json(doc: &Document, c: DeviceId) -> Result<Value, String> {
    ecad_electrical::references::devices::contacts_json_measured(doc, c, &EngineMetrics).map_err(String::from)
}

/// The refusal of an edit of a drawn cross-reference: the contact text's or the table's.
pub fn refusal(doc: &Document, mapped: Option<crate::project::Mapped>) -> &'static str {
    match mapped {
        // Brief 051 (review of #89, 2): a PLC card's frame.
        Some(crate::project::Mapped::DeviceReference(r)) if ecad_electrical::plc::draw::is_frame_rep(doc, r) => {
            ecad_electrical::plc::draw::FRAME_DERIVED
        }
        Some(crate::project::Mapped::DeviceReference(r))
            if ecad_electrical::references::devices::class_in(doc, r)
                .is_some_and(|(_, c)| c == ecad_electrical::references::devices::Class::Head) =>
        {
            ecad_electrical::references::devices::TABLE_DERIVED
        }
        Some(crate::project::Mapped::DeviceReference(r))
            if ecad_electrical::references::devices::class_in(doc, r)
                .is_some_and(|(_, c)| c == ecad_electrical::references::devices::Class::Arrow) =>
        {
            ecad_electrical::continuations::TEXT_DERIVED
        }
        _ => ecad_electrical::references::devices::TEXT_DERIVED,
    }
}
