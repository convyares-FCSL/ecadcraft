//! `terminalblocks` (Place Terminals): one terminal block, or one
//! level, on each wire a fence line crosses. The rules are
//! [`crate::terminal_strips::commands::terminal_strips`]; the command is generic
//! ([`crate::commands::run`]) and keeps the last placement for the click tool's defaults.

use ecad_model::objects::wire::WireNumber;
use ecad_model::{ConnectionPointId, DeviceId, Document, Pt, RouteId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::terminal_strips::{TerminalBlocksInput, block_type, connection_points_message, plan_terminal_blocks};
use crate::commands::input::Point;
use crate::commands::input::{MAX_COORD, OUT_OF_RANGE};
use crate::commands::run::{Cx, JsonGeneric, LastPlaced, Outcome, Slot, State};
use crate::error::CmdError;

/// One terminal block, or one level, on each wire the line from `from` to `to` crosses (at most 200). A level
/// fills the next position of its type that lacks it. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalBlocksParams {
    /// The strip to join (null or absent: a new strip).
    #[serde(default)]
    strip: Option<DeviceId>,
    /// PE terminal blocks (without `type`).
    #[serde(default)]
    pe: Option<bool>,
    /// The terminal block type (also its registry code or key).
    #[serde(default, rename = "type")]
    ty: Option<crate::commands::input::Said<&'static crate::terminal_strips::terminal_blocks::BlockType>>,
    /// The level: a level name, a level number, or All.
    #[serde(default)]
    level: Option<String>,
    /// The position label for the first crossing.
    #[serde(default)]
    position: Option<String>,
    /// One end of the line.
    from: Point,
    /// The other end of the line.
    to: Point,
}

impl JsonGeneric for TerminalBlocksParams {
    const ID: &'static str = "terminalblocks";
    const TITLE: &'static str = "Place Terminals";
    const STATE: State = State::LastPlaced;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let ty = block_type(self.ty.map(|t| t.0), self.pe.unwrap_or(false))?;
        let (from, to) = (self.from.0, self.to.0);
        let bad = |q: Pt| !(q.x.abs() <= MAX_COORD && q.y.abs() <= MAX_COORD);
        if bad(from) || bad(to) {
            return Err(CmdError::refused(OUT_OF_RANGE));
        }
        let page = cx.page.ok_or(CmdError::refused("page view not found"))?;
        let input = TerminalBlocksInput { strip: self.strip, ty, level: self.level, position: self.position, from, to };
        let done = plan_terminal_blocks(cx.ledger, cx.ids, cx.tag_high, page, cx.plain.clone(), input, cx.tolerance)?;
        let f = done.fence;
        // The wire numbers on the crossed lines before: a half that ends up in another net (a fuse
        // position) does not keep its number, which is said.
        let before: Vec<Option<WireNumber>> = f
            .placed
            .iter()
            .map(|(_, _, r, _)| crate::wires::routes::wire_of_route(cx.doc, *r).and_then(|w| cx.doc.wires().get(&w)).and_then(|w| w.number.clone()))
            .collect();
        if let Slot::LastPlaced(last) = &mut cx.state {
            **last = Some(LastPlaced { starter: Some(ty.starter_key(done.level)), device: f.strip });
        }
        let (label, level, ops) = (done.label, done.level, f.ops.clone());
        Ok(Outcome::edit_then(label, ops, move |doc| {
            let dropped = dropped_numbers(doc, &f.placed, &before);
            let (shown, message) = connection_points_message(doc, ty, level, &f);
            let message = if dropped.is_empty() {
                message
            } else {
                let list: Vec<&str> = dropped.iter().map(|(_, n)| n.text.as_str()).collect();
                format!(
                    "{message} Out halves no longer carry wire number{} {}; they get their own.",
                    if list.len() == 1 { "" } else { "s" },
                    list.join(", ")
                )
            };
            json!({
                "strip": f.strip.hex(),
                "tag": f.tag,
                "type": ty.code,
                "connection_points": f.placed.iter().zip(&shown).map(|((t, l, r, n), sl)| json!({"connection_point": t.hex(), "position": l, "label": sl, "route": r.hex(), "new": n.hex()})).collect::<Vec<_>>(),
                "skipped": f.skipped,
                "dropped_wire_numbers": dropped.iter().map(|(w, n)| json!({"conductor": w.hex(), "number": n.text.as_str(), "fixed": n.fixed})).collect::<Vec<_>>(),
                "message": message,
            })
        })
        .echo_message())
    }
}

/// The halves of crossed lines that no longer carry the number their line's wire carried before
/// (`before`, by placed connection point): each half's wire and the number it lost.
fn dropped_numbers(
    doc: &Document, placed: &[(ConnectionPointId, String, RouteId, RouteId)], before: &[Option<WireNumber>],
) -> Vec<(WireId, WireNumber)> {
    let mut out = Vec::new();
    for ((_, _, r, n), was) in placed.iter().zip(before) {
        let Some(was) = was else { continue };
        for half in [*r, *n] {
            let Some(w) = crate::wires::routes::wire_of_route(doc, half) else { continue };
            if doc.wires().get(&w).and_then(|x| x.number.as_ref()).map(|x| &x.text) != Some(&was.text) {
                out.push((w, was.clone()));
            }
        }
    }
    out
}
