//! The rules behind `linkdevices`, `unlinkdevices` (design 072 §2.6) and `userfield` (§2.8), and
//! `setprop`'s `user.<name>` row.
//!
//! # Semantics
//! - A link joins two different devices with a kind; the same two devices with the same kind are
//!   linked once. Deleting a device removes its links in the same transaction ([`unlink_ops`]),
//!   so undo restores them.
//! - A user field is added to the project's list with a name unique ignoring case. Removing one
//!   clears its value on every device that has one, in the same transaction, before the list
//!   changes. A field's kind is fixed once made: remove it and add it again to change it.
//! - `setprop {key: "user.<name>"}` on a device reads the value as the field's kind; an empty
//!   value clears it.
//!
//! This module excludes the JSON shims (ecad-cadcraft's `cmd::device_link`) and any check that
//! reads a link (lane F's interlock checks).

use ecad_model::id::{LinkId, UserFieldId};
use ecad_model::user::{DeviceLink, FieldName, LinkKind, UserFieldDef, UserFieldKind, UserValue};
use ecad_model::{DeviceId, Document, IdAllocator, ObjectRef, Op, Word};

use crate::error::CmdError;

/// The key prefix of a user field's value in `setprop` (`user.Panel row`).
pub const USER_PREFIX: &str = "user.";

/// Link devices `a` and `b` with `kind`: one `AddDeviceLink`.
///
/// # Errors
/// A device that is not there, a device linked to itself, or the same link already there.
pub fn plan_link(doc: &Document, ids: &mut IdAllocator, kind: LinkKind, a: DeviceId, b: DeviceId) -> Result<(LinkId, Vec<Op>), CmdError> {
    for d in [a, b] {
        if !doc.devices.contains_key(&d) {
            return Err(CmdError::refused(format!("There is no device {d}.")));
        }
    }
    if a == b {
        return Err(CmdError::refused("A device cannot be linked to itself."));
    }
    let link = DeviceLink { kind, a, b };
    if doc.device_links.values().any(|l| l.same_as(&link)) {
        return Err(CmdError::refused(format!(
            "{} and {} already have a {} link.",
            crate::devices::tag_words(doc, a),
            crate::devices::tag_words(doc, b),
            kind.word()
        )));
    }
    let id: LinkId = ids.fresh();
    Ok((id, vec![Op::AddDeviceLink { id, link }]))
}

/// Remove link `id`: one `RemoveDeviceLink`.
///
/// # Errors
/// No such link.
pub fn plan_unlink(doc: &Document, id: LinkId) -> Result<Vec<Op>, CmdError> {
    let link = doc.device_links.get(&id).ok_or_else(|| CmdError::refused(format!("There is no device link {id}.")))?;
    Ok(vec![Op::RemoveDeviceLink { id, link: *link }])
}

/// The `RemoveDeviceLink`s of every link naming device `d`: deleting it goes after them.
pub fn unlink_ops(doc: &Document, d: DeviceId) -> Vec<Op> {
    doc.device_links.iter().filter(|(_, l)| l.names(d)).map(|(id, l)| Op::RemoveDeviceLink { id: *id, link: *l }).collect()
}

/// Links naming device `from` re-pointed at `into` (a merge of two devices): each is removed and
/// added again with the same id, or only removed when it would join `into` to itself or repeat a
/// link `into` already has.
pub fn repoint_ops(doc: &Document, from: DeviceId, into: DeviceId) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut kept: Vec<DeviceLink> = doc.device_links.values().filter(|l| !l.names(from)).copied().collect();
    for (id, l) in doc.device_links.iter().filter(|(_, l)| l.names(from)) {
        ops.push(Op::RemoveDeviceLink { id: *id, link: *l });
        let swap = |d: DeviceId| if d == from { into } else { d };
        let moved = DeviceLink { kind: l.kind, a: swap(l.a), b: swap(l.b) };
        if moved.a != moved.b && !kept.iter().any(|k| k.same_as(&moved)) {
            kept.push(moved);
            ops.push(Op::AddDeviceLink { id: *id, link: moved });
        }
    }
    ops
}

