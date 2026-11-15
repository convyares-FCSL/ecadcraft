//! `iolist` (brief 045, design 045 §7): the I/O list rows, read only. The rows are
//! [`crate::plc::iolist`].

use cadcraft_engine::Session;

use crate::commands::{err, opt_typed, with_any_page};
use crate::plc::iolist::*;
use crate::plc::rack::RackRef;
use serde_json::{Value, json};

/// `iolist {rack?, card?, state?, limit?, after?}` (design 045 §8.1): the cards and their rows;
/// read only.
pub(crate) fn run_iolist(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let state = opt_typed(params, "state", "used, reserved or spare", Value::as_str).map_err(err)?.map(str::to_string);
    if let Some(st) = &state
        && !["used", "reserved", "spare"].contains(&st.as_str())
    {
        return Err(err("`state` is used, reserved or spare"));
    }
    let limit = opt_typed(params, "limit", "a whole number", Value::as_u64)
        .map_err(err)?
        .map_or(crate::outputs::DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX))
        .clamp(1, crate::outputs::MAX_LIMIT);
    let after = opt_typed(params, "after", "a whole number", Value::as_u64).map_err(err)?.map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
    with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        let doc = p.ledger.document();
        let only_rack = match params.get("rack") {
            None | Some(Value::Null) => None,
            Some(v) => Some(crate::plc::rack::rack_param(doc, v)?),
        };
        let only_card = match params.get("card") {
            None | Some(Value::Null) => None,
            v => Some(crate::plc::rack::card_param(doc, v)?),
        };
        let plc = &o.plc;
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
            for row in all.iter().filter(|x| state.as_deref().is_none_or(|s| x.state == s)) {
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
        Ok(v)
    })
}
