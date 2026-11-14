//! PLC sync (brief 056, design 056 §6): the snapshot of the last tag table imported, each channel's
//! state against it, and the commands `plcsync`, `plctake`, `plcimport` and `plcexport`.
//!
//! # Semantics
//! - The snapshot is three project properties (FP2): [`TABLE_KEY`], the I/O tags as canonical
//!   generic CSV sorted by address then name; [`SOURCE_KEY`], the file name and format; [`AT_KEY`],
//!   the import time (RFC 3339, UTC). Merge treats each as one value.
//! - The kept [`Sync`] index lives in the kept PLC model ([`super::Plc::sync`]): built once when the
//!   snapshot changes, and per card when a card's rows change (its tags, its addresses). A commit
//!   with no PLC data touches nothing.
//! - Matching (design 056 §6.3): by normalised address ([`super::tagtable::normalise`]); names
//!   compare exactly. A channel's state is one of [`State`]; *differs*, *moved*, *not in PLC* and
//!   *only in PLC* are counted, per card and in all, with the snapshot tags no channel has (*not on
//!   any card*).
//! - Import never adds cards, channels or addresses. Export changes nothing (Q10).

use std::collections::{BTreeMap, HashMap};

use cadcraft_engine::Session;
use ecad_model::{ComponentId, Document, ObjectRef, Op};
use serde_json::{Value, json};

use super::channels::ch_key;
use super::tagtable::{self, Caps, Read, TagFormat, TagRow};
use super::{CardData, Plc, iolist, signals};
use crate::commands::{err, opt_typed, with_any_page, with_project};

/// The snapshot's project properties (FP2).
pub const PREFIX: &str = "plc.sync.";
pub const TABLE_KEY: &str = "plc.sync.table";
pub const SOURCE_KEY: &str = "plc.sync.source";
pub const AT_KEY: &str = "plc.sync.at";
/// Largest snapshot text kept: a file record holds strings of at most 1 MiB (`ecad-file`
/// `MAX_STRING`), and the snapshot goes in one property and in its op.
pub const MAX_SNAPSHOT_BYTES: usize = 1_000_000;
/// Most compare rows `plcsync` and `plcimport` return in one call.
pub const MAX_ROWS_OUT: usize = 2_000;
/// The transaction label of an import (design 056 §6.7).
pub const IMPORT_LABEL: &str = "Import PLC tags";

/// A channel's state against the snapshot (design 056 §6.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum State {
    /// No snapshot yet.
    #[default]
    Nothing,
    /// A used or tagged channel with no I/O address.
    NoAddress,
    InSync,
    /// The PLC has another name at the address.
    Differs(String),
    /// Nothing at the address; the channel's tag is at this PLC address.
    Moved(String),
    /// A used or tagged channel, nothing at its address, its tag nowhere.
    NotInPlc,
    /// A free channel with a PLC tag at its address.
    OnlyInPlc(String),
    /// A free channel, nothing in the PLC.
    Free,
}

impl State {
    /// Whether it counts as a difference.
    pub fn counted(&self) -> bool {
        matches!(self, State::Differs(_) | State::Moved(_) | State::NotInPlc | State::OnlyInPlc(_))
    }

    /// The word `plcsync {state}` filters by.
    pub fn word(&self) -> &'static str {
        match self {
            State::Nothing => "none",
            State::NoAddress => "no_address",
            State::InSync => "in_sync",
            State::Differs(_) => "differs",
            State::Moved(_) => "moved",
            State::NotInPlc => "not_in_plc",
            State::OnlyInPlc(_) => "only_in_plc",
            State::Free => "free",
        }
    }

    /// The PLC column's words (design 056 §6.3).
    pub fn words(&self) -> String {
        match self {
            State::Nothing | State::Free => String::new(),
            State::NoAddress => "no address".into(),
            State::InSync => "\u{2713} in sync".into(),
            State::Differs(n) => format!("\u{26a0} {n}"),
            State::Moved(a) => format!("\u{26a0} at {}", tagtable::percent(a)),
            State::NotInPlc => "\u{24d8} not in PLC".into(),
            State::OnlyInPlc(n) => format!("\u{26a0} only in PLC: {n}"),
        }
    }
}

