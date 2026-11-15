//! `plcmaketags` (brief 056): make the PLC tags of the channels. The rules are
//! [`crate::plc::signals`].

use cadcraft_engine::Session;

use crate::commands::with_project;
use crate::plc::rack::{self};
use crate::plc::signals::*;
use ecad_model::ComponentId;
use serde_json::{Value, json};

/// `plcmaketags {card | rack}` (design 056 §4.3): fill the empty tags of a card's (or a rack's
/// cards') used and reserved channels from their descriptions, in one transaction.
pub(crate) fn run_plcmaketags(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let (cards, who) = match (params.get("card").filter(|v| !v.is_null()), params.get("rack").filter(|v| !v.is_null())) {
            (Some(c), None) => {
                let c = rack::card_param(doc, Some(c))?;
                (vec![c], crate::devices::tag_words(doc, c))
            }
            (None, Some(r)) => {
                let r = rack::rack_param(doc, r)?;
                let cards: Vec<ComponentId> = rack::cards_naming(doc, r).into_iter().map(|(c, _)| c).collect();
                (cards, format!("rack {}", crate::devices::tag_words(doc, r)))
            }
            _ => return Err("Give `card` or `rack` (an ID or tag).".into()),
        };
        let (ops, counts) = make_tags_ops(doc, &cards);
        let made = ops.len();
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
