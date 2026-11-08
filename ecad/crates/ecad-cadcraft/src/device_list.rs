//! The Devices tab's list (brief 016's Project panel, owner request of 2026-11-08): every device
//! grouped by its tag prefix, each with its symbols, page and column, and what a "go to" selects.
//! Read only: edits go through `setprop` (brief 018's rules), never through here.
//!
//! Built from the tag index ([`ecad_model::Document::tagged`]) and each device's representations
//! (the subject index), only when the project or the filter changed; nothing here runs per frame.
//!
//! Excluded: cables, terminals and wires; editing.

use std::collections::{BTreeSet, HashMap};

use cadcraft_engine::doc::Handle;
use ecad_model::{ComponentId, Document, ItemKind, ObjectRef, Op, PageId, Pt, RepresentationId, Subject};

use crate::devices::{self, PROPERTIES};
use crate::project::Project;

/// One symbol (representation) of a device, as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolEntry {
    pub rep: RepresentationId,
    pub page: PageId,
    /// The page's number as shown ([`crate::references::page_number`]).
    pub page_number: String,
    /// Its column (`ecad.column-width` grid), when it can be located.
    pub column: Option<u32>,
    /// `NO contact`, `Contactor / relay coil`, or `drawn` for a converted outline.
    pub name: String,
}

/// One device in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceEntry {
    pub id: ComponentId,
    /// Its tag; `None` for a component without one (converted outlines, older coils).
    pub tag: Option<String>,
    /// The group it is listed under: `-K`, `-Q`… (its tag's prefix), or [`NO_TAG`].
    pub group: String,
    pub description: String,
    /// The six device properties ([`PROPERTIES`]) as stored, in that order (`None`: not set).
    pub properties: Vec<Option<String>>,
    /// Its symbols, in page order, then column.
    pub symbols: Vec<SymbolEntry>,
}

/// What a change could change in the Devices list (owner, 2026-11-08: act only on the ops that
/// matter): the devices whose row or symbol rows may differ, and whether page numbers moved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceTouch {
    pub components: BTreeSet<ComponentId>,
    /// Pages were added, removed, moved or renamed: the `/page.column` texts may differ.
    pub pages: bool,
}

impl DeviceTouch {
    /// Nothing in the list can differ.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty() && !self.pages
    }
}

/// What `ops`, about to be applied to `doc`, could change in the Devices list: a component
/// created, deleted or given a property; a representation of a component linked, unlinked or
/// changed; an item of a component's representation changed (a symbol moved); pages added,
/// removed or moved. Every other op (wires, cables, plain lines, terminals) changes nothing there.
pub fn devices_touched(doc: &Document, ops: &[Op]) -> DeviceTouch {
    let mut t = DeviceTouch::default();
    let mut component = |s: &Subject| {
        if let Subject::Component(c) = s {
            t.components.insert(*c);
        }
    };
    let mut pages = false;
    for op in ops {
        match op {
            Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } => component(&Subject::Component(*id)),
            Op::SetProperty { object: ObjectRef::Component(c), .. } => component(&Subject::Component(*c)),
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => component(&rep.subject),
            Op::ModifyRepresentation { before, after, .. } => {
                component(&before.subject);
                component(&after.subject);
            }
            Op::ModifyItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    component(&r.subject);
                }
            }
            Op::AddPage { .. } | Op::RemovePage { .. } => pages = true,
            _ => {}
        }
    }
    t.pages = pages;
    t
}

/// The list entry of component `c` as it is now (`None`: gone, or a component without a tag that
/// is drawn nowhere).
pub fn device_entry(doc: &Document, c: ComponentId) -> Option<DeviceEntry> {
    let comp = doc.components.get(&c)?;
    let prefix = match comp.properties.get(ecad_model::tag::TAG_KEY) {
        Some(t) => Some(ecad_model::tag::parse(t).map_or_else(|| ecad_model::tag::key(t), |(p, _)| p)),
        None if doc.representations_of(Subject::Component(c)).next().is_none() => return None,
        None => None,
    };
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    Some(entry(doc, c, prefix, ecad_model::derive::column_width(doc), &order))
}

/// The order of the list: tagged devices by prefix, number, tag; then those without a tag.
pub fn sort_key(e: &DeviceEntry) -> (bool, String, u64, String, ComponentId) {
    match e.tag.as_deref() {
        Some(t) => {
            let (p, n) = ecad_model::tag::parse(t).unwrap_or_else(|| (ecad_model::tag::key(t), 0));
            (false, p, n, t.to_string(), e.id)
        }
        None => (true, String::new(), 0, String::new(), e.id),
    }
}

/// Set the page numbers of every listed symbol again (pages were added, removed or moved): no
/// other part of the list depends on the page order.
pub fn renumber(doc: &Document, entries: &mut [DeviceEntry]) {
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i + 1)).collect();
    for e in entries {
        for s in &mut e.symbols {
            s.page_number = order.get(&s.page).map_or_else(|| "?".into(), usize::to_string);
        }
        e.symbols.sort_by_key(|s| (order.get(&s.page).copied().unwrap_or(usize::MAX), s.column, s.rep));
    }
}

/// The group a component without a tag is listed under.
pub const NO_TAG: &str = "No tag";

/// Whether `e` matches the filter `q` (case-insensitive): its tag as Find Symbol matches tags
/// (`k1` finds `-K01`, `-K10`…), or its description, maker or part number containing `q`.
pub fn matches(e: &DeviceEntry, q: &str) -> bool {
    let q = q.trim();
    if q.is_empty() {
        return true;
    }
    if e.tag.as_deref().is_some_and(|t| devices::tag_matches(t, q)) {
        return true;
    }
    let low = q.to_lowercase();
    let has = |i: usize| e.properties.get(i).and_then(Option::as_ref).is_some_and(|v| v.to_lowercase().contains(&low));
    e.description.to_lowercase().contains(&low) || has(2) || has(3)
}

