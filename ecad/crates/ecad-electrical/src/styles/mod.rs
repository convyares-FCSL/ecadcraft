//! The style catalogues (text, location, frame, wire, page, cable, rating table) and what spans
//! them: finding a style by name, who uses a style, the pages a style change touches, and the checks
//! a written record passes. A catalogue's mechanism (fields, built-ins, how a field resolves) is
//! [`ecad_model::registry::styles`]; which style an object uses is a registry choice entry
//! (`defaults/styles.rs`).
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
use ecad_model::{Document, ObjectRef, Op, PageId, Properties, Subject, Word};

use ecad_model::registry::Scope;
use ecad_model::registry::styles::{Catalogue, Count, Row, chosen, exists, hex_rgb, name, rows};

use crate::CmdError;

pub mod cable;
pub mod checks;
pub mod commands;
pub mod frame;
pub mod location;
pub mod page;
pub mod ratingtable;
pub mod text;
pub mod text_style;
pub mod wire;

/// Every catalogue: one line each (a new catalogue adds a line).
#[rustfmt::skip]
pub static CATALOGUES: &[&Catalogue] = &[
    &text::CATALOGUE,
    &location::CATALOGUE,
    &frame::CATALOGUE,
    &wire::CATALOGUE,
    &page::CATALOGUE,
    &cable::CATALOGUE,
    &ratingtable::CATALOGUE,
];

/// The catalogue's description.
pub fn catalogue(c: StyleCatalogue) -> &'static Catalogue {
    match c {
        StyleCatalogue::Text => &text::CATALOGUE,
        StyleCatalogue::Location => &location::CATALOGUE,
        StyleCatalogue::Frame => &frame::CATALOGUE,
        StyleCatalogue::Wire => &wire::CATALOGUE,
        StyleCatalogue::Page => &page::CATALOGUE,
        StyleCatalogue::Cable => &cable::CATALOGUE,
        StyleCatalogue::RatingTable => &ratingtable::CATALOGUE,
    }
}

