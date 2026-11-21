//! The rules of `setprop` (brief 070 item 3; design 018 §6.1, 026 §4 and §7, 052 §5.7; brief 060):
//! [`PROP_RULES`], one table saying, per property key, which rule checks the value and builds the
//! change. `setprop` on one object and on several devices (`devices`) both walk it
//! ([`rule_for`]), so the two paths cannot drift apart.
//!
//! Excludes: reading the JSON, locking the project, committing and answering (the engine crate's
//! `setprop` shim), and the function and location codes themselves ([`super::location`]).

use ecad_model::tag::{self, FIXED_KEY, TAG_KEY};
use ecad_model::{CableId, DeviceId, Document, IdAllocator, ObjectRef, Op, Properties, Subject};
use serde_json::{Value, json};

use crate::devices::{self, Holder, TagHigh};
use crate::error::CmdError;

/// The refusal of `tag` on several devices at once.
pub const ONE_TAG_PER_DEVICE: &str = "Tags are one per device.";

/// The refusal of fixing a provisional tag (design 026 §4).
pub const PROVISIONAL_FIXED: &str = "A provisional tag can't be fixed; give it a number first.";

/// The refusal of an object the input names that the project does not have.
pub const NO_SUCH_OBJECT: &str = "no such object";

/// A row's check: the value to store for `key` on `object` (`None` clears it), or the refusal.
pub type Check = fn(&Document, ObjectRef, &str, Option<&str>) -> Result<Option<String>, CmdError>;

/// A row's plan for a key that became a struct field (design 072): the `Modify*` op recording the
/// whole record (`None`: unchanged), or the refusal.
pub type Plan = fn(&Document, ObjectRef, &str, Option<&str>) -> Result<Option<Op>, CmdError>;

/// How a row of [`PROP_RULES`] builds the change.
#[derive(Clone, Copy)]
pub enum Build {
    /// A function or location code of the project's list (brief 052): [`super::location::plan_set`],
    /// for all the devices at once. It claims by key alone ([`super::location::aspect_property`]).
    Aspect,
    /// The tag (design 026 §4, §7): typing one fixes it, `-K?` takes the next provisional one, a
    /// tag another device or cable holds needs a conflict answer ([`plan_tag`]). One object only.
    Tag,
    /// The tag's fixed flag: "yes" or null ([`plan_fixed`]).
    Fixed,
    /// A device property (design 018 §6.1): checked; a tag another device holds merges the two
    /// with `merge` ([`plan_device`]).
    Device(Check),
    /// A value the check reads (and may store canonically): one `SetProperty` per object
    /// ([`plan_value`]).
    Value(Check),
    /// A key that is a struct field (design 072): one `Modify*` op per object ([`plan_record`]).
    Record(Plan),
}

/// One row of [`PROP_RULES`].
pub struct PropRule {
    /// What the row covers, in words.
    pub what: &'static str,
    /// Whether the row decides `key` on `object`.
    pub claims: fn(&Document, ObjectRef, &str) -> bool,
    /// How it builds the change.
    pub build: Build,
}

/// The last row: a key no rule claims stays free text, as typed.
const FREE_TEXT: PropRule = PropRule { what: "free text", claims: |_, _, _| true, build: Build::Value(free_text) };

