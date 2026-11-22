//! Cable types (brief 057, design 057 §4.4, FP4): the `cable` catalogue, the word 050 FP7 reserved.
//!
//! A cable type holds what a field cable's rating needs from its datasheet: the size, the
//! conductor's material, maximum temperature and class, and a rated current at a reference
//! temperature; and the part number, maker and description, shown only (2.7 and the BOM later).
//! There are **no built-in cable types**. A cable chooses one with its field
//! [`ecad_model::Cable::cable_type`] (design 072 §4.5); the registry has no cable level.
//! The cable's free `type` text (028) is unchanged and still drives its core offer.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ecad_model::geometry::units::{Amps, Celsius, Mm};
use ecad_model::objects::styles::cable::{CoreColours, CoreScheme};
use ecad_model::objects::styles::{CableType, StyleLayer};
use ecad_model::objects::wire::{Colours, WireSize};
use ecad_model::{CableId, Document, StyleRef, Word};
use serde::Deserialize;

use crate::CmdError;
use crate::circuits::fields;
use crate::defaults::{Kind, Slot, Value};
use crate::styles::{Catalogue, Field};

/// The input word (and style-choice index key) naming a cable's cable type (FP2); stored in
/// [`ecad_model::Cable::cable_type`], never as a property.
pub const CABLE_TYPE: &str = ecad_model::document::doc::CABLE_TYPE_KEY;

fn any_text(_: &str) -> bool {
    true
}

const TEXT64: Kind = Kind::Parsed { max_chars: 64, parse: any_text };

/// The field naming the cores' colours, in core order (`BN,BK,GY,GN/YE`): board 2.06's drawn cable.
pub const CORE_COLOURS: &str = "core_colours";

fn is_colour_list(s: &str) -> bool {
    CoreColours::parse(s).is_some()
}

/// The field naming a scheme the cores' colours come from (design 072 F37).
pub const CORE_SCHEME: &str = "core_scheme";

fn is_size(s: &str) -> bool {
    WireSize::parse(s).is_some()
}