/// The snapshot read from the project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub rows: Vec<TagRow>,
    /// Row index by normalised address.
    by_address: HashMap<String, usize>,
    /// Normalised address by exact name (the first row with it).
    by_name: HashMap<String, String>,
    pub source: Option<String>,
    pub at: Option<String>,
}

impl Snapshot {
    /// From rows (sorted and indexed).
    pub fn of_rows(mut rows: Vec<TagRow>, source: Option<String>, at: Option<String>) -> Snapshot {
        rows.sort_by(|a, b| tagtable::order(&a.address).cmp(&tagtable::order(&b.address)).then_with(|| a.name.cmp(&b.name)));
        let mut by_address = HashMap::with_capacity(rows.len());
        let mut by_name = HashMap::with_capacity(rows.len());
        for (k, r) in rows.iter().enumerate() {
            by_address.entry(r.address.clone()).or_insert(k);
            by_name.entry(r.name.clone()).or_insert_with(|| r.address.clone());
        }
        Snapshot { rows, by_address, by_name, source, at }
    }

    /// The project's snapshot, if any.
    pub fn of_project(doc: &Document) -> Option<Snapshot> {
        let p = &doc.properties;
        let text = p.get(TABLE_KEY)?;
        let read = tagtable::GenericCsv.read(text.as_bytes(), &Caps::default()).ok()?;
        let s = |k: &str| p.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Some(Snapshot::of_rows(read.rows, s(SOURCE_KEY), s(AT_KEY)))
    }

    /// The row at a normalised address.
    pub fn at_address(&self, a: &str) -> Option<&TagRow> {
        self.by_address.get(a).and_then(|k| self.rows.get(*k))
    }

    /// Where a name is in the PLC.
    pub fn address_of(&self, name: &str) -> Option<&str> {
        self.by_name.get(name).map(String::as_str)
    }

    /// The canonical text kept in [`TABLE_KEY`].
    pub fn text(&self) -> String {
        let rows: Vec<TagRow> = self.rows.iter().map(|r| TagRow { extra: None, ..r.clone() }).collect();
        String::from_utf8(tagtable::GenericCsv.write(&rows).unwrap_or_default()).unwrap_or_default()
    }
}

/// One channel's facts the state reads.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Facts {
    address: Option<String>,
    tag: Option<String>,
    named: bool,
}

/// The kept sync index (design 056 §12.2).
#[derive(Debug, Clone, Default)]
pub struct Sync {
    pub snapshot: Option<Snapshot>,
    states: BTreeMap<(ComponentId, u32), State>,
    /// Each card's channels' normalised addresses.
    addrs: BTreeMap<ComponentId, Vec<Option<String>>>,
    /// How many channels sit at each normalised address.
    occupied: HashMap<String, u32>,
    /// Snapshot addresses no channel has, in snapshot order.
    pub no_card: Vec<String>,
    counts: BTreeMap<ComponentId, usize>,
    /// Full builds and single cards worked out (tests and bench).
    pub builds: u64,
    pub card_updates: u64,
}

impl PartialEq for Sync {
    fn eq(&self, o: &Self) -> bool {
        self.snapshot == o.snapshot && self.states == o.states && self.no_card == o.no_card && self.counts == o.counts
    }
}

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

/// The facts of card `card`'s channels.
fn facts_of(doc: &Document, plc: &Plc, card: &CardData) -> Vec<(u32, Facts)> {
    let idx = iolist::indexes(card);
    let addrs = card_addresses(doc, plc, card, &idx);
    idx.into_iter()
        .zip(addrs)
        .map(|(i, address)| {
            let ch = card.chans.get(&i);
            let tag = ch.and_then(|c| c.tag.clone());
            // Design 056 §6.3: *not in PLC* is for used or tagged channels; one with only a
            // description is free here.
            let named = ch.is_some_and(|c| c.used || c.tag.is_some());
            (i, Facts { address, tag, named })
        })
        .collect()
}

