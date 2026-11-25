//! The rules' style catalogues (wire, page, cable, rating table) and what spans every catalogue,
//! the look's text, location and frame catalogues too (`ecad_drawing_style::catalogues`): finding
//! a style by name, who uses a style, the pages a style change touches, and the checks a written
//! record passes. A catalogue's mechanism (fields, built-ins, how a field resolves) is
//! [`ecad_model::registry::styles`]; which style an object uses is a registry choice entry
//! (`defaults/styles.rs` here, and the look's in `ecad_drawing_style::defaults::styles`).
//!
//! Bounds (`ecad_model::objects::styles::MAX_*`): records past them are kept, ignored here and
//! reported by Checks.
//!
//! # Update map
//! [`pages_touched`] says which pages an op changes the look of: a style change marks the pages
//! of its explicit users (through the document's style-choice index) and, when the project's
//! choice resolves to it, every page. Nothing here rescans the project on an ordinary commit;
//! [`users`] (the consequence line, Used by) is asked for one style at a time, on demand.
//!
//! This module draws nothing and records nothing; `ecad_engine::commands::style` builds the ops.

use std::collections::BTreeSet;

use ecad_model::objects::styles::{self as ms, MAX_STYLES_PER_CATALOGUE, StyleBody, StyleCatalogue, StyleLayer, StyleRecord, StyleRef};
use ecad_model::{Document, ObjectRef, Op, PageId, Subject, Word};

use ecad_model::registry::Scope;
use ecad_model::registry::data::Registry;
use ecad_model::registry::styles::{Catalogue, Count, Reach, Row, chosen, exists, name, rows};

use crate::CmdError;

pub mod cable;
pub(crate) mod checks;
pub mod commands;
pub mod page;
pub mod ratingtable;
pub mod wire;

/// This crate's catalogues: one line each (a new catalogue adds a line).
#[rustfmt::skip]
pub static CATALOGUES: &[&Catalogue] = &[
    &wire::CATALOGUE,
    &page::CATALOGUE,
    &cable::CATALOGUE,
    &ratingtable::CATALOGUE,
];

/// The style of `cat` that `text` names: an ID (built-in word or `#<hex>`) of an existing
/// style, else a name (without case) that exactly one style has.
///
/// # Errors
/// In words: no such style, or the name is shared and the ID is needed.
pub fn find(doc: &Document, cat: &Catalogue, text: &str) -> Result<StyleRef, CmdError> {
    let t = text.trim();
    if let Some(id) = StyleRef::parse(t).filter(|id| exists(doc, cat, id)) {
        return Ok(id);
    }
    let hits: Vec<Row> = rows(doc, cat).into_iter().filter(|r| r.name.eq_ignore_ascii_case(t)).collect();
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(CmdError::refused(format!("No {} is called {t:?}.", cat.one.to_lowercase()))),
        _ => Err(CmdError::refused(format!("{} {} are called {t:?}; give the ID ({}).", hits.len(), cat.plural.to_lowercase(), ids(&hits)))),
    }
}

fn ids(rows: &[Row]) -> String {
    rows.iter().map(|r| r.id.text()).collect::<Vec<_>>().join(", ")
}

/// The catalogue words, for messages.
pub fn words(registry: &Registry) -> String {
    registry.catalogues().map(|c| c.catalogue.word()).collect::<Vec<_>>().join(", ")
}

/// Who uses style `id` of `cat`. Explicit users come from the document's style-choice index and
/// the location boxes; implicit ones are counted from the resolved choices, one
/// pass over the kind of object the catalogue styles. Asked for one style at a time (a selected
/// row, a consequence line), never per frame.
pub fn users(doc: &Document, cat: &Catalogue, id: &StyleRef) -> Count {
    let value = id.text();
    let mut c = Count::default();
    // A cable's type is its field (cables have no registry level).
    if cat.catalogue == StyleCatalogue::Cable {
        c.explicit = doc.style_users(cable::CABLE_TYPE, &value).count();
    }
    for entry in (cat.choices)() {
        c.explicit += doc.style_users(entry.key, &value).count();
        c.implicit += (cat.implicit)(doc, entry, id);
    }
    c.explicit += box_users(doc, cat.catalogue, id).len();
    c
}