static FIELDS: [Field; 15] = [
    Field { key: "size", label: "Size", kind: Kind::Parsed { max_chars: 24, parse: is_size }, slot: Slot::new(), over: None },
    fields::MATERIAL,
    fields::MAX_TEMPERATURE,
    fields::CONDUCTOR_CLASS,
    fields::RATED_CURRENT,
    fields::RATED_AT,
    Field { key: CORE_COLOURS, label: "Core colours", kind: Kind::Parsed { max_chars: 256, parse: is_colour_list }, slot: Slot::new(), over: None },
    Field { key: CORE_SCHEME, label: "Core colour scheme", kind: Kind::Choice(<CoreScheme as Word>::WORDS), slot: Slot::new(), over: None },
    Field { key: "part_number", label: "Part number", kind: TEXT64, slot: Slot::new(), over: None },
    Field { key: "maker", label: "Maker", kind: TEXT64, slot: Slot::new(), over: None },
    Field { key: "description", label: "Description", kind: TEXT64, slot: Slot::new(), over: None },
    // Brief 059 (design 059 §2.2, FD3): the cable's defaults.
    Field { key: TYPE_CORES, label: "Cores", kind: Kind::Parsed { max_chars: 256, parse: is_offer }, slot: Slot::new(), over: None },
    Field { key: TYPE_SHIELD, label: "Shield", kind: Kind::Parsed { max_chars: 16, parse: is_shield }, slot: Slot::new(), over: None },
    Field { key: TYPE_GLAND, label: "Gland", kind: Kind::Parsed { max_chars: MAX_GLAND_CHARS, parse: is_gland }, slot: Slot::new(), over: None },
    Field {
        key: TYPE_OUTER_DIAMETER,
        label: "Outer diameter (mm)",
        kind: Kind::Parsed { max_chars: 16, parse: is_outer_diameter },
        slot: Slot::new(),
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
    crate::terminal_strips::cores::parse_cores(s).is_ok()
}

fn is_shield(s: &str) -> bool {
    SHIELDS.iter().any(|w| w.eq_ignore_ascii_case(s.trim()))
}

/// A gland size: 1 to [`MAX_GLAND_CHARS`] characters, with no control or invisible (bidi,
/// zero-width) characters (`ecad_model::naming::text::check`, tester 059 f4).
pub fn is_gland(s: &str) -> bool {
    ecad_model::naming::text::check(s, MAX_GLAND_CHARS).is_ok()
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
pub static CATALOGUE: Catalogue = Catalogue {
    catalogue: ecad_model::StyleCatalogue::Cable,
    plural: "Cable types",
    one: "Cable type",
    fields: &FIELDS,
    builtins: &[],
    choices,
    project_field: None,
};

/// The cable type cable `k` names, when it names one that exists.
pub fn of(doc: &Document, k: CableId) -> Option<StyleRef> {
    doc.cables.get(&k)?.cable_type.clone().filter(|id| crate::styles::exists(doc, ecad_model::StyleCatalogue::Cable, id))
}

/// The text of cable type `id`'s field `key`, when set (empty counts as unset).
pub fn type_text(doc: &Document, id: &StyleRef, key: &str) -> Option<String> {
    crate::styles::field_of(doc, ecad_model::StyleCatalogue::Cable, id, key, None)
        .map(|r| r.text())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Brief 059 (design 059 §2.2): cable `k`'s core offer and where it comes from: its own `cores`,
/// else its cable type's `cores`, else its `type` notation; `None` when unknown.
pub fn offer(doc: &Document, k: CableId) -> Option<(Vec<String>, crate::terminal_strips::cores::OfferSource, String)> {
    offer_of(doc, doc.cables.get(&k)?)
}

/// [`offer`] for cable `cab` (one about to be written).
pub fn offer_of(doc: &Document, cab: &ecad_model::Cable) -> Option<(Vec<String>, crate::terminal_strips::cores::OfferSource, String)> {
    let props = &cab.properties;
    if let Some(v) = props.get(crate::terminal_strips::CORES_KEY).map(|v| v.trim()).filter(|v| !v.is_empty())
        && let Ok(c) = crate::terminal_strips::cores::parse_cores(v)
    {
        return Some((c, crate::terminal_strips::cores::OfferSource::Cores, v.to_string()));
    }
    let ty = cab.cable_type.as_ref().filter(|id| crate::styles::exists(doc, ecad_model::StyleCatalogue::Cable, id));
    if let Some(v) = ty.as_ref().and_then(|id| type_text(doc, id, TYPE_CORES))
        && let Ok(c) = crate::terminal_strips::cores::parse_cores(&v)
    {
        return Some((c, crate::terminal_strips::cores::OfferSource::Cores, v));
    }
    crate::terminal_strips::cores::cable_offer(props)
}

/// Store the cores `after`'s offer names (design 072 §4.5, a write rule): every offered core no
/// core of the cable holds by label goes to a placed core with no label (in core order), else
/// becomes an unplaced in-use core, and an unplaced core the old offer
/// (`before`'s) named and the new one does not goes. Placed cores never move. Run by
/// the commands that set `cores`, `type` or `cable_type`; reading never does it. Labels compare
/// without case.
pub fn sync_offered_cores(doc: &Document, before: &ecad_model::Cable, after: &mut ecad_model::Cable) {
    let labels = |cab: &ecad_model::Cable| -> Vec<String> { offer_of(doc, cab).map(|(o, _, _)| o).unwrap_or_default() };
    let (old, new) = (labels(before), labels(after));
    let named = |list: &[String], l: &str| list.iter().any(|x| x.eq_ignore_ascii_case(l.trim()));
    after.cores.retain(|m| m.position != ecad_model::CorePosition::Unplaced || m.label_str().is_none_or(|l| !named(&old, l) || named(&new, l)));
    let held = |cores: &[ecad_model::Core], l: &str| cores.iter().any(|m| m.label_str().is_some_and(|x| x.trim().eq_ignore_ascii_case(l)));
    // A placed core with no label takes the next offered label no core holds, in core order.
    for i in 0..after.cores.len() {
        let unlabelled = after.cores.get(i).is_some_and(|m| m.position != ecad_model::CorePosition::Unplaced && m.label.is_none());
        if !unlabelled {
            continue;
        }
        let Some(label) = new.iter().find(|l| !held(&after.cores, l)).and_then(|l| ecad_model::CoreLabel::new(l).ok()) else { break };
        if let Some(m) = after.cores.get_mut(i) {
            m.label = Some(label);
        }
    }
    for l in &new {
        if after.cores.len() >= crate::cables::slash::MAX_CABLE_MEMBERS {
            break;
        }
        let has = after.cores.iter().any(|m| m.label_str().is_some_and(|x| x.trim().eq_ignore_ascii_case(l)));
        if !has && let Ok(label) = ecad_model::CoreLabel::new(l) {
            after.cores.push(ecad_model::Core::unplaced(Some(label)));
        }
    }
}

/// Place a core of `cable` at `position` (design 072 §4.5, a write rule): the first unplaced in-use
/// core labelled `label` moves there; with none (or no label), a new in-use core labelled `label`
/// is added at the end.
pub fn place_core(cable: &mut ecad_model::Cable, label: Option<ecad_model::CoreLabel>, position: ecad_model::CorePosition) {
    let free = |m: &ecad_model::Core| m.position == ecad_model::CorePosition::Unplaced && !m.is_spare();
    let at = label
        .as_ref()
        .and_then(|l| cable.cores.iter().position(|m| free(m) && m.label_str().is_some_and(|x| x.trim().eq_ignore_ascii_case(l.as_str().trim()))));
    match at.and_then(|i| cable.cores.get_mut(i)) {
        Some(m) => m.position = position,
        None => cable.cores.push(ecad_model::Core { position, ..ecad_model::Core::unplaced(label) }),
    }
}

/// The core colours of cable `k`, in core order, from its cable type: the type's own list, else
/// its scheme's colours for the cable's core offer (design 072 F37); empty when neither gives any.
pub fn core_colours(doc: &Document, k: CableId) -> Vec<String> {
    let Some(id) = of(doc, k) else { return Vec::new() };
    let own = |get: &dyn Fn(&CableType) -> bool| -> Option<&CableType> {
        [StyleLayer::Project, StyleLayer::Standard]
            .into_iter()
            .filter_map(|l| crate::styles::record(doc, l, ecad_model::StyleCatalogue::Cable, &id).and_then(|r| r.body.cable()))
            .find(|t| get(t))
    };
    if let Some(list) = own(&|t| t.core_colours.is_some()).and_then(|t| t.core_colours.as_ref()) {
        return list.list().iter().map(ToString::to_string).collect();
    }
    let Some(scheme) = own(&|t| t.core_scheme.is_some()).and_then(|t| t.core_scheme) else { return Vec::new() };
    let Some((labels, _, text)) = offer(doc, k) else { return Vec::new() };
    scheme_colours(scheme, labels.len(), offer_has_earth(&text)).map(|v| v.iter().map(ToString::to_string).collect()).unwrap_or_default()
}

/// The `setprop` key word ticking core `<label>` as **Not used** (owner rule, board 2.06):
/// `unused.<label>` = `yes`. Nothing is stored under it: it sets the core's
/// [`ecad_model::Purpose`] (design 072 §4.5). An in-use core that is not connected at both ends is
/// an open end (an error).
pub const UNUSED_PREFIX: &str = "unused.";
/// The longest core label read in an `unused.` key.
pub const MAX_LABEL_CHARS: usize = ecad_model::MAX_CORE_LABEL;

/// The `setprop` key ticking core `label` as not used.
pub fn unused_key(label: &str) -> String {
    format!("{UNUSED_PREFIX}{}", label.trim().chars().take(MAX_LABEL_CHARS).collect::<String>())
}

/// Whether core `label` of cable `k` is ticked **Not used**: a stored core of that label is
/// [`ecad_model::Purpose::Spare`].
pub fn is_unused(doc: &Document, k: CableId, label: &str) -> bool {
    doc.cables.get(&k).is_some_and(|c| c.cores.iter().any(|m| m.is_spare() && m.label_str() == Some(label)))
}

/// The cores of cable `k`, by label: its offer (`4G1.5`: 1, 2, 3, GNYE), then each stored core's
/// label not in it (its place `1`, `2`… when it has none). Read as stored (design 072 §4.5).
pub fn core_labels(doc: &Document, k: CableId) -> Vec<String> {
    let Some(cab) = doc.cables.get(&k) else { return Vec::new() };
    let mut out: Vec<String> = offer(doc, k).map(|(o, _, _)| o).unwrap_or_default();
    for (i, m) in cab.cores.iter().enumerate().take(crate::cables::slash::MAX_CABLE_MEMBERS) {
        let l = m.label_str().map(str::trim).filter(|l| !l.is_empty()).map_or_else(|| (i + 1).to_string(), str::to_string);
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&l)) {
            out.push(l);
        }
    }
    out
}

/// The ops of `setprop unused.<core>` on cable `k` (tester re-check 2, item 4): the core is one
/// of the cable's ([`core_labels`]), named by 1 to [`MAX_LABEL_CHARS`] characters with no control
/// characters; the value is `yes` (ticked) or `no` / null (not ticked). Ticking sets the purpose
/// of the stored core of that label to Spare, storing the core unplaced when no line carries it
/// yet; unticking sets it back to InUse, and an unplaced core stored only for that goes. Empty
/// when nothing changes.
///
/// # Errors
/// In plain words, bounded: the core is not the cable's, or the value is not yes or no.
pub fn unused_ops(doc: &Document, k: CableId, key: &str, value: Option<&str>) -> Result<Vec<ecad_model::Op>, CmdError> {
    use ecad_model::{Core, CoreLabel, CorePosition, Op, Purpose};
    let label = key.strip_prefix(UNUSED_PREFIX).unwrap_or_default();
    let tick = match value.map(str::trim) {
        None | Some("") => false,
        Some(v) => ecad_model::naming::words::flag(v).ok_or_else(|| CmdError::refused("Not used is yes or no (or null to clear it)."))?,
    };
    let Ok(text) = CoreLabel::new(label).map_err(|_| ()).and_then(|t| if t.as_str() == label { Ok(t) } else { Err(()) }) else {
        return Err(CmdError::refused(format!(
            "Not used names a core of the cable: unused.<core>, the core 1 to {MAX_LABEL_CHARS} characters with no control characters."
        )));
    };
    let cab = doc.cables.get(&k).ok_or_else(|| CmdError::refused("No cable with that ID."))?;
    // A core with no label is named by its place (`core_labels`): ticking it labels it so.
    let by_place = label.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).filter(|i| cab.cores.get(*i).is_some_and(|m| m.label.is_none()));
    let stored = cab.cores.iter().position(|m| m.label.as_ref() == Some(&text)).or(by_place.filter(|_| tick));
    if stored.is_none() && tick {
        let cores = core_labels(doc, k);
        if !cores.iter().any(|c| c == label) {
            let shown: Vec<&str> = cores.iter().take(12).map(String::as_str).collect();
            let more = if cores.len() > shown.len() { ", …" } else { "" };
            let list = if cores.is_empty() { "none".to_string() } else { format!("{}{more}", shown.join(", ")) };
            return Err(CmdError::refused(format!("This cable has no core {label}; its cores are {list}.")));
        }
    }
    let purpose = if tick { Purpose::Spare } else { Purpose::InUse };
    let offered = offer(doc, k).is_some_and(|(o, _, _)| o.iter().any(|x| x.eq_ignore_ascii_case(label)));
    let mut after = cab.clone();
    match stored {
        Some(i) if after.cores.get(i).is_some_and(|m| m.purpose == purpose) => return Ok(Vec::new()),
        // An unplaced core stored only for its tick goes; an offered one stays, in use again.
        Some(i) if !tick && !offered && after.cores.get(i).is_some_and(|m| m.position == CorePosition::Unplaced) => {
            after.cores.remove(i);
        }
        Some(i) => {
            if let Some(m) = after.cores.get_mut(i) {
                m.purpose = purpose;
                m.label.get_or_insert(text);
            }
        }
        None if tick => after.cores.push(Core { purpose, ..Core::unplaced(Some(text)) }),
        None => return Ok(Vec::new()),
    }
    Ok(Op::modify_cable(k, cab, after).into_iter().collect())
}