/// The project's user field named `name` (ignoring case).
pub fn field_named<'a>(doc: &'a Document, name: &str) -> Option<&'a UserFieldDef> {
    doc.project.user_fields.iter().find(|f| f.name.as_str().eq_ignore_ascii_case(name.trim()))
}

/// Add user field `name` of `kind`: one `ModifyProject`.
///
/// # Errors
/// A name that is empty, too long or already used; a choice field with no choices, or one
/// twice; too many fields ([`ecad_model::user::fields_problem`]).
pub fn plan_add_field(doc: &Document, ids: &mut IdAllocator, name: &str, kind: UserFieldKind) -> Result<(UserFieldId, Vec<Op>), CmdError> {
    let name = FieldName::new(name.trim()).map_err(|e| CmdError::refused(format!("Field name: {e}")))?;
    if field_named(doc, name.as_str()).is_some() {
        return Err(CmdError::refused(format!("The project already has a field \"{}\".", name.as_str())));
    }
    let id: UserFieldId = ids.fresh();
    let mut after = doc.project.clone();
    after.user_fields.push(UserFieldDef { id, name, kind });
    if let Some(p) = ecad_model::user::fields_problem(&after.user_fields) {
        return Err(CmdError::refused(format!("Not added: {p}.")));
    }
    Ok((id, vec![Op::ModifyProject { before: doc.project.clone(), after }]))
}

/// Remove user field `name`: a `ModifyDevice` clearing it on each device that has a value, then
/// one `ModifyProject`. Answers the ops and how many values were cleared.
///
/// # Errors
/// No such field.
pub fn plan_remove_field(doc: &Document, name: &str) -> Result<(Vec<Op>, usize), CmdError> {
    let field = field_named(doc, name).ok_or_else(|| CmdError::refused(format!("The project has no field \"{}\".", name.trim())))?;
    let mut ops = Vec::new();
    for (id, d) in doc.devices.iter().filter(|(_, d)| d.user.contains_key(&field.id)) {
        let mut after = d.clone();
        after.user.remove(&field.id);
        ops.push(Op::ModifyDevice { id: *id, before: d.clone(), after });
    }
    let cleared = ops.len();
    let mut after = doc.project.clone();
    after.user_fields.retain(|f| f.id != field.id);
    ops.push(Op::ModifyProject { before: doc.project.clone(), after });
    Ok((ops, cleared))
}

/// `setprop`'s `user.<name>` row on a device: the value read as the field's kind, or cleared
/// when empty. One `ModifyDevice`, none when nothing changes.
///
/// # Errors
/// Not a device, no such field, a value that does not read as the field's kind.
pub fn user_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let ObjectRef::Device(c) = o else {
        return Err(CmdError::refused("User fields are set on devices."));
    };
    let d = doc.devices.get(&c).ok_or_else(|| CmdError::refused(format!("There is no device {c}.")))?;
    let name = key.strip_prefix(USER_PREFIX).unwrap_or(key);
    let field = field_named(doc, name).ok_or_else(|| CmdError::refused(format!("The project has no field \"{name}\"; add it with userfield.")))?;
    let mut after = d.clone();
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => {
            after.user.remove(&field.id);
        }
        Some(v) => {
            let read = UserValue::parse(&field.kind, v).map_err(|why| CmdError::refused(format!("{}: {why}.", field.name.as_str())))?;
            after.user.insert(field.id, read);
        }
    }
    Ok((after != *d).then(|| Op::ModifyDevice { id: c, before: d.clone(), after }))
}

/// Device `d`'s value of field `name` as shown, if it has one.
pub fn shown_value(doc: &Document, d: DeviceId, name: &str) -> Option<String> {
    let field = field_named(doc, name)?;
    doc.devices.get(&d)?.user.get(&field.id).map(|v| v.show(&field.kind))
}
