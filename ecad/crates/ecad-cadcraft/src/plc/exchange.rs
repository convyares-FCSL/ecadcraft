//! Tag tables in and out (brief 056, design 056 §6.6, §6.7 as the owner changed them on
//! 2026-11-14): **Export tags** writes a rack's (or every rack's) tagged channels as a tag table;
//! **Import an I/O list** fills channels' tags and descriptions from one, one way.
//!
//! # Semantics
//! - Files only: nothing is kept of an imported table, and nothing is compared with a PLC
//!   project afterwards (owner, 2026-11-14: no sync with an outside PLC).
//! - Import matches a table's rows to channels by normalised address
//!   ([`super::tagtable::normalise`]); each matched channel takes the row's name as its tag and,
//!   when the row has a comment, the comment as its description. Rows at an address no channel has
//!   are counted, never placed. Import never adds cards, channels or addresses.
//! - Tags stay unique per PLC (without case): an import that would give two channels of one PLC
//!   the same tag is refused whole, in words.
//! - One import is one transaction, `Import an I/O list`; `dry_run` says what it would do and
//!   writes nothing. Export changes nothing.
//! - Exported fields that a spreadsheet would run as a formula are written with a leading `'`
//!   ([`super::tagtable::neutral`]), which the reader takes off again.

use std::collections::BTreeMap;

use ecad_model::{ComponentId, Document, ObjectRef, Op};

use super::channels::ch_key;
use super::tagtable::{self, Caps, Read, TagFormat, TagRow};
use super::{CardData, Plc, iolist, signals};

/// The transaction label of an import.
pub const IMPORT_LABEL: &str = "Import an I/O list";
/// Longest `name` given with `text` (the review of #112, 1).
pub const MAX_NAME: usize = 255;

/// The normalised addresses of card `card`'s channels `idx`, with the rack's style and the card's
/// word size read once per card (not once per channel).
fn card_addresses(doc: &Document, plc: &Plc, card: &CardData, idx: &[u32]) -> Vec<Option<String>> {
    use super::address;
    let auto: Option<(address::Style, &address::Block, u32, u32)> = match (card.rack, plc.blocks.get(&card.id), card.slot) {
        (super::rack::RackRef::Rack(r), Some(b), Some(slot)) => {
            Some((address::Style::of(doc, r), b, slot, address::word_size(doc, card.id, card.ty)))
        }
        _ => None,
    };
    let output = address::shows_output(card.ty);
    idx.iter()
        .map(|i| {
            let a = match card.chans.get(i).and_then(|c| c.typed.clone()) {
                Some(t) => Some(t),
                None => auto.as_ref().and_then(|(style, b, slot, word)| {
                    address::automatic(*style, card.ty, b, *slot, *i, i.saturating_add(card.first), *word, output)
                }),
            };
            a.and_then(|a| tagtable::key_of(&a))
        })
        .collect()
}

/// The data type of a card type's channels on export (Q4): `Bool`, `Int`, `Word`.
pub fn data_type(t: super::rack::CardType) -> &'static str {
    use super::rack::CardType;
    match t {
        CardType::Di | CardType::Dq => "Bool",
        CardType::Iol => "Word",
        _ => "Int",
    }
}

/// What an export wrote and left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Export {
    pub rows: Vec<TagRow>,
    pub no_tag: usize,
    pub no_address: usize,
    pub refused: usize,
}

impl Export {
    /// `left out 3 without a tag` (and the other counts), or empty.
    pub fn left_out_words(&self) -> String {
        let mut parts = Vec::new();
        if self.no_tag > 0 {
            parts.push(format!("{} without a tag", self.no_tag));
        }
        if self.no_address > 0 {
            parts.push(format!("{} without an address", self.no_address));
        }
        if self.refused > 0 {
            parts.push(format!("{} with a name the format cannot hold", self.refused));
        }
        if parts.is_empty() { String::new() } else { format!("left out {}", parts.join(", ")) }
    }
}