/// A channel's state.
fn state_of(snap: &Snapshot, f: &Facts) -> State {
    let Some(a) = &f.address else { return if f.named { State::NoAddress } else { State::Free } };
    if let Some(row) = snap.at_address(a) {
        return match &f.tag {
            Some(t) if *t == row.name => State::InSync,
            _ if !f.named => State::OnlyInPlc(row.name.clone()),
            _ => State::Differs(row.name.clone()),
        };
    }
    if let Some(t) = &f.tag
        && let Some(other) = snap.address_of(t)
    {
        return State::Moved(other.to_string());
    }
    if f.named { State::NotInPlc } else { State::Free }
}

impl Sync {
    /// From scratch: the project's snapshot against every card.
    pub fn build(doc: &Document, plc: &Plc) -> Sync {
        Sync::with_snapshot(doc, plc, Snapshot::of_project(doc))
    }

    /// Against a given snapshot (an import's preview).
    pub fn with_snapshot(doc: &Document, plc: &Plc, snapshot: Option<Snapshot>) -> Sync {
        let mut s = Sync { snapshot, builds: 1, ..Sync::default() };
        for card in plc.cards.values().filter(|c| c.ty.has_channels()) {
            s.set_card(doc, plc, card);
        }
        s.set_no_card();
        s.card_updates = 0;
        s
    }

    /// Work out card `card` again; returns whether its addresses changed.
    pub fn set_card(&mut self, doc: &Document, plc: &Plc, card: &CardData) -> bool {
        self.card_updates += 1;
        let facts = facts_of(doc, plc, card);
        let addrs: Vec<Option<String>> = facts.iter().map(|(_, f)| f.address.clone()).collect();
        let moved = self.addrs.get(&card.id) != Some(&addrs);
        if moved {
            self.forget_addrs(card.id);
            for a in addrs.iter().flatten() {
                *self.occupied.entry(a.clone()).or_default() += 1;
            }
            self.addrs.insert(card.id, addrs);
        }
        self.states.retain(|(c, _), _| *c != card.id);
        let mut n = 0;
        for (i, f) in &facts {
            let st = self.snapshot.as_ref().map_or(State::Nothing, |s| state_of(s, f));
            n += usize::from(st.counted());
            self.states.insert((card.id, *i), st);
        }
        if n > 0 {
            self.counts.insert(card.id, n);
        } else {
            self.counts.remove(&card.id);
        }
        moved
    }

    fn forget_addrs(&mut self, c: ComponentId) {
        for a in self.addrs.remove(&c).into_iter().flatten().flatten() {
            if let Some(n) = self.occupied.get_mut(&a) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    self.occupied.remove(&a);
                }
            }
        }
    }

    /// Card `c` is gone (or no longer has channels).
    pub fn remove_card(&mut self, c: ComponentId) -> bool {
        let had = self.addrs.contains_key(&c);
        self.forget_addrs(c);
        self.states.retain(|(x, _), _| *x != c);
        self.counts.remove(&c);
        had
    }

    /// Work out the snapshot tags no channel has.
    pub fn set_no_card(&mut self) {
        self.no_card = match &self.snapshot {
            Some(s) => s.rows.iter().filter(|r| !self.occupied.contains_key(&r.address)).map(|r| r.address.clone()).collect(),
            None => Vec::new(),
        };
    }

    /// Channel `i` of card `c`'s state.
    pub fn state(&self, c: ComponentId, i: u32) -> State {
        self.states.get(&(c, i)).cloned().unwrap_or_default()
    }

    /// The differences on card `c`.
    pub fn card_count(&self, c: ComponentId) -> usize {
        self.counts.get(&c).copied().unwrap_or(0)
    }

    /// The differences in all, *not on any card* included.
    pub fn total(&self) -> usize {
        self.counts.values().sum::<usize>() + self.no_card.len()
    }

    /// Whether a snapshot exists.
    pub fn has_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }

    /// Every channel's state, in card then channel order.
    pub fn states(&self) -> impl Iterator<Item = (&(ComponentId, u32), &State)> {
        self.states.iter()
    }

    /// `Compared with P56-tags.csv (csv), imported 2026-11-14 15:02. Project: 3 differences.`
    pub fn tip_words(&self) -> String {
        let Some(s) = &self.snapshot else { return "No tag table imported yet.".into() };
        let when = s.at.as_deref().and_then(at_ms).map(|ms| crate::history::full_time(Some(ms))).unwrap_or_default();
        let n = self.total();
        format!(
            "Compared with {}, imported {when}. Project: {n} {}.",
            s.source.as_deref().unwrap_or("a tag table"),
            if n == 1 { "difference" } else { "differences" }
        )
    }
}