/// Every property rule of `setprop`, in the order they are asked: the first row that claims a
/// key on an object decides it. Both the one-object and the several-device paths walk it.
pub static PROP_RULES: &[PropRule] = &[
    PropRule {
        what: "function or location (brief 052)",
        claims: |_, o, k| is_component(o) && super::location::aspect_property(k).is_some(),
        build: Build::Aspect,
    },
    PropRule { what: "tag (design 026)", claims: |_, o, k| k == TAG_KEY && tagged(o), build: Build::Tag },
    PropRule { what: "tag fixed (design 026)", claims: |_, o, k| k == FIXED_KEY && tagged(o), build: Build::Fixed },
    PropRule { what: "device property (design 018)", claims: |_, o, k| is_device_property(o, k), build: Build::Device(device_value) },
    PropRule {
        what: "wire, junction or continuation (briefs 022, 049)",
        claims: |doc, o, k| !matches!(super::wire::setprop_rule(doc, o, k, None), Ok(None)),
        build: Build::Value(wire_value),
    },
    PropRule {
        what: "strip offer (brief 028)",
        claims: |doc, o, k| k == crate::strips::OFFER_KEY && strip(doc, o).is_some(),
        build: Build::Value(offer_value),
    },
    PropRule {
        what: "PE row part (brief 028)",
        claims: |_, o, k| is_component(o) && (k == crate::strips::PE_PART_KEY || k == crate::strips::PE_MAKER_KEY),
        build: Build::Value(pe_value),
    },
    PropRule {
        what: "connector (brief 044)",
        claims: |doc, o, k| component(o).is_some_and(|c| crate::connectors::setprop_value(doc, c, k, None).is_some()),
        build: Build::Value(connector_value),
    },
    PropRule {
        what: "strip terminal defaults (brief 036)",
        claims: |doc, o, k| k.starts_with("terminal.") && strip(doc, o).is_some(),
        build: Build::Value(terminal_default_value),
    },
    PropRule {
        what: "level label (brief 036)",
        claims: |doc, o, k| k.starts_with(crate::terminal_types::LABEL_PREFIX) && strip(doc, o).is_some(),
        build: Build::Value(level_label_value),
    },
    PropRule {
        what: "PLC card (brief 045)",
        claims: |doc, o, k| component(o).is_some_and(|c| crate::plc::setprop_rule(doc, c, k, None).is_some()),
        build: Build::Value(plc_value),
    },
    PropRule { what: "rating (brief 034)", claims: |_, o, k| is_component(o) && crate::symbols::is_rating_key(k), build: Build::Value(rating_value) },
    PropRule {
        what: "parts and items (brief 047)",
        claims: |_, o, k| component(o).is_some() && crate::bom::is_bom_key(k),
        build: Build::Value(bom_value),
    },
    PropRule { what: "strip accessories, or free text (brief 030)", claims: |_, o, _| is_component(o), build: Build::Value(accessory_value) },
    FREE_TEXT,
];

/// The row of [`PROP_RULES`] that decides `key` on `object`.
pub fn rule_for(doc: &Document, object: ObjectRef, key: &str) -> &'static PropRule {
    super::props::LANES.iter().flat_map(|(_, rows)| rows.iter()).chain(PROP_RULES).find(|r| (r.claims)(doc, object, key)).unwrap_or(&FREE_TEXT)
}

/// `setprop` of a `Record` row on one object: its `Modify*` op, `None` when unchanged.
///
/// # Errors
/// The row's refusal; a row of another kind.
pub fn plan_record(doc: &Document, object: ObjectRef, key: &str, typed: Option<&str>) -> Result<Option<Op>, CmdError> {
    match rule_for(doc, object, key).build {
        Build::Record(plan) => plan(doc, object, key, typed),
        Build::Aspect | Build::Tag | Build::Fixed | Build::Device(_) | Build::Value(_) => {
            Err(CmdError::input("that property is set through its own rule"))
        }
    }
}

fn is_component(o: ObjectRef) -> bool {
    matches!(o, ObjectRef::Device(_))
}

fn component(o: ObjectRef) -> Option<DeviceId> {
    match o {
        ObjectRef::Device(c) => Some(c),
        _ => None,
    }
}

fn strip(doc: &Document, o: ObjectRef) -> Option<DeviceId> {
    component(o).filter(|c| crate::strips::strip_of(doc, *c).is_some())
}

/// Whether `o` takes a tag: a device or a cable.
fn tagged(o: ObjectRef) -> bool {
    matches!(o, ObjectRef::Device(_) | ObjectRef::Cable(_))
}

