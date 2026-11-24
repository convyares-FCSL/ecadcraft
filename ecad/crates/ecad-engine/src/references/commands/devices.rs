//! `refgoto`: opens the place a reference shows, so it is the
//! engine's; its interactive form is in [`crate::references::interact::devices`]. The other cross-reference commands
//! are generic ([`ecad_electrical::references::commands::devices`]).

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::references::commands::devices as rules;
use ecad_model::RepresentationId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::on_any_page;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<RefgotoParams>("refgoto")];

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("refgoto", "Go to Reference", crate::commands::input::typed::<RefgotoParams>).noundo()]
}

/// Opens the place a cable reference end, a contact, a coil or a continuation arrow shows. Records nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RefgotoParams {
    /// The reference clicked (a representation).
    source: RepresentationId,
    /// Which of its other ends (default: the first, or the next after the last go-to).
    #[serde(default)]
    target: Option<RepresentationId>,
}

impl Command for RefgotoParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let (uid, jump, opened) = on_any_page(s, |p, pg| {
            let ring = p.display.goto_ring.map(|(page, at, _)| (page, at));
            let jump = rules::goto(p.ledger.document(), p.reference(), self.source, self.target, ring)?;
            let opened = p.pages.get(&jump.page).and_then(|v| v.uid).is_none();
            let uid = p.open_page(pg, jump.page).map_err(CmdError::refused)?;
            // The go-to ring (display only, until the next change), as the Checks tab's.
            p.display.goto_ring = Some((jump.page, jump.at, ecad_electrical::checks::Severity::Info));
            Ok((uid, jump, opened))
        })?;
        if opened {
            let _ = s.zoom_extents();
        }
        if let Ok(st) = s.state_mut() {
            let height = st.view().height;
            let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(jump.at.x, jump.at.y), height, twist: 0.0 });
        }
        let page = jump.page;
        let item = jump.item;
        let handle = on_any_page(s, |p, _| Ok(item.and_then(|i| p.pages.get(&page).and_then(|v| v.map.item(i)))))?;
        if let Some(h) = handle {
            s.set_selection(vec![h]);
        }
        Ok(json!({
            "page": jump.page.hex(),
            "document": uid,
            "target": jump.target.hex(),
            "item": jump.item.map(|i| i.hex()),
            "message": jump.message,
        }))
    }
}
