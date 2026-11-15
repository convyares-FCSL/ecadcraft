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
    // Brief 059 (design 059 §2.2, FD3): the cable's defaults.
    Field { key: TYPE_CORES, label: "Cores", kind: Kind::Parsed { max_chars: 256, parse: is_offer }, builtin: Value::Text(""), over: None },
    Field { key: TYPE_SHIELD, label: "Shield", kind: Kind::Parsed { max_chars: 16, parse: is_shield }, builtin: Value::Text("none"), over: None },
    Field {
        key: TYPE_GLAND,
        label: "Gland",
        kind: Kind::Parsed { max_chars: MAX_GLAND_CHARS, parse: is_gland },
        builtin: Value::Text(""),
        over: None,
    },
    Field {
        key: TYPE_OUTER_DIAMETER,
        label: "Outer diameter (mm)",
        kind: Kind::Parsed { max_chars: 16, parse: is_outer_diameter },
        builtin: Value::Text(""),
        over: None,
    },
];

/// Brief 059: the cable type's core offer (`4G`, `BN,BK,GY,GNYE`), used when the cable has no
/// `cores` of its own.
pub const TYPE_CORES: &str = "cores";
/// Brief 059: the cable type's shield (`none`, `overall`, `pairs`, `overall+pairs`).
pub const TYPE_SHIELD: &str = "shield";
/// Brief 059: the cable type's gland at both ends (`M20`).
pub const TYPE_GLAND: &str = "gland";
/// Brief 059: the cable type's outer diameter in millimetres, 1 to 200 (shown; computed glands later).
pub const TYPE_OUTER_DIAMETER: &str = "outer_diameter";
/// The longest gland size stored.
pub const MAX_GLAND_CHARS: usize = 16;
/// The shield words, as stored.
pub const SHIELDS: [&str; 4] = ["none", "overall", "pairs", "overall+pairs"];

fn is_offer(s: &str) -> bool {
    crate::strips::parse_cores(s).is_ok()
}

fn is_shield(s: &str) -> bool {
    SHIELDS.iter().any(|w| w.eq_ignore_ascii_case(s.trim()))
}

/// A gland size: 1 to [`MAX_GLAND_CHARS`] characters, no control characters.
pub fn is_gland(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.chars().count() <= MAX_GLAND_CHARS && !t.chars().any(char::is_control)
}

/// An outer diameter: a number of millimetres from 1 to 200.
pub fn outer_diameter(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite() && (1.0..=200.0).contains(v))
}

fn is_outer_diameter(s: &str) -> bool {
    outer_diameter(s).is_some()
}

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