/// Whether `key` on `object` is one of design 018 §6.1's device properties.
pub fn is_device_property(object: ObjectRef, key: &str) -> bool {
    is_component(object) && devices::PROPERTIES.iter().any(|(k, _)| *k == key)
}

fn free_text(_: &Document, _: ObjectRef, _: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    Ok(value.map(str::to_string))
}

fn device_value(doc: &Document, _: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    devices::property_value(doc, key, value)
}

fn wire_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    Ok(super::wire::setprop_rule(doc, o, key, value)?.unwrap_or_else(|| value.map(str::to_string)))
}

fn offer_value(_: &Document, _: ObjectRef, _: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    crate::strips::offer_value(value)
}

fn pe_value(doc: &Document, _: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    devices::property_value(doc, key, value)
}

fn connector_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    match component(o).and_then(|c| crate::connectors::setprop_value(doc, c, key, value)) {
        Some(r) => r,
        None => Ok(value.map(str::to_string)),
    }
}

fn terminal_default_value(_: &Document, _: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    match crate::defaults::terminals::setprop_value(key, value) {
        Some(r) => r,
        None => Ok(value.map(str::to_string)),
    }
}

fn level_label_value(_: &Document, _: ObjectRef, _: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    use crate::terminal_types::MAX_LABEL_CHARS;
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) if v.chars().count() > MAX_LABEL_CHARS || v.chars().any(char::is_control) => {
            Err(CmdError::refused(format!("A level's label is 1 to {MAX_LABEL_CHARS} characters, with no control characters.")))
        }
        Some(v) => Ok(Some(v.to_string())),
    }
}

fn plc_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    match component(o).and_then(|c| crate::plc::setprop_rule(doc, c, key, value)) {
        Some(r) => r,
        None => Ok(None),
    }
}

fn rating_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    crate::symbols::setprop_rule(doc, o, key, value)
}

fn bom_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    match component(o) {
        Some(c) => crate::bom::setprop_rule(doc, c, key, value),
        None => Ok(value.map(str::to_string)),
    }
}

fn accessory_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let rule = match component(o) {
        Some(c) => crate::accessories::setprop_rule(doc, c, key, value)?,
        None => None,
    };
    Ok(rule.unwrap_or_else(|| value.map(str::to_string)))
}

// ---------------------------------------------------------------- the key

/// Refuse a key `setprop` never writes: `text.` keys are written by the text commands only
/// (brief 031, §10.6).
///
/// # Errors
/// [`crate::text_style::RESERVED_KEY`].
pub fn reserved(key: &str) -> Result<(), CmdError> {
    match crate::text_style::is_reserved_key(key) {
        true => Err(CmdError::refused(crate::text_style::RESERVED_KEY)),
        false => Ok(()),
    }
}

/// The key `setprop` writes on component `c` for a typed `key`: on a PLC card with channels,
/// `channels` is its channel count (`plc.channels`, pack re-run bug 7), as `terminals` is a strip's
/// offer; on anything else the key as typed.
pub fn component_key(doc: &Document, c: DeviceId, key: &str) -> String {
    if key.trim() == "channels" && crate::plc::card_type_of(doc, c).is_some_and(|t| t.has_channels()) {
        crate::plc::rack::CHANNELS_KEY.to_string()
    } else {
        key.to_string()
    }
}

/// The op clearing an older build's text size, weight or font on the project (brief 050a, design
/// 050 §3.4: its Checks row goes; it is never written), when `key` is one and the project has it.
pub fn clear_old_key(doc: &Document, key: &str) -> Option<Op> {
    if !crate::styles::checks::is_old_key(key) {
        return None;
    }
    let before = doc.display.get(key).cloned()?;
    Some(Op::SetProperty { object: ObjectRef::Project, key: key.to_string(), before: Some(before), after: None })
}

