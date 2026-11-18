//! The kept Devices list (see [`super`]): refreshed from a change count and the devices each change
//! touched. ecad-cadcraft's `crate::device_list::DeviceList` wraps it, reading them from its
//! project.

use ecad_model::Document;

use super::*;

/// The Devices tab's list kept up to date (owner, 2026-11-08): every device, built whole once (on
/// open, reload or first show), then refreshed only for the devices each change touched (`crate::derived::DevicesFeed`, through
/// the project's change log), and renumbered when pages moved. A change that touches no
/// device does nothing.
#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    pub entries: Vec<DeviceEntry>,
    /// (project serial, change count) the list is up to date with.
    seen: Option<(u64, u64)>,
    /// Full builds so far (on open, on reload, when shown again).
    pub rebuilds: u64,
    /// Refreshes that changed the list, and the devices they refreshed.
    pub updates: u64,
    pub devices_refreshed: u64,
}

impl DeviceList {
    /// Bring the list up to date with `doc`, the project numbered `serial` by the caller after
    /// `count` changes; `changes_since(n)` is what the changes after the `n`th touched (`None` when
    /// they are no longer known). Returns whether it changed. Filtering is the caller's
    /// (`matches`): it never rebuilds the list.
    pub fn sync_with(&mut self, serial: u64, count: u64, doc: &Document, changes_since: impl FnOnce(u64) -> Option<DeviceTouch>) -> bool {
        let since = match self.seen {
            Some((s, n)) if s == serial => Some(n),
            _ => None,
        };
        let changed = match since.and_then(changes_since) {
            Some(t) if t.is_empty() => false,
            Some(t) => self.apply(doc, &t),
            None => {
                self.entries = device_entries(doc, "");
                self.rebuilds += 1;
                true
            }
        };
        self.seen = Some((serial, count));
        changed
    }

    /// Forget how up to date the list is: the next [`DeviceList::sync_with`] builds it whole.
    pub fn invalidate(&mut self) {
        self.seen = None;
    }

    /// Refresh the devices `t` names, and the page numbers when pages moved.
    fn apply(&mut self, doc: &Document, t: &DeviceTouch) -> bool {
        let mut changed = false;
        let mut resort = false;
        for c in &t.devices {
            let now = device_entry(doc, *c);
            let at = self.entries.iter().position(|e| e.id == *c);
            match (at, now) {
                (Some(i), Some(e)) => {
                    if self.entries.get(i) != Some(&e) {
                        resort |= self.entries.get(i).map(sort_key) != Some(sort_key(&e));
                        if let Some(slot) = self.entries.get_mut(i) {
                            *slot = e;
                        }
                        changed = true;
                        self.devices_refreshed += 1;
                    }
                }
                (Some(i), None) => {
                    self.entries.remove(i);
                    changed = true;
                    self.devices_refreshed += 1;
                }
                (None, Some(e)) => {
                    self.entries.push(e);
                    resort = true;
                    changed = true;
                    self.devices_refreshed += 1;
                }
                (None, None) => {}
            }
        }
        if t.pages {
            renumber(doc, &mut self.entries);
            changed = true;
        }
        if resort {
            self.entries.sort_by_key(sort_key);
        }
        if changed {
            self.updates += 1;
        }
        changed
    }
}
