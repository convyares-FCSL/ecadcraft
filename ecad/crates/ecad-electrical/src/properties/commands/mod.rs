//! The rules of `setprop`:
//! [`PROP_RULES`], one table saying, per property key, which rule checks the value and builds the
//! change. `setprop` on one object and on several devices (`devices`) both walk it
//! ([`rule_for`]), so the two paths cannot drift apart.
//!
//! `setprop` itself is a generic command ([`crate::commands::run`]); a function or location goes
//! through [`crate::locations::commands::locations`]'s set, as `setlocation` does.
//!
//! Excludes: the function and location codes themselves ([`crate::locations::commands::locations`]).

pub mod devices;
pub mod plc;
pub mod wires;
use ecad_model::naming::tag::{self, TAG_KEY, TagSlot, TagState};
use ecad_model::{AidId, CableId, DeviceId, Document, IdAllocator, ObjectRef, Op, Properties, Subject, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cables::words::MAX_HANDLES;
use crate::commands::input::{Bounded, OrNull, nullable};
use crate::commands::run::{Cx, Generic, Outcome, State};
use crate::devices::tags::{Holder, TagHigh};
use crate::error::CmdError;
use crate::locations::commands::locations::{self, MAX_DEVICES};
use crate::locations::designations::Aspect;

/// The refusal of `tag` on several devices at once.
pub(crate) const ONE_TAG_PER_DEVICE: &str = "Tags are one per device.";

/// The refusal of changing a provisional tag's state: Finalise or Renumber gives it
/// its number, and typing `-K?` makes one.
pub(crate) const PROVISIONAL_FIXED: &str = "A provisional tag's state can't be set; give it a number first (Finalise).";

/// The refusal of an object the input names that the project does not have.
pub(crate) const NO_SUCH_OBJECT: &str = "no such object";

/// A row's check: the value to store for `key` on `object` (`None` clears it), or the refusal.
pub type Check = fn(&Document, &ecad_model::registry::data::Registry, ObjectRef, &str, Option<&str>) -> Result<Option<String>, CmdError>;

/// A row's plan for a key that became a struct field: the `Modify*` op recording the
/// whole record (`None`: unchanged), or the refusal.
pub type Plan = fn(&Document, ObjectRef, &str, Option<&str>) -> Result<Option<Op>, CmdError>;

/// How a row of [`PROP_RULES`] builds the change.
#[derive(Clone, Copy)]
pub enum Build {
    /// A function or location code of the project's list: [`crate::locations::commands::locations::plan_set`],
    /// for all the devices at once. It claims by key alone ([`crate::locations::commands::locations::aspect_property`]).
    Aspect,
    /// The tag: typing one fixes it, `-K?` takes the next provisional one, a
    /// tag another device or cable holds needs a conflict answer ([`plan_tag`]). One object only.
    Tag,
    /// A device property: checked; a tag another device holds merges the two
    /// with `merge` ([`plan_device`]).
    Device(Check),
    /// A value the check reads (and may store canonically): one `SetProperty` per object
    /// ([`plan_value`]).
    Value(Check),
    /// A key that is a struct field: one `Modify*` op per object ([`plan_record`]).
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
        claims: |_, o, k| is_component(o) && crate::locations::commands::locations::aspect_property(k).is_some(),
        build: Build::Aspect,
    },
    PropRule { what: "tag (design 026)", claims: |_, o, k| k == TAG_KEY && tagged(o), build: Build::Tag },
    PropRule { what: "device property (design 018)", claims: |_, o, k| is_device_property(o, k), build: Build::Device(device_value) },
    PropRule {
        what: "wire, junction or continuation (briefs 022, 049)",
        claims: |doc, o, k| !matches!(crate::wires::commands::wires::setprop_rule(doc, o, k, None), Ok(None)),
        build: Build::Value(wire_value),
    },
    PropRule {
        what: "strip offer (brief 028)",
        claims: |doc, o, k| k == crate::terminal_strips::OFFER_KEY && strip(doc, o).is_some(),
        build: Build::Value(offer_value),
    },
    PropRule {
        what: "PE row part (brief 028)",
        claims: |_, o, k| is_component(o) && (k == crate::terminal_strips::PE_PART_KEY || k == crate::terminal_strips::PE_MAKER_KEY),
        build: Build::Value(pe_value),
    },
    PropRule {
        what: "connector (brief 044)",
        claims: |doc, o, k| component(o).is_some_and(|c| crate::connectors::setprop_value(doc, c, k, None).is_some()),
        build: Build::Value(connector_value),
    },
    PropRule {
        what: "strip terminal defaults (brief 036)",
        claims: |doc, o, k| ecad_model::naming::keys::in_area(k, ecad_model::naming::keys::Area::TerminalBlock) && strip(doc, o).is_some(),
        build: Build::Value(connection_point_default_value),
    },
    PropRule {
        what: "level label (brief 036)",
        claims: |doc, o, k| k.starts_with(crate::terminal_strips::terminal_blocks::LABEL_PREFIX) && strip(doc, o).is_some(),
        build: Build::Value(level_label_value),
    },
    PropRule {
        what: "PLC card (brief 045)",
        claims: |doc, o, k| component(o).is_some_and(|c| crate::plc::setprop_rule(doc, c, k, None).is_some()),
        build: Build::Value(plc_value),
    },
    PropRule {
        what: "rating (brief 034)",
        claims: |_, o, k| is_component(o) && crate::symbols::rules::is_rating_key(k),
        build: Build::Value(rating_value),
    },
    PropRule {
        what: "parts and items (brief 047)",
        claims: |_, o, k| component(o).is_some() && crate::reports::bom::is_bom_key(k),
        build: Build::Value(bom_value),
    },
    PropRule { what: "strip accessories, or free text (brief 030)", claims: |_, o, _| is_component(o), build: Build::Value(accessory_value) },
    FREE_TEXT,
];

