//! `plcslot`, `plcracks` and `plccardtype` (brief 045, design 045 §3): cards in racks. The rules
//! are [`crate::plc::rack`] and [`ecad_electrical::cmd::plc`].

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::cmd::plc::{self as rules, RackChoice};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::Command;
use crate::cmd::{err, on_any_page, on_project};
use crate::plc::rack::*;
use ecad_electrical::cmd::input::{TextOrNumber, nullable};

/// `plcslot {card, rack?, slot?, swap?}` (design 045 §2.5): put a card in a slot, move it, swap it
/// with the card there (`swap: true`), or take it out (`rack: null`). One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcslotParams {
    /// The card: an ID or a tag.
    card: String,
    /// The rack: an ID or a tag; null takes the card out of its rack; absent: its own rack, else
    /// the only one.
    #[serde(default, deserialize_with = "nullable")]
    rack: Option<Option<String>>,
    /// The slot, 0–999 (absent: the next free slot).
    #[serde(default)]
    slot: Option<TextOrNumber>,
    /// Swap with the card in that slot.
    #[serde(default)]
    swap: Option<bool>,
}

impl Command for PlcslotParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let swap = self.swap.unwrap_or(false);
        let out = matches!(self.rack, Some(None));
        let slot = rules::slot_given(out, self.slot.map(|t| t.text()).as_deref()).map_err(|e| err(e.to_string()))?;
        let rack = match &self.rack {
            None => RackChoice::Default,
            Some(None) => RackChoice::Out,
            Some(Some(t)) => RackChoice::Named(t),
        };
        let v = on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let (card, ops) = rules::slot_plan(doc, &self.card, rack, slot, swap)?;
            let who = crate::devices::tag_words(doc, card);
            let was = place_words(doc, card);
            let before = crate::plc::moved_blocks_before(doc, card);
            if ops.is_empty() {
                return Ok(json!({"card": card.hex(), "changed": false, "message": format!("{who} is already in {was}.")}));
            }
            p.apply_edit(pg, "plcslot", "Move card", ops)?;
            let doc = p.ledger.document();
            let now = place_words(doc, card);
            let moved = crate::plc::moved_cards(doc, card, &before);
            let tail = if moved > 0 { format!(" (addresses of {moved} card{} change)", if moved == 1 { "" } else { "s" }) } else { String::new() };
            let message = format!("{who}: {was} → {now}{tail}.");
            let (rack, slot) = match (rack_ref(doc, card), slot_of(doc, card)) {
                (RackRef::Rack(r), s) => (Some(r.hex()), s),
                _ => (None, None),
            };
            Ok(json!({"card": card.hex(), "rack": rack, "slot": slot, "changed": true, "moved": moved, "message": message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

/// `plcracks {rack?}` (design 045 §2.5): each rack with its slots in order, and the cards in no
/// rack; read only, from the kept model.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcracksParams {
    /// Only this rack: an ID or a tag.
    #[serde(default)]
    rack: Option<String>,
}

impl Command for PlcracksParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| {
            let only = match &self.rack {
                Some(t) => Some(rack_named(p.ledger.document(), t)?),
                None => None,
            };
            p.outputs();
            let o = p.kept_outputs().ok_or(CmdError::refused("The PLC model is not available."))?;
            let doc = p.ledger.document();
            let plc = &o.plc;
            let mut racks = Vec::new();
            for r in plc.racks_by_tag() {
                if only.is_some_and(|x| x != r.id) {
                    continue;
                }
                racks.push(plc.rack_json(doc, r));
            }
            let unracked: Vec<Value> =
                if only.is_some() { Vec::new() } else { plc.unracked_by_tag().into_iter().map(|c| plc.card_json(doc, c)).collect() };
            Ok(json!({"racks": racks, "unracked": unracked}))
        })
    }
}

/// `plccardtype {card, type}`: change what an I/O card is, in one transaction
/// ([`rules::plan_card_type`]). Refused, in words, when a signal (a channel in use, named or drawn)
/// sits at or past the new type's channel count.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlccardtypeParams {
    /// The card: an ID or a tag.
    card: String,
    /// The new type: DI, DQ, AI, AO, RTD or IOL.
    #[serde(rename = "type")]
    ty: String,
}

impl Command for PlccardtypeParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let to = rules::io_type(&self.ty).map_err(|e| err(e.to_string()))?;
        let v = on_project(s, |p, pg, _| {
            let plan = rules::plan_card_type(p.ledger.document(), &mut p.ids, &self.card, to)?;
            if plan.ops.is_empty() {
                return Ok(json!({"card": plan.card.hex(), "changed": false, "message": plan.message}));
            }
            p.apply_edit(pg, "plccardtype", "Card type", plan.ops)?;
            Ok(json!({"card": plan.card.hex(), "changed": true, "type": to.code(), "message": plan.message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
