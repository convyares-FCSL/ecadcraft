//! The rules behind `linkdevices`, `unlinkdevices` and `userfield`, and
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
//! The commands are generic ([`crate::commands::run`]). This module excludes any check that
//! reads a link (the interlock checks).

use ecad_model::naming::id::{LinkId, UserFieldId};
use ecad_model::objects::user::{Choice, DeviceLink, FieldName, FieldUnit, LinkKind, UserFieldDef, UserFieldKind, UserValue};
use ecad_model::{DeviceId, Document, IdAllocator, ObjectRef, Op, Word};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;

/// The key prefix of a user field's value in `setprop` (`user.Panel row`).
pub(crate) const USER_PREFIX: &str = "user.";

/// Link devices `a` and `b` with `kind`: one `AddDeviceLink`.
///
/// # Errors
/// A device that is not there, a device linked to itself, or the same link already there.
pub(crate) fn plan_link(doc: &Document, ids: &mut IdAllocator, kind: LinkKind, a: DeviceId, b: DeviceId) -> Result<(LinkId, Vec<Op>), CmdError> {
    for d in [a, b] {
        if !doc.devices().contains_key(&d) {
            return Err(CmdError::refused(format!("There is no device {d}.")));
        }
    }
    if a == b {
        return Err(CmdError::refused("A device cannot be linked to itself."));
    }
    let link = DeviceLink { kind, a, b };
    if doc.device_links().values().any(|l| l.same_as(&link)) {
        return Err(CmdError::refused(format!(
            "{} and {} already have a {} link.",
            crate::devices::tags::tag_words(doc, a),
            crate::devices::tags::tag_words(doc, b),
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
pub(crate) fn plan_unlink(doc: &Document, id: LinkId) -> Result<Vec<Op>, CmdError> {
    let link = doc.device_links().get(&id).ok_or_else(|| CmdError::refused(format!("There is no device link {id}.")))?;
    Ok(vec![Op::RemoveDeviceLink { id, link: *link }])
}

/// The `RemoveDeviceLink`s of every link naming device `d`: deleting it goes after them.
pub(crate) fn unlink_ops(doc: &Document, d: DeviceId) -> Vec<Op> {
    doc.device_links().iter().filter(|(_, l)| l.names(d)).map(|(id, l)| Op::RemoveDeviceLink { id: *id, link: *l }).collect()
}

/// Links naming device `from` re-pointed at `into` (a merge of two devices): each is removed and
/// added again with the same id, or only removed when it would join `into` to itself or repeat a
/// link `into` already has.
pub(crate) fn repoint_ops(doc: &Document, from: DeviceId, into: DeviceId) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut kept: Vec<DeviceLink> = doc.device_links().values().filter(|l| !l.names(from)).copied().collect();
    for (id, l) in doc.device_links().iter().filter(|(_, l)| l.names(from)) {
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
pub(crate) fn field_named<'a>(doc: &'a Document, name: &str) -> Option<&'a UserFieldDef> {
    doc.project().user_fields.iter().find(|f| f.name.as_str().eq_ignore_ascii_case(name.trim()))
}

/// Add user field `name` of `kind`: one `ModifyProject`.
///
/// # Errors
/// A name that is empty, too long or already used; a choice field with no choices, or one
/// twice; too many fields ([`ecad_model::objects::user::fields_problem`]).
pub(crate) fn plan_add_field(doc: &Document, ids: &mut IdAllocator, name: &str, kind: UserFieldKind) -> Result<(UserFieldId, Vec<Op>), CmdError> {
    let name = FieldName::new(name.trim()).map_err(|e| CmdError::refused(format!("Field name: {e}")))?;
    if field_named(doc, name.as_str()).is_some() {
        return Err(CmdError::refused(format!("The project already has a field \"{}\".", name.as_str())));
    }
    let id: UserFieldId = ids.fresh();
    let mut after = doc.project().clone();
    after.user_fields.push(UserFieldDef { id, name, kind });
    if let Some(p) = ecad_model::objects::user::fields_problem(&after.user_fields) {
        return Err(CmdError::refused(format!("Not added: {p}.")));
    }
    Ok((id, vec![Op::ModifyProject { before: doc.project().clone(), after }]))
}

/// Remove user field `name`: a `ModifyDevice` clearing it on each device that has a value, then
/// one `ModifyProject`. Answers the ops and how many values were cleared.
///
/// # Errors
/// No such field.
pub(crate) fn plan_remove_field(doc: &Document, name: &str) -> Result<(Vec<Op>, usize), CmdError> {
    let field = field_named(doc, name).ok_or_else(|| CmdError::refused(format!("The project has no field \"{}\".", name.trim())))?;
    let mut ops = Vec::new();
    for (id, d) in doc.devices().iter().filter(|(_, d)| d.user.contains_key(&field.id)) {
        let mut after = d.clone();
        after.user.remove(&field.id);
        ops.push(Op::ModifyDevice { id: *id, before: d.clone(), after });
    }
    let cleared = ops.len();
    let mut after = doc.project().clone();
    after.user_fields.retain(|f| f.id != field.id);
    ops.push(Op::ModifyProject { before: doc.project().clone(), after });
    Ok((ops, cleared))
}

/// `setprop`'s `user.<name>` row on a device: the value read as the field's kind, or cleared
/// when empty. One `ModifyDevice`, none when nothing changes.
///
/// # Errors
/// Not a device, no such field, a value that does not read as the field's kind.
pub(crate) fn user_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let ObjectRef::Device(c) = o else {
        return Err(CmdError::refused("User fields are set on devices."));
    };
    let d = doc.devices().get(&c).ok_or_else(|| CmdError::refused(format!("There is no device {c}.")))?;
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
    doc.devices().get(&d)?.user.get(&field.id).map(|v| v.show(&field.kind))
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// What joins the two devices.
#[derive(Deserialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KindParam {
    /// Two contactors that must not close together.
    MechanicalInterlock,
    /// An auxiliary contact block fitted to a device.
    AuxiliaryBlock,
}

impl From<KindParam> for LinkKind {
    fn from(k: KindParam) -> Self {
        match k {
            KindParam::MechanicalInterlock => LinkKind::MechanicalInterlock,
            KindParam::AuxiliaryBlock => LinkKind::AuxiliaryBlock,
        }
    }
}

/// Links two devices and answers with the link's id.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkParams {
    /// What joins them.
    kind: KindParam,
    /// The two devices.
    devices: [DeviceId; 2],
}

impl Generic for LinkParams {
    const ID: &'static str = "linkdevices";
    const TITLE: &'static str = "Link Devices";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let [a, b] = self.devices;
        let (id, ops) = plan_link(cx.doc, cx.ids, self.kind.into(), a, b)?;
        Ok(Outcome::edit("Link Devices", ops, json!({"link": id.hex()})))
    }
}

/// Removes a device link.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnlinkParams {
    /// The link.
    link: LinkId,
}

