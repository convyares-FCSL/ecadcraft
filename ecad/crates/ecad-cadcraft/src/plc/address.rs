//! Addressing (design 045 §5): live addresses by slot and channel, pinned starts, typed overrides,
//! and `plcaddress`.
//!
//! # Semantics
//! - Per rack, cards in slot order; one input and one output byte counter, starting at the rack's
//!   `plc.address.start.input` / `.output`. Each card takes a block (§5.1's sizes; a card's
//!   `plc.addr.size` overrides its size); its block starts at its pinned `plc.addr.start` when set,
//!   else at the counter, and the counter then moves to the end of that block if that is further.
//! - Formats by the rack's `plc.address.style` (§5.2): `byte.bit`, `iec`, `slot`, `none`.
//! - A typed `plc.ch.<i>.address` wins everywhere (§5.4).
//! - Checked arithmetic: a block passing byte 65 535 has no address (`address-overflow`), never a
//!   panic (§5.5).
//!
//! Every write is one transaction, committed through [`crate::project::Project::apply_edit`].

use std::collections::BTreeMap;

use cadcraft_engine::Session;
use ecad_model::{ComponentId, Document, ObjectRef, Op};
use serde_json::{Value, json};

use super::rack::{self, CardType, Direction, RackRef};
use crate::commands::with_project;
use crate::defaults::{self, Scope};

/// A card's pinned start byte (FP3).
pub const START_KEY: &str = "plc.addr.start";
/// A card's own block size in bytes (FP3).
pub const SIZE_KEY: &str = "plc.addr.size";
/// The highest byte an address may use.
pub const MAX_BYTE: u32 = 65_535;
/// The largest block size a card may set.
pub const MAX_SIZE: u32 = 4_096;

/// An address style (design 045 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    ByteBit,
    Iec,
    Slot,
    None,
}

impl Style {
    /// Rack `r`'s style (registry, rack level allowed).
    pub fn of(doc: &Document, r: ComponentId) -> Style {
        let at = Scope { component: Some(r), ..Scope::project() };
        match defaults::resolve(doc, &defaults::plc::ADDRESS_STYLE, at).word() {
            "iec" => Style::Iec,
            "slot" => Style::Slot,
            "none" => Style::None,
            _ => Style::ByteBit,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Style::ByteBit => "byte.bit",
            Style::Iec => "iec",
            Style::Slot => "slot",
            Style::None => "none",
        }
    }
}

/// A whole number property 0..=`max`, digits only.
fn number(doc: &Document, c: ComponentId, key: &str, max: u32) -> Option<u32> {
    let t = doc.components.get(&c)?.properties.get(key)?.trim();
    (!t.is_empty() && t.len() <= 5 && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.parse::<u32>().ok()).flatten().filter(|n| *n <= max)
}

/// A card's pinned start, when set.
pub fn pinned(doc: &Document, c: ComponentId) -> Option<u32> {
    number(doc, c, START_KEY, MAX_BYTE)
}

/// The bytes one analogue or IO-Link channel of card `c` takes.
pub fn word_size(doc: &Document, c: ComponentId, ty: CardType) -> u32 {
    let at = Scope { component: Some(c), ..Scope::project() };
    let e = if ty == CardType::Iol { &defaults::plc::IOL_BYTES } else { &defaults::plc::ANALOG_BYTES };
    let n = defaults::resolve(doc, e, at).number();
    if n.is_finite() && n >= 1.0 { (n as u32).min(32) } else { 2 }
}

/// Card `c`'s (input, output) block size in bytes for `n` channels (design 045 §5.1).
pub fn sizes(doc: &Document, c: ComponentId, ty: CardType, n: u32) -> (u32, u32) {
    let own = number(doc, c, SIZE_KEY, MAX_SIZE).filter(|s| *s >= 1);
    let digital = n.div_ceil(8);
    let word = word_size(doc, c, ty).saturating_mul(n);
    let (i, o) = match ty {
        CardType::Di => (digital, 0),
        CardType::Dq => (0, digital),
        CardType::Ai | CardType::Rtd => (word, 0),
        CardType::Ao => (0, word),
        CardType::Iol => (word, word),
        _ => (0, 0),
    };
    match own {
        Some(s) => (if i > 0 { s } else { 0 }, if o > 0 { s } else { 0 }),
        None => (i, o),
    }
}

