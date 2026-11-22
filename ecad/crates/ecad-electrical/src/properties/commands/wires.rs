//! Lane 072 D's rows of `setprop` (design 072 §4.2, brief 072 "Lanes"): a wire's and a cable's
//! keys. The keys that became [`ecad_model::Wire`] fields build one `ModifyWire`; the wire number
//! and the potential belong to the whole net and keep their own commands; a wire's other keys are
//! its display choices and defaults-registry values, or refused. A cable is written whole: every
//! key but its tag and fixed flag (the tag rows' conflicts) builds one `ModifyCable`.

use ecad_model::naming::tag::{FIXED_KEY, TAG_KEY};
use ecad_model::naming::words::Word;
use ecad_model::objects::wire::{Colours, WireSize};
use ecad_model::{Document, ObjectRef, Op, Wire, WireId};

use crate::defaults::{self, Level};
use crate::error::{CmdError, msg};
use crate::properties::commands::{Build, PropRule};

/// The `setprop` key of a wire's size.
pub const SIZE: &str = "size";
/// The `setprop` key of a wire's colours.
pub const COLOURS: &str = "colours";

/// The refusal of a key a wire does not have.
pub fn no_such_key(key: &str) -> CmdError {
    let shown = crate::cables::clip(key);
    CmdError::refused(format!(
        "A wire has no property {shown:?}: its values are size, colours and the rating inputs (`rating`); its number and potential belong to its net (`wirenumber`, `potential`)."
    ))
}

/// This lane's rows, asked before [`crate::properties::commands::PROP_RULES`].
pub static RULES: &[PropRule] = &[
    PropRule { what: "wire number and potential (brief 022)", claims: |_, o, k| is_wire(o) && is_net_key(k), build: Build::Value(net_key) },
    PropRule { what: "wire size and colours (072 D)", claims: |_, o, k| is_wire(o) && (k == SIZE || k == COLOURS), build: Build::Record(wire_value) },
    PropRule {
        what: "wire rating inputs (brief 057, 072 D)",
        claims: |_, o, k| is_wire(o) && rating_key(k).is_some(),
        build: Build::Record(wire_rating),
    },
    PropRule {
        what: "wire type (072 §4.6)",
        claims: |_, o, k| is_wire(o) && k == ecad_model::connectivity::wiring::WIRE_CATEGORY,
        build: Build::Record(wire_type),
    },
    PropRule { what: "wire display choices and registry values (072 §9.2)", claims: |_, o, _| is_wire(o), build: Build::Value(wire_display) },
    PropRule {
        what: "cable values, cores and Not used (059, 072 D)",
        claims: |_, o, k| matches!(o, ObjectRef::Cable(_)) && k != TAG_KEY && k != FIXED_KEY,
        build: Build::Record(cable_value),
    },
];

/// A cable's value by `setprop` (design 072 §4.5): one `ModifyCable`. `cores` stores the cores it
/// offers, `unused.<label>` a core's purpose, `shield` and `cable_type` the typed fields; any other
/// key is stored as typed.
///
/// # Errors
/// A cable that is not there; a bad offer; a shield that is not a word of [`ecad_model::objects::styles::cable::Shield`];
/// an `unused.` key naming no core.
pub fn cable_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let ObjectRef::Cable(k) = o else { return Err(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT)) };
    let cab = doc.cables.get(&k).ok_or(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT))?;
    if key.starts_with(crate::styles::cable::UNUSED_PREFIX) {
        return Ok(crate::styles::cable::unused_ops(doc, k, key, value)?.into_iter().next());
    }
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    let mut after = cab.clone();
    if let Some(r) = cable_rating_key(key) {
        let checked = value.map(|v| crate::circuits::commands::check_value(doc, r, v)).transpose()?;
        crate::circuits::commands::set_cable(doc, &mut after, r, checked.as_deref());
        return Ok(Op::modify_cable(k, cab, after));
    }
    // Design 072 §4.5: the cable type is a field; its cores follow it. A type's name or ID; an ID
    // naming no type is kept, as wire types are (a check row).
    if key == crate::styles::cable::CABLE_TYPE {
        after.cable_type = match value {
            None => None,
            Some(v) => Some(
                crate::styles::find(doc, ecad_model::StyleCatalogue::Cable, v)
                    .ok()
                    .or_else(|| ecad_model::StyleRef::parse(v))
                    .ok_or_else(|| CmdError::refused(format!("No cable type {:?} in this project's standards.", crate::cables::clip(v))))?,
            ),
        };
        crate::styles::cable::sync_offered_cores(doc, cab, &mut after);
        return Ok(Op::modify_cable(k, cab, after));
    }
    if key == crate::cables::CableField::Shield.key() {
        after.shield = match value {
            None => None,
            Some(v) => Some(<ecad_model::objects::styles::cable::Shield as Word>::parse(v).ok_or_else(|| {
                CmdError::refused(format!("{:?} is not a shield: give none, overall, pairs or overall+pairs.", crate::cables::clip(v)))
            })?),
        };
        return Ok(Op::modify_cable(k, cab, after));
    }
    let stored = if key == crate::terminal_strips::CORES_KEY { crate::terminal_strips::cores_value(value)? } else { value.map(str::to_string) };
    // The description is a field; any other key is a property.
    after.set_value(key, stored.as_deref()).map_err(|e| CmdError::refused(e.to_string()))?;
    if key == crate::terminal_strips::CORES_KEY || key == crate::cables::CableField::Type.key() {
        crate::styles::cable::sync_offered_cores(doc, cab, &mut after);
    }
    Ok(Op::modify_cable(k, cab, after))
}