/// A stored core on no line (neither a wire nor a loose line, design 072 §4.2) and in use (not
/// ticked Not used). (A wire always runs between two connection points, so a core with a wire is
/// never open at an end.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCore {
    pub label: String,
}

impl OpenCore {
    /// The check's message: `Core 3 is open at an end: connect it, or tick Not used.`
    pub fn message(&self) -> String {
        format!("Core {} is open at an end: connect it, or tick Not used.", self.label)
    }
}

/// The open cores of cable `k` (owner rule): its stored in-use cores on no line, in core order.
/// Read as stored (design 072 §4.5); bounded by the cable's member bound.
pub fn open_cores(doc: &Document, k: CableId) -> Vec<OpenCore> {
    let Some(cab) = doc.cables.get(&k) else { return Vec::new() };
    cab.cores
        .iter()
        .take(crate::cables::slash::MAX_CABLE_MEMBERS)
        .filter(|m| m.position == ecad_model::CorePosition::Unplaced && !m.is_spare())
        .filter_map(|m| m.label_str().map(|l| OpenCore { label: l.to_string() }))
        .collect()
}

/// A cable's core size: its cable type's cross-section, else its type notation's (`4G1.5` → 1.5).
pub fn size(doc: &Document, k: CableId) -> Option<String> {
    let from_type = of(doc, k)
        .and_then(|id| crate::styles::field_of(doc, ecad_model::StyleCatalogue::Cable, &id, "size", None))
        .and_then(|r| crate::devices::ratings::parse::cross_section(&r.text()));
    from_type
        .or_else(|| doc.cables.get(&k).and_then(|x| x.properties.get("type")).and_then(|t| crate::terminal_strips::cores::notation_size(t)))
        .map(crate::devices::ratings::dataset::fmt_num)
}