/// The row of [`PROP_RULES`] that decides `key` on `object`.
pub fn rule_for(doc: &Document, object: ObjectRef, key: &str) -> &'static PropRule {
    LANES.iter().flat_map(|(_, rows)| rows.iter()).chain(PROP_RULES).find(|r| (r.claims)(doc, object, key)).unwrap_or(&FREE_TEXT)
}

/// `setprop` of a `Record` row on one object: its `Modify*` op, `None` when unchanged.
///
/// # Errors
/// The row's refusal; a row of another kind.
pub(crate) fn plan_record(doc: &Document, object: ObjectRef, key: &str, typed: Option<&str>) -> Result<Option<Op>, CmdError> {
    match rule_for(doc, object, key).build {
        Build::Record(plan) => plan(doc, object, key, typed),
        Build::Aspect | Build::Tag | Build::Device(_) | Build::Value(_) => Err(CmdError::input("that property is set through its own rule")),
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
    component(o).filter(|c| crate::terminal_strips::strip_of(doc, *c).is_some())
}

/// Whether `o` takes a tag: a device or a cable.
fn tagged(o: ObjectRef) -> bool {
    matches!(o, ObjectRef::Device(_) | ObjectRef::Cable(_))
}

/// Whether `key` on `object` is one of the device properties.
pub(crate) fn is_device_property(object: ObjectRef, key: &str) -> bool {
    is_component(object) && crate::devices::PROPERTIES.iter().any(|(k, _)| *k == key)
}

fn free_text(_: &Document, _: &ecad_model::registry::data::Registry, _: ObjectRef, _: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    Ok(value.map(str::to_string))
}

fn device_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, _: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    crate::devices::json::property_value(doc, key, value)
}

fn wire_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, o: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    Ok(crate::wires::commands::wires::setprop_rule(doc, o, key, value)?.unwrap_or_else(|| value.map(str::to_string)))
}

fn offer_value(
    _: &Document, _: &ecad_model::registry::data::Registry, _: ObjectRef, _: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    crate::terminal_strips::offer_value(value)
}

fn pe_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, _: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    crate::devices::json::property_value(doc, key, value)
}

fn connector_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, o: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    match component(o).and_then(|c| crate::connectors::setprop_value(doc, c, key, value)) {
        Some(r) => r,
        None => Ok(value.map(str::to_string)),
    }
}

fn connection_point_default_value(
    _: &Document, _: &ecad_model::registry::data::Registry, _: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    match crate::defaults::terminal_blocks::setprop_value(key, value) {
        Some(r) => r,
        None => Ok(value.map(str::to_string)),
    }
}

fn level_label_value(
    _: &Document, _: &ecad_model::registry::data::Registry, _: ObjectRef, _: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    use crate::terminal_strips::terminal_blocks::MAX_LABEL_CHARS;
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) if v.chars().count() > MAX_LABEL_CHARS || v.chars().any(char::is_control) => {
            Err(CmdError::refused(format!("A level's label is 1 to {MAX_LABEL_CHARS} characters, with no control characters.")))
        }
        Some(v) => Ok(Some(v.to_string())),
    }
}

fn plc_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, o: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    match component(o).and_then(|c| crate::plc::setprop_rule(doc, c, key, value)) {
        Some(r) => r,
        None => Ok(None),
    }
}

fn rating_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, o: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    crate::symbols::rules::setprop_rule(doc, o, key, value)
}

fn bom_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, o: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    match component(o) {
        Some(c) => crate::reports::bom::setprop_rule(doc, c, key, value),
        None => Ok(value.map(str::to_string)),
    }
}

fn accessory_value(
    doc: &Document, _: &ecad_model::registry::data::Registry, o: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    let rule = match component(o) {
        Some(c) => crate::terminal_strips::accessories::setprop_rule(doc, c, key, value)?,
        None => None,
    };
    Ok(rule.unwrap_or_else(|| value.map(str::to_string)))
}

// ---------------------------------------------------------------- the key

/// Refuse a key `setprop` never writes: `text.` keys are written by the text commands only.
///
/// # Errors
/// [`ecad_model::look::RESERVED_KEY`].
pub fn reserved(key: &str) -> Result<(), CmdError> {
    match ecad_model::look::is_reserved_key(key) {
        true => Err(CmdError::refused(ecad_model::look::RESERVED_KEY)),
        false => Ok(()),
    }
}

