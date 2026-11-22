//! `iolist` (brief 045, design 045 §7): the I/O list rows, read only. The rows are
//! [`crate::plc::iolist`]; the command is generic ([`crate::commands::run`]).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::error::CmdError;
use crate::plc::iolist::*;
use crate::plc::rack::{RackRef, card_named, rack_named};

/// A channel's state in the I/O list.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum StateParam {
    /// In use.
    Used,
    /// Named but not wired.
    Reserved,
    /// Free.
    Spare,
}

impl StateParam {
    fn word(self) -> &'static str {
        match self {
            StateParam::Used => "used",
            StateParam::Reserved => "reserved",
            StateParam::Spare => "spare",
        }
    }
}

/// `iolist {rack?, card?, state?, limit?, after?}` (design 045 §8.1): the cards and their rows;
/// read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IolistParams {
    /// Only the cards of this rack: an ID or a tag.
    #[serde(default)]
    rack: Option<String>,
    /// Only this card: an ID or a tag.
    #[serde(default)]
    card: Option<String>,
    /// Only rows in this state.
    #[serde(default)]
    state: Option<StateParam>,
    /// At most this many rows (default 200, at most 2000).
    #[serde(default)]
    limit: Option<u64>,
    /// The row index to start at (`next` of the previous page).
    #[serde(default)]
    after: Option<u64>,
}

impl Generic for IolistParams {
    const ID: &'static str = "iolist";
    const TITLE: &'static str = "I/O List";
    const HELP: &'static str = "{rack?: id or tag, card?: id or tag, state?: used|reserved|spare, limit?: rows (200, max 2000), after?: row index} (one row per channel of every card, drawn or not; read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let state = self.state.map(StateParam::word);
        let limit =
            self.limit.map_or(crate::reports::DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX)).clamp(1, crate::reports::MAX_LIMIT);
        let after = self.after.map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
        let doc = cx.doc;
        let plc = &cx.derived.reports(doc).plc;
        let only_rack = self.rack.as_deref().map(|t| rack_named(doc, t)).transpose()?;
        let only_card = self.card.as_deref().map(|t| card_named(doc, t)).transpose()?;
        let mut cards = Vec::new();
        let mut rows = Vec::new();
        let mut total = 0usize;
        for card in cards_in_order(plc) {
            if only_card.is_some_and(|c| c != card.id) || only_rack.is_some_and(|r| card.rack != RackRef::Rack(r)) {
                continue;
            }
            let all = rows_of(doc, plc, card);
            let (u, r, sp) = summary(&all);
            let b = plc.blocks.get(&card.id);
            let span = |x: Option<(u32, u32)>| x.map(|(s, n)| json!([s, s.saturating_add(n).saturating_sub(1)]));
            cards.push(json!({
                "card": card.id.hex(),
                "tag": card.tag,
                "type": card.ty.code(),
                "rack": all.first().and_then(|x| x.rack.clone()),
                "slot": card.slot,
                "part_number": card.part_number,
                "maker": card.maker,
                "channels": card.channels,
                "used": u,
                "reserved": r,
                "spare": sp,
                "inputs": span(b.and_then(|b| b.input)),
                "outputs": span(b.and_then(|b| b.output)),
            }));
            for row in all.iter().filter(|x| state.is_none_or(|s| x.state == s)) {
                if total >= after && rows.len() < limit {
                    rows.push(row_json(row));
                }
                total += 1;
            }
        }
        let mut v = json!({"cards": cards, "rows": rows, "total": total});
        if after.saturating_add(rows.len()) < total {
            v["next"] = json!(after + rows.len());
        }
        Ok(Outcome::answer(v))
    }
}