/// The cables that name cable type `id`, through the style-choice index.
/// A catalogue edit of the type's cores syncs none of their stored cores yet (design 072 §4.5):
/// they follow when the cable's own `cores`, `type` or `cable_type` is next set.
pub fn users(doc: &Document, id: &StyleRef) -> Vec<CableId> {
    doc.style_users(CABLE_TYPE, &id.text())
        .filter_map(|o| match o {
            ecad_model::ObjectRef::Cable(k) => Some(k),
            _ => None,
        })
        .collect()
}

/// The value of field `key` that `t` sets, if it sets one.
pub fn get<'a>(t: &'a CableType, key: &str) -> Option<Value<'a>> {
    let r = &t.rating;
    let text = |v: Option<&'a str>| v.map(Value::Text);
    match key {
        "size" => t.size.map(Value::Size),
        "material" => r.material.map(|v| Value::Choice(v.word())),
        "max_temperature" => r.max_temperature.map(|v| Value::Number(v.0)),
        "conductor_class" => r.conductor_class.map(|v| Value::Choice(v.word())),
        "rated_current" => r.rated_current.map(|v| Value::Number(v.0)),
        "rated_at" => r.rated_at.map(|v| Value::Number(v.0)),
        CORE_COLOURS => t.core_colours.as_ref().map(Value::CoreColours),
        CORE_SCHEME => t.core_scheme.map(|v| Value::Choice(v.word())),
        "part_number" => text(t.part_number.as_ref().map(|v| v.as_str())),
        "maker" => text(t.maker.as_ref().map(|v| v.as_str())),
        "description" => text(t.description.as_ref().map(|v| v.as_str())),
        TYPE_CORES => text(t.cores.as_ref().map(|v| v.as_str())),
        TYPE_SHIELD => t.shield.map(|v| Value::Choice(v.word())),
        TYPE_GLAND => text(t.gland.as_ref().map(|v| v.as_str())),
        TYPE_OUTER_DIAMETER => t.outer_diameter.map(|v| Value::Number(v.0)),
        _ => None,
    }
}