/// The key `setprop` writes on component `c` for a typed `key`: on a PLC card with channels,
/// `channels` is its channel count (`plc.channels`, pack re-run bug 7), as `connection_points` is a strip's
/// offer; on anything else the key as typed.
pub(crate) fn component_key(doc: &Document, c: DeviceId, key: &str) -> String {
    if key.trim() == "channels" && crate::plc::card_type_of(doc, c).is_some_and(|t| t.has_channels()) {
        crate::plc::rack::CHANNELS_KEY.to_string()
    } else {
        key.to_string()
    }
}

// ---------------------------------------------------------------- values (Device and Value rows)

/// The value `setprop` stores for `key` on `object` by its row of [`PROP_RULES`] (a `Device` or
/// `Value` row): an item first refuses the keys it cannot take.
///
/// # Errors
/// The rule's refusal; a key decided by an `Aspect` or `Fixed` row is refused (those build their
/// change in [`plan_state`] and [`crate::locations::commands::locations::plan_set`]).
pub fn value(
    doc: &Document, registry: &ecad_model::registry::data::Registry, object: ObjectRef, key: &str, value: Option<&str>,
) -> Result<Option<String>, CmdError> {
    let check = match rule_for(doc, object, key).build {
        Build::Device(f) | Build::Value(f) => f,
        // A device's tag is a device property too: merging into the device that holds it reads it
        // here ([`plan_device`]).
        Build::Tag => device_value,
        Build::Aspect | Build::Record(_) => return Err(CmdError::input("that property is set through its own rule")),
    };
    check(doc, registry, object, key, value)
}

fn props_of(doc: &Document, o: ObjectRef) -> Option<&Properties> {
    match o {
        ObjectRef::Device(c) => doc.devices().get(&c).map(|x| &x.properties),
        ObjectRef::Wire(c) => doc.wires().get(&c).map(|x| &x.display),
        ObjectRef::Cable(c) => doc.cables().get(&c).map(|x| &x.properties),
        ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard => None,
    }
}

/// A `Value` row's change on one object.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValueSet {
    /// The op, `None` when the value is already stored.
    pub op: Option<Op>,
    /// What the value meant, when it could be read two ways (a typed `2.1` is
    /// position 2, level 1).
    pub note: Option<String>,
}

/// `setprop` of a `Value` row on one object.
///
/// # Errors
/// The rule's refusal; [`NO_SUCH_OBJECT`].
pub(crate) fn plan_value(
    doc: &Document, registry: &ecad_model::registry::data::Registry, object: ObjectRef, key: &str, typed: Option<&str>,
) -> Result<ValueSet, CmdError> {
    let after = value(doc, registry, object, key, typed)?;
    let before = match object {
        // A cable's tag and description are fields: read them as `setprop` does.
        ObjectRef::Cable(c) => doc.cables().get(&c).ok_or(CmdError::refused(NO_SUCH_OBJECT))?.get(key).map(str::to_string),
        _ => props_of(doc, object).ok_or(CmdError::refused(NO_SUCH_OBJECT))?.get(key).cloned(),
    };
    if before == after {
        return Ok(ValueSet { op: None, note: None });
    }
    let note = match (object, &after) {
        (ObjectRef::Device(c), Some(_)) if ecad_model::connectivity::jumper_bars::jumper_bar_number(key).is_some() => {
            typed.and_then(|v| crate::terminal_strips::terminal_blocks::dotted_position_words(doc, c, v))
        }
        _ => None,
    };
    Ok(ValueSet { op: Some(Op::SetProperty { object, key: key.to_string(), before, after }), note })
}

/// `setprop {devices: [id…], key, value}`: a property written to several devices in one
/// transaction. Each device walks [`PROP_RULES`] as one would on its own:
/// a refusal on any refuses all. A tag is one per device, so `tag` is refused.
///
/// # Errors
/// [`ONE_TAG_PER_DEVICE`], [`NO_SUCH_OBJECT`], or the first rule that refuses.
pub(crate) fn plan_many(
    doc: &Document, registry: &ecad_model::registry::data::Registry, cs: &[DeviceId], key: &str, typed: Option<&str>,
) -> Result<Vec<Op>, CmdError> {
    if key == TAG_KEY {
        return Err(CmdError::refused(ONE_TAG_PER_DEVICE));
    }
    let mut ops = Vec::new();
    for c in cs {
        let comp = doc.devices().get(c).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
        let key = component_key(doc, *c, key);
        let o = ObjectRef::Device(*c);
        match rule_for(doc, o, &key).build {
            Build::Record(plan) => {
                ops.extend(plan(doc, o, &key, typed)?);
                continue;
            }
            Build::Aspect | Build::Tag | Build::Device(_) | Build::Value(_) => {}
        }
        let after = value(doc, registry, o, &key, typed)?;
        let before = comp.properties.get(&key).cloned();
        if before != after {
            ops.push(Op::SetProperty { object: o, key, before, after });
        }
    }
    Ok(ops)
}