/// The `rating` key a `setprop` key on a cable names: `load`, or a rating input by its stored key
/// (`rating.basis`), `protected_by`; the cable type has its own row (its cores follow it).
fn cable_rating_key(key: &str) -> Option<&'static str> {
    crate::circuits::commands::KEYS.iter().find(|r| r.3 && r.1 == key && r.0 != "cable_type").map(|r| r.0)
}

fn is_wire(o: ObjectRef) -> bool {
    matches!(o, ObjectRef::Wire(_))
}

fn is_net_key(k: &str) -> bool {
    k == ecad_model::connectivity::wiring::WIRE_NUMBER_KEY
        || k == ecad_model::connectivity::WIRE_NUMBER_FIXED
        || k == ecad_model::connectivity::wiring::POTENTIAL_KEY
}

fn net_key(_: &Document, _: ObjectRef, key: &str, _: Option<&str>) -> Result<Option<String>, CmdError> {
    Err(CmdError::refused(if key == ecad_model::connectivity::wiring::POTENTIAL_KEY { msg::USE_POTENTIAL } else { msg::USE_WIRENUMBER }))
}

/// The `rating` key a `setprop` key on a wire names: `basis` or `rating.basis`, `load`,
/// `protected_by`.
fn rating_key(key: &str) -> Option<&'static str> {
    let k = key.strip_prefix("rating.").unwrap_or(key);
    crate::circuits::commands::KEYS.iter().find(|r| r.0 == k && r.2).map(|r| r.0)
}

fn wire_of(doc: &Document, o: ObjectRef) -> Result<(WireId, &Wire), CmdError> {
    match o {
        ObjectRef::Wire(c) => doc.wires.get(&c).map(|w| (c, w)).ok_or(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT)),
        _ => Err(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT)),
    }
}

/// The op setting `after` on wire `c`, `None` when nothing changes.
fn modify(c: WireId, before: &Wire, after: Wire) -> Option<Op> {
    (after != *before).then(|| Op::ModifyWire { id: c, before: before.clone(), after })
}

/// A wire's size or colours, checked.
///
/// # Errors
/// A value that is not a size (`2.5`, `18 AWG`) or a colour list (`BU`, `GN/YE`).
pub fn wire_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, w) = wire_of(doc, o)?;
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    let mut after = w.clone();
    if key == SIZE {
        after.size = match value {
            None => None,
            Some(v) => Some(WireSize::parse(v).ok_or_else(|| {
                CmdError::refused(format!("{:?} is not a wire size: give mm\u{b2} (2.5) or a gauge (18 AWG).", crate::cables::clip(v)))
            })?),
        };
    } else {
        after.colours = match value {
            None => None,
            Some(v) => Some(Colours::parse(v).ok_or_else(|| {
                CmdError::refused(format!(
                    "{:?} is not a colour list: give 1 to 3 IEC 60757 colours (BU, GN/YE, BLK, Brown).",
                    crate::cables::clip(v)
                ))
            })?),
        };
    }
    Ok(modify(c, w, after))
}

/// A wire's own wire type (design 072 §4.6): a style reference of catalogue `wire`, or nothing
/// to clear it. A reference to a type that does not exist is kept and shown as a check row.
///
/// # Errors
/// A value that is not a style reference (a built-in word or `#` and 32 hex digits).
pub fn wire_type(doc: &Document, o: ObjectRef, _: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, w) = wire_of(doc, o)?;
    let mut after = w.clone();
    after.wire_type = match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => None,
        Some(v) => {
            Some(ecad_model::StyleRef::parse(v).ok_or_else(|| CmdError::refused(format!("{:?} is not a wire type's ID.", crate::cables::clip(v))))?)
        }
    };
    Ok(modify(c, w, after))
}

/// A wire's rating input, checked as `rating` checks it.
///
/// # Errors
/// The value's range, in words ([`crate::circuits::commands::check_value`]).
pub fn wire_rating(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, w) = wire_of(doc, o)?;
    let key = rating_key(key).ok_or_else(|| no_such_key(key))?;
    let checked = match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => None,
        Some(v) => Some(crate::circuits::commands::check_value(doc, key, v)?),
    };
    let mut after = w.clone();
    crate::circuits::commands::set_wire(doc, &mut after, key, checked.as_deref());
    Ok(modify(c, w, after))
}

/// A wire's display choice or defaults-registry value: checked by its registry entry; any other
/// key is refused.
///
/// # Errors
/// [`no_such_key`]; a value the entry does not take.
pub fn wire_display(_: &Document, _: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let entry = defaults::find(key).filter(|e| e.allows(Level::Object)).ok_or_else(|| no_such_key(key))?;
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) if entry.parse(v).is_some() => Ok(Some(v.to_string())),
        Some(v) => Err(CmdError::refused(format!("{:?} is not a value of {}.", crate::cables::clip(v), entry.label))),
    }
}
