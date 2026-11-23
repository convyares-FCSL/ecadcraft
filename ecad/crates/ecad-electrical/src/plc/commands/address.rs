//! `plcaddress`: a rack's address style and starts, a card's pinned
//! start. The rules are [`crate::plc::address`] and [`super::plan_address`]; the command is
//! generic ([`crate::commands::run`]).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{AddressInput, plan_address};
use crate::commands::input::{TextOrNumber, nullable};
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;
use crate::plc::address::*;

/// A PLC rack's addressing (`{rack, style?, input_start?, output_start?, fix?, clear?}`) or one card's (`{card,
/// start?, size?}`). One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlcaddressParams {
    /// The rack: an ID or a tag.
    #[serde(default)]
    rack: Option<String>,
    /// The card (without `rack`): an ID or a tag.
    #[serde(default)]
    card: Option<String>,
    /// The rack's address style: byte.bit, iec, percent, slot or none; null: the default.
    #[serde(default, deserialize_with = "nullable")]
    style: Option<Option<TextOrNumber>>,
    /// The rack's first input byte; null: the default.
    #[serde(default, deserialize_with = "nullable")]
    input_start: Option<Option<TextOrNumber>>,
    /// The rack's first output byte; null: the default.
    #[serde(default, deserialize_with = "nullable")]
    output_start: Option<Option<TextOrNumber>>,
    /// The card's pinned start byte; null unpins it.
    #[serde(default, deserialize_with = "nullable")]
    start: Option<Option<TextOrNumber>>,
    /// The card's size in bytes; null: by its type.
    #[serde(default, deserialize_with = "nullable")]
    size: Option<Option<TextOrNumber>>,
    /// Pin every card of the rack at its current start.
    #[serde(default)]
    fix: Option<bool>,
    /// Unpin every card of the rack.
    #[serde(default)]
    clear: Option<bool>,
}

impl Generic for PlcaddressParams {
    const ID: &'static str = "plcaddress";
    const TITLE: &'static str = "PLC Addresses";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let text = |v: Option<Option<TextOrNumber>>| v.map(|v| v.map(|t| t.text()));
        let input = AddressInput {
            rack: self.rack,
            card: self.card,
            style: text(self.style),
            input_start: text(self.input_start),
            output_start: text(self.output_start),
            start: text(self.start),
            size: text(self.size),
            fix: self.fix.unwrap_or(false),
            clear: self.clear.unwrap_or(false),
        };
        let doc = cx.doc;
        let plan = plan_address(doc, &input)?;
        let who = plan.who;
        if plan.ops.is_empty() {
            return Ok(Outcome::answer(json!({"changed": false, "message": format!("{who}: nothing to change.")})).echo_message());
        }
        let rack = plan.rack;
        let before = rack.map(|r| snapshot(doc, r)).unwrap_or_default();
        let tail = plan.tail;
        Ok(Outcome::edit_then("Addresses", plan.ops, move |doc| {
            let after = rack.map(|r| snapshot(doc, r)).unwrap_or_default();
            let moved = after.iter().filter(|(c, b)| before.get(c).is_some_and(|x| x.input != b.input || x.output != b.output)).count();
            let mut parts = Vec::new();
            if !tail.is_empty() {
                parts.push(tail);
            }
            parts.push(format!("addresses of {moved} card{} change", if moved == 1 { "" } else { "s" }));
            let message = format!("{who}: {}.", parts.join("; "));
            json!({"changed": true, "moved": moved, "message": message})
        })
        .echo_message())
    }
}
