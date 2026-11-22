//! Devices and tags (brief 018, design 018).
//!
//! A *device* is a component with a tag. By the concept spec a tag is an identity: the same tag
//! anywhere means the same device ([`ecad_model::naming::tag`]). A device may be drawn by several symbols
//! (a coil and its contacts), each a symbol representation; the first is its own definition's,
//! the others carry their terminal positions as connection points in their definition's local
//! frame (I14 as relaxed, decision 2026-11-08).
//!
//! # Semantics
//! - Automatic tags: the symbol's prefix (IEC 81346 by default, see [`prefix_for`]) and the
//!   highest number that prefix has ever had in the project's history + 1 ([`TagHigh`]), so a
//!   deleted tag is not reused. Numbers are padded ([`padding`], 2 by default).
//! - A duplicate tag is never a dead end: placing with an existing tag adds the symbol to that
//!   device, and renaming a device to an existing tag merges it, when asked ([`plan_merge`]).
//! - Joining labels follow design 018 §3.1 ([`join_labels`]).
//! - Tag texts and terminal labels are derived on every draw, never stored (`texts_on_page`).
//!
//! This module builds ops and words; it commits nothing and holds no UI.

pub mod commands;
pub mod device_list;
pub mod hover;
pub mod json;
pub mod kinds;
pub mod merge;
pub mod placing;
pub mod ratings;
pub mod tags;
pub mod texts;
pub mod traits;
pub(crate) mod view;
use self::{merge::*, placing::*, tags::*, texts::*};
use std::collections::{BTreeMap, BTreeSet};

use crate::CmdError;
use ecad_model::Word;
use ecad_model::connectivity::derive::connection_point_positions;
use ecad_model::naming::tag::{self, TAG_KEY};
use ecad_model::{
    ConnectionPoint, ConnectionPointId, Definition, DefinitionId, Device, DeviceId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Op,
    PageId, Placement, Pt, Representation, RepresentationId, Style, Subject, SymbolInstance,
};
use ecad_model::{DeviceType, Noun, PointKey, TextHAlign as HAlign, TextVAlign as VAlign};

/// `t` as a device's stored tag (design 072 §2.1): refused when it is not 1 to 40 printable
/// characters.
///
/// # Errors
/// [`tag::BAD_TAG`]'s words.
pub fn stored_tag(t: &str) -> Result<tag::Tag, CmdError> {
    tag::Tag::new(t).map_err(|_| CmdError::refused(tag::BAD_TAG))
}

/// A new device's description from a symbol's or default's name: `None` when the name is not a
/// valid description (blank, too long, a hidden character).
pub fn description_of(name: &str) -> Option<ecad_model::objects::devices::Description> {
    ecad_model::objects::devices::Description::new(name).ok()
}

/// Device `id` as the planned `ops` (not committed) leave it on `doc`: created, modified or
/// deleted there, or its properties set; `None` when it does not exist.
pub fn planned_device(doc: &Document, ops: &[Op], id: DeviceId) -> Option<Device> {
    planned_device_on(doc, &[], ops, id)
}

/// [`planned_device`] after `base` and then `ops`.
fn planned_device_on(doc: &Document, base: &[Op], ops: &[Op], id: DeviceId) -> Option<Device> {
    let mut d = doc.devices.get(&id).cloned();
    for op in base.iter().chain(ops) {
        match op {
            Op::CreateDevice { id: x, device } | Op::ModifyDevice { id: x, after: device, .. } if *x == id => d = Some(device.clone()),
            Op::DeleteDevice { id: x, .. } if *x == id => d = None,
            Op::SetProperty { object: ecad_model::ObjectRef::Device(x), key, after, .. } if *x == id => {
                if let Some(d) = d.as_mut() {
                    match after {
                        Some(v) => d.properties.insert(key.clone(), v.clone()),
                        None => d.properties.remove(key),
                    };
                }
            }
            _ => {}
        }
    }
    d
}

/// Change device `id`'s typed fields with `f` within the planned `ops` (design 072: one
/// `ModifyDevice` records the whole device): folded into the `CreateDevice` or `ModifyDevice` of
/// it already in `ops` when no property op on it follows that one, else a new `ModifyDevice`.
/// Returns whether anything changed (`false` also when the device does not exist).
pub fn edit_planned(doc: &Document, ops: &mut Vec<Op>, id: DeviceId, f: impl FnOnce(&mut Device)) -> bool {
    edit_planned_on(doc, &[], ops, id, f)
}

