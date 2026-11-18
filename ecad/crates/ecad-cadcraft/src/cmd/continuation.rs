//! Brief 049's command (design 049 §5.1): `contcaption` sets or removes the caption of continuation
//! arrows, one transaction of `SetProperty` ops (only for arrows whose caption changes). Its
//! interactive form (select arrows, type the caption) is in [`crate::interact`].
//!
//! Excludes: what a caption draws ([`crate::continuations`]); `setprop` and `place` take the same
//! rules through [`crate::continuations::caption_value`].
//!
//! A thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::continuation`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::continuation as rules;
use ecad_model::DeviceId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

pub use ecad_electrical::cmd::continuation::{MAX_ARROWS, label};

use crate::cmd::input::{Bounded, Command, OrNull, Typed, nullable};
use crate::cmd::{err, on_project};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<ContcaptionParams>("contcaption")];

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("contcaption", "Caption Continuations", crate::cmd::input::typed::<ContcaptionParams>)
            .params("{continuations: [id…] (1-2000), caption: \"TO CONTROL SUPPLY\" | null} (sets or removes the arrows' caption, one transaction)")
            .interactive(crate::interact::contcaption)
            .noundo(),
    ]
}

/// `contcaption {continuations, caption}` (design 049 §5.1): sets (or with null removes) the
/// arrows' caption, one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContcaptionParams {
    /// The continuation arrows (1 to 2000).
    continuations: Bounded<DeviceId, 0, MAX_ARROWS>,
    /// The caption, or null to remove it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<String>")]
    caption: Option<Option<String>>,
}

impl Command for ContcaptionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let caption = self.caption.ok_or_else(|| err("`caption` (text, or null) is required"))?;
        let arrows = self.continuations.0;
        let v = on_project(s, |p, pg, _| {
            let plan = rules::plan_contcaption(p.ledger.document(), &arrows, caption.as_deref())?;
            if !plan.ops.is_empty() {
                p.apply_edit(pg, "contcaption", &plan.label, plan.ops)?;
            }
            if let Some(t) = &plan.caption {
                p.last_caption = Some(t.clone());
            }
            Ok(json!({"changed": plan.changed, "unchanged": plan.unchanged, "caption": plan.caption, "message": plan.message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