/// Milliseconds of an RFC 3339 time.
pub fn at_ms(at: &str) -> Option<u64> {
    let t = chrono::DateTime::parse_from_rfc3339(at.trim()).ok()?;
    u64::try_from(t.timestamp_millis()).ok()
}

/// The chip's words (design 056 §6.4): `PLC: not imported`, `PLC: in sync`, `⚠ PLC: 2 differences`.
pub fn chip_words(has_snapshot: bool, n: usize) -> String {
    match (has_snapshot, n) {
        (false, _) => "PLC: not imported".into(),
        (true, 0) => "PLC: in sync".into(),
        (true, 1) => "\u{26a0} PLC: 1 difference".into(),
        (true, n) => format!("\u{26a0} PLC: {n} differences"),
    }
}

// ---------------------------------------------------------------- compare rows

/// One row of Compare (design 056 §6.5).
#[derive(Debug, Clone, PartialEq)]
pub struct CompareRow {
    /// The card and channel index; `None` for a tag on no card.
    pub at: Option<(ComponentId, u32)>,
    pub card_tag: String,
    pub channel: Option<u32>,
    /// `%Q2.2`.
    pub address: String,
    pub tag: Option<String>,
    pub text: Option<String>,
    pub ty: &'static str,
    pub plc: Option<TagRow>,
    pub state: State,
}

impl CompareRow {
    /// As `plcsync` returns it.
    pub fn json(&self) -> Value {
        json!({
            "card": self.at.map(|(c, _)| c.hex()),
            "card_tag": if self.at.is_some() { Value::from(self.card_tag.clone()) } else { Value::Null },
            "channel": self.channel,
            "address": self.address,
            "tag": self.tag,
            "text": self.text,
            "type": self.ty,
            "plc": self.plc.as_ref().map(|r| json!({"name": r.name, "address": tagtable::percent(&r.address), "type": r.data_type, "comment": r.comment})),
            "state": self.state.word(),
            "words": self.state.words(),
        })
    }
}

/// The compare rows of `cards` (every card when `None`), only the counted ones unless `all`, then
/// the tags on no card (project scope only).
pub fn compare_rows(doc: &Document, plc: &Plc, sync: &Sync, cards: Option<&[ComponentId]>, all: bool) -> Vec<CompareRow> {
    let mut out = Vec::new();
    let Some(snap) = &sync.snapshot else { return out };
    for card in iolist::cards_in_order(plc) {
        if cards.is_some_and(|cs| !cs.contains(&card.id)) {
            continue;
        }
        let idx = iolist::indexes(card);
        if !idx.iter().any(|i| {
            let st = sync.state(card.id, *i);
            st.counted() || (all && !matches!(st, State::Free | State::Nothing))
        }) {
            continue;
        }
        let addrs = card_addresses(doc, plc, card, &idx);
        for (i, addr) in idx.into_iter().zip(addrs) {
            let st = sync.state(card.id, i);
            if !(st.counted() || (all && !matches!(st, State::Free | State::Nothing))) {
                continue;
            }
            let ch = card.chans.get(&i);
            let plc_row = match &st {
                State::Moved(a) => snap.at_address(a).cloned(),
                _ => addr.as_deref().and_then(|a| snap.at_address(a)).cloned(),
            };
            out.push(CompareRow {
                at: Some((card.id, i)),
                card_tag: card.tag.clone(),
                channel: Some(i.saturating_add(card.first)),
                address: addr.as_deref().map(tagtable::percent).unwrap_or_default(),
                tag: ch.and_then(|c| c.tag.clone()),
                text: ch.and_then(|c| c.text.clone()),
                ty: card.ty.code(),
                plc: plc_row,
                state: st,
            });
        }
    }
    if cards.is_none() {
        for a in &sync.no_card {
            out.push(CompareRow {
                at: None,
                card_tag: String::new(),
                channel: None,
                address: tagtable::percent(a),
                tag: None,
                text: None,
                ty: "",
                plc: snap.at_address(a).cloned(),
                state: State::Nothing,
            });
        }
    }
    out
}

