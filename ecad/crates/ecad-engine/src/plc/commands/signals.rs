//! `plcmaketags` (brief 056): make the PLC tags of the channels. The rules are
//! [`ecad_electrical::plc::signals`].

use cadcraft_engine::Session;
use ecad_electrical::plc::commands as rules;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::commands::input::Command;
use crate::commands::on_project;
use ecad_electrical::plc::signals::*;
use serde_json::{Value, json};

/// `plcmaketags {card | rack}` (design 056 §4.3): fill the empty tags of a card's (or a rack's
/// cards') used and reserved channels from their descriptions, in one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcmaketagsParams {
    /// The card: an ID or a tag.
    #[serde(default)]
    card: Option<String>,
    /// Every card of this rack: an ID or a tag.
    #[serde(default)]
    rack: Option<String>,
}

impl Command for PlcmaketagsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let v = on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let (cards, who) = rules::tag_cards(doc, self.card.as_deref(), self.rack.as_deref())?;
            let (ops, counts) = make_tags_ops(doc, &cards);
            let made: usize = counts.iter().map(|(_, n)| *n).sum();
            if made == 0 {
                return Ok(
                    json!({"made": 0, "changed": false, "message": format!("Every used channel of {who} has a tag or no description to make one from.")}),
                );
            }
            let per: Vec<Value> = counts.iter().filter(|(_, n)| *n > 0).map(|(c, n)| json!({"card": c.hex(), "made": n})).collect();
            p.apply_edit(pg, "plcmaketags", "Make PLC tags", ops)?;
            let t = if made == 1 { "tag" } else { "tags" };
            Ok(json!({"made": made, "cards": per, "changed": true, "message": format!("Made {made} {t} on {who}.")}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
