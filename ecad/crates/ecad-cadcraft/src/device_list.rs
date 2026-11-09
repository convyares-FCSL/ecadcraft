//! The Devices tab's list (brief 016's Project panel, owner request of 2026-11-08): every device
//! grouped by its tag prefix, each with its symbols, page and column, and what a "go to" selects.
//! Read only: edits go through `setprop` (brief 018's rules), never through here.
//!
//! Built from the tag index ([`ecad_model::Document::tagged`]) and each device's representations
//! (the subject index), only when the project or the filter changed; nothing here runs per frame.
//!
//! Excluded: cables, terminals and wires; editing.

use std::collections::BTreeSet;

use ecad_model::book::Book;

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
    /// Its frame column and row (brief 019), when it can be located.
    pub column: Option<u32>,
    pub row: Option<u32>,
    /// Where it is, in the project's reference style (`105-C-4`), or `?`.
    pub place: String,
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
    /// The project settings changed (tag prefixes, padding): the Devices list shows none of them,
    /// but the symbol search's prefixes and next tags may differ.
    pub settings: bool,
}

impl DeviceTouch {
    /// Nothing in the list can differ.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty() && !self.pages
    }

    /// Nothing the symbol search shows can differ (its rows and its second step): no device, no
    /// page and no setting was touched.
    pub fn leaves_search(&self) -> bool {
        self.is_empty() && !self.settings
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
    let mut settings = false;
    // Pages whose book position changes (worked out once, only for ops that can move pages): only
    // their symbols' places and order may differ; a rename in place or an append moves none.
    let mut extra: BTreeSet<ComponentId> = BTreeSet::new();
    let mut moved = false;
    let mut moved_pages = |t: &mut BTreeSet<ComponentId>| {
        if std::mem::replace(&mut moved, true) {
            return;
        }
        for p in ecad_model::book::positions_changed(doc, ops) {
            for (_, r) in doc.representations_on(p) {
                if let Subject::Component(c) = r.subject {
                    t.insert(c);
                }
            }
        }
    };
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
            Op::AddPage { .. } | Op::RemovePage { .. } => moved_pages(&mut extra),
            // Brief 019: a page's number or sheet changes the place of the symbols on it; the book
            // order (folders) or the project's grid may change every place.
            Op::SetProperty { object: ObjectRef::Page(pg), key, .. }
                if key == ecad_model::book::NUMBER_KEY || key == ecad_model::sheet::SHEET_KEY =>
            {
                for (_, r) in doc.representations_on(*pg) {
                    component(&r.subject);
                }
            }
            Op::SetProperty { object: ObjectRef::Page(_), key, .. } if key == ecad_model::book::FOLDER_KEY => {
                moved_pages(&mut extra)
            }
            Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => moved_pages(&mut extra),
            Op::SetProjectSettings { before, after } => {
                settings = true;
                if before.sheet != after.sheet
                    || before.length_unit != after.length_unit
                    || before.reference_style != after.reference_style
                {
                    pages = true;
                }
            }
            _ => {}
        }
    }
    t.components.extend(extra);
    t.pages = pages;
    t.settings = settings;
    t
}

