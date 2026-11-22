//! The engine side of a PLC card's frame (brief 051, design 065 §3): its parts with their texts
//! measured in CADCraft's fonts. The runs and the frame rules are [`ecad_electrical::plc::draw`].

use ecad_model::{Document, PageId};

use crate::styles::draw::EngineMetrics;
use ecad_electrical::references::devices::Part;

/// Every frame part on `page` (design 051 §2.6): [`ecad_electrical::plc::draw::parts_on_page_measured`].
pub fn parts_on_page(doc: &Document, page: PageId) -> Vec<Part> {
    ecad_electrical::plc::draw::parts_on_page_measured(doc, page, &EngineMetrics)
}