// ---------------------------------------------------------------- device properties

/// A device property's change ([`plan_device`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DeviceSet {
    /// The value is already stored (as stored).
    Unchanged(Option<String>),
    /// One op, with its undo label.
    Set { op: Box<Op>, label: &'static str, value: Option<String> },
    /// The device becomes part of `holder` (a tag another device holds, with `merge`).
    Merged { ops: Vec<Op>, holder: DeviceId, into: String, before: Option<String> },
}

/// `setprop {component, key, value, merge?}` of a device property
/// the value rules; a tag another device holds is refused with the ways
/// forward, unless `merge`, which makes this device part of that one.
///
/// # Errors
/// [`NO_SUCH_OBJECT`]; the value rule's refusal; a taken tag without `merge`.
pub(crate) fn plan_device(
    doc: &Document, registry: &ecad_model::registry::data::Registry, ids: &mut IdAllocator, tag_high: &TagHigh, c: DeviceId, key: &str,
    typed: Option<&str>, merge: bool,
) -> Result<DeviceSet, CmdError> {
    let component = doc.devices().get(&c).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
    let value = value(doc, registry, ObjectRef::Device(c), key, typed)?;
    let before = component.value(key);
    if key == TAG_KEY
        && let Some(t) = &value
        && let Some(holder) = crate::devices::tags::holder_of(doc, &tag::Key::real(t), Some(c))
    {
        if !merge {
            let next = crate::devices::tags::tag_of(doc, c)
                .and_then(tag::parse)
                .or_else(|| tag::parse(t))
                .and_then(|(prefix, _)| tag_high.peek(doc, &prefix).ok());
            let way = match next {
                Some(n) => format!("Set `merge` to true to make this the same device, or choose another tag (the next free one is {n})."),
                None => "Set `merge` to true to make this the same device, or choose another tag.".into(),
            };
            return Err(CmdError::refused(crate::devices::tags::duplicate_words(doc, t, holder, &way)));
        }
        let ops = crate::devices::merge::plan_merge(doc, ids, c, holder)?;
        return Ok(DeviceSet::Merged { ops, holder, into: crate::devices::tags::tag_words(doc, holder), before });
    }
    if before == value {
        return Ok(DeviceSet::Unchanged(value));
    }
    let label = if key == TAG_KEY { "Set tag" } else { "Set property" };
    let op = if key == TAG_KEY {
        // A typed tag is fixed with it.
        let after = component.with_tag(value.as_deref(), TagState::Fixed);
        Op::ModifyDevice { id: c, before: component.clone(), after }
    } else {
        Op::SetProperty { object: ObjectRef::Device(c), key: key.to_string(), before, after: value.clone() }
    };
    Ok(DeviceSet::Set { op: Box::new(op), label, value })
}

/// The echo after a merge (with the merged document): `-K2 is now part of -K1 (3 symbols on 1, 2).`
pub(crate) fn merged_device_words(doc: &Document, before: Option<&str>, holder: DeviceId, into: &str) -> String {
    format!(
        "{} is now part of {into} ({} symbols on {}).",
        before.unwrap_or("The device"),
        doc.representations_of(Subject::Device(holder)).count(),
        crate::devices::tags::pages_words(doc, holder)
    )
}

// ---------------------------------------------------------------- tags

/// How a taken tag is resolved.
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

/// The tag `o` holds with its state: a device's fields, a cable's fields. The
/// outer `None` is anything else, or an object that is not there.
fn tag_of(doc: &Document, o: ObjectRef) -> Option<Option<tag::Given>> {
    let held = |t: Option<&str>, s: TagState| t.map(|t| tag::Given::new(t, s));
    match o {
        ObjectRef::Device(c) => doc.devices().get(&c).map(|d| held(d.tag(), d.tag.state().unwrap_or_default())),
        ObjectRef::Cable(c) => doc.cables().get(&c).map(|d| held(d.tag(), d.tag.state().unwrap_or_default())),
        _ => None,
    }
}

