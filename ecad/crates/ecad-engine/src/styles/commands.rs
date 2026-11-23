//! The session commands `showtext` and `hiddentext`. Writes commit
//! through [`crate::project::Project::apply_edit`] as one transaction; `textstyle`, `pagetexts`,
//! `movetext` and `defaults` are generic commands in `ecad_electrical::styles::commands`, run by
//! [`crate::generic`].
//!
//! Each command is a thin shim: its params struct reads the JSON, the rules in
//! [`ecad_drawing_style::text::commands`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_drawing_style::text::commands::{self as rules, ShowLevel, ShowTarget};
use ecad_model::Word;
use ecad_model::{DeviceId, PageId, RepresentationId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::err;
use crate::commands::input::{Command, Typed};
use crate::commands::{on_any_page, on_project};
use ecad_electrical::commands::input::nullable;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<ShowtextParams>("showtext"), Typed::of::<HiddentextParams>("hiddentext")];

/// The commands of this module, appended to the registry by [`crate::commands::specs`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("showtext", "Show/Hide Text", crate::commands::input::typed::<ShowtextParams>)
            .interactive(crate::styles::interact::showtext)
            .noundo(),
        CommandSpec::new("hiddentext", "Show Hidden Text", crate::commands::input::typed::<HiddentextParams>).noundo(),
    ]
}

// ---------------------------------------------------------------- showtext

/// Whether a kind of text is shown, at one level: a symbol, a device, a wire or a page (at most one of them; none
/// is the project). Null inherits.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShowtextParams {
    /// The kind: tag, point_label, …
    kind: String,
    /// "on", "off" or "first" (tag only), or null to inherit.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "ecad_electrical::commands::input::OrNull<String>")]
    show: Option<Option<String>>,
    /// A symbol of a device.
    #[serde(default)]
    representation: Option<RepresentationId>,
    /// A device.
    #[serde(default)]
    component: Option<DeviceId>,
    /// A wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// A page.
    #[serde(default)]
    page: Option<PageId>,
}

impl Command for ShowtextParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let kind = ecad_electrical::styles::commands::text::kind(&self.kind).map_err(|e| err(e.to_string()))?;
        let show = rules::show_word(self.show.flatten().as_deref()).map_err(|e| err(e.to_string()))?;
        let target = ShowTarget { representation: self.representation, component: self.component, conductor: self.conductor, page: self.page };
        on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let level = rules::show_level(doc, &target)?;
            let ops = rules::show_ops(doc, level, kind, show.as_deref())?;
            let label = rules::show_label(doc, level, kind, show.as_deref());
            let changed = !ops.is_empty();
            if changed {
                p.apply_edit(pg, "showtext", &label, ops)?;
            }
            let target = match level {
                ShowLevel::Symbol(r, _) => json!(r.hex()),
                ShowLevel::Device(c) => json!(c.hex()),
                ShowLevel::Wire(c) => json!(c.hex()),
                ShowLevel::Page(pg) => json!(pg.hex()),
                ShowLevel::Project => Value::Null,
            };
            Ok(json!({"level": level.word(), "target": target, "kind": kind.word(), "show": show, "changed": changed}))
        })
    }
}

/// Show Hidden Text on, off, or toggled when `on` is absent. Display only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct HiddentextParams {
    /// On or off (default: toggle).
    #[serde(default)]
    on: Option<bool>,
}

impl Command for HiddentextParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let on = self.on;
        on_any_page(s, |p, pg| {
            let want = on.unwrap_or(!p.show_hidden_text);
            let changed = p.set_hidden_text(pg, want);
            Ok(json!({"on": want, "changed": changed}))
        })
    }
}
