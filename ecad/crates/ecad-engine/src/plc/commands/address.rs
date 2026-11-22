//! `plcaddress` (brief 045, design 045 §5): a rack's address style and starts, a card's pinned
//! start. The rules are [`crate::plc::address`] and [`ecad_electrical::plc::commands::plan_address`].

use cadcraft_engine::Session;
use ecad_electrical::plc::commands::{self as rules, AddressInput};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::commands::input::Command;
use crate::commands::on_project;
use crate::plc::address::*;
use ecad_electrical::commands::input::{TextOrNumber, nullable};
use serde_json::{Value, json};

/// `plcaddress {rack | card, style?, input_start?, output_start?, start?, size?, fix?, clear?}`
/// (design 045 §5.3). One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcaddressParams {
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

impl Command for PlcaddressParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
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
        let v = on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let plan = rules::plan_address(doc, &input)?;
            let who = plan.who;
            if plan.ops.is_empty() {
                return Ok(json!({"changed": false, "message": format!("{who}: nothing to change.")}));
            }
            let before = plan.rack.map(|r| snapshot(doc, r)).unwrap_or_default();
            p.apply_edit(pg, "plcaddress", "Addresses", plan.ops)?;
            let doc = p.ledger.document();
            let after = plan.rack.map(|r| snapshot(doc, r)).unwrap_or_default();
            let moved = after.iter().filter(|(c, b)| before.get(c).is_some_and(|x| x.input != b.input || x.output != b.output)).count();
            let mut parts = Vec::new();
            if !plan.tail.is_empty() {
                parts.push(plan.tail);
            }
            parts.push(format!("addresses of {moved} card{} change", if moved == 1 { "" } else { "s" }));
            let message = format!("{who}: {}.", parts.join("; "));
            Ok(json!({"changed": true, "moved": moved, "message": message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