// ---------------------------------------------------------------- values (Device and Value rows)

/// The value `setprop` stores for `key` on `object` by its row of [`PROP_RULES`] (a `Device` or
/// `Value` row): an item first refuses the keys it cannot take (brief 047, design 047 §3).
///
/// # Errors
/// The rule's refusal; a key decided by an `Aspect` or `Fixed` row is refused (those build their
/// change in [`plan_fixed`] and [`super::location::plan_set`]).
pub fn value(doc: &Document, object: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let check = match rule_for(doc, object, key).build {
        Build::Device(f) | Build::Value(f) => f,
        // A device's tag is a device property too: merging into the device that holds it reads it
        // here ([`plan_device`]).
        Build::Tag => device_value,
        Build::Fixed | Build::Aspect | Build::Record(_) => return Err(CmdError::input("that property is set through its own rule")),
    };
    check(doc, object, key, value)
}

fn props_of(doc: &Document, o: ObjectRef) -> Option<&Properties> {
    match o {
        ObjectRef::Device(c) => doc.devices.get(&c).map(|x| &x.properties),
        ObjectRef::Wire(c) => doc.wires.get(&c).map(|x| &x.display),
        ObjectRef::Cable(c) => doc.cables.get(&c).map(|x| &x.properties),
        ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard => None,
    }
}

/// A `Value` row's change on one object.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueSet {
    /// The op, `None` when the value is already stored.
    pub op: Option<Op>,
    /// What the value meant, when it could be read two ways (brief 036 §5.1: a typed `2.1` is
    /// position 2, level 1).
    pub note: Option<String>,
}

/// `setprop` of a `Value` row on one object.
///
/// # Errors
/// The rule's refusal; [`NO_SUCH_OBJECT`].
pub fn plan_value(doc: &Document, object: ObjectRef, key: &str, typed: Option<&str>) -> Result<ValueSet, CmdError> {
    let after = value(doc, object, key, typed)?;
    let before = props_of(doc, object).ok_or(CmdError::refused(NO_SUCH_OBJECT))?.get(key).cloned();
    if before == after {
        return Ok(ValueSet { op: None, note: None });
    }
    let note = match (object, &after) {
        (ObjectRef::Device(c), Some(_)) if ecad_model::links::link_number(key).is_some() => {
            typed.and_then(|v| crate::terminal_types::dotted_position_words(doc, c, v))
        }
        _ => None,
    };
    Ok(ValueSet { op: Some(Op::SetProperty { object, key: key.to_string(), before, after }), note })
}

/// `setprop {devices: [id…], key, value}`: a property written to several devices in one
/// transaction (design 019 §8.2). Each device walks [`PROP_RULES`] as one would on its own
/// (brief 060): a refusal on any refuses all. A tag is one per device, so `tag` is refused.
///
/// # Errors
/// [`ONE_TAG_PER_DEVICE`], [`NO_SUCH_OBJECT`], or the first rule that refuses.
pub fn plan_many(doc: &Document, cs: &[DeviceId], key: &str, typed: Option<&str>) -> Result<Vec<Op>, CmdError> {
    if key == TAG_KEY {
        return Err(CmdError::refused(ONE_TAG_PER_DEVICE));
    }
    let mut ops = Vec::new();
    for c in cs {
        let comp = doc.devices.get(c).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
        let key = component_key(doc, *c, key);
        let o = ObjectRef::Device(*c);
        match rule_for(doc, o, &key).build {
            Build::Fixed => {
                ops.extend(plan_fixed(doc, o, typed)?.op);
                continue;
            }
            Build::Record(plan) => {
                ops.extend(plan(doc, o, &key, typed)?);
                continue;
            }
            Build::Aspect | Build::Tag | Build::Device(_) | Build::Value(_) => {}
        }
        let after = value(doc, o, &key, typed)?;
        let before = comp.properties.get(&key).cloned();
        if before != after {
            ops.push(Op::SetProperty { object: o, key, before, after });
        }
    }
    Ok(ops)
}