/// A card's address block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Block {
    /// (start byte, size) of its inputs and outputs.
    pub input: Option<(u32, u32)>,
    pub output: Option<(u32, u32)>,
    pub pinned: bool,
    /// It would pass byte 65 535: no address.
    pub overflow: bool,
}

/// One card of a rack for the counters: (card, type, channel count, slot).
pub type Member = (ComponentId, CardType, u32, u32);

/// The blocks of a rack's cards, `members` in slot order (design 045 §5.1).
pub fn blocks(doc: &Document, r: ComponentId, members: &[Member]) -> BTreeMap<ComponentId, Block> {
    let at = Scope { component: Some(r), ..Scope::project() };
    let start = |e: &defaults::Entry| {
        let n = defaults::resolve(doc, e, at).number();
        if n.is_finite() && n >= 0.0 { (n as u32).min(MAX_BYTE) } else { 0 }
    };
    let (mut cin, mut cout) = (start(&defaults::plc::START_INPUT), start(&defaults::plc::START_OUTPUT));
    let mut out = BTreeMap::new();
    for (c, ty, n, _) in members {
        let (si, so) = sizes(doc, *c, *ty, *n);
        let pin = pinned(doc, *c);
        let mut b = Block { pinned: pin.is_some(), ..Block::default() };
        for (size, counter, slot) in [(si, &mut cin, &mut b.input), (so, &mut cout, &mut b.output)] {
            if size == 0 {
                continue;
            }
            let s = pin.unwrap_or(*counter);
            match s.checked_add(size).filter(|end| *end <= MAX_BYTE.saturating_add(1)) {
                Some(end) => {
                    *slot = Some((s, size));
                    *counter = (*counter).max(end);
                }
                None => b.overflow = true,
            }
        }
        if b.overflow {
            b.input = None;
            b.output = None;
        }
        out.insert(*c, b);
    }
    out
}

/// The automatic address of channel `i` of a card (design 045 §5.2): `None` with style `none`, no
/// block, or past the block.
#[allow(clippy::too_many_arguments)]
pub fn automatic(style: Style, ty: CardType, block: &Block, slot: u32, i: u32, shown: u32, word: u32, output: bool) -> Option<String> {
    let (start, size) = if output { block.output? } else { block.input? };
    let (letter, iec) = if output { ('Q', "%Q") } else { ('I', "%I") };
    let digital = matches!(ty, CardType::Di | CardType::Dq);
    match style {
        Style::None => None,
        Style::Slot => Some(format!("{}:{slot}/{shown:02}", if output { 'O' } else { 'I' })),
        Style::ByteBit | Style::Iec if digital => {
            let byte = start.checked_add(i / 8)?;
            if byte >= start.saturating_add(size) {
                return None;
            }
            let bit = i % 8;
            Some(if style == Style::Iec { format!("{iec}X{byte}.{bit}") } else { format!("{letter}{byte}.{bit}") })
        }
        Style::ByteBit | Style::Iec => {
            let byte = start.checked_add(i.checked_mul(word)?)?;
            if byte.checked_add(word)? > start.saturating_add(size) {
                return None;
            }
            Some(if style == Style::Iec { format!("{iec}W{byte}") } else { format!("{letter}W{byte}") })
        }
    }
}

/// `I2–I3`: a block in words.
pub fn block_words(style: Style, output: bool, (start, size): (u32, u32)) -> String {
    let letter = match (style, output) {
        (Style::Iec, false) => "%I",
        (Style::Iec, true) => "%Q",
        (_, false) => "I",
        (_, true) => "Q",
    };
    let end = start.saturating_add(size).saturating_sub(1);
    if end == start { format!("{letter}{start}") } else { format!("{letter}{start}–{letter}{end}") }
}

/// The address of channel `i` of card `c` as shown: the typed one (`typed: true`), else the
/// automatic input address (an output card's output address).
pub fn shown(doc: &Document, c: ComponentId, i: u32, auto: Option<String>) -> (Option<String>, bool) {
    let typed = doc
        .components
        .get(&c)
        .and_then(|x| x.properties.get(&super::channels::ch_key(i, "address")))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    match typed {
        Some(t) => (Some(t), true),
        None => (auto, false),
    }
}

