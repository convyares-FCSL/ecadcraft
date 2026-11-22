//! The Devices tab's list (brief 016's Project panel, owner request of 2026-11-08): every device
//! grouped by its tag prefix, each with its symbols, page and column, and what a "go to" selects.
//! Read only: edits go through `setprop` (brief 018's rules), never through here.
//!
//! Built from the tag index ([`ecad_model::Document::tagged`]) and each device's representations
//! (the subject index), only when the project or the filter changed; nothing here runs per frame.
//!
//! Excluded: cables, terminals and wires; editing.

use std::collections::BTreeSet;

use ecad_model::document::book::Book;

use ecad_model::{DeviceId, Document, ItemKind, PageId, Pt, RepresentationId, Subject};

use crate::devices::{self, PROPERTIES};

pub mod kept;

/// One symbol (representation) of a device, as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolEntry {
    pub rep: RepresentationId,
    pub page: PageId,
    /// The page's number as shown ([`crate::references::cables::page_number`]).
    pub page_number: String,
    /// Its frame column and row (brief 019), when it can be located.
    pub column: Option<u32>,
    pub row: Option<u32>,
    /// Where it is, in the project's reference style (`105-C-4`), or `?`.
    pub place: String,
    /// `NO contact`, `Contactor / relay coil`, or `drawn` for a converted outline.
    pub name: String,
    /// Brief 048 (design §8.4): its project symbol has a newer version.
    pub update: bool,
}

/// One device in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceEntry {
    pub id: DeviceId,
    /// Its tag as stored (bare: see [`DeviceEntry::shown`]); `None` for a component without one
    /// (converted outlines, older coils).
    pub tag: Option<String>,
    /// The group it is listed under: `-K`, `-Q`… (its tag's prefix), or [`NO_TAG`].
    pub group: String,
    pub description: String,
    /// The six device properties ([`PROPERTIES`]) as stored, in that order (`None`: not set).
    pub properties: Vec<Option<String>>,
    /// Its symbols, in page order, then column.
    pub symbols: Vec<SymbolEntry>,
    /// What its tag is (brief 026, S6 #3): `F` after a fixed one, provisional in `ecad.provisional`.
    pub tag_state: ecad_model::naming::tag::TagState,
}

impl DeviceEntry {
    /// Its tag as drawn: a provisional one with its `?`.
    pub fn shown(&self) -> Option<String> {
        self.tag.as_deref().map(|t| ecad_model::naming::tag::shown(t, self.tag_state))
    }
}

/// What a change could change in the Devices list (owner, 2026-11-08: act only on the ops that
/// matter): the devices whose row or symbol rows may differ, and whether page numbers moved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceTouch {
    pub devices: BTreeSet<DeviceId>,
    /// Pages were added, removed, moved or renamed: the `/page.column` texts may differ.
    pub pages: bool,
    /// The project settings changed (tag prefixes, padding): the Devices list shows none of them,
    /// but the symbol search's prefixes and next tags may differ.
    pub settings: bool,
}

impl DeviceTouch {
    /// Nothing in the list can differ.
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty() && !self.pages
    }

    /// Nothing the symbol search shows can differ (its rows and its second step): no device, no
    /// page and no setting was touched.
    pub fn leaves_search(&self) -> bool {
        self.is_empty() && !self.settings
    }
}

/// The list entry of component `c` as it is now (`None`: gone, or a component without a tag that
/// is drawn nowhere).
pub fn device_entry(doc: &Document, c: DeviceId) -> Option<DeviceEntry> {
    let comp = doc.devices.get(&c)?;
    let prefix = match comp.tag() {
        Some(t) => Some(group_and_number(t, comp.tag.state().unwrap_or_default()).0),
        None if doc.representations_of(Subject::Device(c)).next().is_none() => return None,
        None => None,
    };
    Some(entry(doc, c, prefix, &Book::of(doc)))
}