// ---------------------------------------------------------------- device properties (design 018 §6)

/// A device property's change ([`plan_device`]).
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceSet {
    /// The value is already stored (as stored).
    Unchanged(Option<String>),
    /// One op, with its undo label.
    Set { op: Box<Op>, label: &'static str, value: Option<String> },
    /// The device becomes part of `holder` (a tag another device holds, with `merge`).
    Merged { ops: Vec<Op>, holder: DeviceId, into: String, before: Option<String> },
}

/// `setprop {component, key, value, merge?}` of a device property (design 018 §6, owner decisions
/// 2026-11-08): the value rules of §6.1; a tag another device holds is refused with the ways
/// forward, unless `merge`, which makes this device part of that one.
///
/// # Errors
/// [`NO_SUCH_OBJECT`]; the value rule's refusal; a taken tag without `merge`.
pub fn plan_device(
    doc: &Document, ids: &mut IdAllocator, tag_high: &TagHigh, c: DeviceId, key: &str, typed: Option<&str>, merge: bool,
) -> Result<DeviceSet, CmdError> {
    let component = doc.devices.get(&c).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
    let value = value(doc, ObjectRef::Device(c), key, typed)?;
    let before = component.value(key);
    if key == TAG_KEY
        && let Some(t) = &value
        && let Some(holder) = devices::holder_of(doc, t, Some(c))
    {
        if !merge {
            let next =
                devices::tag_of(doc, c).and_then(tag::parse).or_else(|| tag::parse(t)).and_then(|(prefix, _)| tag_high.peek(doc, &prefix).ok());
            let way = match next {
                Some(n) => format!("Set `merge` to true to make this the same device, or choose another tag (the next free one is {n})."),
                None => "Set `merge` to true to make this the same device, or choose another tag.".into(),
            };
            return Err(CmdError::refused(devices::duplicate_words(doc, t, holder, &way)));
        }
        let ops = devices::plan_merge(doc, ids, c, holder)?;
        return Ok(DeviceSet::Merged { ops, holder, into: devices::tag_words(doc, holder), before });
    }
    if before == value {
        return Ok(DeviceSet::Unchanged(value));
    }
    let label = if key == TAG_KEY { "Set tag" } else { "Set property" };
    let op = if key == TAG_KEY {
        let after = ecad_model::Device { tag_fixed: component.tag_fixed && value.is_some(), ..component.with_tag(value.as_deref()) };
        Op::ModifyDevice { id: c, before: component.clone(), after }
    } else {
        Op::SetProperty { object: ObjectRef::Device(c), key: key.to_string(), before, after: value.clone() }
    };
    Ok(DeviceSet::Set { op: Box::new(op), label, value })
}

/// The echo after a merge (with the merged document): `-K2 is now part of -K1 (3 symbols on 1, 2).`
pub fn merged_device_words(doc: &Document, before: Option<&str>, holder: DeviceId, into: &str) -> String {
    format!(
        "{} is now part of {into} ({} symbols on {}).",
        before.unwrap_or("The device"),
        doc.representations_of(Subject::Device(holder)).count(),
        devices::pages_words(doc, holder)
    )
}

// ---------------------------------------------------------------- tags (design 026 §4, §7)

/// How a taken tag is resolved (design 026 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    Swap,
    Merge,
    Next,
}

/// The conflict answer `word` names; `merge: true` is `merge`.
///
/// # Errors
/// Any other word.
pub fn conflict(word: Option<&str>, merge: bool) -> Result<Option<Conflict>, CmdError> {
    let c = match word.map(|w| w.trim().to_ascii_lowercase()) {
        None => None,
        Some(w) => Some(match w.as_str() {
            "swap" => Conflict::Swap,
            "merge" | "same" | "one" => Conflict::Merge,
            "next" | "another" => Conflict::Next,
            other => return Err(CmdError::refused(format!("`conflict` must be \"swap\", \"merge\" or \"next\", not {other:?}"))),
        }),
    };
    Ok(c.or(merge.then_some(Conflict::Merge)))
}