impl Generic for UnlinkParams {
    const ID: &'static str = "unlinkdevices";
    const TITLE: &'static str = "Unlink Devices";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let ops = plan_unlink(cx.doc, self.link)?;
        Ok(Outcome::edit("Unlink Devices", ops, json!({"unlinked": true})))
    }
}

/// A user field's kind.
#[derive(Deserialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FieldKindParam {
    /// Free text.
    Text,
    /// A number, with `unit` shown after it.
    Number,
    /// Yes or no.
    Flag,
    /// One of `options`.
    Choice,
}

/// Adds a project user field, or with `remove: true` removes one and clears its values. A device's value is set
/// with `setprop` key "user.<name>".
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UserFieldParams {
    /// The field's name, unique in the project ignoring case.
    name: String,
    /// What it holds; required when adding.
    #[serde(default)]
    kind: Option<FieldKindParam>,
    /// A number field's unit (`mm`).
    #[serde(default)]
    unit: Option<String>,
    /// A choice field's choices, in order.
    #[serde(default)]
    options: Vec<String>,
    /// Remove the field instead.
    #[serde(default)]
    remove: bool,
}

impl UserFieldParams {
    fn kind(&self) -> Result<UserFieldKind, CmdError> {
        let refused = |e: String| CmdError::Refused(e.into());
        match self.kind {
            None => Err(refused("Give the field's kind: text, number, flag or choice.".into())),
            Some(FieldKindParam::Text) => Ok(UserFieldKind::Text),
            Some(FieldKindParam::Flag) => Ok(UserFieldKind::Flag),
            Some(FieldKindParam::Number) => {
                let unit = self.unit.as_deref().map(str::trim).filter(|u| !u.is_empty());
                let unit = unit.map(FieldUnit::new).transpose().map_err(|e| refused(format!("Unit: {e}")))?;
                Ok(UserFieldKind::Number { unit })
            }
            Some(FieldKindParam::Choice) => {
                let options = self.options.iter().map(|o| Choice::new(o.trim())).collect::<Result<Vec<_>, _>>();
                Ok(UserFieldKind::Choice { options: options.map_err(|e| refused(format!("Choice: {e}")))? })
            }
        }
    }
}

impl Generic for UserFieldParams {
    const ID: &'static str = "userfield";
    const TITLE: &'static str = "User Field";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        if self.remove {
            let (ops, cleared) = plan_remove_field(cx.doc, &self.name)?;
            return Ok(Outcome::edit("Remove User Field", ops, json!({"removed": self.name.trim(), "cleared": cleared})));
        }
        let kind = self.kind()?;
        let (id, ops) = plan_add_field(cx.doc, cx.ids, &self.name, kind)?;
        Ok(Outcome::edit("Add User Field", ops, json!({"field": id.hex(), "name": self.name.trim()})))
    }
}
