//! `plcslot`, `plcracks` and `plccardtype` (brief 045, design 045 §3): cards in racks. The rules
//! are [`crate::plc::rack`].

use cadcraft_engine::Session;

use crate::cmd::{err, opt_typed, with_any_page, with_project};
use crate::plc::rack::*;
use ecad_model::{ComponentId, ObjectRef, Op, Subject};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// `plcslot {card, rack?, slot?, swap?}` (design 045 §2.5): put a card in a slot, move it, swap it
/// with the card there (`swap: true`), or take it out (`rack: null`). One transaction.
pub(crate) fn run_plcslot(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let swap = opt_typed(params, "swap", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let out = params.get("rack").is_some_and(Value::is_null);
    let slot = match params.get("slot") {
        None | Some(Value::Null) => None,
        Some(v) => Some(slot_value(v).map_err(err)?),
    };
    if out && slot.is_some() {
        return Err(err("`slot` is for a card in a rack; `rack: null` takes the card out."));
    }
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let card = card_param(doc, params.get("card"))?;
        let who = crate::devices::tag_words(doc, card);
        let was = place_words(doc, card);
        let before = crate::plc::moved_blocks_before(doc, card);
        let ops = if out {
            slot_ops(doc, card, None)
        } else {
            let rack = match params.get("rack") {
                None => None,
                Some(v) => Some(rack_param(doc, v)?),
            };
            plan_slot(doc, card, rack, slot, swap)?
        };
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

/// `plcracks {rack?}` (design 045 §2.5): each rack with its slots in order, and the cards in no rack;
/// read only, from the kept model.
pub(crate) fn run_plcracks(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let only: Option<ComponentId> = match params.get("rack") {
        None | Some(Value::Null) => None,
        Some(v) => Some(with_any_page(s, |p, _| rack_param(p.ledger.document(), v))?),
    };
    with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
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

/// `plccardtype {card, type}`: change what an I/O card is, in one transaction. Its body symbols
/// show the new type's body; the component's own symbol follows; a description that was the old
/// body's name becomes the new one's; safe states the new type cannot hold are cleared. Channel
/// cells already drawn keep their own symbols (a `channel-type` Check says so). Refused, in words,
/// when a signal (a channel in use, named or drawn) sits at or past the new type's channel count.
pub(crate) fn run_plccardtype(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let word = params.get("type").and_then(Value::as_str).ok_or_else(|| err("`type` (DI, DQ, AI, AO, RTD or IOL) is required"))?;
    let to = CardType::parse(word).filter(|t| IO_TYPES.contains(t)).ok_or_else(|| err("`type` is DI, DQ, AI, AO, RTD or IOL"))?;
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let c = card_param(doc, params.get("card"))?;
        let who = crate::devices::tag_words(doc, c);
        let from = crate::plc::card_type_of(doc, c).ok_or_else(|| format!("{who} is not a PLC card."))?;
        if !IO_TYPES.contains(&from) {
            return Err(format!("{who} is a {}; only an I/O card's type can be changed.", from.noun()));
        }
        if from == to {
            return Ok(json!({"card": c.hex(), "changed": false, "message": format!("{who} is {} already.", with_article(to))}));
        }
        // No signal may be left on a channel the new type does not have (overseer's look check of
        // 2026-11-14): a channel in use, named or drawn at or past the new count refuses it.
        let room = channel_count(doc, c, to);
        let chans = crate::plc::channel_data(doc, c);
        let in_use = |ch: &crate::plc::ChannelData| ch.used || ch.mark().is_some() || !ch.points.is_empty();
        let signals = chans.values().filter(|ch| in_use(ch)).count();
        let outside = chans.iter().filter(|(i, ch)| **i >= room && in_use(ch)).count();
        if outside > 0 {
            let n = |k: usize, one: &str, many: &str| if k == 1 { format!("1 {one}") } else { format!("{k} {many}") };
            return Err(format!(
                "{who} has {}; {} has {}. Move or remove {} first.",
                n(signals, "signal", "signals"),
                with_article(to),
                n(usize::try_from(room).unwrap_or(usize::MAX), "channel", "channels"),
                n(outside, "signal", "signals")
            ));
        }
        let new_starter = crate::symbols::by_key(to.body()).ok_or("No card body for that type.")?;
        let mut ops = Vec::new();
        let def = match crate::symbols::embedded(doc, new_starter) {
            Some(d) => d,
            None => {
                let d: ecad_model::DefinitionId = p.ids.fresh();
                ops.push(Op::EmbedDefinition { id: d, definition: new_starter.definition()?.clone() });
                d
            }
        };
        let mut bodies = 0;
        let mut old_defs = BTreeSet::new();
        for (_, r) in doc.representations_of(Subject::Component(c)) {
            let Some(st) = crate::devices::rep_starter(doc, r) else { continue };
            if CardType::of_body(st.key) != Some(from) {
                continue;
            }
            for item in &r.items {
                let Some(before) = doc.items.get(item).cloned() else { continue };
                let ecad_model::ItemKind::Symbol(sym) = &before.kind else { continue };
                old_defs.insert(sym.definition);
                let after = ecad_model::Item {
                    kind: ecad_model::ItemKind::Symbol(ecad_model::SymbolInstance { definition: def, placement: sym.placement }),
                    ..before.clone()
                };
                ops.push(Op::ModifyItem { id: *item, before, after });
                bodies += 1;
            }
        }
        if bodies == 0 {
            return Err(format!("{who} has no card body drawn; its type comes from its channel symbols. Place a {} body for it.", to.noun()));
        }
        let comp = doc.components.get(&c).ok_or("No such device.")?;
        if let Some(d) = comp.definition.filter(|d| old_defs.contains(d)) {
            ops.push(Op::SetComponentDefinition { id: c, before: Some(d), after: Some(def) });
        }
        // A description that was the old body's name (placing writes it) follows the type.
        let old_name = crate::symbols::by_key(from.body()).map(|s| s.name);
        if let Some(d) = comp.properties.get("description").filter(|d| Some(d.trim()) == old_name) {
            ops.push(Op::SetProperty {
                object: ObjectRef::Component(c),
                key: "description".into(),
                before: Some(d.clone()),
                after: Some(new_starter.name.to_string()),
            });
        }
        // Safe states the new type cannot hold go.
        for (k, v) in comp.properties.range(crate::plc::channels::CH_PREFIX.to_string()..) {
            if !k.starts_with(crate::plc::channels::CH_PREFIX) {
                break;
            }
            if crate::plc::channels::parse_ch_key(k).is_some_and(|(_, w)| w == crate::plc::signals::SAFE)
                && crate::plc::signals::safe_value(Some(to), Some(v)).is_err()
            {
                ops.push(Op::SetProperty { object: ObjectRef::Component(c), key: k.clone(), before: Some(v.clone()), after: None });
            }
        }
        p.apply_edit(pg, "plccardtype", "Card type", ops)?;
        Ok(json!({
            "card": c.hex(),
            "changed": true,
            "type": to.code(),
            "message": format!("{who} is now {}. {}", with_article(to), change_words(from, to)),
        }))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}
