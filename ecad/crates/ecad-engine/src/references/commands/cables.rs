//! `cablegoto`: opens the other end's page and moves the view
//! there, so it is the engine's; its interactive form is in [`crate::references::interact::cables`]. The other cable
//! reference commands are generic ([`ecad_electrical::references::commands::cables`]).

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::references::commands::cables as rules;
use ecad_model::RepresentationId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::on_any_page;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<CablegotoParams>("cablegoto")];

/// Opens (or fronts) the page of the other end of a cable reference, centres the view on it and selects its
/// point. Records nothing; works on a read-only project.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablegotoParams {
    /// A reference end.
    reference: RepresentationId,
}

impl Command for CablegotoParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let (uid, there, opened) = on_any_page(s, |p, pg| {
            let there = rules::other_end(p.ledger.document(), self.reference)?;
            let opened = p.pages.get(&there.page).and_then(|v| v.uid).is_none();
            let uid = p.open_page(pg, there.page).map_err(CmdError::refused)?;
            Ok((uid, there, opened))
        })?;
        if opened {
            let _ = s.zoom_extents();
        }
        if let Ok(st) = s.state_mut() {
            let height = st.view().height;
            let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(there.at.x, there.at.y), height, twist: 0.0 });
        }
        let handle = on_any_page(s, |p, _| Ok(p.pages.get(&there.page).and_then(|v| v.map.item(there.item))))?;
        if let Some(h) = handle {
            s.set_selection(vec![h]);
        }
        Ok(json!({
            "page": there.page.hex(),
            "document": uid,
            "reference": there.reference.hex(),
            "item": there.item.hex(),
            "message": there.message,
        }))
    }
}
