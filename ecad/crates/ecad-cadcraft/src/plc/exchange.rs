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

use cadcraft_engine::Session;
use ecad_model::{ComponentId, Document, ObjectRef, Op};
use serde_json::{Value, json};

use super::channels::ch_key;
use super::tagtable::{self, Caps, Read, TagFormat, TagRow};
use super::{CardData, Plc, iolist, signals};
use crate::commands::{err, opt_typed, with_any_page, with_project};

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

/// The format named by `format`, else the generic CSV.
fn format_param(params: &Value) -> Result<&'static dyn TagFormat, String> {
    match opt_typed(params, "format", "a format ID (csv)", Value::as_str)? {
        None => Ok(&tagtable::GenericCsv),
        Some(id) => tagtable::format(id).ok_or_else(|| format!("No tag table format \"{id}\"; the formats are: csv.")),
    }
}

/// The file name of `path` (no directory, house rule 10).
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(|| "a file".to_string(), |n| n.to_string_lossy().into_owned())
}

/// The bytes of `{path}` or `{text}` (at most [`tagtable::MAX_BYTES`]), and the source's name.
fn input_bytes(params: &Value) -> Result<(Vec<u8>, String), String> {
    if let Some(t) = opt_typed(params, "text", "the tag table as text", Value::as_str)? {
        if t.len() > tagtable::MAX_BYTES {
            return Err(format!("The text is {} MB; at most {} MB is read.", t.len() >> 20, tagtable::MAX_BYTES >> 20));
        }
        let name = match opt_typed(params, "name", "a file name", Value::as_str)? {
            None => "pasted text".to_string(),
            Some(n) if n.trim().is_empty() || n.chars().count() > MAX_NAME || n.chars().any(char::is_control) => {
                return Err(format!("`name` is 1 to {MAX_NAME} characters, with no control characters."));
            }
            Some(n) => file_name(n),
        };
        return Ok((t.as_bytes().to_vec(), name));
    }
    let path = opt_typed(params, "path", "a file path", Value::as_str)?.ok_or("Give `path` (a file) or `text` (the tag table).")?;
    read_file(path).map(|b| (b, file_name(path)))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &str) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let f = std::fs::File::open(path).map_err(|e| format!("{} could not be read: {e}.", file_name(path)))?;
    let mut buf = Vec::new();
    // One byte over the cap says the file is too large without reading all of it.
    let cap = u64::try_from(tagtable::MAX_BYTES).unwrap_or(u64::MAX).saturating_add(1);
    f.take(cap).read_to_end(&mut buf).map_err(|e| format!("{} could not be read: {e}.", file_name(path)))?;
    if buf.len() > tagtable::MAX_BYTES {
        return Err(format!(
            "{} is larger than {} MB; at most {} MB is read.",
            file_name(path),
            tagtable::MAX_BYTES >> 20,
            tagtable::MAX_BYTES >> 20
        ));
    }
    Ok(buf)
}