/// The style of `catalogue` that `text` names: an ID (built-in word or `#<hex>`) of an existing
/// style, else a name (without case) that exactly one style has.
///
/// # Errors
/// In words: no such style, or the name is shared and the ID is needed.
pub fn find(doc: &Document, catalogue: StyleCatalogue, text: &str) -> Result<StyleRef, CmdError> {
    let t = text.trim();
    if let Some(id) = StyleRef::parse(t).filter(|id| exists(doc, crate::styles::catalogue(catalogue), id)) {
        return Ok(id);
    }
    let cat = crate::styles::catalogue(catalogue);
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
pub fn words() -> String {
    CATALOGUES.iter().map(|c| c.catalogue.word()).collect::<Vec<_>>().join(", ")
}

/// Who uses style `id` of `catalogue`. Explicit users come from the document's style-choice index;
/// implicit ones are counted from the resolved choices, one pass over the kind of object the
/// catalogue styles. Asked for one style at a time (a selected row, a consequence line), never per
/// frame.
pub fn users(doc: &Document, catalogue: StyleCatalogue, id: &StyleRef) -> Count {
    let cat = crate::styles::catalogue(catalogue);
    let value = id.text();
    let mut c = Count::default();
    // A cable's type is its field (cables have no registry level).
    if cat.catalogue == StyleCatalogue::Cable {
        c.explicit = doc.style_users(cable::CABLE_TYPE, &value).count();
    }
    for entry in (cat.choices)() {
        c.explicit += doc.style_users(entry.key, &value).count();
        c.implicit += implicit(doc, cat, entry, id);
    }
    c.explicit += box_users(doc, catalogue, id).len();
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

/// The objects of `entry`'s kind with no own choice that resolve to `id` through the page,
/// project, standard or built-in.
fn implicit(doc: &Document, cat: &Catalogue, entry: &ecad_model::registry::Entry, id: &StyleRef) -> usize {
    let project = chosen(doc, entry, Scope::project());
    let own = |p: &Properties| p.get(entry.key).is_some_and(|v| entry.parse_in(doc, v).is_some());
    match cat.catalogue {
        ecad_model::StyleCatalogue::Frame => doc.pages().iter().filter(|(_, p)| !own(&p.display)).filter(|_| project == *id).count(),
        // Pages with no own type whose generated kind (or none) gives `id`.
        ecad_model::StyleCatalogue::Page => doc
            .pages()
            .values()
            .filter(|p| !p.page_type.as_ref().is_some_and(|t| exists(doc, &page::CATALOGUE, t)))
            .filter(|p| ecad_model::document::sheet::page_type_of(doc, p) == *id)
            .count(),
        ecad_model::StyleCatalogue::Location if project == *id => {
            doc.aids().iter().filter(|(a, x)| x.as_box().is_some() && location::own_style(doc, **a).is_none()).count()
        }
        // Conductors whose resolved category is `id` and that have none of their own.
        ecad_model::StyleCatalogue::Wire => {
            let draws = wire::WireDraws::new(doc);
            doc.wires()
                .iter()
                .filter(|(_, x)| !x.wire_type.as_ref().is_some_and(|t| exists(doc, &wire::CATALOGUE, t)))
                .filter(|(c, _)| draws.category(doc, **c).0.as_ref() == Some(id))
                .count()
        }
        ecad_model::StyleCatalogue::Text if project == *id => match text::noun(entry.key) {
            text::Noun::Devices => doc.devices().values().filter(|x| !own(&x.properties)).count(),
            text::Noun::Wires => doc.wires().values().filter(|x| !own(&x.display)).count(),
            text::Noun::Pages => doc.pages().len(),
        },
        _ => 0,
    }
}

/// The explicit users of `id` (any choice key of `catalogue`), through the index.
pub fn explicit_users(doc: &Document, catalogue: StyleCatalogue, id: &StyleRef) -> Vec<(&'static str, ObjectRef)> {
    let cat = crate::styles::catalogue(catalogue);
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
/// for an op this module does not cover.
pub fn pages_touched(doc: &Document, op: &Op) -> Option<BTreeSet<PageId>> {
    match op {
        // The header shows on the Standards screen only.
        Op::SetStandard { .. } => Some(BTreeSet::new()),
        Op::SetStyle { catalogue, id, .. } => {
            let cat = crate::styles::catalogue(*catalogue);
            // A page type reaches its pages only.
            if let Some((pages, _)) = page::touched_by(doc, op) {
                return Some(pages.into_iter().collect());
            }
            // A style the project (or standard, or built-in) choice resolves to is on every page.
            if (cat.choices)().iter().any(|e| chosen(doc, e, Scope::project()) == *id) {
                return None;
            }
            // A text style a location style labels with reaches every box.
            if cat.catalogue == ecad_model::StyleCatalogue::Text && location::labels_with(doc, id) {
                return None;
            }
            let mut out = BTreeSet::new();
            // A box drawn in its own style: that style's boxes, or the boxes whose own
            // style labels with this text style.
            for (a, x) in doc.aids() {
                let Some(b) = x.as_box() else { continue };
                let hit = match cat.catalogue {
                    ecad_model::StyleCatalogue::Location => b.style.as_ref() == Some(id),
                    ecad_model::StyleCatalogue::Text => location::own_style(doc, *a).is_some_and(|s| location::draw_of(doc, s).label_style == *id),
                    _ => false,
                };
                if hit {
                    out.extend(doc.representations_of(Subject::Aid(*a)).map(|(_, r)| r.page));
                }
            }
            // A category that is (or becomes, or was) a potential's default reaches every
            // net of that potential, on any page (there is no potential index): every page.
            if cat.catalogue == ecad_model::StyleCatalogue::Wire {
                let names_potential = |r: &Option<StyleRecord>| r.as_ref().and_then(|r| r.body.wire()).is_some_and(|w| w.potential.is_some());
                let (before, after) = match op {
                    Op::SetStyle { before, after, .. } => (before, after),
                    _ => return None,
                };
                let draws = wire::WireDraws::new(doc);
                let default = draws.by_potential_ids().any(|x| x == id);
                if default || names_potential(before) || names_potential(after) {
                    return None;
                }
            }
            for (_, o) in explicit_users(doc, *catalogue, id) {
                pages_of(doc, o, &mut out);
            }
            Some(out)
        }
        _ => None,
    }
}

/// Whether record `r` may be written as `(layer, catalogue, id)`: a name that is
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
/// in catalogue `catalogue`. The values are as [`crate::styles::commands::styles::fields`] checked them.
///
/// # Errors
/// A value the typed field does not take, in words.
pub(crate) fn with_fields(
    catalogue: StyleCatalogue, before: Option<&StyleRecord>, name: Option<&str>, fields: &[(String, Option<String>)],
) -> Result<StyleRecord, CmdError> {
    let mut r = before.cloned().unwrap_or_else(|| StyleRecord::empty(catalogue));
    if let Some(n) = name {
        r.name = Some(ms::StyleName::new(n).map_err(|e| CmdError::refused(format!("A style's name: {e}.")))?);
    }
    for (k, v) in fields {
        match &mut r.body {
            StyleBody::Wire(w) => wire::set(w, k, v.as_deref())?,
            StyleBody::Text(t) => text::set(t, k, v.as_deref())?,
            StyleBody::Location(l) => location::set(l, k, v.as_deref())?,
            StyleBody::RatingTable(t) => ratingtable::set(t, k, v.as_deref())?,
            StyleBody::Page(t) => page::set(t, k, v.as_deref())?,
            StyleBody::Cable(t) => cable::set(t, k, v.as_deref())?,
            StyleBody::Frame(f) => frame::set(f, k, v.as_deref())?,
        }
    }
    Ok(r)
}

/// Whether `s` is a colour a style field takes: `none`, one IEC 60757 code
/// (`RD`, `BLU`), or `#rrggbb`.
pub(crate) fn is_colour(s: &str) -> bool {
    let t = s.trim();
    t.eq_ignore_ascii_case("none") || hex_rgb(t).is_some() || crate::cables::words::colour_code(t).is_some_and(|(_, two)| two.is_none())
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

    #[test]
    fn colours_are_none_one_code_or_hex() {
        for ok in ["none", "NONE", "RD", "blu", "#c62828"] {
            assert!(is_colour(ok), "{ok}");
        }
        for bad in ["", "GNYE", "#12345", "#gggggg", "purple"] {
            assert!(!is_colour(bad), "{bad}");
        }
    }
}
