//! The engine side of a PLC card's frame (brief 051, design 065 §3): its parts with their texts
//! measured in CADCraft's fonts. The runs and the frame rules are [`crate::plc::look`].

use ecad_model::{Document, PageId};

use crate::draw::text_style::EngineMetrics;
use crate::xref::Part;

/// Every frame part on `page` (design 051 §2.6): [`crate::plc::look::parts_on_page_measured`].
pub fn parts_on_page(doc: &Document, page: PageId) -> Vec<Part> {
    crate::plc::look::parts_on_page_measured(doc, page, &EngineMetrics)
}