/// [`edit_planned`] for ops that will follow `base` (planned by the caller, such as the ops
/// creating the device): the device is read after `base`, and only `ops` are changed.
pub fn edit_planned_on(doc: &Document, base: &[Op], ops: &mut Vec<Op>, id: DeviceId, f: impl FnOnce(&mut Device)) -> bool {
    let Some(before) = planned_device_on(doc, base, ops, id) else { return false };
    let mut after = before.clone();
    f(&mut after);
    if after == before {
        return false;
    }
    let touches = |op: &Op| match op {
        Op::CreateDevice { id: x, .. } | Op::ModifyDevice { id: x, .. } | Op::DeleteDevice { id: x, .. } => *x == id,
        Op::SetProperty { object: ecad_model::ObjectRef::Device(x), .. } => *x == id,
        _ => false,
    };
    match ops.iter_mut().rev().find(|op| touches(op)) {
        Some(Op::CreateDevice { device, .. } | Op::ModifyDevice { after: device, .. }) => *device = after,
        _ => ops.push(Op::ModifyDevice { id, before, after }),
    }
    true
}
/// After device `c`'s symbol is swapped for `definition` (a `SetDeviceDefinition` already in the
/// plan), the op giving it the type that symbol declares (design 072 §2.3): `None` while the group
/// stays, so the data the device has is kept.
///
/// # Semantics
/// `device` is the device as it is before the swap; the op's `before` is it with the new
/// definition, so it goes straight after the `SetDeviceDefinition`.
pub fn retype(c: DeviceId, device: &Device, definition: DefinitionId, declared: &DeviceType) -> Option<Op> {
    if device.device_type.group() == declared.group() {
        return None;
    }
    let before = Device { definition: Some(definition), ..device.clone() };
    let after = Device { device_type: declared.clone(), ..before.clone() };
    Some(Op::ModifyDevice { id: c, before, after })
}
use serde_json::{Value, json};

use crate::styles::text_style::fonts::TextMetrics;
use crate::symbols::catalogue::STARTERS;
use crate::symbols::{self, Kind, Starter, StarterKey as K};

/// Reserved layer of tag texts and terminal labels (design 018 §2.4).
pub const TAG_LAYER: &str = "ECAD-TAGS";
/// `ecad.provisional` (design 026 §10): ACI 30, orange, for provisional tags and wire numbers.
pub const PROVISIONAL_ACI: u8 = 30;
/// Most letters in a tag prefix a project may set.
pub const MAX_PREFIX_LETTERS: usize = ecad_model::naming::prefix::MAX_PREFIX_LETTERS;
/// Refusal of an edit of a derived tag text or terminal label.
pub const TAG_DERIVED: &str = "The tag text is derived: change the tag with Device Properties, or move it with Move Text.";
/// Most characters in a property value (design 018 §6.1).
pub const MAX_PROPERTY_CHARS: usize = 200;
/// Most devices a JSON list returns.
pub const MAX_LISTED: usize = 500;

/// The device properties, as stored and as shown (design 018 §6.1).
pub const PROPERTIES: [(&str, &str); 6] = [
    ("tag", "Tag"),
    ("description", "Description"),
    ("maker", "Maker"),
    ("part_number", "Part number"),
    ("location", "Location"),
    ("function", "Function"),
];

/// How many digits this project pads tag numbers to (its `tag_padding` setting, at most
/// [`ecad_model::naming::tag::MAX_PADDING`]).
pub fn padding(doc: &Document) -> u8 {
    doc.settings.tag_padding().min(tag::MAX_PADDING)
}

/// A prefix as a project may set it: 1 to [`MAX_PREFIX_LETTERS`] ASCII letters, with or without a
/// leading `-`; stored in upper case without the `-`.
pub fn valid_prefix(text: &str) -> Option<String> {
    ecad_model::naming::prefix::TagPrefix::typed(text).map(|p| p.to_string())
}

/// Set (with `None` remove) the project's prefix of symbol `of` in `s` (design 072 F5): a
/// starter's own built-in prefix, or text that is not a prefix, is stored as none.
pub fn set_symbol_prefix(s: &mut ecad_model::ProjectSettings, of: ecad_model::naming::prefix::PrefixOf, prefix: Option<&str>) {
    let builtin = match &of {
        ecad_model::naming::prefix::PrefixOf::Starter(w) => crate::symbols::catalogue::by_key(w).map(|x| x.prefix()),
        ecad_model::naming::prefix::PrefixOf::Project(_) => None,
    };
    match prefix.and_then(ecad_model::naming::prefix::TagPrefix::typed) {
        Some(p) if builtin != Some(p.as_str()) => {
            s.tag_prefixes.insert(of, p);
        }
        Some(_) | None => {
            s.tag_prefixes.remove(&of);
        }
    }
}

/// The tag prefix a new device of symbol `s` gets in this project (letters, without `-`): the
/// project's own (`tag_prefixes`), else the built-in IEC 81346 default. A value a hostile file
/// stored that is not a valid prefix is ignored.
pub fn prefix_for(doc: &Document, s: &Starter) -> String {
    crate::defaults::symbols::prefix(doc, s.key)
}