/// The tag `o` holds and whether its fixed flag is stored: a device's fields (design 072 §2.1), a
/// cable's properties. `None` for anything else, or an object that is not there.
fn tag_of(doc: &Document, o: ObjectRef) -> Option<(Option<String>, bool)> {
    match o {
        ObjectRef::Device(c) => doc.devices.get(&c).map(|d| (d.tag().map(str::to_string), d.tag_fixed)),
        ObjectRef::Cable(_) => {
            props_of(doc, o).map(|p| (p.get(TAG_KEY).cloned(), ecad_model::keys::PropsExt::flag(p, ecad_model::keys::TAG_FIXED) == Some(true)))
        }
        _ => None,
    }
}

/// Whether `o`'s tag counts as fixed ([`ecad_model::numbering::is_fixed`]).
fn fixed_of(doc: &Document, o: ObjectRef) -> bool {
    match o {
        ObjectRef::Device(c) => doc.devices.get(&c).is_some_and(ecad_model::Device::is_fixed),
        _ => props_of(doc, o).is_some_and(ecad_model::numbering::is_fixed),
    }
}

/// The op giving `o` tag `tag` and fixed flag `fixed`: one `ModifyDevice` on a device, one
/// `ModifyCable` on a cable.
fn write_tag(doc: &Document, o: ObjectRef, tag: Option<String>, fixed: bool) -> Vec<Op> {
    match o {
        ObjectRef::Device(c) => {
            let Some(d) = doc.devices.get(&c) else { return Vec::new() };
            let after = ecad_model::Device { tag_fixed: fixed, ..d.with_tag(tag.as_deref()) };
            if after == *d { Vec::new() } else { vec![Op::ModifyDevice { id: c, before: d.clone(), after }] }
        }
        ObjectRef::Cable(k) => {
            let mut ops = Vec::new();
            Op::set_cable_property(doc, &mut ops, k, TAG_KEY, tag);
            Op::set_cable_property(doc, &mut ops, k, FIXED_KEY, fixed.then(|| ecad_model::words::flag_word(true).to_string()));
            ops
        }
        ObjectRef::Wire(_) | ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard => Vec::new(),
    }
}

/// The ops writing `tag` on `o` and its fixed flag (`fixed`; a provisional tag is never fixed).
fn tag_ops(doc: &Document, o: ObjectRef, tag_value: Option<String>, fixed: bool) -> Vec<Op> {
    let fixed = fixed && tag_value.as_deref().is_some_and(|t| !tag::is_provisional(t));
    write_tag(doc, o, tag_value, fixed)
}

