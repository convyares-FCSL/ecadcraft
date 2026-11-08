//! The Devices tab's list (brief 016's Project panel, owner request of 2026-11-08): every device
//! grouped by its tag prefix, each with its symbols, page and column, and what a "go to" selects.
//! Read only: edits go through `setprop` (brief 018's rules), never through here.
//!
//! Built from the tag index ([`ecad_model::Document::tagged`]) and each device's representations
//! (the subject index), only when the project or the filter changed; nothing here runs per frame.
//!
//! Excluded: cables, terminals and wires; editing.

use std::collections::HashMap;

use cadcraft_engine::doc::Handle;
use ecad_model::{ComponentId, Document, ItemKind, PageId, Pt, RepresentationId, Subject};

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