/// The location boxes whose own style is `id` (a box is a drawing aid, so it is not
/// in the document's style-choice index). Empty for other catalogues. Boxes
/// are few: one pass over the aids.
pub fn box_users(doc: &Document, catalogue: StyleCatalogue, id: &StyleRef) -> Vec<ecad_model::AidId> {
    if catalogue != StyleCatalogue::Location {
        return Vec::new();
    }
    doc.aids().iter().filter(|(_, a)| a.as_box().and_then(|b| b.style.as_ref()) == Some(id)).map(|(a, _)| *a).collect()
}

/// The explicit users of `id` (any choice key of `cat`), through the index.
pub fn explicit_users(doc: &Document, cat: &Catalogue, id: &StyleRef) -> Vec<(&'static str, ObjectRef)> {
    let value = id.text();
    let mut out = Vec::new();
    // A cable's type is its field (cables have no registry level).
    if cat.catalogue == StyleCatalogue::Cable {
        out.extend(doc.style_users(cable::CABLE_TYPE, &value).map(|o| (cable::CABLE_TYPE, o)));
    }
    for entry in (cat.choices)() {
        out.extend(doc.style_users(entry.key, &value).map(|o| (entry.key, o)));
    }
    out
}

/// The pages an object is drawn on.
fn pages_of(doc: &Document, o: ObjectRef, out: &mut BTreeSet<PageId>) {
    match o {
        ObjectRef::Device(c) => out.extend(doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page)),
        ObjectRef::Wire(c) => out.extend(crate::wires::routes::wire_pages(doc, c)),
        ObjectRef::Cable(c) => out.extend(doc.representations_of(Subject::Cable(c)).map(|(_, r)| r.page)),
        ObjectRef::Page(p) => {
            out.insert(p);
        }
        ObjectRef::Project | ObjectRef::Standard => {}
    }
}

/// The pages a style op changes the look of, or `None` for every page. `None`
/// for an op this module does not cover, and for a catalogue `registry` does not declare.
pub fn pages_touched(doc: &Document, registry: &Registry, op: &Op) -> Option<BTreeSet<PageId>> {
    match op {
        // The header shows on the Standards screen only.
        Op::SetStandard { .. } => Some(BTreeSet::new()),
        Op::SetStyle { catalogue, id, .. } => {
            let cat = registry.catalogue(*catalogue)?;
            let mut out = match (cat.reach)(doc, op, id) {
                Reach::Only(pages) => return Some(pages),
                Reach::Every => return None,
                Reach::Also(pages) => pages,
            };
            // A style the project (or standard, or built-in) choice resolves to is on every page.
            if (cat.choices)().iter().any(|e| chosen(doc, e, Scope::project()) == *id) {
                return None;
            }
            for (_, o) in explicit_users(doc, cat, id) {
                pages_of(doc, o, &mut out);
            }
            Some(out)
        }
        _ => None,
    }
}