// ---------------------------------------------------------------- export rows

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
        let name = opt_typed(params, "name", "a file name", Value::as_str)?.map_or_else(|| "pasted text".to_string(), file_name);
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

/// The rows a read gives, as a snapshot, refused when its text is too large to keep.
fn snapshot_of(read: &Read, source: String, at: Option<String>) -> Result<Snapshot, String> {
    let snap = Snapshot::of_rows(read.rows.clone(), Some(source), at);
    let n = snap.text().len();
    if n > MAX_SNAPSHOT_BYTES {
        return Err(format!(
            "The file's {} I/O tags make {:.1} MB; at most {:.1} MB is kept. Export only this PLC's tags from the PLC tool.",
            read.rows.len(),
            n as f64 / 1e6,
            MAX_SNAPSHOT_BYTES as f64 / 1e6
        ));
    }
    Ok(snap)
}

/// The import time now (RFC 3339, UTC), where a clock exists.
fn now_rfc3339() -> Option<String> {
    let ms = crate::project::now_ms()?;
    let t = chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)?;
    Some(t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// The ops that set channel `i` of `c`'s tag to `name` (and, `with_text`, its description to
/// `comment` when it has none).
fn take_ops(doc: &Document, c: ComponentId, i: u32, name: &str, comment: Option<&str>, ops: &mut Vec<Op>) {
    let props = doc.components.get(&c).map(|x| &x.properties);
    let key = ch_key(i, signals::TAG);
    let before = props.and_then(|p| p.get(&key)).cloned();
    if before.as_deref() != Some(name) {
        ops.push(Op::SetProperty { object: ObjectRef::Component(c), key, before, after: Some(name.to_string()) });
    }
    if let Some(cm) = comment.map(str::trim).filter(|c| !c.is_empty()) {
        let key = ch_key(i, "text");
        let before = props.and_then(|p| p.get(&key)).cloned();
        if before.as_deref().is_none_or(|b| b.trim().is_empty())
            && let Ok(Some(v)) = super::channels::ch_value("text", Some(cm))
        {
            ops.push(Op::SetProperty { object: ObjectRef::Component(c), key, before, after: Some(v) });
        }
    }
}

/// The take ops of every *differs* and *only in PLC* state of `sync` on `cards` (all: `None`).
fn take_all_ops(doc: &Document, sync: &Sync, cards: Option<&[ComponentId]>, ops: &mut Vec<Op>) -> usize {
    let Some(snap) = &sync.snapshot else { return 0 };
    let mut n = 0;
    for ((c, i), st) in sync.states() {
        if cards.is_some_and(|cs| !cs.contains(c)) {
            continue;
        }
        let (name, with_text) = match st {
            State::Differs(name) => (name, false),
            State::OnlyInPlc(name) => (name, true),
            _ => continue,
        };
        let comment = if with_text { snap.address_of(name).and_then(|a| snap.at_address(a)).map(|r| r.comment.as_str()) } else { None };
        if !signals::tag_value(Some(name)).is_ok_and(|v| v.is_some()) {
            continue;
        }
        take_ops(doc, *c, *i, name, comment, ops);
        n += 1;
    }
    n
}

/// Read a tag table's bytes (the format given, else the one that sniffs it) into its rows and the
/// snapshot it makes (`source`: the file's name, without a directory).
///
/// # Errors
/// No format reads it, the format's own refusal, or a snapshot too large to keep; in words.
pub fn read_tags(bytes: &[u8], source: &str, format: Option<&'static dyn TagFormat>) -> Result<(Read, Snapshot), String> {
    let f = match format {
        Some(f) => f,
        None => tagtable::sniff(bytes).ok_or(tagtable::UNKNOWN)?,
    };
    if !f.sniff(bytes) {
        return Err(tagtable::UNKNOWN.into());
    }
    let read = f.read(bytes, &Caps::default())?;
    let snap = snapshot_of(&read, format!("{source} ({})", f.id()), now_rfc3339())?;
    Ok((read, snap))
}

/// [`read_tags`] of the file at `path` (at most [`tagtable::MAX_BYTES`]; the source is its file
/// name). The web version reads no files.
///
/// # Errors
/// As [`read_tags`], or the file cannot be read.
pub fn read_file_tags(path: &str) -> Result<(Read, Snapshot), String> {
    let bytes = read_file(path)?;
    read_tags(&bytes, &file_name(path), None)
}

/// An import's preview (design 056 §6.7 step 2): the project against the new snapshot, written
/// nowhere.
#[derive(Debug, Clone)]
pub struct Preview {
    pub sync: Sync,
    pub summary: String,
}

impl Preview {
    /// The preview of `read` (made into `snap`) against the project.
    pub fn of(doc: &Document, plc: &Plc, read: &Read, snap: &Snapshot) -> Preview {
        let sync = Sync::with_snapshot(doc, plc, Some(snap.clone()));
        let per_card: Vec<String> = iolist::cards_in_order(plc)
            .iter()
            .filter(|c| sync.card_count(c.id) > 0)
            .take(8)
            .map(|c| {
                let n = sync.card_count(c.id);
                format!("{n} {} on {}", if n == 1 { "difference" } else { "differences" }, c.tag)
            })
            .collect();
        let mut summary = read.words();
        if !per_card.is_empty() || !sync.no_card.is_empty() {
            let mut parts = per_card;
            if !sync.no_card.is_empty() {
                parts.push(format!("{} not on any card", sync.no_card.len()));
            }
            summary.push_str(&format!(" {}.", parts.join(", ")));
        }
        Preview { sync, summary }
    }

    /// As `plcimport` returns it.
    fn json(&self, doc: &Document, plc: &Plc, source: &str) -> Value {
        let rows: Vec<Value> = compare_rows(doc, plc, &self.sync, None, false).iter().take(MAX_ROWS_OUT).map(CompareRow::json).collect();
        json!({
            "differences": self.sync.total(),
            "not_on_any_card": self.sync.no_card.len(),
            "rows": rows,
            "summary": self.summary,
            "source": source,
        })
    }
}

/// `plcimport {path | text, format?, dry_run?, take?: none|names}` (design 056 §6.7, §10).
pub(crate) fn run_plcimport(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let dry = opt_typed(params, "dry_run", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let take = match opt_typed(params, "take", "none or names", Value::as_str).map_err(err)? {
        None | Some("none") => false,
        Some("names") => true,
        Some(_) => return Err(err("`take` is none or names")),
    };
    let (bytes, source) = input_bytes(params).map_err(err)?;
    let format = match params.get("format").filter(|v| !v.is_null()) {
        Some(_) => Some(format_param(params).map_err(err)?),
        None => None,
    };
    let (read, snap) = read_tags(&bytes, &source, format).map_err(err)?;
    let preview = |p: &mut crate::project::Project| -> Result<(Sync, Value), String> {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        let pv = Preview::of(p.ledger.document(), &o.plc, &read, &snap);
        let v = pv.json(p.ledger.document(), &o.plc, &source);
        Ok((pv.sync, v))
    };
    let counts = |v: &mut Value| {
        v["read"] = json!(read.total);
        v["io"] = json!(read.rows.len());
        v["skipped"] = json!({"not_io": read.not_io, "bad_address": read.bad, "bad_name": read.bad_name, "twice": read.twice});
    };
    if dry {
        let mut v = with_any_page(s, |p, _| preview(p).map(|x| x.1))?;
        counts(&mut v);
        return Ok(json!({"dry_run": true, "written": false, "preview": v}));
    }
    let v = with_project(s, |p, pg, _| {
        let (sync, mut v) = preview(p)?;
        counts(&mut v);
        let doc = p.ledger.document();
        let text = snap.text();
        let mut ops = Vec::new();
        for (key, after) in [(TABLE_KEY, Some(text)), (SOURCE_KEY, snap.source.clone()), (AT_KEY, snap.at.clone())] {
            let before = doc.properties.get(key).cloned();
            if before != after {
                ops.push(Op::SetProperty { object: ObjectRef::Project, key: key.into(), before, after });
            }
        }
        let taken = if take { take_all_ops(doc, &sync, None, &mut ops) } else { 0 };
        if !ops.is_empty() {
            p.apply_edit(pg, "plcimport", IMPORT_LABEL, ops)?;
        }
        let mut message = format!("Imported {}. {}", source, v["summary"].as_str().unwrap_or_default());
        if taken > 0 {
            message.push_str(&format!(" Took {taken} {} from the PLC.", if taken == 1 { "name" } else { "names" }));
        }
        v["taken"] = json!(taken);
        v["message"] = json!(message);
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(json!({"dry_run": false, "written": true, "preview": v}))
}

/// `plcsync {card?, rack?, state?, all?}` (design 056 §10): the compare rows, counts, source and
/// time; read only.
pub(crate) fn run_plcsync(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let state = opt_typed(params, "state", "a state word", Value::as_str).map_err(err)?.map(str::to_string);
    let all = opt_typed(params, "all", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        let doc = p.ledger.document();
        let plc = &o.plc;
        let cards: Option<Vec<ComponentId>> = match (params.get("card").filter(|v| !v.is_null()), params.get("rack").filter(|v| !v.is_null())) {
            (Some(c), _) => Some(vec![super::rack::card_param(doc, Some(c))?]),
            (None, Some(r)) => {
                let r = super::rack::rack_param(doc, r)?;
                Some(plc.cards_of(r).iter().map(|c| c.id).collect())
            }
            _ => None,
        };
        let sync = &plc.sync;
        let mut rows = compare_rows(doc, plc, sync, cards.as_deref(), all || state.as_deref() == Some("in_sync"));
        if let Some(st) = &state {
            let st = if st == "no_card" { "none" } else { st.as_str() };
            rows.retain(|r| r.state.word() == st && (st != "none" || r.at.is_none()));
        }
        let card_counts: Vec<Value> = iolist::cards_in_order(plc)
            .iter()
            .filter(|c| cards.as_ref().is_none_or(|cs| cs.contains(&c.id)))
            .map(|c| json!({"card": c.id.hex(), "tag": c.tag, "differences": sync.card_count(c.id)}))
            .collect();
        let snap = sync.snapshot.as_ref();
        Ok(json!({
            "imported": snap.is_some(),
            "source": snap.and_then(|s| s.source.clone()),
            "at": snap.and_then(|s| s.at.clone()),
            "tags": snap.map_or(0, |s| s.rows.len()),
            "differences": sync.total(),
            "not_on_any_card": sync.no_card.len(),
            "cards": card_counts,
            "total": rows.len(),
            "rows": rows.iter().take(MAX_ROWS_OUT).map(CompareRow::json).collect::<Vec<_>>(),
        }))
    })
}

/// `plctake {items: [{card, channel}]} | {card} | {all: true}`, `what?: name|address` (design 056
/// §6.5): take the PLC's names (or, for *moved*, its address) in one transaction.
pub(crate) fn run_plctake(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let what = opt_typed(params, "what", "name or address", Value::as_str).map_err(err)?.unwrap_or("name").to_string();
    if what != "name" && what != "address" {
        return Err(err("`what` is name or address"));
    }
    let v = with_project(s, |p, pg, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        let doc = p.ledger.document();
        let plc = &o.plc;
        let sync = &plc.sync;
        let snap = sync.snapshot.as_ref().ok_or("Import a tag table first.")?;
        let mut ops = Vec::new();
        let mut n = 0usize;
        let mut skipped = 0usize;
        if let Some(items) = params.get("items").filter(|v| !v.is_null()) {
            let items = items.as_array().ok_or("`items` is a list of {card, channel}")?;
            if items.len() > 20_000 {
                return Err("At most 20 000 items per call.".into());
            }
            for it in items {
                let c = super::rack::card_param(doc, it.get("card"))?;
                let shown = it
                    .get("channel")
                    .and_then(Value::as_u64)
                    .and_then(|x| u32::try_from(x).ok())
                    .ok_or("Each item needs `channel` (a number as shown).")?;
                let first = super::channels::first_number(doc, c);
                let i = shown.checked_sub(first).ok_or_else(|| format!("{}'s channels start at {first}.", crate::devices::tag_words(doc, c)))?;
                match (sync.state(c, i), what.as_str()) {
                    (State::Differs(name), "name") => {
                        take_ops(doc, c, i, &name, None, &mut ops);
                        n += 1;
                    }
                    (State::OnlyInPlc(name), "name") => {
                        let comment = snap.address_of(&name).and_then(|a| snap.at_address(a)).map(|r| r.comment.clone());
                        take_ops(doc, c, i, &name, comment.as_deref(), &mut ops);
                        n += 1;
                    }
                    (State::Moved(addr), "address") => {
                        let key = ch_key(i, "address");
                        let before = doc.components.get(&c).and_then(|x| x.properties.get(&key)).cloned();
                        ops.push(Op::SetProperty { object: ObjectRef::Component(c), key, before, after: Some(tagtable::percent(&addr)) });
                        n += 1;
                    }
                    _ => skipped += 1,
                }
            }
        } else if params.get("all").and_then(Value::as_bool) == Some(true) {
            n = take_all_ops(doc, sync, None, &mut ops);
        } else if let Some(c) = params.get("card").filter(|v| !v.is_null()) {
            let c = super::rack::card_param(doc, Some(c))?;
            n = take_all_ops(doc, sync, Some(&[c]), &mut ops);
        } else {
            return Err("Give `items`, `card` or `all: true`.".into());
        }
        if ops.is_empty() {
            return Ok(json!({"taken": 0, "skipped": skipped, "changed": false, "message": "Nothing to take: no difference there."}));
        }
        let label = if what == "address" { "Use the PLC's address" } else { "Use the PLC's names" };
        p.apply_edit(pg, "plctake", label, ops)?;
        let noun = if what == "address" {
            if n == 1 { "address" } else { "addresses" }
        } else if n == 1 {
            "name"
        } else {
            "names"
        };
        let mut message = format!("Took {n} {noun} from the PLC.");
        if what == "address" {
            message.push_str(" This types an address; it no longer follows the slot.");
        }
        Ok(json!({"taken": n, "skipped": skipped, "changed": true, "message": message}))
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
        CommandSpec::new("plcimport", "Import PLC Tags", run_plcimport)
            .params(
                "{path: file | text: the tag table, name?: its file name (with text), format?: csv, dry_run?: bool, take?: none|names} \
(keeps the PLC's I/O tags as the project's snapshot in one transaction; dry_run writes nothing)",
            )
            .noundo(),
        CommandSpec::new("plcsync", "PLC Sync", run_plcsync)
            .params("{card?: id or tag, rack?: id or tag, state?: differs|moved|not_in_plc|only_in_plc|no_card|in_sync, all?: bool} (read only)")
            .noundo(),
        CommandSpec::new("plctake", "Use the PLC's Names", run_plctake)
            .params("{items: [{card, channel}]} | {card: id or tag} | {all: true}, what?: name|address (default name)")
            .noundo(),
    ]
}