/// `device` or `cable`.
pub fn noun(o: ObjectRef) -> &'static str {
    match o {
        ObjectRef::Cable(_) => "cable",
        _ => "device",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The refusal of a taken tag with all three ways (design 026 §7.1, 022's wording pattern).
fn taken_words(doc: &Document, tag_high: &TagHigh, o: ObjectRef, t: &str, holder: Holder) -> String {
    let next = tag::prefix_of(t).and_then(|px| tag_high.peek(doc, &px).ok());
    let next = next.map_or_else(|| "another tag".to_string(), |n| n.to_string());
    let same_kind = matches!((o, holder), (ObjectRef::Device(_), Holder::Device(_)) | (ObjectRef::Cable(_), Holder::Cable(_)));
    let ways = if same_kind {
        format!("Give conflict: \"swap\" (exchange the two tags), \"merge\" (make this the same {}) or \"next\" (use {next}).", noun(o))
    } else {
        format!("It is a {}, so only conflict: \"next\" (use {next}) works.", noun(holder.object()))
    };
    format!("{t} is used by {}. {ways}", devices::holder_words_of(doc, holder))
}

/// A tag's change ([`plan_tag`]).
#[derive(Debug, Clone, PartialEq)]
pub enum TagSet {
    /// Nothing to change: the tag it has.
    Unchanged(Option<String>),
    /// The tag cleared (`fixed` is `None`) or set ("Set tag").
    Set { ops: Vec<Op>, value: Option<String>, fixed: Option<bool> },
    /// The two tags exchanged ("Swap tags"); `line` is the echo.
    Swapped { ops: Vec<Op>, value: String, other: ObjectRef, other_value: String, line: String },
    /// Two cables made one: this one's members join `into` ("Merge into …").
    MergedCable { ops: Vec<Op>, into: CableId, tag: String, before: Option<String> },
    /// The next free tag of the prefix taken instead ("Set tag"); `line` is the echo.
    Next { ops: Vec<Op>, value: String, fixed: bool, line: String },
}

/// `setprop {component|cable, key: "tag", value, conflict?}` (design 026 §4, §7): a typed tag is
/// fixed with it (Q4); `-K?` takes the next provisional number; a tag another device or cable
/// holds needs `conflict`. Merging two devices is [`plan_device`]'s (labels, contacts, connection
/// points), so `Conflict::Merge` on a device does not come here. `tag_high` is caught up.
///
/// # Errors
/// [`NO_SUCH_OBJECT`]; a tag that does not read; a connector's half tag; a taken tag without a
/// conflict answer, or with one that does not apply.
pub fn plan_tag(doc: &Document, tag_high: &mut TagHigh, o: ObjectRef, typed: Option<&str>, conflict: Option<Conflict>) -> Result<TagSet, CmdError> {
    let (before, _) = tag_of(doc, o).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
    let value = match typed.map(str::trim).filter(|v| !v.is_empty()) {
        None => None,
        Some(v) => {
            let t = tag::normalise(v, crate::defaults::print::tag_style(doc))?;
            match tag::provisional(&t) {
                Some((px, 0)) => Some(tag_high.next_provisional_in(doc, &px)),
                _ => Some(t),
            }
        }
    };
    let Some(t) = value else {
        let ops = tag_ops(doc, o, None, false);
        if ops.is_empty() {
            return Ok(TagSet::Unchanged(None));
        }
        return Ok(TagSet::Set { ops, value: None, fixed: None });
    };
    let typed_fixed = !tag::is_provisional(&t);
    // Brief 044 (design 044 §5.1): a connector's half tag is not free for another device.
    if let Some(Ok((c, Some(side), None))) = crate::connectors::resolve(doc, &t)
        && ObjectRef::Device(c) != o
    {
        return Err(CmdError::refused(format!("{t} is the {} half of {}. Pick another tag.", side.word(), devices::tag_words(doc, c))));
    }
    let Some(holder) = devices::tag_holder(doc, &t, Some(o)) else {
        let ops = tag_ops(doc, o, Some(t.clone()), typed_fixed);
        if ops.is_empty() {
            return Ok(TagSet::Unchanged(Some(t)));
        }
        return Ok(TagSet::Set { ops, value: Some(t), fixed: Some(typed_fixed) });
    };
    let same_kind = matches!((o, holder), (ObjectRef::Device(_), Holder::Device(_)) | (ObjectRef::Cable(_), Holder::Cable(_)));
    match conflict {
        None => Err(CmdError::refused(taken_words(doc, tag_high, o, &t, holder))),
        Some(Conflict::Swap) => {
            if !same_kind {
                return Err(CmdError::refused(format!("{t} is a {}; swap needs two of a kind. Give conflict: \"next\".", noun(holder.object()))));
            }
            let Some(mine) = before else {
                return Err(CmdError::refused(format!("This {} has no tag to give the other; give conflict: \"next\" or \"merge\".", noun(o))));
            };
            let h = holder.object();
            let h_fixed = fixed_of(doc, h);
            let mut ops = tag_ops(doc, o, Some(t.clone()), typed_fixed);
            ops.extend(tag_ops(doc, h, Some(mine.clone()), h_fixed));
            let other = if tag::is_provisional(&mine) { format!("provisional {mine}") } else { mine.clone() };
            let line = format!("Swapped: this is {t}, the other is now {other}.");
            Ok(TagSet::Swapped { ops, value: t, other: h, other_value: mine, line })
        }
        Some(Conflict::Merge) => {
            // Two cables: the members join the holder (design §7.1); the slash follows.
            let (ObjectRef::Cable(from), Holder::Cable(into)) = (o, holder) else {
                return Err(CmdError::refused(format!(
                    "{t} is a {}; only two of a kind can be one. Give conflict: \"next\".",
                    noun(holder.object())
                )));
            };
            // One join for the merge and for setprop (reviewer 026, 2).
            let ops = ecad_model::merge::join_cables(doc, from, into)?;
            Ok(TagSet::MergedCable { ops, into, tag: t, before })
        }
        Some(Conflict::Next) => {
            let px = tag::prefix_of(&t).ok_or_else(|| CmdError::refused(format!("{t} has no number to follow; type another tag.")))?;
            let next = if tag::is_provisional(&t) { tag_high.next_provisional_in(doc, &px) } else { tag_high.next_in(doc, &px)? };
            let fixed = !tag::is_provisional(&next);
            let ops = tag_ops(doc, o, Some(next.clone()), fixed);
            let line = format!("{} is now {next}.", before.unwrap_or_else(|| format!("The {}", noun(o))));
            Ok(TagSet::Next { ops, value: next, fixed, line })
        }
    }
}

/// The echo after two cables became one (with the merged document): `-W2 is now part of -W1
/// (5 wires).`
pub fn merged_cable_words(doc: &Document, before: Option<&str>, tag: &str, into: CableId) -> String {
    let n = doc.cables.get(&into).map_or(0, |c| c.cores.len());
    format!("{} is now part of {tag} ({}).", before.unwrap_or("The cable"), plural(n, "wire", "wires"))
}

/// The tag's fixed flag ([`plan_fixed`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FixedSet {
    /// Fixed (`true`) or released.
    pub on: bool,
    /// The op, `None` when it already is.
    pub op: Option<Op>,
}

/// `setprop {component|cable, key: "tag_fixed", value: "yes"|null}` (design 026 §4, FP1).
///
/// # Errors
/// A value other than yes or no; [`NO_SUCH_OBJECT`]; fixing no tag, or a provisional one
/// ([`PROVISIONAL_FIXED`]).
pub fn plan_fixed(doc: &Document, o: ObjectRef, typed: Option<&str>) -> Result<FixedSet, CmdError> {
    let on = match typed.map(str::trim) {
        None => false,
        Some("") => false,
        Some(v) => ecad_model::words::flag(v).ok_or_else(|| CmdError::refused(format!("`tag_fixed` is \"yes\" or null, not {v:?}")))?,
    };
    let (tag_now, _) = tag_of(doc, o).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
    if on {
        match &tag_now {
            None => return Err(CmdError::refused(format!("This {} has no tag to fix.", noun(o)))),
            Some(t) if tag::is_provisional(t) => return Err(CmdError::refused(PROVISIONAL_FIXED)),
            Some(_) => {}
        }
    }
    Ok(FixedSet { on, op: write_tag(doc, o, tag_now, on).into_iter().next() })
}

pub fn object_json(o: ObjectRef) -> Value {
    match o {
        ObjectRef::Device(c) => json!({"component": c.hex()}),
        ObjectRef::Cable(k) => json!({"cable": k.hex()}),
        _ => Value::Null,
    }
}