/// The op giving `o` the tag `given` (`None` clears it): one `ModifyDevice` on a device, one
/// `ModifyCable` on a cable.
fn write_tag(doc: &Document, o: ObjectRef, given: Option<tag::Given>) -> Vec<Op> {
    let (text, state) = match &given {
        Some(g) => (Some(g.text.as_str()), g.state),
        None => (None, TagState::Automatic),
    };
    match o {
        ObjectRef::Device(c) => {
            let Some(d) = doc.devices().get(&c) else { return Vec::new() };
            let after = d.with_tag(text, state);
            if after == *d { Vec::new() } else { vec![Op::ModifyDevice { id: c, before: d.clone(), after }] }
        }
        ObjectRef::Cable(k) => {
            let mut ops = Vec::new();
            Op::edit_cable(doc, &mut ops, k, |c| c.tag = TagSlot::of_text(text, state));
            ops
        }
        ObjectRef::Wire(_) | ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard => Vec::new(),
    }
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

/// The refusal of a taken tag with all three ways.
fn taken_words(doc: &Document, tag_high: &TagHigh, o: ObjectRef, given: &tag::Given, holder: Holder) -> String {
    let t = given.shown();
    let next = tag::prefix_of(&given.text).and_then(|px| tag_high.peek(doc, &px).ok());
    let next = next.map_or_else(|| "another tag".to_string(), |n| n.to_string());
    let same_kind = matches!((o, holder), (ObjectRef::Device(_), Holder::Device(_)) | (ObjectRef::Cable(_), Holder::Cable(_)));
    let ways = if same_kind {
        format!("Give conflict: \"swap\" (exchange the two tags), \"merge\" (make this the same {}) or \"next\" (use {next}).", noun(o))
    } else {
        format!("It is a {}, so only conflict: \"next\" (use {next}) works.", noun(holder.object()))
    };
    format!("{t} is used by {}. {ways}", crate::devices::tags::holder_words_of(doc, holder))
}

/// A tag's change ([`plan_tag`]). Tags in the echoes are as drawn (`-K?3`); states are
/// [`TagState`] words in the answers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TagSet {
    /// Nothing to change: the tag it has.
    Unchanged(Option<String>),
    /// The tag cleared (`slot` is none) or set ("Set tag").
    Set { ops: Vec<Op>, value: Option<String>, slot: TagSlot },
    /// The two tags exchanged ("Swap tags"); `line` is the echo.
    Swapped { ops: Vec<Op>, value: String, other: ObjectRef, other_value: String, line: String },
    /// Two cables made one: this one's members join `into` ("Merge into …").
    MergedCable { ops: Vec<Op>, into: CableId, tag: String, before: Option<String> },
    /// The next free tag of the prefix taken instead ("Set tag"); `line` is the echo.
    Next { ops: Vec<Op>, value: String, slot: TagSlot, line: String },
}

/// `setprop {component|cable, key: "tag", value, conflict?}`: the typed text is
/// read once ([`tag::enter`]); a typed tag is fixed with it; `-K?` takes the next provisional
/// number; a tag another device or cable holds needs `conflict`. Merging two devices is
/// [`plan_device`]'s (labels, contacts, connection points), so `Conflict::Merge` on a device does not
/// come here. `tag_high` is caught up.
///
/// # Errors
/// [`NO_SUCH_OBJECT`]; a tag that does not read; a connector's half tag; a taken tag without a
/// conflict answer, or with one that does not apply.
pub(crate) fn plan_tag(
    doc: &Document, tag_high: &mut TagHigh, o: ObjectRef, typed: Option<&str>, conflict: Option<Conflict>,
) -> Result<TagSet, CmdError> {
    let before = tag_of(doc, o).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
    let value = match typed.map(str::trim).filter(|v| !v.is_empty()) {
        None => None,
        Some(v) => Some(match enter_tag_value(v, crate::defaults::print::tag_style(doc))? {
            tag::Entry::Next(px) => tag::Given::new(tag_high.next_provisional_in(doc, &px), TagState::Provisional),
            tag::Entry::Tag(g) => g,
        }),
    };
    let Some(given) = value else {
        let ops = write_tag(doc, o, None);
        if ops.is_empty() {
            return Ok(TagSet::Unchanged(None));
        }
        return Ok(TagSet::Set { ops, value: None, slot: TagSlot::None });
    };
    let t = given.shown();
    // A provisional tag's state is not changed here, nor is a tag given the state provisional: Finalise
    // gives it its number, and typing `-K?` makes one.
    if let Some(now) = &before
        && now.text == given.text
        && now.state != given.state
        && (now.state.is_provisional() || given.state.is_provisional())
    {
        return Err(CmdError::refused(PROVISIONAL_FIXED));
    }
    // A connector's half tag is not free for another device.
    if !given.state.is_provisional()
        && let Some(Ok((c, Some(side), None))) = crate::connectors::resolve(doc, &given.text)
        && ObjectRef::Device(c) != o
    {
        return Err(CmdError::refused(format!("{t} is the {} half of {}. Pick another tag.", side.word(), crate::devices::tags::tag_words(doc, c))));
    }
    let Some(holder) = crate::devices::tags::tag_holder(doc, &given.key(), Some(o)) else {
        let ops = write_tag(doc, o, Some(given.clone()));
        if ops.is_empty() {
            return Ok(TagSet::Unchanged(Some(t)));
        }
        return Ok(TagSet::Set { ops, value: Some(t), slot: TagSlot::of_text(Some(&given.text), given.state) });
    };
    let same_kind = matches!((o, holder), (ObjectRef::Device(_), Holder::Device(_)) | (ObjectRef::Cable(_), Holder::Cable(_)));
    match conflict {
        None => Err(CmdError::refused(taken_words(doc, tag_high, o, &given, holder))),
        Some(Conflict::Swap) => {
            if !same_kind {
                return Err(CmdError::refused(format!("{t} is a {}; swap needs two of a kind. Give conflict: \"next\".", noun(holder.object()))));
            }
            let Some(mine) = before else {
                return Err(CmdError::refused(format!("This {} has no tag to give the other; give conflict: \"next\" or \"merge\".", noun(o))));
            };
            let h = holder.object();
            // Each tag moves with its state.
            let mut ops = write_tag(doc, o, Some(given.clone()));
            ops.extend(write_tag(doc, h, Some(mine.clone())));
            let other = if mine.state.is_provisional() { format!("provisional {}", mine.shown()) } else { mine.shown() };
            let line = format!("Swapped: this is {t}, the other is now {other}.");
            Ok(TagSet::Swapped { ops, value: t, other: h, other_value: mine.shown(), line })
        }
        Some(Conflict::Merge) => {
            // Two cables: the members join the holder; the slash follows.
            let (ObjectRef::Cable(from), Holder::Cable(into)) = (o, holder) else {
                return Err(CmdError::refused(format!(
                    "{t} is a {}; only two of a kind can be one. Give conflict: \"next\".",
                    noun(holder.object())
                )));
            };
            // One join for the merge and for setprop.
            let ops = ecad_model::document::doc::cable::join_cables(doc, from, into)?;
            Ok(TagSet::MergedCable { ops, into, tag: t, before: before.map(|g| g.shown()) })
        }
        Some(Conflict::Next) => {
            let px = tag::prefix_of(&given.text).ok_or_else(|| CmdError::refused(format!("{t} has no number to follow; type another tag.")))?;
            let next = if given.state.is_provisional() {
                tag::Given::new(tag_high.next_provisional_in(doc, &px), TagState::Provisional)
            } else {
                // The next free number is a typed choice of the person: fixed, as a typed tag is.
                tag::Given::new(tag_high.next_in(doc, &px)?, TagState::Fixed)
            };
            let ops = write_tag(doc, o, Some(next.clone()));
            let line = format!("{} is now {}.", before.map_or_else(|| format!("The {}", noun(o)), |g| g.shown()), next.shown());
            Ok(TagSet::Next { ops, value: next.shown(), slot: TagSlot::of_text(Some(&next.text), next.state), line })
        }
    }
}