/// The order of the list: tagged devices by prefix, number, tag; then those without a tag.
pub(crate) fn sort_key(e: &DeviceEntry) -> (bool, String, u64, String, DeviceId) {
    match e.tag.as_deref() {
        Some(t) => {
            let (p, n) = group_and_number(t, e.tag_state);
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
fn symbol_entry(doc: &Document, book: &Book, rid: RepresentationId, r: &ecad_model::Representation, name: String) -> (usize, SymbolEntry) {
    let page_number = book.number(doc, r.page);
    let grid = ecad_model::document::sheet::Grid::of(doc, r.page);
    let place = position(doc, r).and_then(|p| ecad_model::connectivity::derive::place_on(&grid, &page_number, p));
    let style = doc.settings.reference_style();
    let n = book.position(r.page).unwrap_or(usize::MAX);
    (
        n,
        SymbolEntry {
            rep: rid,
            page: r.page,
            page_number,
            column: place.as_ref().map(|p| p.column),
            row: place.as_ref().map(|p| p.row),
            place: place.map_or_else(|| ecad_model::connectivity::derive::unlocated(style).to_string(), |p| p.text(style)),
            name,
            update: crate::symbols::graphics::update_available(doc, rid).is_some(),
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
    if e.tag.as_deref().is_some_and(|t| devices::json::tag_matches(t, q)) {
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
    let mut tagged: Vec<((String, u64, String), DeviceId)> = doc
        .tagged()
        .filter_map(|(_, c)| {
            let d = doc.devices.get(&c)?;
            let t = d.tag()?;
            let (p, n) = group_and_number(t, d.tag.state().unwrap_or_default());
            Some(((p, n, t.to_string()), c))
        })
        .collect();
    tagged.sort();
    let untagged = doc
        .devices
        .iter()
        .filter(|(_, c)| c.tag.is_none())
        .map(|(id, _)| *id)
        .filter(|id| doc.representations_of(Subject::Device(*id)).next().is_some());
    tagged
        .into_iter()
        .map(|((p, _, _), c)| (Some(p), c))
        .chain(untagged.map(|c| (None, c)))
        .map(|(prefix, c)| entry(doc, c, prefix, &book))
        .filter(|e| matches(e, filter))
        .collect()
}

/// A tag's group (its prefix, or the whole tag when it has none) and its place in the group: its
/// number; a provisional tag after every numbered one of its prefix (brief 026, design §9).
pub(crate) fn group_and_number(t: &str, state: ecad_model::naming::tag::TagState) -> (String, u64) {
    match ecad_model::naming::tag::parse(t) {
        Some((p, n)) if state.is_provisional() => (p, ecad_model::naming::tag::MAX_TAG_NUMBER.saturating_add(n)),
        Some((p, n)) => (p, n),
        None => (ecad_model::naming::tag::key(t), 0),
    }
}

fn entry(doc: &Document, c: DeviceId, prefix: Option<String>, book: &Book) -> DeviceEntry {
    let device = doc.devices.get(&c);
    let mut symbols: Vec<(usize, Option<u32>, SymbolEntry)> = doc
        .representations_of(Subject::Device(c))
        .map(|(rid, r)| {
            let name = devices::tags::rep_definition(doc, r).map_or_else(|| "drawn".to_string(), |(d, _)| d.name.clone());
            let (n, entry) = symbol_entry(doc, book, rid, r, name);
            (n, entry.column, entry)
        })
        .collect();
    symbols.sort_by_key(|(n, col, s)| (*n, *col, s.rep));
    DeviceEntry {
        id: c,
        tag: devices::tags::tag_of(doc, c).map(str::to_string),
        group: prefix.map_or_else(|| NO_TAG.to_string(), |p| format!("-{p}")),
        description: devices::tags::description(doc, c),
        properties: PROPERTIES.iter().map(|(k, _)| device.and_then(|d| d.value(k))).collect(),
        symbols: symbols.into_iter().map(|(_, _, s)| s).collect(),
        tag_state: device.and_then(|d| d.tag.state()).unwrap_or_default(),
    }
}

/// Where a representation is: a symbol's insertion point, else its first connection point.
pub fn position(doc: &Document, r: &ecad_model::Representation) -> Option<Pt> {
    if let Some(ItemKind::Symbol(s)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) {
        return Some(s.placement.origin);
    }
    r.connection_points.values().next().copied()
}

// The engine half (design 065 §3), at its old path until step 6 turns this into a facade.