fn text_of<const N: usize>(key: &str, v: &str) -> Result<ecad_model::naming::text::Text<N>, CmdError> {
    ecad_model::naming::text::Text::new(v).map_err(|e| CmdError::refused(format!("{key}: {e}")))
}

/// Set (`Some`) or clear (`None`) field `key` of `t` from its text, as the `style` command
/// checked it.
///
/// # Errors
/// An unknown field, or text the field does not take, in words.
pub fn set(t: &mut CableType, key: &str, value: Option<&str>) -> Result<(), CmdError> {
    let bad = |what: &str| CmdError::refused(format!("{key} takes {what}, not {:?}.", value.unwrap_or_default()));
    let number = |v: &str| v.trim().parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| bad("a number"));
    let r = &mut t.rating;
    match (key, value) {
        ("size", None) => t.size = None,
        ("material", None) => r.material = None,
        ("max_temperature", None) => r.max_temperature = None,
        ("conductor_class", None) => r.conductor_class = None,
        ("rated_current", None) => r.rated_current = None,
        ("rated_at", None) => r.rated_at = None,
        (CORE_COLOURS, None) => t.core_colours = None,
        (CORE_SCHEME, None) => t.core_scheme = None,
        ("part_number", None) => t.part_number = None,
        ("maker", None) => t.maker = None,
        ("description", None) => t.description = None,
        (TYPE_CORES, None) => t.cores = None,
        (TYPE_SHIELD, None) => t.shield = None,
        (TYPE_GLAND, None) => t.gland = None,
        (TYPE_OUTER_DIAMETER, None) => t.outer_diameter = None,
        ("size", Some(v)) => t.size = Some(WireSize::parse(v).ok_or_else(|| bad("a wire size"))?),
        ("material", Some(v)) => r.material = Some(Word::parse(v).ok_or_else(|| bad("cu"))?),
        ("max_temperature", Some(v)) => r.max_temperature = Some(Celsius(number(v)?)),
        ("conductor_class", Some(v)) => r.conductor_class = Some(Word::parse(v).ok_or_else(|| bad("1, 2, 5 or 6"))?),
        ("rated_current", Some(v)) => r.rated_current = Some(Amps(crate::devices::ratings::parse::current(v).ok_or_else(|| bad("a current"))?)),
        ("rated_at", Some(v)) => r.rated_at = Some(Celsius(number(v)?)),
        (CORE_COLOURS, Some(v)) => t.core_colours = Some(CoreColours::parse(v).ok_or_else(|| bad("a list of core colours"))?),
        (CORE_SCHEME, Some(v)) => t.core_scheme = Some(Word::parse(v).ok_or_else(|| bad(&<CoreScheme as Word>::WORDS.join(", ")))?),
        ("part_number", Some(v)) => t.part_number = Some(text_of(key, v)?),
        ("maker", Some(v)) => t.maker = Some(text_of(key, v)?),
        ("description", Some(v)) => t.description = Some(text_of(key, v)?),
        (TYPE_CORES, Some(v)) => {
            crate::terminal_strips::cores::parse_cores(v)?;
            t.cores = Some(text_of(key, v)?);
        }
        (TYPE_SHIELD, Some(v)) => t.shield = Some(Word::parse(v).ok_or_else(|| bad(&SHIELDS.join(", ")))?),
        (TYPE_GLAND, Some(v)) => t.gland = Some(text_of(key, v)?),
        (TYPE_OUTER_DIAMETER, Some(v)) => t.outer_diameter = Some(Mm(outer_diameter(v).ok_or_else(|| bad("1 to 200 mm"))?)),
        _ => return Err(CmdError::refused(format!("Cable types have no field {key:?}."))),
    }
    Ok(())
}

