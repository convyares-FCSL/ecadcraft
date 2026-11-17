//! Lane 072 D's rows of `setprop` (design 072 §4.2, brief 072 "Lanes"): a wire's keys. The keys
//! that became [`ecad_model::Wire`] fields build one `ModifyWire`; the wire number and the
//! potential belong to the whole net and keep their own commands; a wire's other keys are its
//! display choices and defaults-registry values, or refused.

use ecad_model::wire::{Colours, WireSize};
use ecad_model::{Document, ObjectRef, Op, Wire, WireId};

use super::super::prop::{Build, PropRule};
use crate::defaults::{self, Level};
use crate::error::{CmdError, msg};

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

/// This lane's rows, asked before [`super::super::prop::PROP_RULES`].
pub static RULES: &[PropRule] = &[
    PropRule { what: "wire number and potential (brief 022)", claims: |_, o, k| is_wire(o) && is_net_key(k), build: Build::Value(net_key) },
    PropRule { what: "wire size and colours (072 D)", claims: |_, o, k| is_wire(o) && (k == SIZE || k == COLOURS), build: Build::Record(wire_value) },
    PropRule {
        what: "wire rating inputs (brief 057, 072 D)",
        claims: |_, o, k| is_wire(o) && rating_key(k).is_some(),
        build: Build::Record(wire_rating),
    },
    PropRule { what: "wire display choices and registry values (072 §9.2)", claims: |_, o, _| is_wire(o), build: Build::Value(wire_display) },
];

fn is_wire(o: ObjectRef) -> bool {
    matches!(o, ObjectRef::Wire(_))
}

fn is_net_key(k: &str) -> bool {
    k == ecad_model::wiring::WIRE_NUMBER_KEY || k == ecad_model::connectivity::WIRE_NUMBER_FIXED || k == ecad_model::wiring::POTENTIAL_KEY
}

fn net_key(_: &Document, _: ObjectRef, key: &str, _: Option<&str>) -> Result<Option<String>, CmdError> {
    Err(CmdError::refused(if key == ecad_model::wiring::POTENTIAL_KEY { msg::USE_POTENTIAL } else { msg::USE_WIRENUMBER }))
}

/// The `rating` key a `setprop` key on a wire names: `basis` or `rating.basis`, `load`,
/// `protected_by`.
fn rating_key(key: &str) -> Option<&'static str> {
    let k = key.strip_prefix("rating.").unwrap_or(key);
    super::super::rating::KEYS.iter().find(|r| r.0 == k && r.2).map(|r| r.0)
}

fn wire_of(doc: &Document, o: ObjectRef) -> Result<(WireId, &Wire), CmdError> {
    match o {
        ObjectRef::Wire(c) => doc.wires.get(&c).map(|w| (c, w)).ok_or(CmdError::refused(super::super::prop::NO_SUCH_OBJECT)),
        _ => Err(CmdError::refused(super::super::prop::NO_SUCH_OBJECT)),
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

/// A wire's rating input, checked as `rating` checks it.
///
/// # Errors
/// The value's range, in words ([`super::super::rating::check_value`]).
pub fn wire_rating(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, w) = wire_of(doc, o)?;
    let key = rating_key(key).ok_or_else(|| no_such_key(key))?;
    let checked = match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => None,
        // A device named by hand is kept as typed (Checks says when it names nothing) until lane
        // D step 3 stores the device itself.
        Some(v) if key == "protected_by" => Some(v.to_string()),
        Some(v) => Some(super::super::rating::check_value(doc, key, v)?),
    };
    let mut after = w.clone();
    super::super::rating::set_wire(&mut after, key, checked.as_deref());
    Ok(modify(c, w, after))
}

/// A wire's display choice or defaults-registry value (`wire.category`): checked by its registry
/// entry; any other key is refused.
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