/// The members of rack `r` read straight from the document (texts and commands, which have no
/// kept model): its cards with a slot, in slot order.
pub fn members_of(doc: &Document, r: ComponentId) -> Vec<Member> {
    rack::cards_naming(doc, r)
        .into_iter()
        .filter_map(|(c, s)| {
            let ty = super::card_type_of(doc, c)?;
            (!ty.is_rack()).then_some((c, ty, rack::channel_count(doc, c, ty), s?))
        })
        .collect()
}

/// The direction of channel addresses a card shows first: its input, else its output.
pub fn shows_output(ty: CardType) -> bool {
    ty.direction() == Direction::Output
}

/// Card `c`'s automatic address of channel `i` (input; and output for IO-Link), worked out from the
/// document (texts, commands).
pub fn of_channel(
    doc: &Document, c: ComponentId, i: u32, kept: Option<&super::Plc>, cache: &mut BTreeMap<ComponentId, BTreeMap<ComponentId, Block>>,
) -> (Option<String>, Option<String>) {
    let (RackRef::Rack(r), Some(slot)) = (rack::rack_ref(doc, c), rack::slot_of(doc, c)) else { return (None, None) };
    let Some(ty) = super::card_type_of(doc, c) else { return (None, None) };
    // The kept model's blocks when there is one (review of #84, 3); a walk otherwise, once per rack.
    let b = match kept {
        Some(p) => p.blocks.get(&c).copied(),
        None => cache.entry(r).or_insert_with(|| blocks(doc, r, &members_of(doc, r))).get(&c).copied(),
    };
    let Some(b) = b else { return (None, None) };
    let b = &b;
    let style = Style::of(doc, r);
    let shown_no = i.saturating_add(super::channels::first_number(doc, c));
    let word = word_size(doc, c, ty);
    let main = automatic(style, ty, b, slot, i, shown_no, word, shows_output(ty));
    let second = (ty == CardType::Iol).then(|| automatic(style, ty, b, slot, i, shown_no, word, true)).flatten();
    (main, second)
}

/// A snapshot of every card's block in `r` (for counting what a change moves).
pub fn snapshot(doc: &Document, r: ComponentId) -> BTreeMap<ComponentId, Block> {
    blocks(doc, r, &members_of(doc, r))
}