/// The echo after two cables became one (with the merged document): `-W2 is now part of -W1
/// (5 wires).`
pub(crate) fn merged_cable_words(doc: &Document, before: Option<&str>, tag: &str, into: CableId) -> String {
    let n = doc.cables().get(&into).map_or(0, |c| c.cores.len());
    format!("{} is now part of {tag} ({}).", before.unwrap_or("The cable"), plural(n, "wire", "wires"))
}

pub fn object_json(o: ObjectRef) -> Value {
    match o {
        ObjectRef::Device(c) => json!({"component": c.hex()}),
        ObjectRef::Cable(k) => json!({"cable": k.hex()}),
        _ => Value::Null,
    }
}

// ---------------------------------------------------------------------------------------------
// The command (generic: crate::commands::run)

/// `setprop {component|conductor|cable, key, value, conflict?, merge?}` or `{components, key,
/// value}`: a property of one object, or of several devices in one transaction (not `tag`). A
/// device's or cable's tag: typing fixes it; `-K?` is the next provisional; a tag another device
/// or cable holds needs `conflict`. The `tag` value is typed text (fixed; `-K?` provisional) or `{"state": "automatic"|"fixed"|"provisional", "tag": "-K1"}`; null clears it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetpropParams {
    /// A device (or any component).
    #[serde(default)]
    component: Option<DeviceId>,
    /// Several devices, in one transaction.
    #[serde(default)]
    devices: Option<Bounded<DeviceId, 0, MAX_HANDLES>>,
    /// Drawing aids, in one transaction: continuations' `name` or `caption`,
    /// location boxes' `function`, `location` or `panel_temperature`.
    #[serde(default)]
    aids: Option<Bounded<AidId, 0, MAX_HANDLES>>,
    /// A wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// A cable.
    #[serde(default)]
    cable: Option<CableId>,
    /// The property's key.
    key: String,
    /// The value, or null to clear the property.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<String>")]
    value: Option<Option<String>>,
    /// A tag another device or cable holds: swap, merge or next.
    #[serde(default)]
    conflict: Option<String>,
    /// A device tag another device holds: make this the same device (as conflict merge).
    #[serde(default)]
    merge: Option<bool>,
}

/// The devices a function or location `setprop` names.
fn location_objects(component: Option<DeviceId>, devices: Option<&[DeviceId]>) -> Result<Vec<DeviceId>, CmdError> {
    match devices {
        Some(cs) if cs.len() > MAX_DEVICES => Err(CmdError::refused(format!("at most {MAX_DEVICES} entries in `devices` per call"))),
        Some(cs) => locations::objects(None, Some(cs.to_vec())),
        None => locations::objects(component, None),
    }
}

/// The one object a `setprop` names.
fn one_object(p: &SetpropParams) -> Result<ObjectRef, CmdError> {
    if let Some(c) = p.component {
        Ok(ObjectRef::Device(c))
    } else if let Some(c) = p.conductor {
        Ok(ObjectRef::Wire(c))
    } else if let Some(c) = p.cable {
        Ok(ObjectRef::Cable(c))
    } else {
        Err(CmdError::refused("`component`, `conductor` or `cable` (hex id) is required"))
    }
}