/// The export rows of rack `rack` (all racks and unracked cards when `None`), design 056 §6.6:
/// used and reserved channels with a tag and an address, one row each (IO-Link: one per
/// direction).
pub fn export_rows(doc: &Document, plc: &Plc, rack: Option<ComponentId>, f: &dyn TagFormat) -> Export {
    let mut out = Export::default();
    for card in iolist::cards_in_order(plc) {
        if rack.is_some_and(|r| card.rack != super::rack::RackRef::Rack(r)) {
            continue;
        }
        for row in iolist::rows_of(doc, plc, card) {
            if row.state == "spare" && row.tag.is_none() {
                continue;
            }
            let Some(name) = row.tag.clone() else {
                out.no_tag += 1;
                continue;
            };
            if !f.name_ok(&name) {
                out.refused += 1;
                continue;
            }
            let mut addrs: Vec<String> = row.address.iter().filter_map(|a| tagtable::key_of(a)).collect();
            if let Some(o) = row.address_out.as_deref().and_then(tagtable::key_of) {
                addrs.push(o);
            }
            addrs.dedup();
            if addrs.is_empty() {
                out.no_address += 1;
                continue;
            }
            for a in addrs {
                out.rows.push(TagRow {
                    name: name.clone(),
                    address: a,
                    data_type: data_type(card.ty).into(),
                    comment: row.text.clone().unwrap_or_default(),
                    extra: Some(tagtable::Extra {
                        card: row.card_tag.clone(),
                        channel: row.channel.to_string(),
                        safe: row.safe_shown().map(|(s, _)| signals::safe_words(s)).unwrap_or_default(),
                        drives: signals::drives_words(&row.targets),
                        page: row.place.clone().unwrap_or_default(),
                    }),
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------- commands

/// Read a tag table's bytes (the format given, else the one that sniffs it).
///
/// # Errors
/// No format reads it, or the format's own refusal; in words.
pub fn read_tags(bytes: &[u8], format: Option<&'static dyn TagFormat>) -> Result<Read, String> {
    let f = match format {
        Some(f) => f,
        None => tagtable::sniff(bytes).ok_or(tagtable::UNKNOWN)?,
    };
    if !f.sniff(bytes) {
        return Err(tagtable::UNKNOWN.into());
    }
    f.read(bytes, &Caps::default())
}

/// What an import would set (design 056 §6.7, owner's change).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fill {
    pub ops: Vec<Op>,
    pub tags: usize,
    pub descriptions: usize,
    /// Rows whose channel already has that tag and description.
    pub unchanged: usize,
    /// Rows at an address no channel has (normalised).
    pub no_card: Vec<String>,
    /// The cards that change, by tag.
    pub cards: Vec<String>,
}

impl Fill {
    /// `9 tags and 9 descriptions set on -A03; 1 row at an address no card has (%I8.0).`
    pub fn words(&self) -> String {
        let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        let mut s = if self.tags + self.descriptions == 0 {
            "Nothing to change".to_string()
        } else {
            format!(
                "{} and {} set on {}",
                n(self.tags, "tag", "tags"),
                n(self.descriptions, "description", "descriptions"),
                self.cards.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
            )
        };
        if self.unchanged > 0 {
            s.push_str(&format!("; {} already so", n(self.unchanged, "channel", "channels")));
        }
        if !self.no_card.is_empty() {
            let at: Vec<String> = self.no_card.iter().take(3).map(|a| tagtable::percent(a)).collect();
            let more = if self.no_card.len() > 3 { ", …" } else { "" };
            s.push_str(&format!("; {} at an address no card has ({}{more})", n(self.no_card.len(), "row", "rows"), at.join(", ")));
        }
        s.push('.');
        s
    }
}

/// Plan the import of `rows` into the project (`what`: set tags, descriptions, or both).
///
/// # Errors
/// Two channels of one PLC would share a tag; in words, naming both.
pub fn plan_fill(doc: &Document, plc: &Plc, rows: &[TagRow], tags: bool, texts: bool) -> Result<Fill, String> {
    // Every channel by its normalised address, per card in list order.
    let mut at: BTreeMap<String, (ComponentId, u32)> = BTreeMap::new();
    for card in iolist::cards_in_order(plc) {
        let idx = iolist::indexes(card);
        for (i, a) in idx.iter().zip(card_addresses(doc, plc, card, &idx)) {
            if let Some(a) = a {
                at.entry(a).or_insert((card.id, *i));
            }
        }
    }
    let mut fill = Fill::default();
    // The tag each changed channel gets.
    let mut new_tags: BTreeMap<(ComponentId, u32), String> = BTreeMap::new();
    let mut cards: Vec<ComponentId> = Vec::new();
    for r in rows {
        let Some((c, i)) = at.get(&r.address).copied() else {
            fill.no_card.push(r.address.clone());
            continue;
        };
        let Some(card) = plc.cards.get(&c) else { continue };
        let ch = card.chans.get(&i);
        let mut changed = false;
        if tags && ch.and_then(|x| x.tag.as_deref()) != Some(r.name.as_str()) {
            let name = signals::tag_value(Some(&r.name))?.unwrap_or_default();
            new_tags.insert((c, i), name.clone());
            let before = doc.components.get(&c).and_then(|x| x.properties.get(&ch_key(i, signals::TAG))).cloned();
            fill.ops.push(Op::SetProperty { object: ObjectRef::Component(c), key: ch_key(i, signals::TAG), before, after: Some(name) });
            fill.tags += 1;
            changed = true;
        }
        let comment = r.comment.trim();
        if texts && !comment.is_empty() && ch.and_then(|x| x.text.as_deref()) != Some(comment) {
            let text = super::channels::ch_value("text", Some(comment))?;
            let before = doc.components.get(&c).and_then(|x| x.properties.get(&ch_key(i, "text"))).cloned();
            fill.ops.push(Op::SetProperty { object: ObjectRef::Component(c), key: ch_key(i, "text"), before, after: text });
            fill.descriptions += 1;
            changed = true;
        }
        if changed {
            if !cards.contains(&c) {
                cards.push(c);
            }
        } else {
            fill.unchanged += 1;
        }
    }
    // Tags stay unique per PLC: the channels' tags after the import, compared without case.
    if !new_tags.is_empty() {
        let mut seen: BTreeMap<(Option<ComponentId>, String), (ComponentId, u32)> = BTreeMap::new();
        for card in iolist::cards_in_order(plc) {
            let rack = match card.rack {
                super::rack::RackRef::Rack(r) => Some(r),
                _ => None,
            };
            for i in iolist::indexes(card) {
                let tag = match new_tags.get(&(card.id, i)) {
                    Some(t) => Some(t.clone()),
                    None => card.chans.get(&i).and_then(|x| x.tag.clone()),
                };
                let Some(tag) = tag else { continue };
                if let Some(first) = seen.insert((rack, signals::name_key(&tag)), (card.id, i)) {
                    let words = |(c, j): (ComponentId, u32)| {
                        let (auto, _) = super::address::of_channel(doc, c, j, Some(plc), &mut BTreeMap::new());
                        let (a, _) = super::address::shown(doc, c, j, auto);
                        let a = a.map(|a| format!(" ({a})")).unwrap_or_default();
                        format!("{}{a}", super::channels::channel_words(doc, c, j))
                    };
                    return Err(format!(
                        "The import would give two channels the tag \"{tag}\": {} and {}. Nothing was imported.",
                        words(first),
                        words((card.id, i))
                    ));
                }
            }
        }
    }
    fill.cards = cards.iter().filter_map(|c| plc.cards.get(c)).map(|c| c.tag.clone()).collect();
    Ok(fill)
}