/// Whether record `r` may be written as `(layer, cat, id)`: a name that is
/// none or 1–64 characters, at most [`MAX_FIELDS_PER_STYLE`] fields of at most
/// [`MAX_FIELD_CHARS`] characters, and room in the layer for a new style.
///
/// # Errors
/// The bound it breaks, in words.
pub fn check_record(doc: &Document, cat: &Catalogue, id: &StyleRef, layer: StyleLayer, r: &StyleRecord) -> Result<(), CmdError> {
    if r.name.as_ref().is_some_and(|n| !ms::is_name(n.as_str())) {
        return Err(CmdError::refused("A style needs a name (1 to 64 characters, no control characters)."));
    }
    if r.catalogue() != cat.catalogue {
        return Err(CmdError::refused(format!("A {} record is not a {}.", r.catalogue(), cat.one.to_lowercase())));
    }
    // A rating's reference temperature is below the conductor's
    // maximum.
    if let Some(why) = crate::circuits::check_style(cat.catalogue, r) {
        return Err(CmdError::refused(why));
    }
    // One category per potential.
    if let Some(p) = r.body.wire().and_then(|w| w.potential.as_ref()).map(|p| p.as_str())
        && let Some(other) = wire::WireDraws::new(doc).for_potential(p).filter(|o| *o != id)
    {
        return Err(CmdError::refused(format!("{} is already the default for {p}; clear it there first.", name(doc, &wire::CATALOGUE, other))));
    }
    let held = ms::layer_of(doc, layer).and_then(|c| c.get(&cat.catalogue));
    if held.is_some_and(|m| !m.contains_key(id) && m.len() >= MAX_STYLES_PER_CATALOGUE) {
        return Err(CmdError::refused(format!("{} holds at most {MAX_STYLES_PER_CATALOGUE} styles per layer.", cat.plural)));
    }
    Ok(())
}

/// The record a `style` command writes for `fields` set or cleared on `before`,
/// in catalogue `cat`. The values are as [`crate::styles::commands::styles::fields`] checked them.
///
/// # Errors
/// A value the typed field does not take, in words.
pub fn with_fields(
    cat: &Catalogue, before: Option<&StyleRecord>, name: Option<&str>, fields: &[(String, Option<String>)],
) -> Result<StyleRecord, CmdError> {
    let mut r = before.cloned().unwrap_or_else(|| StyleRecord::empty(cat.catalogue));
    if let Some(n) = name {
        r.name = Some(ms::StyleName::new(n).map_err(|e| CmdError::refused(format!("A style's name: {e}.")))?);
    }
    for (k, v) in fields {
        (cat.set)(&mut r.body, k, v.as_deref()).map_err(|e| CmdError::refused(e.0))?;
    }
    Ok(r)
}

/// A record body that is not of the catalogue whose `set` was given it, in words.
pub fn wrong_body(b: &StyleBody, one: &str) -> ecad_model::registry::styles::Refused {
    ecad_model::registry::styles::Refused(format!("A {} record is not a {}.", b.catalogue(), one.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::registry::styles::builtin_word;
    use ecad_model::registry::{Kind, Value};

    #[test]
    fn every_builtin_value_is_valid_for_its_field() {
        for c in CATALOGUES {
            assert!(ms::is_word(c.catalogue.word()), "{}", c.catalogue);
            for f in c.fields {
                // An empty built-in is "no value" (potential, part number).
                let empty = matches!(f.builtin(), Value::Text(""));
                assert!(empty || ecad_model::registry::accepts(f.kind, &f.builtin()), "{}.{} built-in", c.catalogue, f.key);
            }
            for b in c.builtins {
                assert!(ms::is_word(b.id), "{}", b.id);
                for (k, v) in b.values() {
                    let f = c.field(k);
                    assert!(f.is_some(), "{}.{} names no field {k}", c.catalogue, b.id);
                    assert!(f.is_some_and(|f| ecad_model::registry::accepts(f.kind, v)), "{}.{}.{k}", c.catalogue, b.id);
                }
            }
            for e in (c.choices)() {
                assert!(matches!(e.kind, Kind::Style(w) if w.catalogue == c.catalogue), "{}", e.key);
                // The category choice's built-in is `none`, no category.
                let b = StyleRef::parse(&builtin_word(e));
                // No cable type and no rating table, either.
                let none = matches!(
                    c.catalogue,
                    ecad_model::StyleCatalogue::Wire | ecad_model::StyleCatalogue::Cable | ecad_model::StyleCatalogue::RatingTable
                ) && builtin_word(e) == wire::NONE;
                assert!(none || b.as_ref().is_some_and(|b| c.builtin(b).is_some()), "{}: built-in names no built-in style", e.key);
            }
        }
    }
}
