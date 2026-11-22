//! `plcmaketags` (brief 056): make the PLC tags of the channels. The rules are
//! [`crate::plc::signals`]; the command is generic ([`crate::commands::run`]).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::tag_cards;
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;
use crate::plc::signals::make_tags_ops;

/// `plcmaketags {card | rack}` (design 056 §4.3): fill the empty tags of a card's (or a rack's
/// cards') used and reserved channels from their descriptions, in one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlcmaketagsParams {
    /// The card: an ID or a tag.
    #[serde(default)]
    card: Option<String>,
    /// Every card of this rack: an ID or a tag.
    #[serde(default)]
    rack: Option<String>,
}

impl Generic for PlcmaketagsParams {
    const ID: &'static str = "plcmaketags";
    const TITLE: &'static str = "Make PLC Tags";
    const HELP: &'static str = "{card: id or tag} or {rack: id or tag} (fill the empty tags of used and reserved channels from their descriptions)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let doc = cx.doc;
        let (cards, who) = tag_cards(doc, self.card.as_deref(), self.rack.as_deref())?;
        let (ops, counts) = make_tags_ops(doc, &cards);
        let made: usize = counts.iter().map(|(_, n)| *n).sum();
        if made == 0 {
            let message = format!("Every used channel of {who} has a tag or no description to make one from.");
            return Ok(Outcome::answer(json!({"made": 0, "changed": false, "message": message})).echo_message());
        }
        let per: Vec<Value> = counts.iter().filter(|(_, n)| *n > 0).map(|(c, n)| json!({"card": c.hex(), "made": n})).collect();
        let t = if made == 1 { "tag" } else { "tags" };
        let message = format!("Made {made} {t} on {who}.");
        Ok(Outcome::edit("Make PLC tags", ops, json!({"made": made, "cards": per, "changed": true, "message": message})).echo_message())
    }
}