#[cfg(target_arch = "wasm32")]
fn read_file(_path: &str) -> Result<Vec<u8>, String> {
    Err("The web version reads no files; paste the tag table as `text`.".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{} could not be written: {e}.", file_name(path)))
}

#[cfg(target_arch = "wasm32")]
fn write_file(_path: &str, _bytes: &[u8]) -> Result<(), String> {
    Err("The web version writes no files; leave out `path` to get the text.".into())
}

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

/// [`read_tags`] of the file at `path` (at most [`tagtable::MAX_BYTES`]), and its file name. The
/// web version reads no files.
///
/// # Errors
/// As [`read_tags`], or the file cannot be read.
pub fn read_file_tags(path: &str) -> Result<(Read, String), String> {
    let bytes = read_file(path)?;
    Ok((read_tags(&bytes, None)?, file_name(path)))
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

/// `plcimport {path | text, name?, format?, dry_run?, what?: both|tags|descriptions}`: fill the
/// channels' tags and descriptions from an I/O list, one way, in one transaction.
pub(crate) fn run_plcimport(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let dry = opt_typed(params, "dry_run", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let (tags, texts) = match opt_typed(params, "what", "both, tags or descriptions", Value::as_str).map_err(err)? {
        None | Some("both") => (true, true),
        Some("tags") => (true, false),
        Some("descriptions") => (false, true),
        Some(_) => return Err(err("`what` is both, tags or descriptions")),
    };
    let (bytes, source) = input_bytes(params).map_err(err)?;
    let format = match params.get("format").filter(|v| !v.is_null()) {
        Some(_) => Some(format_param(params).map_err(err)?),
        None => None,
    };
    let read = read_tags(&bytes, format).map_err(err)?;
    let counts = json!({"not_io": read.not_io, "bad_address": read.bad, "bad_name": read.bad_name, "twice": read.twice});
    let plan = |p: &mut crate::project::Project| -> Result<Fill, String> {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        plan_fill(p.ledger.document(), &o.plc, &read.rows, tags, texts)
    };
    let report = |fill: &Fill, written: bool| {
        let lead = if written { format!("Imported {source}") } else { format!("{source} would set") };
        let words = if written { fill.words() } else { fill.words().replacen(" set on ", " on ", 1) };
        json!({
            "read": read.total,
            "io": read.rows.len(),
            "skipped": counts,
            "tags": fill.tags,
            "descriptions": fill.descriptions,
            "unchanged": fill.unchanged,
            "no_card": fill.no_card.iter().map(|a| tagtable::percent(a)).collect::<Vec<_>>(),
            "written": written,
            "message": format!("{lead}: {words} {}", read.words()),
        })
    };
    if dry {
        let fill = with_any_page(s, |p, _| plan(p))?;
        return Ok(report(&fill, false));
    }
    let v = with_project(s, |p, pg, _| {
        let fill = plan(p)?;
        if !fill.ops.is_empty() {
            p.apply_edit(pg, "plcimport", IMPORT_LABEL, fill.ops.clone())?;
        }
        Ok(report(&fill, !fill.ops.is_empty()))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// `plcexport {format?, rack?, path?}` (design 056 §6.6): the tag table of a rack (or every rack);
/// writes the file, or returns `{text, rows, left_out}` without `path`. Changes no project data.
pub(crate) fn run_plcexport(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let f = format_param(params).map_err(err)?;
    let path = opt_typed(params, "path", "a file path", Value::as_str).map_err(err)?.map(str::to_string);
    let v = with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        let doc = p.ledger.document();
        let rack = match params.get("rack").filter(|v| !v.is_null()) {
            Some(r) => Some(super::rack::rack_param(doc, r)?),
            None => None,
        };
        let ex = export_rows(doc, &o.plc, rack, f);
        let bytes = f.write(&ex.rows)?;
        let scope = rack.map_or_else(|| "every rack".to_string(), |r| format!("rack {}", crate::devices::tag_words(doc, r)));
        let n = ex.rows.len();
        let left = ex.left_out_words();
        let left_out = json!({"no_tag": ex.no_tag, "no_address": ex.no_address, "refused": ex.refused});
        let tags = if n == 1 { "tag" } else { "tags" };
        let tail = if left.is_empty() { ".".to_string() } else { format!("; {left}.") };
        match &path {
            Some(path) => {
                write_file(path, &bytes)?;
                let message = format!("Wrote {n} {tags} of {scope} to {}{tail}", file_name(path));
                Ok(json!({"rows": n, "left_out": left_out, "file": file_name(path), "format": f.id(), "message": message}))
            }
            None => {
                let message = format!("Made {n} {tags} of {scope}{tail}");
                let text = String::from_utf8(bytes).map_err(|_| "The tag table is not text.".to_string())?;
                Ok(json!({"rows": n, "left_out": left_out, "text": text, "format": f.id(), "message": message}))
            }
        }
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The commands of brief 056.
pub fn specs() -> Vec<cadcraft_engine::CommandSpec> {
    use cadcraft_engine::CommandSpec;
    vec![
        CommandSpec::new("plcmaketags", "Make PLC Tags", signals::run_plcmaketags)
            .params("{card: id or tag} or {rack: id or tag} (fill the empty tags of used and reserved channels from their descriptions)")
            .noundo(),
        CommandSpec::new("plcexport", "Export PLC Tags", run_plcexport)
            .params("{format?: csv, rack?: id or tag (absent: every rack), path?: file} (without path: {text, rows, left_out}; changes nothing)")
            .noundo(),
        CommandSpec::new("plcimport", "Import an I/O List", run_plcimport)
            .params(
                "{path: file | text: the I/O list, name?: its file name (with text, at most 255 characters), format?: csv, dry_run?: bool, \
what?: both|tags|descriptions} (sets the tags and descriptions of the channels at the list's addresses, one way, in one transaction; \
dry_run writes nothing)",
            )
            .noundo(),
        CommandSpec::new("plccardtype", "Card Type", super::rack::run_plccardtype)
            .params("{card: id or tag, type: DI|DQ|AI|AO|RTD|IOL} (changes what an I/O card is: its body symbol, one transaction)")
            .noundo(),
    ]
}
