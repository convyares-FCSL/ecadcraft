//! Selection and relations (brief 040, step 5; spec principle 3, "one device, many places, shown
//! as one"): selecting a placement of a device marks its other placements on the page (a dashed
//! outline, [`super::markers`]) and lists the rest ([`Related::elsewhere`], which 042's inspector
//! shows with a go-to). The selection itself stays CADCraft's; nothing here selects.

use cadcraft_engine::doc::Handle;
use ecad_cadcraft::Project;
use ecad_cadcraft::project::Mapped;
use ecad_model::{ComponentId, PageId, RepresentationId, Subject};

/// Most selected objects looked at.
pub const MAX_LOOKED_AT: usize = 256;
/// Most placements listed.
pub const MAX_PLACEMENTS: usize = 1_000;

/// The placements related to a selection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Related {
    /// The devices the selection stands for, in ID order.
    pub devices: Vec<ComponentId>,
    /// Their other placements on the selection's page, each as its drawn handles.
    pub here: Vec<Vec<Handle>>,
    /// Their placements on other pages, in page order of the document and then ID order.
    pub elsewhere: Vec<(PageId, RepresentationId)>,
}

/// The devices `selection` (handles on `page`) stands for, and their placements other than the
/// selected ones: on this page (to outline) and on other pages (to list). Reads only.
pub fn related(p: &Project, page: PageId, selection: &[Handle]) -> Related {
    let mut out = Related::default();
    let doc = p.ledger.document();
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return out };
    let mut selected: Vec<RepresentationId> = Vec::new();
    for h in selection.iter().take(MAX_LOOKED_AT) {
        let rep = match map.get(*h) {
            Some(Mapped::Item(i)) => doc.representation_of_item(i).map(|(id, _)| id),
            Some(Mapped::Marker(r, _) | Mapped::Tag(r) | Mapped::Xref(r)) => Some(r),
            _ => None,
        };
        let Some((rid, r)) = rep.and_then(|id| doc.representations.get(&id).map(|r| (id, r))) else { continue };
        if let Subject::Component(c) = r.subject {
            selected.push(rid);
            if !out.devices.contains(&c) {
                out.devices.push(c);
            }
        }
    }
    out.devices.sort();
    for c in &out.devices {
        for (rid, r) in doc.representations_of(Subject::Component(*c)) {
            if selected.contains(&rid) || out.here.len() + out.elsewhere.len() >= MAX_PLACEMENTS {
                continue;
            }
            if r.page == page {
                let handles = ecad_cadcraft::device_list::symbol_target(p, rid).map(|t| t.2).unwrap_or_default();
                if !handles.is_empty() {
                    out.here.push(handles);
                }
            } else {
                out.elsewhere.push((r.page, rid));
            }
        }
    }
    let order: Vec<PageId> = doc.pages.keys().copied().collect();
    out.elsewhere.sort_by_key(|(pg, rid)| (order.iter().position(|o| o == pg).unwrap_or(usize::MAX), *rid));
    out
}