// ---------------------------------------------------------------- core colour schemes (F37)

/// The built-in core colour schemes' data.
pub const SCHEMES: &str = include_str!("../../defaults/core_schemes.toml");
/// Its name, for messages.
pub const SCHEMES_FILE: &str = "core_schemes.toml";

/// One scheme's data (see the file's own description).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeData {
    #[serde(default)]
    pub with_earth: BTreeMap<String, Vec<Colours>>,
    #[serde(default)]
    pub without_earth: BTreeMap<String, Vec<Colours>>,
    #[serde(default)]
    pub sequence: Vec<Colours>,
    #[serde(default)]
    pub core: Option<Colours>,
    #[serde(default)]
    pub earth: Option<Colours>,
    #[serde(default)]
    pub then: Option<CoreScheme>,
}

/// Read the built-in schemes.
///
/// # Errors
/// The data's first problem ([`SCHEMES_FILE`]'s key, value or syntax).
pub fn read_schemes(text: &str) -> Result<BTreeMap<CoreScheme, SchemeData>, toml::de::Error> {
    toml::from_str(text)
}

fn schemes() -> &'static BTreeMap<CoreScheme, SchemeData> {
    static READ: OnceLock<BTreeMap<CoreScheme, SchemeData>> = OnceLock::new();
    // A damaged built-in file gives no scheme colours; `core_schemes_data_reads` keeps it from
    // shipping.
    READ.get_or_init(|| read_schemes(SCHEMES).unwrap_or_default())
}