impl SetpropParams {
    /// Whether this sets a function or location on devices.
    fn sets_location(&self) -> bool {
        (self.component.is_some() || self.devices.is_some()) && locations::aspect_property(&self.key).is_some()
    }
}

impl Generic for SetpropParams {
    const ID: &'static str = "setprop";
    const TITLE: &'static str = "Set Property";
    const HELP: &'static str = "{component|conductor|cable: id, key, value: string|null, conflict?: \"swap\"|\"merge\"|\"next\", merge?: bool} | {devices: [id], key, value} (several devices in one transaction; not `tag`) (a device's or cable's tag: typing fixes it; `-K?` is the next provisional; a tag another device or cable holds needs `conflict`; `tag` also takes {\"state\": \"automatic\"|\"fixed\"|\"provisional\", \"tag\": \"-K1\"}; null clears it)";
    const STATE: State = State::Conflicts;

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        reserved(&self.key)?;
        if self.sets_location() {
            return location_objects(self.component, self.devices.as_ref().map(|c| c.0.as_slice())).map(|_| ());
        }
        if self.aids.is_some() || self.devices.is_some() {
            return Ok(());
        }
        one_object(self).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        reserved(&self.key)?;
        let object = one_object(&self);
        let key = self.key;
        let typed = self.value.flatten();
        // A function or location (PROP_RULES' first row, which claims by
        // key alone) is stored as an entry key of the project's list, for all the devices at once.
        if (self.component.is_some() || self.devices.is_some())
            && let Some(prop) = locations::aspect_property(&key)
        {
            let objects = location_objects(self.component, self.devices.as_ref().map(|c| c.0.as_slice()))?;
            let a = locations::aspect_of(&prop)?;
            let plan = locations::plan_set(cx.doc, cx.ids, &objects, &prop, typed.as_deref())?;
            return Ok(locations::set_outcome(cx.doc, a, plan, "devices", &mut cx.state));
        }
        // Drawing aids, one transaction: a continuation's name or caption; a
        // location box's function or location (with its proposals, as `setlocation`) or panel
        // temperature.
        if let Some(aids) = self.aids {
            if let Some(a) = Aspect::of_property(&key)
                && aids.0.iter().all(|x| crate::locations::box_values(cx.doc, *x).is_some())
            {
                let plan = locations::plan_box_set(cx.doc, cx.ids, &aids.0, a, typed.as_deref())?;
                return Ok(locations::set_outcome(cx.doc, a, plan, "aids", &mut cx.state));
            }
            let ops = crate::continuations::commands::plan_aid_prop(cx.doc, cx.ids, &aids.0, &key, typed.as_deref())?;
            let n = ops.len();
            return Ok(match n {
                0 => Outcome::answer(json!({"changed": false})),
                _ => Outcome::edit("Set property", ops, json!({"changed": true, "aids": n})),
            });
        }
        // Several devices at once (the Properties panel): one transaction for all.
        if let Some(cs) = self.devices {
            let ops = plan_many(cx.doc, cx.registry, &cs.0, &key, typed.as_deref())?;
            let n = ops.len();
            return Ok(match n {
                0 => Outcome::answer(json!({"changed": false})),
                _ => Outcome::edit("Set property", ops, json!({"changed": true, "devices": n})),
            });
        }
        let object = object?;
        let (word, merge) = (self.conflict, self.merge.unwrap_or(false));
        let doc = cx.doc;
        // Pack re-run, bug 7: on a PLC card with channels, `channels` is its channel count.
        let key = match object {
            ObjectRef::Device(c) => component_key(doc, c, &key),
            _ => key,
        };
        match rule_for(doc, object, &key).build {
            Build::Tag => {
                let conflict = conflict(word.as_deref(), merge)?;
                match (conflict, object) {
                    // Merging two devices keeps the usual path (labels, contacts, connection points).
                    (Some(Conflict::Merge), ObjectRef::Device(c)) => set_device(cx, c, TAG_KEY, typed.as_deref(), true),
                    _ => set_tag(cx, object, typed.as_deref(), conflict),
                }
            }
            Build::Device(_) => match object {
                ObjectRef::Device(c) => set_device(cx, c, &key, typed.as_deref(), merge),
                _ => set_value(doc, cx.registry, object, &key, typed.as_deref()),
            },
            Build::Value(_) | Build::Aspect => set_value(doc, cx.registry, object, &key, typed.as_deref()),
            Build::Record(_) => {
                let op = plan_record(doc, object, &key, typed.as_deref())?;
                let changed = op.is_some();
                // Every way in shows what a rating read as.
                let answer = move |doc: &Document| {
                    let mut answer = record_answer(doc, object, &key, changed);
                    if let ObjectRef::Device(c) = object
                        && let Some(m) = doc.devices().get(&c).and_then(|d| crate::devices::ratings::echo(d, &key))
                    {
                        answer["message"] = json!(m);
                    }
                    answer
                };
                let out = Outcome::edit_then("Set property", op.into_iter().collect(), answer);
                Ok(if changed { out.echo_message() } else { out })
            }
        }
    }
}

