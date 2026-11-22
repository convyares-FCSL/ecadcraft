//! The engine side of the Devices tab (design 065 §3): the kept list read from a live project, and
//! "Go to" a symbol selects its drawn handles.

use cadcraft_engine::doc::Handle;
use ecad_model::{PageId, Pt, RepresentationId};

use ecad_electrical::devices::device_list::kept;
use crate::project::Project;

/// For a "go to" from the list: representation `rep`'s page, a point to centre on, and its drawn
/// handles on that page (empty unless that page is the one shown).
pub fn symbol_target(p: &Project, rep: RepresentationId) -> Option<(PageId, Option<Pt>, Vec<Handle>)> {
    let doc = p.ledger.document();
    let r = doc.representations.get(&rep)?;
    let mut handles = Vec::new();
    if let Some(view) = p.pages.get(&r.page).filter(|v| v.uid.is_some()) {
        handles.extend(r.items.iter().filter_map(|i| view.map.item(*i)));
        handles.extend(r.connection_points.keys().filter_map(|t| view.map.marker(rep, *t)));
    }
    Some((r.page, ecad_electrical::devices::device_list::position(doc, r), handles))
}

/// The Devices tab's list kept up to date from a project ([`kept::DeviceList`]): built whole once
/// (on open, reload or first show), then refreshed only for the devices each change touched
/// (through [`Project::device_changes_since`]).
#[derive(Debug, Clone, Default)]
pub struct DeviceList(pub kept::DeviceList);

impl std::ops::Deref for DeviceList {
    type Target = kept::DeviceList;

    fn deref(&self) -> &kept::DeviceList {
        &self.0
    }
}

impl std::ops::DerefMut for DeviceList {
    fn deref_mut(&mut self) -> &mut kept::DeviceList {
        &mut self.0
    }
}

impl DeviceList {
    /// Bring the list up to date with `p` (the project numbered `serial` by the caller). Returns
    /// whether it changed. Filtering is the caller's ([`ecad_electrical::devices::device_list::matches`]): it never
    /// rebuilds the list.
    pub fn sync(&mut self, serial: u64, p: &Project) -> bool {
        self.0.sync_with(serial, p.change_count(), p.ledger.document(), |n| p.device_changes_since(n))
    }
}
