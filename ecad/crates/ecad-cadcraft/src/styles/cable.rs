//! Cable types (brief 057, design 057 §4.4, FP4): the `cable` catalogue, the word 050 FP7 reserved.
//!
//! A cable type holds what a field cable's rating needs from its datasheet: the size, the
//! conductor's material, maximum temperature and class, and a rated current at a reference
//! temperature; and the part number, maker and description, shown only (2.7 and the BOM later).
//! There are **no built-in cable types**. A cable chooses one with its own property
//! [`CABLE_TYPE`] (a style ID): a plain cable property, since the registry has no cable level.
//! The cable's free `type` text (028) is unchanged and still drives its core offer.

use ecad_model::{CableId, Document, StyleRef};

use super::{Catalogue, Field};
use crate::defaults::{Kind, Value};
use crate::rating::fields;

/// The catalogue word.
pub const WORD: &str = "cable";
/// The cable property naming its cable type (FP2).
pub const CABLE_TYPE: &str = "cable_type";

fn any_text(_: &str) -> bool {
    true
}

const TEXT64: Kind = Kind::Parsed { max_chars: 64, parse: any_text };

/// The field naming the cores' colours, in core order (`BN,BK,GY,GNYE`): board 2.06's drawn cable.
pub const CORE_COLOURS: &str = "core_colours";
/// Most cores a colour list names (the core offer's own bound).
pub const MAX_COLOURS: usize = 64;

/// A comma-separated list of IEC 60757 colour codes, 1 to [`MAX_COLOURS`] of them.
pub fn colour_list(s: &str) -> Option<Vec<String>> {
    let v: Vec<String> = s.split(',').map(|c| c.trim().to_ascii_uppercase()).collect();
    (!v.is_empty() && v.len() <= MAX_COLOURS && v.iter().all(|c| crate::look::colour_code(c).is_some())).then_some(v)
}

fn is_colour_list(s: &str) -> bool {
    colour_list(s).is_some()
}

static FIELDS: &[Field] = &[
    fields::CROSS_SECTION,
    fields::MATERIAL,
    fields::MAX_TEMPERATURE,
    fields::CONDUCTOR_CLASS,
    fields::RATED_CURRENT,
    fields::RATED_AT,
    Field {
        key: CORE_COLOURS,
        label: "Core colours",
        kind: Kind::Parsed { max_chars: 256, parse: is_colour_list },
        builtin: Value::Text(""),
        over: None,
    },
    Field { key: "part_number", label: "Part number", kind: TEXT64, builtin: Value::Text(""), over: None },
    Field { key: "maker", label: "Maker", kind: TEXT64, builtin: Value::Text(""), over: None },
    Field { key: "description", label: "Description", kind: TEXT64, builtin: Value::Text(""), over: None },
];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    Vec::new()
}

/// The cable type catalogue: no built-in entries.
pub static CATALOGUE: Catalogue =
    Catalogue { word: WORD, plural: "Cable types", one: "Cable type", fields: FIELDS, builtins: &[], choices, project_field: None };

/// The cable type cable `k` names, when it names one that exists.
pub fn of(doc: &Document, k: CableId) -> Option<StyleRef> {
    let v = doc.cables.get(&k)?.properties.get(CABLE_TYPE)?;
    StyleRef::parse(v).filter(|id| super::exists(doc, WORD, id))
}

/// The core colours of cable `k`'s cable type, in core order; empty when it has no type or no
/// colours.
pub fn core_colours(doc: &Document, k: CableId) -> Vec<String> {
    of(doc, k).and_then(|id| super::field_of(doc, WORD, &id, CORE_COLOURS, None)).and_then(|r| colour_list(&r.text())).unwrap_or_default()
}

/// A cable property ticking core `<label>` as **Not used** (owner rule, board 2.06): `unused.<label>`
/// = `yes`. An unticked core that is not connected at both ends is an open end (an error).
pub const UNUSED_PREFIX: &str = "unused.";
/// The longest core label read in an `unused.` key.
pub const MAX_LABEL_CHARS: usize = 16;

/// The property key ticking core `label` as not used.
pub fn unused_key(label: &str) -> String {
    format!("{UNUSED_PREFIX}{}", label.trim().chars().take(MAX_LABEL_CHARS).collect::<String>())
}

/// Whether core `label` of cable `k` is ticked **Not used**.
pub fn is_unused(doc: &Document, k: CableId, label: &str) -> bool {
    doc.cables.get(&k).and_then(|c| c.properties.get(&unused_key(label))).is_some_and(|v| v.trim().eq_ignore_ascii_case("yes"))
}

/// A core that is not connected at both ends and not ticked Not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCore {
    pub label: String,
    /// No wire at all (an offered core nobody uses).
    pub unwired: bool,
    /// The open end: the conductor and which of its ends is attached to nothing.
    pub end: Option<(ecad_model::ConductorId, ecad_model::End)>,
}

/// The open cores of cable `k` (owner rule): offered cores no member uses, and core chains with an
/// end attached to nothing, unless ticked Not used. Bounded by the cable's member bound.
pub fn open_cores(doc: &Document, k: CableId) -> Vec<OpenCore> {
    let Some(cab) = doc.cables.get(&k) else { return Vec::new() };
    let chains = crate::references::core_chains(doc, k);
    let heads: Vec<ecad_model::CableMember> = chains
        .iter()
        .filter_map(|ch| {
            let first = *ch.first()?;
            let core = ch.iter().find_map(|w| cab.members.iter().find(|m| m.conductor == *w).and_then(|m| m.core.clone()));
            Some(ecad_model::CableMember { conductor: first, core })
        })
        .collect();
    let count = crate::strips::count_cores(crate::strips::cable_offer(&cab.properties), &heads);
    let mut out = Vec::new();
    for label in &count.spare {
        if !is_unused(doc, k, label) {
            out.push(OpenCore { label: label.clone(), unwired: true, end: None });
        }
    }
    for (i, ch) in chains.iter().enumerate() {
        let end = ch.iter().find_map(|w| {
            let [a, b] = doc.conductors.get(w)?.ends;
            if a.is_none() {
                Some((*w, ecad_model::End::A))
            } else if b.is_none() {
                Some((*w, ecad_model::End::B))
            } else {
                None
            }
        });
        let Some(end) = end else { continue };
        let label =
            count.matched.get(i).cloned().flatten().or_else(|| heads.get(i).and_then(|h| h.core.clone())).unwrap_or_else(|| (i + 1).to_string());
        if !is_unused(doc, k, &label) {
            out.push(OpenCore { label, unwired: false, end: Some(end) });
        }
    }
    out
}

/// The cables that name cable type `id` (one pass over the cables, asked for one type at a time).
pub fn users(doc: &Document, id: &StyleRef) -> Vec<CableId> {
    let want = id.text();
    doc.cables.iter().filter(|(_, c)| c.properties.get(CABLE_TYPE).is_some_and(|v| v.trim() == want)).map(|(k, _)| *k).collect()
}