/// The colours scheme `scheme` gives a cable of `n` cores, with an earth core (`G`) or without
/// (`X`); `None` when the scheme (and the ones it hands on to) does not cover `n`.
pub fn scheme_colours(scheme: CoreScheme, n: usize, earth: bool) -> Option<Vec<Colours>> {
    scheme_colours_in(schemes(), scheme, n, earth, 0)
}

fn scheme_colours_in(all: &BTreeMap<CoreScheme, SchemeData>, scheme: CoreScheme, n: usize, earth: bool, depth: usize) -> Option<Vec<Colours>> {
    let d = all.get(&scheme)?;
    let then = || d.then.filter(|_| depth < <CoreScheme as Word>::ALL.len()).and_then(|t| scheme_colours_in(all, t, n, earth, depth + 1));
    let table = if earth { &d.with_earth } else { &d.without_earth };
    if let Some(v) = table.get(&n.to_string()) {
        return (v.len() == n).then(|| v.clone());
    }
    let body = if earth { n.checked_sub(1)? } else { n };
    let mut out: Vec<Colours> = if let Some(core) = &d.core {
        vec![core.clone(); body]
    } else if !d.sequence.is_empty() && body <= d.sequence.len() {
        d.sequence.iter().take(body).cloned().collect()
    } else {
        return then();
    };
    if earth {
        match &d.earth {
            Some(e) => out.push(e.clone()),
            None => return then(),
        }
    }
    Some(out)
}

/// Whether a core offer's text has an earth core: `4G`, `4G1.5` (`X`, or a list of labels, has
/// none).
fn offer_has_earth(text: &str) -> bool {
    text.trim().trim_start_matches(|c: char| c.is_ascii_digit()).starts_with(['G', 'g'])
}

#[cfg(test)]
mod scheme_tests {
    use super::*;

    fn codes(v: Option<Vec<Colours>>) -> Vec<String> {
        v.unwrap_or_default().iter().map(ToString::to_string).collect()
    }

    /// Design 072 F37: the schemes read as typed data, every scheme has data, and a misspelt key
    /// or a colour in another form is refused.
    #[test]
    fn core_schemes_data_reads() {
        let all = read_schemes(SCHEMES).unwrap();
        for s in <CoreScheme as Word>::ALL {
            assert!(all.contains_key(s), "{} has no data in {SCHEMES_FILE}", s.word());
        }
        for (s, d) in &all {
            for (n, v) in d.with_earth.iter().chain(&d.without_earth) {
                assert_eq!(n.parse::<usize>().ok(), Some(v.len()), "{}: {n} cores", s.word());
            }
        }
        assert!(read_schemes("[numbered]\ncolour = \"BK\"\n").is_err());
        assert!(read_schemes("[numbered]\ncore = \"GNYE\"\n").is_err());
        assert!(read_schemes("[rainbow]\ncore = \"BK\"\n").is_err());
    }

    #[test]
    fn a_scheme_gives_colours_by_core_count() {
        assert_eq!(codes(scheme_colours(CoreScheme::Harmonised, 5, true)), ["BN", "BK", "GY", "BU", "GN/YE"]);
        assert_eq!(codes(scheme_colours(CoreScheme::Harmonised, 3, false)), ["BN", "BK", "GY"]);
        // Six cores and up: numbered black cores and the earth.
        assert_eq!(codes(scheme_colours(CoreScheme::Harmonised, 7, true)), ["BK", "BK", "BK", "BK", "BK", "BK", "GN/YE"]);
        assert_eq!(codes(scheme_colours(CoreScheme::Din47100, 4, false)), ["WH", "BN", "GN", "YE"]);
        assert_eq!(codes(scheme_colours(CoreScheme::IceaK2, 3, true)), ["BK", "RD", "GN/YE"]);
        assert_eq!(scheme_colours(CoreScheme::Din47100, 61, false).map(|v| v.len()), Some(61));
        assert!(offer_has_earth("4G1.5") && offer_has_earth("3g") && !offer_has_earth("4X") && !offer_has_earth("BN,BK"));
    }
}
