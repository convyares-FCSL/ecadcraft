//! `linkdevices`, `unlinkdevices` (design 072 §2.6) and `userfield` (§2.8).
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::device_link`] decide, and the shim commits the ops and answers in JSON.
//! A device's value of a user field is set with `setprop {key: "user.<name>"}`.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::device_link as rules;
use ecad_model::DeviceId;
use ecad_model::id::LinkId;
use ecad_model::user::{Choice, FieldUnit, LinkKind, UserFieldKind};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Typed};
use crate::cmd::on_project;
use ecad_electrical::CmdError;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] =
    &[Typed::of::<LinkParams>("linkdevices"), Typed::of::<UnlinkParams>("unlinkdevices"), Typed::of::<UserFieldParams>("userfield")];

/// This module's commands (registered in [`crate::lanes`]).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("linkdevices", "Link Devices", crate::cmd::input::typed::<LinkParams>)
            .params("{kind: \"mechanical_interlock\" | \"auxiliary_block\", devices: [id, id]} (link two devices; answers the link id)"),
        CommandSpec::new("unlinkdevices", "Unlink Devices", crate::cmd::input::typed::<UnlinkParams>)
            .params("{link: id} (remove a device link)"),
        CommandSpec::new("userfield", "User Field", crate::cmd::input::typed::<UserFieldParams>).params(
            "{name, kind?: \"text\" | \"number\" | \"flag\" | \"choice\", unit?, options?: [text], remove?: bool} (add a project user field, or remove one and clear its values; set a device's value with setprop key \"user.<name>\")",
        ),
    ]
}

/// What joins the two devices.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
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

/// `linkdevices {kind, devices: [a, b]}`: link two devices.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct LinkParams {
    /// What joins them.
    kind: KindParam,
    /// The two devices.
    devices: [DeviceId; 2],
}

impl Command for LinkParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let [a, b] = self.devices;
            let (id, ops) = rules::plan_link(p.ledger.document(), &mut p.ids, self.kind.into(), a, b)?;
            p.apply_edit(pg, "linkdevices", "Link Devices", ops)?;
            Ok(json!({"link": id.hex()}))
        })
    }
}

/// `unlinkdevices {link}`: remove a device link.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnlinkParams {
    /// The link.
    link: LinkId,
}

impl Command for UnlinkParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let ops = rules::plan_unlink(p.ledger.document(), self.link)?;
            p.apply_edit(pg, "unlinkdevices", "Unlink Devices", ops)?;
            Ok(json!({"unlinked": true}))
        })
    }
}

/// A user field's kind.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
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

/// `userfield {name, kind, unit?, options?}` adds a project user field; `{name, remove: true}`
/// removes it and clears its values.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserFieldParams {
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

impl Command for UserFieldParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            if self.remove {
                let (ops, cleared) = rules::plan_remove_field(p.ledger.document(), &self.name)?;
                p.apply_edit(pg, "userfield", "Remove User Field", ops)?;
                return Ok(json!({"removed": self.name.trim(), "cleared": cleared}));
            }
            let kind = self.kind()?;
            let (id, ops) = rules::plan_add_field(p.ledger.document(), &mut p.ids, &self.name, kind)?;
            p.apply_edit(pg, "userfield", "Add User Field", ops)?;
            Ok(json!({"field": id.hex(), "name": self.name.trim()}))
        })
    }
}