/// Every device of `doc` matching `filter`: tagged ones by prefix, then number (from the tag
/// index), then the components without a tag that are drawn somewhere.
pub fn device_entries(doc: &Document, filter: &str) -> Vec<DeviceEntry> {
    let width = ecad_model::derive::column_width(doc);
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let mut tagged: Vec<((String, u64, String), ComponentId)> = doc
        .tagged()
        .filter_map(|(_, c)| {
            let t = devices::tag_of(doc, c)?;
            let (p, n) = ecad_model::tag::parse(t).unwrap_or_else(|| (ecad_model::tag::key(t), 0));
            Some(((p, n, t.to_string()), c))
        })
        .collect();
    tagged.sort();
    let untagged = doc
        .components
        .iter()
        .filter(|(_, c)| !c.properties.contains_key(ecad_model::tag::TAG_KEY))
        .map(|(id, _)| *id)
        .filter(|id| doc.representations_of(Subject::Component(*id)).next().is_some());
    tagged
        .into_iter()
        .map(|((p, _, _), c)| (Some(p), c))
        .chain(untagged.map(|c| (None, c)))
        .map(|(prefix, c)| entry(doc, c, prefix, width, &order))
        .filter(|e| matches(e, filter))
        .collect()
}

fn entry(
    doc: &Document,
    c: ComponentId,
    prefix: Option<String>,
    width: f64,
    order: &HashMap<PageId, usize>,
) -> DeviceEntry {
    let props = doc.components.get(&c).map(|x| &x.properties);
    let mut symbols: Vec<(usize, Option<u32>, SymbolEntry)> = doc
        .representations_of(Subject::Component(c))
        .map(|(rid, r)| {
            let column = position(doc, r).and_then(|p| ecad_model::derive::column_of(p.x, width));
            let name = devices::rep_starter(doc, r).map(|s| s.name.to_string()).unwrap_or_else(|| {
                r.items
                    .first()
                    .and_then(|i| doc.items.get(i))
                    .and_then(|i| match &i.kind {
                        ItemKind::Symbol(s) => doc.definitions.get(&s.definition).map(|d| d.name.clone()),
                        ItemKind::Primitive(_) => None,
                    })
                    .unwrap_or_else(|| "drawn".into())
            });
            let n = order.get(&r.page).copied().unwrap_or(usize::MAX);
            let page_number =
                n.checked_add(1).filter(|_| n != usize::MAX).map_or_else(|| "?".into(), |n| n.to_string());
            (n, column, SymbolEntry { rep: rid, page: r.page, page_number, column, name })
        })
        .collect();
    symbols.sort_by_key(|(n, col, s)| (*n, *col, s.rep));
    DeviceEntry {
        id: c,
        tag: devices::tag_of(doc, c).map(str::to_string),
        group: prefix.map_or_else(|| NO_TAG.to_string(), |p| format!("-{p}")),
        description: devices::description(doc, c),
        properties: PROPERTIES.iter().map(|(k, _)| props.and_then(|p| p.get(*k).cloned())).collect(),
        symbols: symbols.into_iter().map(|(_, _, s)| s).collect(),
    }
}

/// Where a representation is: a symbol's insertion point, else its first connection point.
fn position(doc: &Document, r: &ecad_model::Representation) -> Option<Pt> {
    if let Some(ItemKind::Symbol(s)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) {
        return Some(s.placement.origin);
    }
    r.connection_points.values().next().copied()
}

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
    Some((r.page, position(doc, r), handles))
}

/// The Devices tab's list kept up to date (owner, 2026-11-08): built whole once, then refreshed
/// only for the devices each change touched ([`devices_touched`], through
/// [`Project::device_changes_since`]), and renumbered when pages moved. A change that touches no
/// device does nothing.
#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    pub entries: Vec<DeviceEntry>,
    /// (project serial, change count, filter) the list is up to date with.
    seen: Option<(u64, u64, String)>,
    /// Full builds so far (on open, on reload, when shown again, on a filter change).
    pub rebuilds: u64,
    /// Refreshes that changed the list, and the devices they refreshed.
    pub updates: u64,
    pub devices_refreshed: u64,
}

impl DeviceList {
    /// Bring the list up to date with `p` (the project numbered `serial` by the caller) under
    /// `filter`. Returns whether it changed.
    pub fn sync(&mut self, serial: u64, p: &Project, filter: &str) -> bool {
        let count = p.change_count();
        let doc = p.ledger.document();
        let since = match &self.seen {
            Some((s, n, f)) if *s == serial && f == filter => Some(*n),
            _ => None,
        };
        let changed = match since.and_then(|n| p.device_changes_since(n)) {
            Some(t) if t.is_empty() => false,
            Some(t) => self.apply(doc, &t, filter),
            None => {
                self.entries = device_entries(doc, filter);
                self.rebuilds += 1;
                true
            }
        };
        self.seen = Some((serial, count, filter.to_string()));
        changed
    }

    /// Forget how up to date the list is: the next [`DeviceList::sync`] builds it whole.
    pub fn invalidate(&mut self) {
        self.seen = None;
    }

    /// Refresh the devices `t` names, and the page numbers when pages moved.
    fn apply(&mut self, doc: &Document, t: &DeviceTouch, filter: &str) -> bool {
        let mut changed = false;
        let mut resort = false;
        for c in &t.components {
            let now = device_entry(doc, *c).filter(|e| matches(e, filter));
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