/// The text of cable type `id`'s field `key`, when set (empty counts as unset).
pub fn type_text(doc: &Document, id: &StyleRef, key: &str) -> Option<String> {
    super::field_of(doc, WORD, id, key, None).map(|r| r.text()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// Brief 059 (design 059 §2.2): cable `k`'s core offer and where it comes from: its own `cores`,
/// else its cable type's `cores`, else its `type` notation; `None` when unknown.
pub fn offer(doc: &Document, k: CableId) -> Option<(Vec<String>, crate::strips::OfferSource, String)> {
    let cab = doc.cables.get(&k)?;
    offer_of_props(doc, &cab.properties)
}

/// [`offer`] for a cable holding `props` (a cable about to be edited).
pub fn offer_of_props(doc: &Document, props: &ecad_model::Properties) -> Option<(Vec<String>, crate::strips::OfferSource, String)> {
    if let Some(v) = props.get(crate::strips::CORES_KEY).map(|v| v.trim()).filter(|v| !v.is_empty())
        && let Ok(c) = crate::strips::parse_cores(v)
    {
        return Some((c, crate::strips::OfferSource::Cores, v.to_string()));
    }
    let ty = props.get(CABLE_TYPE).and_then(|v| StyleRef::parse(v)).filter(|id| super::exists(doc, WORD, id));
    if let Some(v) = ty.as_ref().and_then(|id| type_text(doc, id, TYPE_CORES))
        && let Ok(c) = crate::strips::parse_cores(&v)
    {
        return Some((c, crate::strips::OfferSource::Cores, v));
    }
    crate::strips::cable_offer(props)
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

/// Most bad `unused.` keys a cable's Checks rows name (a hostile file may hold many).
pub const MAX_BAD_ROWS: usize = 8;

/// The cores of cable `k`, by label: its offer (`4G1.5`: 1, 2, 3, GNYE) and each core chain's
/// label (a stored core label, or its place `1`, `2`… when it has none). Bounded by the cable's
/// member bound.
pub fn core_labels(doc: &Document, k: CableId) -> Vec<String> {
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
    let count = crate::strips::count_cores(offer(doc, k), &heads);
    let mut out: Vec<String> = count.offer.clone().unwrap_or_default();
    for i in 0..heads.len() {
        let l = count.matched.get(i).cloned().flatten().or_else(|| heads.get(i).and_then(|h| h.core.clone())).unwrap_or_else(|| (i + 1).to_string());
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// Check an `unused.<core>` value set on cable `k` (tester re-check 2, item 4): the core is one of
/// the cable's ([`core_labels`]), named by 1 to [`MAX_LABEL_CHARS`] characters with no control
/// characters; the value is `yes` (ticked) or `no` / null (not ticked, the key cleared). Clearing a
/// key a file already holds is always allowed. Returns the value to store.
///
/// # Errors
/// In plain words, bounded: the core is not the cable's, or the value is not yes or no.
pub fn unused_value(doc: &Document, k: CableId, key: &str, value: Option<&str>) -> Result<Option<String>, String> {
    let label = key.strip_prefix(UNUSED_PREFIX).unwrap_or_default();
    let clearing = value.map(str::trim).is_none_or(|v| v.is_empty() || v.eq_ignore_ascii_case("no"));
    if clearing && doc.cables.get(&k).is_some_and(|c| c.properties.contains_key(key)) {
        return Ok(None);
    }
    if label.is_empty() || label.chars().nth(MAX_LABEL_CHARS).is_some() || label.chars().any(char::is_control) {
        return Err(format!(
            "Not used names a core of the cable: unused.<core>, the core 1 to {MAX_LABEL_CHARS} characters with no control characters."
        ));
    }
    let cores = core_labels(doc, k);
    if !cores.iter().any(|c| c == label) {
        let shown: Vec<&str> = cores.iter().take(12).map(String::as_str).collect();
        let more = if cores.len() > shown.len() { ", …" } else { "" };
        let list = if cores.is_empty() { "none".to_string() } else { format!("{}{more}", shown.join(", ")) };
        return Err(format!("This cable has no core {label}; its cores are {list}."));
    }
    if clearing {
        return Ok(None);
    }
    match value.map(str::trim) {
        Some(v) if v.eq_ignore_ascii_case("yes") => Ok(Some("yes".into())),
        _ => Err("Not used is yes or no (or null to clear it).".into()),
    }
}

/// The `unused.` keys of cable `k` that ECAD does not read (a file made elsewhere): (key, value),
/// at most [`MAX_BAD_ROWS`], and how many in all.
pub fn bad_unused(doc: &Document, k: CableId) -> (Vec<(String, String)>, usize) {
    let Some(cab) = doc.cables.get(&k) else { return (Vec::new(), 0) };
    let mut keys = cab.properties.range(UNUSED_PREFIX.to_string()..).take_while(|(key, _)| key.starts_with(UNUSED_PREFIX)).peekable();
    if keys.peek().is_none() {
        return (Vec::new(), 0);
    }
    let cores = core_labels(doc, k);
    let mut out = Vec::new();
    let mut n = 0usize;
    for (key, v) in keys {
        let label = key.strip_prefix(UNUSED_PREFIX).unwrap_or_default();
        let ok = !label.is_empty()
            && label.chars().nth(MAX_LABEL_CHARS).is_none()
            && !label.chars().any(char::is_control)
            && cores.iter().any(|c| c == label)
            && (v.trim().eq_ignore_ascii_case("yes") || v.trim().eq_ignore_ascii_case("no"));
        if !ok {
            n += 1;
            if out.len() < MAX_BAD_ROWS {
                let clip = |t: &str| t.chars().take(24).filter(|c| !c.is_control()).collect::<String>();
                out.push((clip(key), clip(v)));
            }
        }
    }
    (out, n)
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
    let count = crate::strips::count_cores(offer(doc, k), &heads);
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
