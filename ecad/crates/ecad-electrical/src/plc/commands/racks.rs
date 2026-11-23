//! `plcslot`, `plcracks` and `plccardtype`: cards in racks. The rules
//! are [`crate::plc::rack`] and [`super`]; each command is generic ([`crate::commands::run`]).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{RackChoice, io_type, plan_card_type, slot_given, slot_plan};
use crate::commands::input::{TextOrNumber, nullable};
use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::error::CmdError;
use crate::plc::rack::*;

/// `plcslot {card, rack?, slot?, swap?}`: put a card in a slot, move it, swap it
/// with the card there (`swap: true`), or take it out (`rack: null`). One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlcslotParams {
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

impl Generic for PlcslotParams {
    const ID: &'static str = "plcslot";
    const TITLE: &'static str = "Card Slot";
    const HELP: &'static str = "{card: id or tag, rack?: id or tag (null: take the card out of its rack; absent: its own rack, else the only one), slot?: number (absent: the next free slot), swap?: bool (swap with the card in that slot)}";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let swap = self.swap.unwrap_or(false);
        let out = matches!(self.rack, Some(None));
        let slot = slot_given(out, self.slot.map(|t| t.text()).as_deref())?;
        let rack = match &self.rack {
            None => RackChoice::Default,
            Some(None) => RackChoice::Out,
            Some(Some(t)) => RackChoice::Named(t),
        };
        let doc = cx.doc;
        let (card, ops) = slot_plan(doc, &self.card, rack, slot, swap)?;
        let who = crate::devices::tags::tag_words(doc, card);
        let was = place_words(doc, card);
        let before = crate::plc::moved_blocks_before(doc, card);
        if ops.is_empty() {
            let message = format!("{who} is already in {was}.");
            return Ok(Outcome::answer(json!({"card": card.hex(), "changed": false, "message": message})).echo_message());
        }
        Ok(Outcome::edit_then("Move card", ops, move |doc| {
            let now = place_words(doc, card);
            let moved = crate::plc::moved_cards(doc, card, &before);
            let tail = if moved > 0 { format!(" (addresses of {moved} card{} change)", if moved == 1 { "" } else { "s" }) } else { String::new() };
            let message = format!("{who}: {was} → {now}{tail}.");
            let (rack, slot) = match (rack_ref(doc, card), slot_of(doc, card)) {
                (RackRef::Rack(r), s) => (Some(r.hex()), s),
                _ => (None, None),
            };
            json!({"card": card.hex(), "rack": rack, "slot": slot, "changed": true, "moved": moved, "message": message})
        })
        .echo_message())
    }
}

/// `plcracks {rack?}`: each rack with its slots in order, and the cards in no
/// rack; read only, from the kept model.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlcracksParams {
    /// Only this rack: an ID or a tag.
    #[serde(default)]
    rack: Option<String>,
}

impl Generic for PlcracksParams {
    const ID: &'static str = "plcracks";
    const TITLE: &'static str = "PLC Racks";
    const HELP: &'static str = "{rack?: id or tag} (each rack with its slots in order, and the cards in no rack; read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let doc = cx.doc;
        let only = match &self.rack {
            Some(t) => Some(rack_named(doc, t)?),
            None => None,
        };
        let plc = &cx.derived.reports(doc).plc;
        let mut racks = Vec::new();
        for r in plc.racks_by_tag() {
            if only.is_some_and(|x| x != r.id) {
                continue;
            }
            racks.push(plc.rack_json(doc, r));
        }
        let unracked: Vec<Value> =
            if only.is_some() { Vec::new() } else { plc.unracked_by_tag().into_iter().map(|c| plc.card_json(doc, c)).collect() };
        Ok(Outcome::answer(json!({"racks": racks, "unracked": unracked})))
    }
}

/// `plccardtype {card, type}`: change what an I/O card is, in one transaction
/// ([`plan_card_type`]). Refused, in words, when a signal (a channel in use, named or drawn) sits at
/// or past the new type's channel count.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlccardtypeParams {
    /// The card: an ID or a tag.
    card: String,
    /// The new type: DI, DQ, AI, AO, RTD or IOL.
    #[serde(rename = "type")]
    ty: String,
}

impl Generic for PlccardtypeParams {
    const ID: &'static str = "plccardtype";
    const TITLE: &'static str = "Card Type";
    const HELP: &'static str = "{card: id or tag, type: DI|DQ|AI|AO|RTD|IOL} (changes what an I/O card is: its body symbol, one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let to = io_type(&self.ty)?;
        let plan = plan_card_type(cx.doc, cx.ids, &self.card, to)?;
        let (card, message) = (plan.card, plan.message);
        if plan.ops.is_empty() {
            return Ok(Outcome::answer(json!({"card": card.hex(), "changed": false, "message": message})).echo_message());
        }
        let answer = json!({"card": card.hex(), "changed": true, "type": to.code(), "message": message});
        Ok(Outcome::edit("Card type", plan.ops, answer).echo_message())
    }
}