/// `plcaddress {rack | card, style?, input_start?, output_start?, start?, size?, fix?, clear?}`
/// (design 045 §5.3). One transaction.
pub(crate) fn run_plcaddress(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        let set = |ops: &mut Vec<Op>, c: ComponentId, key: &str, after: Option<String>| {
            let before = doc.components.get(&c).and_then(|x| x.properties.get(key)).cloned();
            if before != after {
                ops.push(Op::SetProperty { object: ObjectRef::Component(c), key: key.into(), before, after });
            }
        };
        let word = |v: &Value| -> Result<Option<String>, String> {
            match v {
                Value::Null => Ok(None),
                Value::String(s) => Ok(Some(s.trim().to_string())),
                Value::Number(n) => Ok(Some(n.to_string())),
                _ => Err("a value must be text, a number, or null".into()),
            }
        };
        let (who, rack_id, message_tail) = if let Some(rv) = params.get("rack").filter(|v| !v.is_null()) {
            let r = rack::rack_param(doc, rv)?;
            let before = snapshot(doc, r);
            for (key, entry) in [
                ("style", &defaults::plc::ADDRESS_STYLE),
                ("input_start", &defaults::plc::START_INPUT),
                ("output_start", &defaults::plc::START_OUTPUT),
            ] {
                let Some(v) = params.get(key) else { continue };
                let value = word(v)?;
                if let Some(x) = &value
                    && entry.parse(x).is_none()
                {
                    return Err(format!("`{key}` {x} is not a value of {}.", entry.key));
                }
                set(&mut ops, r, entry.key, value);
            }
            let fix = params.get("fix").and_then(Value::as_bool).unwrap_or(false);
            let clear = params.get("clear").and_then(Value::as_bool).unwrap_or(false);
            if fix && clear {
                return Err("`fix` pins every card; `clear` unpins them: give one.".into());
            }
            let mut pinned_n = 0;
            if fix || clear {
                for (c, b) in &before {
                    let start = b.input.or(b.output).map(|x| x.0);
                    let after = if fix { start.map(|x| x.to_string()) } else { None };
                    let had = ops.len();
                    set(&mut ops, *c, START_KEY, after);
                    pinned_n += usize::from(ops.len() > had);
                }
            }
            let tail = if fix {
                format!("{pinned_n} card{} pinned at their current addresses", if pinned_n == 1 { "" } else { "s" })
            } else if clear {
                format!("{pinned_n} card{} unpinned", if pinned_n == 1 { "" } else { "s" })
            } else {
                String::new()
            };
            (crate::devices::tag_words(doc, r), Some(r), tail)
        } else {
            let c = rack::card_param(doc, params.get("card"))?;
            for (key, prop, max) in [("start", START_KEY, MAX_BYTE), ("size", SIZE_KEY, MAX_SIZE)] {
                let Some(v) = params.get(key) else { continue };
                let value = word(v)?;
                if let Some(x) = &value
                    && !(x.bytes().all(|b| b.is_ascii_digit())
                        && x.len() <= 5
                        && x.parse::<u32>().is_ok_and(|n| n <= max && (key == "start" || n >= 1)))
                {
                    return Err(format!("`{key}` is a whole number of bytes from {} to {max}.", u32::from(key == "size")));
                }
                set(&mut ops, c, prop, value);
            }
            let r = match rack::rack_ref(doc, c) {
                RackRef::Rack(r) => Some(r),
                _ => None,
            };
            (crate::devices::tag_words(doc, c), r, String::new())
        };
        if ops.is_empty() {
            return Ok(json!({"changed": false, "message": format!("{who}: nothing to change.")}));
        }
        let before = rack_id.map(|r| snapshot(doc, r)).unwrap_or_default();
        p.apply_edit(pg, "plcaddress", "Addresses", ops)?;
        let doc = p.ledger.document();
        let after = rack_id.map(|r| snapshot(doc, r)).unwrap_or_default();
        let moved = after.iter().filter(|(c, b)| before.get(c).is_some_and(|x| x.input != b.input || x.output != b.output)).count();
        let mut parts = Vec::new();
        if !message_tail.is_empty() {
            parts.push(message_tail);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_follow_design_045_table_5_2() {
        let b = Block { input: Some((2, 2)), output: None, pinned: false, overflow: false };
        assert_eq!(automatic(Style::ByteBit, CardType::Di, &b, 3, 11, 11, 2, false).as_deref(), Some("I3.3"));
        assert_eq!(automatic(Style::Iec, CardType::Di, &b, 3, 11, 11, 2, false).as_deref(), Some("%IX3.3"));
        assert_eq!(automatic(Style::Slot, CardType::Di, &b, 3, 4, 4, 2, false).as_deref(), Some("I:3/04"));
        assert_eq!(automatic(Style::None, CardType::Di, &b, 3, 4, 4, 2, false), None);
        assert_eq!(automatic(Style::ByteBit, CardType::Di, &b, 3, 16, 16, 2, false), None, "past the block");
        let a = Block { input: Some((4, 8)), output: None, pinned: false, overflow: false };
        assert_eq!(automatic(Style::ByteBit, CardType::Ai, &a, 4, 2, 2, 2, false).as_deref(), Some("IW8"));
        assert_eq!(automatic(Style::Iec, CardType::Ai, &a, 4, 3, 3, 2, false).as_deref(), Some("%IW10"));
        let q = Block { input: None, output: Some((8, 2)), pinned: false, overflow: false };
        assert_eq!(automatic(Style::Slot, CardType::Dq, &q, 6, 1, 1, 2, true).as_deref(), Some("O:6/01"));
        assert_eq!(block_words(Style::ByteBit, false, (2, 2)), "I2–I3");
        // Hostile arithmetic: no panic, no address.
        let edge = Block { input: Some((MAX_BYTE, 1)), output: None, pinned: true, overflow: false };
        assert_eq!(automatic(Style::ByteBit, CardType::Ai, &edge, 1, 9, 9, 2, false), None);
    }
}
