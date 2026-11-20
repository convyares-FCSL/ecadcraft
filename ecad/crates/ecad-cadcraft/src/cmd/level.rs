//! The commands of brief 036 (design 036 §3.5, §4.2): `termlabel` (relabel a position, set or
//! clear one level's own label, move a level to another position) and `toposition` (turn a brief
//! 034 fuse terminal device into a fuse position of a strip).
//!
//! Every write is one transaction through [`crate::project::Project::apply_edit`]; refusals say
//! what to do instead and write nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::level`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::level::{self as rules, MoveTo, TermlabelInput};
use ecad_model::{DeviceId, TerminalId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

pub use ecad_electrical::cmd::level::{ToPosition, plan_toposition};

use crate::cmd::input::{Command, Typed, nullable};
use crate::cmd::on_project;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<TermlabelParams>("termlabel"), Typed::of::<TopositionParams>("toposition")];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("termlabel", "Terminal Label", crate::cmd::input::typed::<TermlabelParams>)
            .params(
                "{terminal?: id | strip?: id + at?: position, level?: n, position?: new position label (relabels every level), \
text?: a level's own label (null clears it), move_to?: \"next\" | \"new\" (a level drawn twice, or another type's levels)} \
(design 036 §4.2; one transaction)",
            )
            .noundo(),
        CommandSpec::new("toposition", "Make Strip Position", crate::cmd::input::typed::<TopositionParams>)
            .params(
                "{device: id (a fuse terminal device), strip?: id (default: the nearest strip on its page), position?: label} \
(the device becomes a fuse position of the strip, wires and rating kept; one transaction)",
            )
            .noundo(),
    ]
}

// ---------------------------------------------------------------- termlabel (§4.2)

/// Where a level moves.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum MoveToParam {
    /// The next position of its type that lacks the level (a level drawn twice).
    Next,
    /// A new position (another type's levels).
    New,
}

/// `termlabel {terminal | strip + at, level?, position?, text?, move_to?}` (design 036 §4.2):
/// relabel a position, set or clear one level's own label, or move a level to another position.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TermlabelParams {
    /// A terminal of the position (and its level).
    #[serde(default)]
    terminal: Option<TerminalId>,
    /// The strip (with `at`).
    #[serde(default)]
    strip: Option<DeviceId>,
    /// The position's current label.
    #[serde(default)]
    at: Option<String>,
    /// With `strip`: the level (default 1).
    #[serde(default)]
    level: Option<u64>,
    /// The position's new label (relabels every level).
    #[serde(default)]
    position: Option<String>,
    /// A level's own label; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    text: Option<Option<String>>,
    /// "next" or "new", with `terminal`: move the level to another position.
    #[serde(default)]
    move_to: Option<MoveToParam>,
}

impl Command for TermlabelParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let input = TermlabelInput {
            terminal: self.terminal,
            strip: self.strip,
            at: self.at,
            level: self.level,
            position: self.position.map(|x| x.trim().to_string()),
            text: self.text.map(|t| t.map(|t| t.trim().to_string())),
            move_to: self.move_to.map(|m| match m {
                MoveToParam::Next => MoveTo::Next,
                MoveToParam::New => MoveTo::New,
            }),
        };
        let v = on_project(s, |p, pg, _| {
            let plan = rules::plan_termlabel(p.ledger.document(), input)?;
            if plan.ops.is_empty() {
                return Ok(json!({"changed": false, "message": "Nothing to change."}));
            }
            p.apply_edit(pg, "termlabel", plan.label, plan.ops)?;
            Ok(json!({"changed": true, "message": plan.message, "lines": plan.lines}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

// ---------------------------------------------------------------- toposition (§3.5)

/// `toposition {device, strip?, position?}` (design 036 §3.5): the fuse terminal device becomes a
/// fuse position of the strip, wires and rating kept; one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TopositionParams {
    /// The fuse terminal device.
    device: DeviceId,
    /// The strip (default: the nearest strip on its page).
    #[serde(default)]
    strip: Option<DeviceId>,
    /// The position's label (default the next free one).
    #[serde(default)]
    position: Option<String>,
}

impl Command for TopositionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let v = on_project(s, |p, pg, _| {
            let plan = plan_toposition(p.ledger.document(), &mut p.ids, self.device, self.strip, self.position.as_deref())?;
            let message = plan.message();
            p.apply_edit(pg, "toposition", &plan.label, plan.ops)?;
            Ok(
                json!({"changed": true, "strip": plan.strip.hex(), "position": plan.position, "lost": plan.lost, "note": plan.note, "message": message}),
            )
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