/// A `Record` row's answer: on a device, the value now stored (and the device, when it changed),
/// as a device property's answer gives them.
fn record_answer(doc: &Document, object: ObjectRef, key: &str, changed: bool) -> Value {
    match object {
        ObjectRef::Device(c) => {
            let value = doc.devices().get(&c).and_then(|d| d.value(key));
            if changed { json!({"changed": true, "value": value, "component": c.hex()}) } else { json!({"changed": false, "value": value}) }
        }
        _ => json!({"changed": changed}),
    }
}

/// A `Value` row's change on one object.
fn set_value(
    doc: &Document, registry: &ecad_model::registry::data::Registry, object: ObjectRef, key: &str, typed: Option<&str>,
) -> Result<Outcome, CmdError> {
    let done = plan_value(doc, registry, object, key, typed)?;
    let Some(op) = done.op else { return Ok(Outcome::answer(json!({"changed": false}))) };
    Ok(match done.note {
        Some(m) => Outcome::edit("Set property", vec![op], json!({"changed": true, "message": m})).echo(Some(m)),
        None => Outcome::edit("Set property", vec![op], json!({"changed": true})),
    })
}

/// A device property, merging with `merge`.
fn set_device(cx: &mut Cx<'_>, c: DeviceId, key: &str, typed: Option<&str>, merge: bool) -> Result<Outcome, CmdError> {
    Ok(match plan_device(cx.doc, cx.registry, cx.ids, cx.tag_high, c, key, typed, merge)? {
        DeviceSet::Unchanged(value) => Outcome::answer(json!({"changed": false, "value": value})),
        DeviceSet::Set { op, label, value } => Outcome::edit(label, vec![*op], json!({"changed": true, "value": value, "component": c.hex()})),
        DeviceSet::Merged { ops, holder, into, before } => Outcome::edit_then(format!("Merge into {into}"), ops, move |doc| {
            let message = merged_device_words(doc, before.as_deref(), holder, &into);
            json!({"changed": true, "merged": true, "component": holder.hex(), "tag": into, "message": message})
        }),
    })
}

/// The tag of a device or cable.
/// A value of the `tag` key (S6, #3): typed text ([`tag::enter`]), or the stored form
/// `{"state": "automatic"|"fixed"|"provisional", "tag": "-K1"}` ([`TagSlot`]), which names its own state.
///
/// # Errors
/// As [`tag::enter`] and [`tag::enter_slot`]; an object that is not a tag slot.
pub(crate) fn enter_tag_value(text: &str, style: tag::Style) -> Result<tag::Entry, CmdError> {
    if !text.trim_start().starts_with('{') {
        return Ok(tag::enter(text, style)?);
    }
    let slot: TagSlot = serde_json::from_str(text).map_err(|e| {
        CmdError::refused(format!("A tag value is text, or {{\"state\": \"automatic\"|\"fixed\"|\"provisional\", \"tag\": \"-K1\"}} ({e})."))
    })?;
    Ok(tag::enter_slot(&slot, style)?)
}

/// A tag slot as the answers give it: `{"state", "tag"}`, `{"state":"none"}` for none.
fn slot_json(slot: &TagSlot) -> Value {
    json!(slot)
}

fn set_tag(cx: &mut Cx<'_>, o: ObjectRef, typed: Option<&str>, conflict: Option<Conflict>) -> Result<Outcome, CmdError> {
    cx.tag_high.catch_up(cx.ledger);
    Ok(match plan_tag(cx.doc, cx.tag_high, o, typed, conflict)? {
        TagSet::Unchanged(value) => Outcome::answer(json!({"changed": false, "value": value})),
        TagSet::Set { ops, value, slot } => Outcome::edit("Set tag", ops, json!({"changed": true, "value": value, "tag": slot_json(&slot)})),
        TagSet::Swapped { ops, value, other, other_value, line } => Outcome::edit(
            "Swap tags",
            ops,
            json!({"changed": true, "swapped": true, "value": value, "other": object_json(other), "other_value": other_value, "message": line}),
        )
        .echo(Some(line)),
        TagSet::MergedCable { ops, into, tag, before } => Outcome::edit_then(format!("Merge into {tag}"), ops, move |doc| {
            let line = merged_cable_words(doc, before.as_deref(), &tag, into);
            json!({"changed": true, "merged": true, "cable": into.hex(), "tag": tag, "message": line})
        })
        .echo_message(),
        TagSet::Next { ops, value, slot, line } => {
            { Outcome::edit("Set tag", ops, json!({"changed": true, "value": value, "tag": slot_json(&slot), "message": line})) }.echo(Some(line))
        }
    })
}

/// The `setprop` rows of each object kind, checked before [`PROP_RULES`].
pub static LANES: &[(&str, &[PropRule])] = &[("072b", devices::RULES), ("072c", plc::RULES), ("072d", wires::RULES)];