/// The list entry of component `c` as it is now (`None`: gone, or a component without a tag that
/// is drawn nowhere).
pub fn device_entry(doc: &Document, c: ComponentId) -> Option<DeviceEntry> {
    let comp = doc.components.get(&c)?;
    // Junctions and continuations are not devices (design 022 §3.1).
    if comp.properties.contains_key(ecad_model::connectivity::KIND) {
        return None;
    }
    let prefix = match comp.properties.get(ecad_model::tag::TAG_KEY) {
        Some(t) => Some(ecad_model::tag::parse(t).map_or_else(|| ecad_model::tag::key(t), |(p, _)| p)),
        None if doc.representations_of(Subject::Component(c)).next().is_none() => return None,
        None => None,
    };
    Some(entry(doc, c, prefix, &Book::of(doc)))
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

/// Set the place of every listed symbol again (pages were added, removed or moved, or the grid
/// changed): no other part of the list depends on the page order or the grid. Reads each listed
/// symbol's own representation; never scans the document.
pub fn renumber(doc: &Document, entries: &mut [DeviceEntry]) {
    let book = Book::of(doc);
    for e in entries {
        for s in &mut e.symbols {
            if let Some(r) = doc.representations.get(&s.rep) {
                let (_, fresh) = symbol_entry(doc, &book, s.rep, r, s.name.clone());
                *s = fresh;
            }
        }
        e.symbols.sort_by_key(|s| (book.position(s.page).unwrap_or(usize::MAX), s.column, s.rep));
    }
}

/// One symbol row: its book position (for sorting) and the row.
fn symbol_entry(
    doc: &Document,
    book: &Book,
    rid: RepresentationId,
    r: &ecad_model::Representation,
    name: String,
) -> (usize, SymbolEntry) {
    let page_number = book.number(doc, r.page);
    let grid = ecad_model::sheet::Grid::of(doc, r.page);
    let place = position(doc, r).and_then(|p| ecad_model::derive::place_on(&grid, &page_number, p));
    let style = doc.settings.reference_style;
    let n = book.position(r.page).unwrap_or(usize::MAX);
    (
        n,
        SymbolEntry {
            rep: rid,
            page: r.page,
            page_number,
            column: place.as_ref().map(|p| p.column),
            row: place.as_ref().map(|p| p.row),
            place: place.map_or_else(|| ecad_model::derive::unlocated(style).to_string(), |p| p.text(style)),
            name,
        },
    )
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
    let book = Book::of(doc);
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
        // Junctions and continuations are not devices (design 022 §3.1).
        .filter(|(_, c)| !c.properties.contains_key(ecad_model::connectivity::KIND))
        .map(|(id, _)| *id)
        .filter(|id| doc.representations_of(Subject::Component(*id)).next().is_some());
    tagged
        .into_iter()
        .map(|((p, _, _), c)| (Some(p), c))
        .chain(untagged.map(|c| (None, c)))
        .map(|(prefix, c)| entry(doc, c, prefix, &book))
        .filter(|e| matches(e, filter))
        .collect()
}

fn entry(doc: &Document, c: ComponentId, prefix: Option<String>, book: &Book) -> DeviceEntry {
    let props = doc.components.get(&c).map(|x| &x.properties);
    let mut symbols: Vec<(usize, Option<u32>, SymbolEntry)> = doc
        .representations_of(Subject::Component(c))
        .map(|(rid, r)| {
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
            let (n, entry) = symbol_entry(doc, book, rid, r, name);
            (n, entry.column, entry)
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

/// The Devices tab's list kept up to date (owner, 2026-11-08): every device, built whole once (on
/// open, reload or first show), then refreshed only for the devices each change touched ([`devices_touched`], through
/// [`Project::device_changes_since`]), and renumbered when pages moved. A change that touches no
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
    /// Bring the list up to date with `p` (the project numbered `serial` by the caller). Returns
    /// whether it changed. Filtering is the caller's ([`matches`]): it never rebuilds the list.
    pub fn sync(&mut self, serial: u64, p: &Project) -> bool {
        let count = p.change_count();
        let doc = p.ledger.document();
        let since = match self.seen {
            Some((s, n)) if s == serial => Some(n),
            _ => None,
        };
        let changed = match since.and_then(|n| p.device_changes_since(n)) {
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

    /// Forget how up to date the list is: the next [`DeviceList::sync`] builds it whole.
    pub fn invalidate(&mut self) {
        self.seen = None;
    }

    /// Refresh the devices `t` names, and the page numbers when pages moved.
    fn apply(&mut self, doc: &Document, t: &DeviceTouch) -> bool {
        let mut changed = false;
        let mut resort = false;
        for c in &t.components {
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
