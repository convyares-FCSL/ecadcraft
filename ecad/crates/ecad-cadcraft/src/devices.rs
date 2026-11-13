//! Devices and tags (brief 018, design 018).
//!
//! A *device* is a component with a tag. By the concept spec a tag is an identity: the same tag
//! anywhere means the same device ([`ecad_model::tag`]). A device may be drawn by several symbols
//! (a coil and its contacts), each a symbol representation; the first is its own definition's,
//! the others carry their terminal positions as connection points in their definition's local
//! frame (I14 as relaxed, decision 2026-11-08).
//!
//! # Semantics
//! - Automatic tags: the symbol's prefix (IEC 81346 by default, see [`prefix_for`]) and the
//!   highest number that prefix has ever had in the project's history + 1 ([`TagHigh`]), so a
//!   deleted tag is not reused. Numbers are padded ([`padding`], 2 by default).
//! - A duplicate tag is never a dead end: placing with an existing tag adds the symbol to that
//!   device, and renaming a device to an existing tag merges it, when asked ([`plan_merge`]).
//! - Joining labels follow design 018 §3.1 ([`join_labels`]).
//! - Tag texts and terminal labels are derived on every draw, never stored ([`texts_on_page`]).
//!
//! This module builds ops and words; it commits nothing and holds no UI.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::doc::{Common, Entity, EntityKind, HAlign, Handle, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::derive::terminal_positions;
use ecad_model::tag::{self, TAG_KEY};
use ecad_model::{
    Component, ComponentId, Definition, DefinitionId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Op, PageId, Placement, Pt,
    Representation, RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalId,
};
use serde_json::{Value, json};

use crate::symbols::{self, Kind, STARTERS, Starter};

/// Reserved layer of tag texts and terminal labels (design 018 §2.4).
pub const TAG_LAYER: &str = "ECAD-TAGS";
/// `ecad.provisional` (design 026 §10): ACI 30, orange, for provisional tags and wire numbers.
pub const PROVISIONAL_ACI: u8 = 30;
/// The tag height before brief 031, millimetres. **Not used for drawing**: a derived text's height
/// comes from [`crate::text_style::text_style`] (`texts_on_page` restyles every text it returns).
/// Kept only because `wires::continuation_texts` (lane W's file) still names it.
pub const TAG_HEIGHT: f64 = 3.5;
/// Tag numbers are padded to this many digits unless the project says otherwise (owner, 2026-11-08).
pub const DEFAULT_PADDING: u8 = ecad_model::DEFAULT_TAG_PADDING;
/// Most letters in a tag prefix a project may set.
pub const MAX_PREFIX_LETTERS: usize = 10;
/// Refusal of an edit of a derived tag text or terminal label.
pub const TAG_DERIVED: &str = "The tag text is derived: change the tag with Device Properties, or move it with Move Text.";
/// Most characters in a property value (design 018 §6.1).
pub const MAX_PROPERTY_CHARS: usize = 200;
/// Most devices a JSON list returns.
pub const MAX_LISTED: usize = 500;

/// The device properties, as stored and as shown (design 018 §6.1).
pub const PROPERTIES: [(&str, &str); 6] = [
    ("tag", "Tag"),
    ("description", "Description"),
    ("maker", "Maker"),
    ("part_number", "Part number"),
    ("location", "Location"),
    ("function", "Function"),
];

/// How many digits this project pads tag numbers to (its `tag_padding` setting, at most
/// [`ecad_model::tag::MAX_PADDING`]).
pub fn padding(doc: &Document) -> u8 {
    doc.settings.tag_padding.min(tag::MAX_PADDING)
}

/// A prefix as a project may set it: 1 to [`MAX_PREFIX_LETTERS`] ASCII letters, with or without a
/// leading `-`; stored in upper case without the `-`.
pub fn valid_prefix(text: &str) -> Option<String> {
    let t = text.trim();
    let t = t.strip_prefix('-').unwrap_or(t);
    ((1..=MAX_PREFIX_LETTERS).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphabetic())).then(|| t.to_ascii_uppercase())
}

/// The tag prefix a new device of symbol `s` gets in this project (letters, without `-`): the
/// project's own (`tag_prefixes`), else the built-in IEC 81346 default. A value a hostile file
/// stored that is not a valid prefix is ignored.
pub fn prefix_for(doc: &Document, s: &Starter) -> String {
    crate::defaults::symbols::prefix(doc, s.key)
}

// ---------------------------------------------------------------- numbering

/// The highest number each tag prefix has had, from the project's history.
///
/// # Semantics
/// Built from the ledger's log on first use and then only from transactions added since
/// ([`TagHigh::catch_up`]), so its cost follows the change. Every tag a transaction wrote or
/// removed counts, undone ones included, so the next automatic number is never one a device has
/// had (owner, 2026-11-08: highest + 1, deleted tags are not reused).
#[derive(Debug, Clone, Default)]
pub struct TagHigh {
    seen: usize,
    high: BTreeMap<String, u64>,
    /// The high-water mark at the ledger's base (a version 2 snapshot carries it, brief 021): the
    /// history before the base is never read for it.
    base: tag::HighWater,
    /// Provisional numbers given out since the last transaction (brief 026), so two new devices of
    /// one edit never share one; cleared when a transaction is read.
    prov: BTreeMap<String, u64>,
}

impl TagHigh {
    /// Starting from `base`, the high-water mark at the ledger's base (empty for a ledger
    /// replayed from the root).
    pub fn starting_at(base: tag::HighWater) -> Self {
        Self { seen: 0, high: base.clone(), base, prov: BTreeMap::new() }
    }

    /// Read the transactions added to `ledger`'s log since the last call.
    pub fn catch_up(&mut self, ledger: &Ledger) {
        let log = ledger.log();
        if self.seen > log.len() {
            *self = TagHigh::starting_at(self.base.clone());
        }
        for tx in log.get(self.seen..).unwrap_or_default() {
            tag::note_ops(&mut self.high, &tx.ops);
        }
        if self.seen != log.len() {
            self.prov.clear();
        }
        self.seen = log.len();
    }

    /// The highest number of each prefix (the high-water mark), as read so far.
    pub fn marks(&self) -> &tag::HighWater {
        &self.high
    }

    /// The next automatic tag for `prefix` (brief 026): provisional (`-K?3`) when the project has
    /// provisional tags on, else [`TagHigh::next`].
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn next_auto(&mut self, ledger: &Ledger, prefix: &str) -> Result<String, String> {
        self.catch_up(ledger);
        self.next_auto_in(ledger.document(), prefix)
    }

    /// [`TagHigh::next_auto`] without reading the log first.
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn next_auto_in(&mut self, doc: &Document, prefix: &str) -> Result<String, String> {
        if ecad_model::numbering::provisional_on(doc) { Ok(self.next_provisional_in(doc, prefix)) } else { self.next_in(doc, prefix) }
    }

    /// The next provisional tag of `prefix` (`-K?3`): the highest provisional number in use, or
    /// given out in this edit, + 1. Counts as given.
    pub fn next_provisional_in(&mut self, doc: &Document, prefix: &str) -> String {
        let p = prefix.trim_start_matches('-').to_ascii_uppercase();
        let in_use = tag::provisional(&ecad_model::numbering::next_provisional(doc, &p)).map_or(1, |(_, n)| n);
        let n = in_use.max(self.prov.get(&p).map_or(0, |g| g.saturating_add(1)));
        self.prov.insert(p.clone(), n);
        crate::defaults::print::tag_style(doc).provisional(&p, n)
    }

    /// The automatic tag a new device of `prefix` would get, without using it up (prompts).
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn peek_auto(&self, doc: &Document, prefix: &str) -> Result<String, String> {
        if ecad_model::numbering::provisional_on(doc) { Ok(ecad_model::numbering::next_provisional(doc, prefix)) } else { self.peek(doc, prefix) }
    }

    /// The next automatic tag for `prefix`: its highest number + 1, skipping (boundedly) any tag a
    /// device holds now.
    ///
    /// # Errors
    /// The prefix has no number left.
    pub fn next(&mut self, ledger: &Ledger, prefix: &str) -> Result<String, String> {
        self.catch_up(ledger);
        self.next_in(ledger.document(), prefix)
    }

    /// [`TagHigh::next`] without reading the log first (the caller caught up). The number given
    /// counts as used, so a second call in the same edit gives the next one.
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn next_in(&mut self, doc: &Document, prefix: &str) -> Result<String, String> {
        let (prefix, n, t) = self.first_free(doc, prefix)?;
        self.high.insert(prefix, n);
        Ok(t)
    }

    /// The tag [`TagHigh::next_in`] would give, without using it up and without reading the log
    /// (for prompts, previews and pick lists, which run every frame). The project keeps its copy
    /// caught up on every change, so this never reads the log.
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn peek(&self, doc: &Document, prefix: &str) -> Result<String, String> {
        self.first_free(doc, prefix).map(|(_, _, t)| t)
    }

    fn first_free(&self, doc: &Document, prefix: &str) -> Result<(String, u64, String), String> {
        let prefix = prefix.trim_start_matches('-').to_ascii_uppercase();
        let first = self.high.get(&prefix).copied().unwrap_or(0).saturating_add(1);
        // Bounded: a hostile file may hold many tags above the highest the log shows.
        for n in (first..=tag::MAX_TAG_NUMBER).take(10_000) {
            let t = tag::format(&prefix, n, crate::defaults::print::tag_style(doc));
            // Devices and cables share one namespace (brief 026).
            if !doc.tag_taken(&t) {
                return Ok((prefix, n, t));
            }
        }
        Err(format!("No tag number is left for -{prefix}; type a tag."))
    }
}

// ---------------------------------------------------------------- words

/// A component's tag as stored.
pub fn tag_of(doc: &Document, c: ComponentId) -> Option<&str> {
    doc.components.get(&c)?.properties.get(TAG_KEY).map(String::as_str)
}

/// A component's tag, or `(no tag)`.
pub fn tag_words(doc: &Document, c: ComponentId) -> String {
    tag_of(doc, c).map_or_else(|| "(no tag)".into(), str::to_string)
}

/// The starter symbol a representation shows, when it shows one.
pub fn rep_starter(doc: &Document, r: &Representation) -> Option<&'static Starter> {
    let item = doc.items.get(r.items.first()?)?;
    let ItemKind::Symbol(inst) = &item.kind else { return None };
    symbols::of_definition(doc.definitions.get(&inst.definition)?)
}

/// The definition a representation's symbol shows.
pub(crate) fn rep_definition<'a>(doc: &'a Document, r: &Representation) -> Option<(&'a Definition, Placement)> {
    let item = doc.items.get(r.items.first()?)?;
    let ItemKind::Symbol(inst) = &item.kind else { return None };
    Some((doc.definitions.get(&inst.definition)?, inst.placement))
}

/// A device's description: its `description`, else its first symbol's name, else `no description`.
pub fn description(doc: &Document, c: ComponentId) -> String {
    if let Some(d) = doc.components.get(&c).and_then(|x| x.properties.get("description")) {
        return d.clone();
    }
    doc.representations_of(Subject::Component(c))
        .find_map(|(_, r)| rep_definition(doc, r).map(|(d, _)| d.name.clone()))
        .unwrap_or_else(|| "no description".into())
}

/// 1-based page numbers a device is drawn on, in page order, each once.
pub fn page_numbers(doc: &Document, c: ComponentId) -> Vec<usize> {
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).collect();
    doc.page_order.iter().enumerate().filter(|(_, p)| pages.contains(p)).map(|(i, _)| i + 1).collect()
}

/// `pages 1, 2` or `page 1`.
pub fn pages_words(doc: &Document, c: ComponentId) -> String {
    let n = page_numbers(doc, c);
    let list = n.iter().map(usize::to_string).collect::<Vec<_>>().join(", ");
    if n.len() == 1 { format!("page {list}") } else { format!("pages {list}") }
}

/// The page of a device's first representation in page order.
fn first_page(doc: &Document, c: ComponentId) -> Option<PageId> {
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).collect();
    doc.page_order.iter().find(|p| pages.contains(p)).copied()
}

/// `-K01 (Contactor / relay coil, page 1 "Supply")`.
pub fn holder_words(doc: &Document, c: ComponentId) -> String {
    let page = first_page(doc, c).map(|p| format!(", {}", crate::references::page_words(doc, p))).unwrap_or_default();
    format!("{} ({}{page})", tag_words(doc, c), description(doc, c))
}

/// The device other than `except` that holds `t` (the same tag), if any.
pub fn holder_of(doc: &Document, t: &str, except: Option<ComponentId>) -> Option<ComponentId> {
    doc.components_tagged(t).into_iter().find(|c| Some(*c) != except)
}

/// What holds a tag: a device or a cable (brief 026, one namespace).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Device(ComponentId),
    Cable(ecad_model::CableId),
}

impl Holder {
    /// The holder as an object reference.
    pub fn object(self) -> ecad_model::ObjectRef {
        match self {
            Holder::Device(c) => ecad_model::ObjectRef::Component(c),
            Holder::Cable(c) => ecad_model::ObjectRef::Cable(c),
        }
    }
}

/// The device or cable other than `except` holding `t`, devices first.
pub fn tag_holder(doc: &Document, t: &str, except: Option<ecad_model::ObjectRef>) -> Option<Holder> {
    doc.components_tagged(t)
        .into_iter()
        .map(Holder::Device)
        .chain(doc.cables_tagged(t).into_iter().map(Holder::Cable))
        .find(|h| Some(h.object()) != except)
}

/// A cable's tag as stored.
pub fn cable_tag_of(doc: &Document, k: ecad_model::CableId) -> Option<&str> {
    doc.cables.get(&k)?.properties.get(TAG_KEY).map(String::as_str)
}

/// `-K03 (Contactor / relay coil, 101-C-1)` or `-W01 (cable, 3 wires)`.
pub fn holder_words_of(doc: &Document, h: Holder) -> String {
    match h {
        Holder::Device(c) => holder_words(doc, c),
        Holder::Cable(k) => {
            let n = doc.cables.get(&k).map_or(0, |c| c.members.len());
            format!("{} (cable, {n} wire{})", cable_tag_of(doc, k).unwrap_or("(no tag)"), if n == 1 { "" } else { "s" })
        }
    }
}

/// The prefix of new cable tags: the project's `tag_prefixes["cable"]`, else `W` (brief 026).
pub fn cable_prefix(doc: &Document) -> String {
    doc.settings.tag_prefixes.get("cable").and_then(|p| valid_prefix(p)).unwrap_or_else(|| "W".to_string())
}

/// The tag a new cable gets (brief 026, design §6): `typed` as stored (normalised; `-W?` is the next
/// provisional), else the next automatic one (`-W01`, or `-W?1` with provisional tags on).
///
/// # Errors
/// A typed tag that cannot be stored, or that a device or cable holds (with the next free one).
pub fn new_cable_tag(ledger: &Ledger, high: &mut TagHigh, typed: Option<&str>) -> Result<String, String> {
    high.catch_up(ledger);
    let doc = ledger.document();
    match typed {
        Some(t) => {
            let t = tag::normalise(t, crate::defaults::print::tag_style(doc))?;
            if let Some((p, 0)) = tag::provisional(&t) {
                return Ok(high.next_provisional_in(doc, &p));
            }
            if let Some(h) = tag_holder(doc, &t, None) {
                let next = tag::prefix_of(&t).and_then(|p| high.peek(doc, &p).ok());
                let way = next.map_or_else(|| "Choose another tag.".to_string(), |n| format!("Choose another tag (the next free one is {n})."));
                return Err(format!("Tag {t} is already used by {}. {way}", holder_words_of(doc, h)));
            }
            Ok(t)
        }
        None => high.next_auto_in(doc, &cable_prefix(doc)),
    }
}

/// The refusal of a duplicate, with the ways forward.
pub fn duplicate_words(doc: &Document, t: &str, holder: ComponentId, way: &str) -> String {
    format!("Tag {t} is already used by {}. {way}", holder_words(doc, holder))
}

/// The noun of an exclusive kind, for "already has a …".
fn noun(s: &Starter) -> &'static str {
    match s.key {
        "coil" => "coil",
        "mcb1" | "mcb2" => "MCB",
        "fuse" => "fuse",
        "psu" => "power supply",
        "lamp" => "lamp",
        "motor3" | "motor1" => "motor",
        "trafo" => "transformer",
        _ => "main symbol",
    }
}

// ---------------------------------------------------------------- labels (§3.1)

/// The terminal labels of component `c`.
pub fn labels_of(doc: &Document, c: ComponentId) -> BTreeSet<String> {
    doc.terminals_of(c).filter_map(|(_, t)| t.label.clone()).collect()
}

/// The labels a symbol `s` gets when it joins `c`, in its definition's terminal order; `extra`
/// are labels already given in the same transaction.
///
/// # Errors
/// Design 018 §3.1's refusals (T3).
pub fn join_labels(doc: &Document, c: ComponentId, s: &Starter, extra: &BTreeSet<String>) -> Result<Vec<String>, String> {
    let mut used = labels_of(doc, c);
    used.extend(extra.iter().cloned());
    let who = tag_words(doc, c);
    // Brief 044: a connector holds connector halves and pins only.
    if crate::connectors::is_connector(doc, c) && !matches!(s.kind, Kind::ConnectorHalf { .. }) {
        return Err(format!("{who} is a connector; place a connector half or pin on it, or Enter for a new device."));
    }
    match s.kind {
        Kind::Contact { make } => {
            // Brief 025 (owner answer Q4): the part's next free offered contact of this kind.
            if let Some(labels) = crate::xref::offer_labels(doc, c, make, &used) {
                return Ok(labels);
            }
            // Brief 034: the digit rule lives in the catalogue (a fuse bank's outputs are not contacts).
            symbols::sequence_labels(&used, &symbols::bank_outputs(doc, c), if make { &['3', '4'] } else { &['1', '2'] }, &who)
        }
        Kind::Terminal => {
            let highest = used.iter().filter_map(|l| l.parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0);
            Ok(vec![(highest + 1).to_string()])
        }
        // Brief 045 (FP7): a channel joins a card as its next free channel; its labels are pins.
        Kind::Channel(_) => crate::plc::channels::join_labels(doc, c, s, extra),
        Kind::Tagless => Err(format!("A {} is not part of a device; place it on its own.", s.name.to_lowercase())),
        // Brief 034: the label rule lives in the catalogue.
        Kind::ChangeOver => symbols::sequence_labels(&used, &symbols::bank_outputs(doc, c), &['1', '2', '4'], &who),
        // Brief 044: a connector half joins by its own rules (design 044 §4.4), through `place`.
        Kind::ConnectorHalf { .. } => {
            Err(format!("A {} joins a connector only: place it with Place ▸ Connector, or name a connector.", s.name.to_lowercase()))
        }
        Kind::Fixed { exclusive } => {
            if exclusive {
                let has = doc
                    .representations_of(Subject::Component(c))
                    .filter_map(|(_, r)| rep_starter(doc, r))
                    .find(|x| matches!(x.kind, Kind::Fixed { exclusive: true }));
                if let Some(x) = has {
                    return Err(format!("{who} already has a {}.", noun(x)));
                }
            }
            let labels = s.labels();
            if let Some(l) = labels.iter().find(|l| used.contains(*l)) {
                return Err(format!("{who} already has connection point {l}."));
            }
            Ok(labels)
        }
    }
}

// ---------------------------------------------------------------- placing

/// What a placement makes: a new device, or another symbol of an existing one.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A new device, with this tag (already normalised) or the next automatic one.
    New(Option<String>),
    /// Another symbol of this device.
    Join(ComponentId),
}

/// A planned `place` transaction and what it reports.
#[derive(Debug, Clone)]
pub struct Placed {
    pub ops: Vec<Op>,
    /// The transaction label.
    pub label: String,
    pub component: ComponentId,
    pub representation: RepresentationId,
    pub item: ItemId,
    pub definition: DefinitionId,
    pub tag: String,
    pub joined: bool,
    /// (definition key, label, terminal), in definition order.
    pub terminals: Vec<(String, String, TerminalId)>,
}

/// Plan placing starter symbol `s` at `placement` on `page` (design 018 §10).
///
/// # Errors
/// The refusals of §3.1 (joining), an unknown device, no tag number left.
#[allow(clippy::too_many_arguments)]
pub fn plan_place(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, s: &Starter, placement: Placement, target: Target,
) -> Result<Placed, String> {
    let doc = ledger.document();
    let mut ops = Vec::new();
    let definition = match symbols::embedded(doc, s) {
        Some(d) => d,
        None => {
            let d: DefinitionId = ids.fresh();
            ops.push(Op::EmbedDefinition { id: d, definition: s.definition()?.clone() });
            d
        }
    };
    let def = s.definition()?;
    // A typed tag a device already holds is that device (a tag is an identity).
    let target = match target {
        Target::New(Some(t)) => match holder_of(doc, &t, None) {
            Some(c) => Target::Join(c),
            // Devices and cables share one namespace (brief 026).
            None if !doc.cables_tagged(&t).is_empty() => {
                return Err(format!("Tag {t} is a cable's; choose another tag for a device."));
            }
            None => Target::New(Some(t)),
        },
        other => other,
    };
    let (item, representation): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let instance = Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition, placement }) };
    let mut terminals = Vec::new();
    match target {
        Target::New(t) => {
            let t = match t {
                // `-K?` typed: the next provisional number (brief 026).
                Some(t) if tag::provisional(&t).is_some_and(|(_, n)| n == 0) => {
                    high.catch_up(ledger);
                    high.next_provisional_in(doc, &tag::provisional(&t).map(|x| x.0).unwrap_or_default())
                }
                Some(t) => t,
                None => high.next_auto(ledger, &prefix_for(doc, s))?,
            };
            let c: ComponentId = ids.fresh();
            let mut properties = ecad_model::Properties::new();
            properties.insert(TAG_KEY.into(), t.clone());
            properties.insert("description".into(), s.name.into());
            ops.push(Op::CreateComponent { id: c, component: Component { properties, definition: Some(definition) } });
            for td in &def.terminals {
                let tid: TerminalId = ids.fresh();
                ops.push(Op::AddTerminal { id: tid, terminal: Terminal { component: c, label: td.label.clone(), key: Some(td.key.clone()) } });
                terminals.push((td.key.clone(), td.label.clone().unwrap_or_else(|| td.key.clone()), tid));
            }
            ops.push(Op::AddItem { id: item, item: instance });
            ops.push(Op::LinkRepresentation {
                id: representation,
                rep: Representation {
                    subject: Subject::Component(c),
                    page,
                    form: Form::Symbol,
                    role: "main".into(),
                    items: vec![item],
                    connection_points: BTreeMap::new(),
                },
            });
            Ok(Placed { ops, label: "Place symbol".into(), component: c, representation, item, definition, tag: t, joined: false, terminals })
        }
        Target::Join(c) => {
            if !doc.components.contains_key(&c) {
                return Err("No such device.".into());
            }
            let labels = join_labels(doc, c, s, &BTreeSet::new())?;
            let mut cps = BTreeMap::new();
            for (td, label) in def.terminals.iter().zip(&labels) {
                let tid: TerminalId = ids.fresh();
                ops.push(Op::AddTerminal { id: tid, terminal: Terminal { component: c, label: Some(label.clone()), key: Some(label.clone()) } });
                cps.insert(tid, td.at);
                terminals.push((td.key.clone(), label.clone(), tid));
            }
            ops.push(Op::AddItem { id: item, item: instance });
            ops.push(Op::LinkRepresentation {
                id: representation,
                rep: Representation {
                    subject: Subject::Component(c),
                    page,
                    form: Form::Symbol,
                    role: s.key.into(),
                    items: vec![item],
                    connection_points: cps,
                },
            });
            let t = tag_words(doc, c);
            Ok(Placed { ops, label: format!("Add symbol to {t}"), component: c, representation, item, definition, tag: t, joined: true, terminals })
        }
    }
}

// ---------------------------------------------------------------- merging

/// The terminals a representation shows, each with where it sits: in the definition's local frame
/// for a symbol, on the page for a drawn representation; in definition order for a symbol.
pub(crate) fn shown_terminals(doc: &Document, c: ComponentId, r: &Representation) -> Vec<(TerminalId, Pt)> {
    match r.form {
        Form::Drawn => r.connection_points.iter().map(|(t, p)| (*t, *p)).collect(),
        Form::Symbol => {
            let Some((def, _)) = rep_definition(doc, r) else { return Vec::new() };
            if r.connection_points.is_empty() {
                // A terminal no other symbol shows first (a joined terminal's key can equal the
                // definition key, review M1 2.1), else the first with the key.
                def.terminals
                    .iter()
                    .filter_map(|td| {
                        let with_key: Vec<TerminalId> =
                            doc.terminals_of(c).filter(|(_, t)| t.key.as_deref() == Some(td.key.as_str())).map(|(t, _)| t).collect();
                        let t = with_key.iter().find(|t| doc.representations_showing(**t).next().is_none()).or(with_key.first())?;
                        Some((*t, td.at))
                    })
                    .collect()
            } else {
                let mut v: Vec<(TerminalId, Pt)> = r.connection_points.iter().map(|(t, p)| (*t, *p)).collect();
                let order = |p: &Pt| def.terminals.iter().position(|td| td.at.near(*p, 1e-9)).unwrap_or(usize::MAX);
                v.sort_by_key(|(t, p)| (order(p), *t));
                v
            }
        }
    }
}

/// Plan merging device `from` into device `into` (design 018, owner decision 2026-11-08): every
/// representation and terminal of `from` moves to `into` (contacts, terminals and PLC channels
/// are renumbered as when joining), wires stay attached, and `from` is deleted. `into` keeps its
/// tag and properties.
///
/// # Errors
/// A label or exclusive symbol `into` already has (T3).
pub fn plan_merge(doc: &Document, ids: &mut IdAllocator, from: ComponentId, into: ComponentId) -> Result<Vec<Op>, String> {
    if from == into {
        return Err("A device cannot be merged with itself.".into());
    }
    let from_c = doc.components.get(&from).ok_or("No such device.")?.clone();
    if !doc.components.contains_key(&into) {
        return Err("No such device.".into());
    }
    let mut assigned: BTreeSet<String> = BTreeSet::new();
    let mut new_terminal: BTreeMap<TerminalId, TerminalId> = BTreeMap::new();
    let mut ops_add = Vec::new();
    let mut ops_reps = Vec::new();
    let mut exclusive_taken = doc
        .representations_of(Subject::Component(into))
        .filter_map(|(_, r)| rep_starter(doc, r))
        .any(|x| matches!(x.kind, Kind::Fixed { exclusive: true }));
    let who = tag_words(doc, into);
    let into_labels = labels_of(doc, into);
    let add = |old: TerminalId, label: String, ops: &mut Vec<Op>, ids: &mut IdAllocator, map: &mut BTreeMap<TerminalId, TerminalId>| -> TerminalId {
        let nt: TerminalId = ids.fresh();
        ops.push(Op::AddTerminal { id: nt, terminal: Terminal { component: into, label: Some(label.clone()), key: Some(label) } });
        map.insert(old, nt);
        nt
    };
    let reps: Vec<(RepresentationId, Representation)> = doc.representations_of(Subject::Component(from)).map(|(id, r)| (id, r.clone())).collect();
    let from_words = tag_words(doc, from);
    // Brief 031 (FP5): each symbol's own text keys move with it to its new representation.
    let mut renamed: Vec<(RepresentationId, RepresentationId)> = Vec::new();
    for (rid, r) in &reps {
        let shown = shown_terminals(doc, from, r);
        // A symbol whose device's connection points do not match its definition (an old or
        // hand-edited file) would move without any, which the model refuses (I14) with words
        // nobody can act on: say what is wrong instead.
        if r.form == Form::Symbol && shown.is_empty() && rep_definition(doc, r).is_some_and(|(def, _)| !def.terminals.is_empty()) {
            let page = doc.pages.get(&r.page).map_or_else(|| r.page.short(), |p| p.title.clone());
            return Err(format!(
                "Cannot merge {from_words}: its symbol on page \"{page}\" is not linked to any of its connection points. Delete that symbol and place it again, then merge."
            ));
        }
        let starter = rep_starter(doc, r);
        let labels: Vec<String> = match starter {
            Some(s) if !matches!(s.kind, Kind::Fixed { .. }) => {
                let l = join_labels(doc, into, s, &assigned)?;
                if l.len() != shown.len() {
                    // A symbol showing fewer terminals than its definition: keep its own labels.
                    shown.iter().map(|(t, _)| label_of(doc, *t)).collect()
                } else {
                    l
                }
            }
            _ => {
                if let Some(s) = starter.filter(|s| matches!(s.kind, Kind::Fixed { exclusive: true })) {
                    if exclusive_taken {
                        return Err(format!("{who} already has a {}.", noun(s)));
                    }
                    exclusive_taken = true;
                }
                shown.iter().map(|(t, _)| label_of(doc, *t)).collect()
            }
        };
        let mut cps = BTreeMap::new();
        for ((old, at), label) in shown.iter().zip(labels) {
            if into_labels.contains(&label) || assigned.contains(&label) {
                return Err(format!("{who} already has connection point {label}."));
            }
            assigned.insert(label.clone());
            let nt = match new_terminal.get(old) {
                Some(nt) => *nt,
                None => add(*old, label, &mut ops_add, ids, &mut new_terminal),
            };
            cps.insert(nt, *at);
        }
        let role = if r.role == "main" { starter.map_or_else(|| r.role.clone(), |s| s.key.to_string()) } else { r.role.clone() };
        let nr: RepresentationId = ids.fresh();
        renamed.push((*rid, nr));
        ops_reps.push((
            Op::UnlinkRepresentation { id: *rid, rep: r.clone() },
            Op::LinkRepresentation { id: nr, rep: Representation { subject: Subject::Component(into), role, connection_points: cps, ..r.clone() } },
        ));
    }
    // Terminals no representation shows move too, with their labels.
    for (t, term) in doc.terminals_of(from) {
        if new_terminal.contains_key(&t) {
            continue;
        }
        let label = term.label.clone().unwrap_or_else(|| t.hex());
        if into_labels.contains(&label) || assigned.contains(&label) {
            return Err(format!("{who} already has connection point {label}."));
        }
        assigned.insert(label.clone());
        add(t, label, &mut ops_add, ids, &mut new_terminal);
    }
    let mut ops = ops_add;
    let (unlinks, links): (Vec<Op>, Vec<Op>) = ops_reps.into_iter().unzip();
    ops.extend(unlinks);
    ops.extend(links);
    for (old, nt) in &new_terminal {
        for (w, end) in doc.ends_at(*old) {
            ops.push(Op::AttachEnd { conductor: w, end, before: Some(*old), after: Some(*nt) });
        }
    }
    for (t, term) in doc.terminals_of(from) {
        ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
    }
    let into_props = doc.components.get(&into).map(|c| &c.properties);
    for (old, new) in renamed {
        for (key, value) in crate::text_style::symbol_text_keys(&from_c.properties, old) {
            let moved = crate::text_style::rename_symbol_key(&key, old, new);
            let before = into_props.and_then(|p| p.get(&moved)).cloned();
            if before.as_ref() != Some(&value) {
                ops.push(Op::SetProperty { object: ecad_model::ObjectRef::Component(into), key: moved, before, after: Some(value) });
            }
        }
    }
    // Brief 045 (FP1 condition 2, review of #84, 1): a rack merged away hands its cards to `into`,
    // which takes its rack body (or already is a rack); otherwise they leave it, in this transaction.
    ops.extend(crate::plc::rack::merge_ops(doc, from, into));
    ops.push(Op::DeleteComponent { id: from, component: from_c });
    Ok(ops)
}

pub(crate) fn label_of(doc: &Document, t: TerminalId) -> String {
    doc.terminals.get(&t).and_then(|x| x.label.clone()).unwrap_or_else(|| t.hex())
}

// ---------------------------------------------------------------- erasing (§3.3)

/// What erasing device symbols adds to a drawing edit (design 018 §3.3): `erased` are the
/// representations of starter-symbol devices the edit unlinks, read on the document before it.
/// A device keeps its other symbols and loses the terminals only the erased ones showed (their
/// wire ends are disconnected); a device whose last symbol goes is deleted. Representations of
/// other components keep today's behaviour (the component stays, DOC-06).
pub fn erase_followups(doc: &Document, erased: &[(RepresentationId, Representation)]) -> (Vec<Op>, Vec<String>) {
    let mut by_component: BTreeMap<ComponentId, Vec<&(RepresentationId, Representation)>> = BTreeMap::new();
    for e in erased {
        if let Subject::Component(c) = e.1.subject
            && rep_starter(doc, &e.1).is_some()
        {
            by_component.entry(c).or_default().push(e);
        }
    }
    let gone: BTreeSet<RepresentationId> = erased.iter().map(|(id, _)| *id).collect();
    let (mut ops, mut notes) = (Vec::new(), Vec::new());
    // A location box (brief 024) goes with its outline.
    let boxes: BTreeSet<ComponentId> = erased
        .iter()
        .filter_map(|(_, r)| if let Subject::Component(c) = r.subject { Some(c) } else { None })
        .filter(|c| crate::locations::is_box(doc, *c))
        .collect();
    for c in boxes {
        if doc.representations_of(Subject::Component(c)).all(|(id, _)| gone.contains(&id))
            && let Some(component) = doc.components.get(&c)
        {
            ops.push(Op::DeleteComponent { id: c, component: component.clone() });
        }
    }
    for (c, reps) in by_component {
        let Some(component) = doc.components.get(&c) else { continue };
        let remaining: Vec<&Representation> =
            doc.representations_of(Subject::Component(c)).filter(|(id, _)| !gone.contains(id)).map(|(_, r)| r).collect();
        let who = tag_words(doc, c);
        let drop: Vec<TerminalId> = if remaining.is_empty() {
            doc.terminals_of(c).map(|(t, _)| t).collect()
        } else {
            let kept: BTreeSet<TerminalId> = remaining.iter().flat_map(|r| shown_terminals(doc, c, r)).map(|(t, _)| t).collect();
            reps.iter()
                .flat_map(|(_, r)| shown_terminals(doc, c, r))
                .map(|(t, _)| t)
                .filter(|t| !kept.contains(t))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        for t in &drop {
            for (w, end) in doc.ends_at(*t) {
                ops.push(Op::AttachEnd { conductor: w, end, before: Some(*t), after: None });
            }
        }
        for t in &drop {
            if let Some(term) = doc.terminals.get(t) {
                ops.push(Op::RemoveTerminal { id: *t, terminal: term.clone() });
            }
        }
        if remaining.is_empty() {
            // Brief 045 (design 045 §2.2, FP1): a rack's cards leave it in the same transaction.
            let (released, n) = if crate::plc::rack::is_rack(doc, c) { crate::plc::rack::release_cards(doc, c) } else { (Vec::new(), 0) };
            ops.extend(released);
            ops.push(Op::DeleteComponent { id: c, component: component.clone() });
            if n > 0 {
                notes.push(format!("Deleted {who}; {}", crate::plc::rack::released_words(n)));
            }
            notes.push(format!("Deleted {who} with its last symbol; U brings it back."));
        } else {
            // Brief 036 (review M1): a level's own label and a fuse position's rating go with
            // the last terminal of their level or position, never left to come back later.
            let (key_ops, key_notes) = crate::terminal_types::orphaned_key_ops(doc, c, &drop);
            ops.extend(key_ops);
            notes.extend(key_notes);
            // Brief 031 (FP5): an erased symbol's own text keys go with it.
            for (rid, _) in &reps {
                for (key, value) in crate::text_style::symbol_text_keys(&component.properties, *rid) {
                    ops.push(Op::SetProperty { object: ecad_model::ObjectRef::Component(c), key, before: Some(value), after: None });
                }
            }
            for (_, r) in &reps {
                let name = rep_starter(doc, r).map_or("symbol", |s| s.name);
                let labels: Vec<String> = shown_terminals(doc, c, r).iter().map(|(t, _)| label_of(doc, *t)).collect();
                notes.push(format!(
                    "Removed {who}'s {name} {} ({}); {who} is still shown on {}.",
                    labels.join("/"),
                    crate::references::page_number_words(doc, r.page),
                    shown_pages_words(doc, &remaining)
                ));
            }
        }
    }
    (ops, notes)
}

fn shown_pages_words(doc: &Document, reps: &[&Representation]) -> String {
    let pages: BTreeSet<PageId> = reps.iter().map(|r| r.page).collect();
    let n: Vec<String> = doc.page_order.iter().enumerate().filter(|(_, p)| pages.contains(p)).map(|(i, _)| (i + 1).to_string()).collect();
    if n.len() == 1 { format!("page {}", n.join(", ")) } else { format!("pages {}", n.join(", ")) }
}

/// The tag a copy of device `c` gets (design 018 §3.3): the next automatic number of its prefix;
/// `None` when its tag is not of the usual form (the copy then has no tag).
pub fn copy_tag(doc: &Document, high: &mut TagHigh, c: &Component) -> Option<String> {
    let t = c.properties.get(TAG_KEY)?;
    let (prefix, _) = tag::parse(t).or_else(|| tag::provisional(t))?;
    high.next_auto_in(doc, &prefix).ok()
}

// ---------------------------------------------------------------- derived texts (§2.4)

/// What a derived text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextOf {
    Tag,
    Terminal(TerminalId),
    /// A continuation's name (brief 022).
    Name,
    /// A continuation's other places (brief 022).
    Places,
    /// A device's description under its tag (brief 031, hidden by default).
    Description,
    /// A device's part number under its tag (brief 031, hidden by default).
    PartNumber,
    /// Another device text a project symbol's text slot places (brief 048, design §2.3): its
    /// index in the definition's `texts`.
    Slot(u8),
    /// Brief 045 (design 045 §7.2): a PLC card body's caption, its rack and slot.
    Caption,
    /// Brief 045 (design 045 §7.1): a channel cell's channel number, address and description
    /// (written turned 90° when `rotated`).
    ChannelNo,
    Address,
    ChannelText {
        rotated: bool,
    },
}

/// One derived text on a page: a device's tag beside one of its symbols, or a terminal label.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedText {
    pub rep: RepresentationId,
    pub of: TextOf,
    pub value: String,
    pub at: Pt,
    pub height: f64,
    pub halign: HAlign,
    pub valign: VAlign,
    /// A colour of its own (ACI), else ByLayer.
    pub aci: Option<u8>,
}

/// The page box of a placed definition.
pub(crate) fn placed_box(def: &Definition, pl: &Placement) -> Option<(Pt, Pt)> {
    let (lo, hi) = symbols::bounds(def);
    let corners = [lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)].map(|q| pl.apply(q));
    let (mut a, mut b) = (corners[0], corners[0]);
    for q in corners {
        a = Pt::new(a.x.min(q.x), a.y.min(q.y));
        b = Pt::new(b.x.max(q.x), b.y.max(q.y));
    }
    (a.is_finite() && b.is_finite()).then_some((a, b))
}

/// Whether a placement's terminal axis is vertical (rotation near 0° or 180°).
pub(crate) fn vertical(pl: &Placement) -> bool {
    let q = pl.rotation.rem_euclid(std::f64::consts::PI);
    q < std::f64::consts::FRAC_PI_4 || q > 3.0 * std::f64::consts::FRAC_PI_4
}

/// The sizes of a symbol's tag and connection point labels in a project (design 031 §3.2, §6.2):
/// heights from the style table, the gap between the box and the tag as `text.tag.gap` × the
/// tag height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TagSizes {
    pub tag: f64,
    pub label: f64,
    pub gap: f64,
}

impl TagSizes {
    /// The sizes of `doc`.
    pub fn of(doc: &Document) -> TagSizes {
        use crate::text_style::{TextKind, text_style};
        let tag = text_style(doc, TextKind::Tag).height;
        let factor = crate::defaults::resolve(doc, &crate::defaults::text::TAG_GAP, crate::defaults::Scope::project()).number();
        TagSizes { tag, label: text_style(doc, TextKind::PointLabel).height, gap: factor * tag }
    }
}

/// The kind of text (design 031 §3.1) a derived text is.
pub fn kind_of(of: TextOf) -> crate::text_style::TextKind {
    use crate::text_style::TextKind;
    match of {
        TextOf::Tag => TextKind::Tag,
        TextOf::Terminal(_) => TextKind::PointLabel,
        TextOf::Name => TextKind::ContinuationName,
        TextOf::Places => TextKind::Reference,
        TextOf::Description | TextOf::Caption | TextOf::ChannelText { .. } => TextKind::Description,
        TextOf::ChannelNo | TextOf::Address => TextKind::PointLabel,
        TextOf::PartNumber => TextKind::PartNumber,
        TextOf::Slot(_) => TextKind::Description,
    }
}

/// The texts of one symbol representation at `pl` (also used by the placing preview): its
/// device's tag and the labels `labels` at their page positions, at `sizes`.
pub fn texts_for(
    def: &Definition, pl: &Placement, tag: Option<&str>, labels: &[(TextOf, String, Pt)], sizes: TagSizes,
) -> Vec<(TextOf, String, Pt, f64, HAlign, VAlign)> {
    let Some((lo, hi)) = placed_box(def, pl) else { return Vec::new() };
    let centre = Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    let vert = vertical(pl);
    let mut out = Vec::new();
    if let Some(t) = tag {
        let (at, h, v) = if vert {
            (Pt::new(lo.x - sizes.gap, centre.y), HAlign::Right, VAlign::Middle)
        } else {
            (Pt::new(centre.x, hi.y + sizes.gap), HAlign::Center, VAlign::Bottom)
        };
        out.push((TextOf::Tag, t.to_string(), at, sizes.tag, h, v));
    }
    for (of, label, at) in labels {
        if !at.is_finite() {
            continue;
        }
        let (p, h, v) = if vert {
            let dy = if at.y > centre.y { -2.5 } else { 2.5 };
            (Pt::new(at.x + 1.0, at.y + dy), HAlign::Left, VAlign::Middle)
        } else {
            let dx = if at.x > centre.x { -2.5 } else { 2.5 };
            (Pt::new(at.x + dx, at.y + 1.0), HAlign::Center, VAlign::Bottom)
        };
        out.push((*of, label.clone(), p, sizes.label, h, v));
    }
    out
}

/// Every derived text shown on `page` (what the page draws and prints): [`all_texts_on_page`]'s
/// shown ones.
pub fn texts_on_page(doc: &Document, page: PageId) -> Vec<DerivedText> {
    all_texts_on_page(doc, page).into_iter().filter(|t| t.shown).map(|t| t.text).collect()
}

/// A derived text and whether it is shown (design 031 §5): hidden texts are still data, listed by
/// `pagetexts` and drawn dimmed by Show Hidden Text.
#[derive(Debug, Clone, PartialEq)]
pub struct PageText {
    pub text: DerivedText,
    pub shown: bool,
    /// Its place is stored (Move Text, design 031 §6.4).
    pub moved: bool,
    /// Its tag block found no free candidate (§6.3).
    pub overlaps: bool,
}

thread_local! {
    /// How many times [`all_texts_on_page`] ran per page on this thread: the update-map tests
    /// count it (a change elsewhere must not work a page's texts out again).
    static TEXT_PASSES: std::cell::RefCell<BTreeMap<PageId, u64>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// How many times this thread has worked out `page`'s derived texts (see `TEXT_PASSES`).
pub fn text_passes(page: PageId) -> u64 {
    TEXT_PASSES.with(|c| c.borrow().get(&page).copied().unwrap_or(0))
}

/// The run rule of a symbol (design 031 §5.3), from its starter kind.
pub fn run_rule(doc: &Document, r: &Representation) -> crate::text_style::RunRule {
    use crate::text_style::RunRule;
    // Brief 044 (design 044 §5.2): a connector's pins show their half's tag once per run.
    if crate::connectors::rep_half(doc, r).is_some_and(crate::connectors::is_pin_starter) {
        return RunRule::Strip;
    }
    match rep_starter(doc, r).map(|s| s.kind) {
        Some(Kind::Terminal) => RunRule::Strip,
        Some(Kind::Channel(_)) => RunRule::Plc,
        _ => RunRule::None,
    }
}

/// Every derived text on `page`, shown or hidden: for each symbol representation of a component,
/// its device's tag block (the tag, then its description and part number when they have values)
/// and its terminals' labels; a continuation's name and other places. Visibility follows design
/// 031 §5.2 (symbol, device, page, project, built-in) and a tag set to once per run is shown on
/// the first symbol of its run only (§5.3). Hostile values (non-finite placements) give nothing
/// for that representation.
pub fn all_texts_on_page(doc: &Document, page: PageId) -> Vec<PageText> {
    all_texts_on_page_with(doc, page, None)
}

/// [`all_texts_on_page`], reading PLC addresses from the kept model `plc` when there is one
/// (brief 045, review of #84, 3).
pub fn all_texts_on_page_with(doc: &Document, page: PageId, plc: Option<&crate::plc::Plc>) -> Vec<PageText> {
    TEXT_PASSES.with(|c| *c.borrow_mut().entry(page).or_default() += 1);
    use crate::text_style::{Block, Shown, TextKind, TextTarget, text_shown, text_style, width_in};
    let mut by_rep: BTreeMap<RepresentationId, Vec<(TextOf, String, Pt)>> = BTreeMap::new();
    for (t, rid, at) in terminal_positions(doc, page) {
        // Brief 044: a pin pair shows its pin name once, beside the socket point.
        if doc.representations.get(&rid).is_some_and(|r| crate::connectors::hides_label(doc, r, t)) {
            continue;
        }
        // Brief 044: a connector pin's label on the page is its pin name (`3`), beside its half's
        // tag; elsewhere it is named `-X10.S:3`.
        if crate::connectors::side_of(doc, t).is_some() {
            if let Some(label) = doc.terminals.get(&t).and_then(|x| x.label.clone()) {
                by_rep.entry(rid).or_default().push((TextOf::Terminal(t), label, at));
            }
            continue;
        }
        // Brief 036: a strip level shows its shown label (`1.L`), not its position.
        if let Some(label) = crate::terminal_types::shown_label(doc, t) {
            by_rep.entry(rid).or_default().push((TextOf::Terminal(t), label, at));
        }
    }
    let mut out = Vec::new();
    let sizes = TagSizes::of(doc);
    let look = crate::text_style::Look::new(doc);
    let pitch = crate::defaults::resolve(doc, &crate::defaults::text::LINE_PITCH, crate::defaults::Scope::project()).number();
    let firsts = crate::text_style::run_firsts(doc, page);
    // Location prefixes (brief 024, design 024 §3.3) read this page's boxes once.
    let mut boxes = crate::locations::BoxCache::default();
    // Brief 045: the racks' address blocks, worked out once per draw.
    let mut plc_blocks = std::collections::BTreeMap::new();
    // Tag blocks wait for placement (design 031 §6), after every fixed text is known.
    struct Pending {
        block: Block,
        lines: Vec<(TextOf, String, f64, bool, Option<u8>)>,
    }
    let mut pending: Vec<Pending> = Vec::new();
    let mut fixed: Vec<crate::text_style::TextBox> = Vec::new();
    for (rid, r) in doc.representations_on(page) {
        let Subject::Component(c) = r.subject else { continue };
        if r.form != Form::Symbol {
            continue;
        }
        let Some((def, pl)) = rep_definition(doc, r) else { continue };
        if !pl.is_valid() {
            continue;
        }
        let labels = by_rep.remove(&rid).unwrap_or_default();
        let target = TextTarget { page: Some(page), component: Some(c), representation: Some(rid), conductor: None, run: run_rule(doc, r) };
        let shown = |kind: TextKind| match text_shown(doc, target, kind) {
            Shown::On => true,
            Shown::Off => false,
            Shown::First => firsts.contains(&rid),
        };
        // A continuation shows its name and its other places instead (brief 022). Their sizes
        // and visibility come from the style table (design 031 §7.1): restyled here, by kind.
        if let Some(texts) = crate::wires::continuation_texts(doc, c, rid, &pl) {
            out.extend(texts.into_iter().map(|mut t| {
                t.height = text_style(doc, kind_of(t.of)).height;
                let shown = shown(kind_of(t.of));
                PageText { text: t, shown, moved: false, overlaps: false }
            }));
            continue;
        }
        // Connection point labels keep their fixed rule (or a stored place, §6.4) and are obstacles.
        let labels_shown = shown(TextKind::PointLabel);
        let label_style = text_style(doc, TextKind::PointLabel);
        let props = doc.components.get(&c).map(|x| &x.properties);
        for (of, value, at, height, halign, valign) in texts_for(def, &pl, None, &labels, sizes) {
            let (mut at, mut halign, mut valign, mut moved) = (at, halign, valign, false);
            if let TextOf::Terminal(t) = of
                && let Some((p, h, v)) = props
                    .and_then(|p| p.get(&point_label_key(doc, rid, t)))
                    .and_then(|s| crate::text_style::parse_at(s))
                    .map(|(p, h, v)| (pl.apply(p), h, v))
                    .filter(|(p, _, _)| p.is_finite())
            {
                (at, halign, valign, moved) = (p, h, v, true);
            }
            if labels_shown {
                let w = width_in(look.fonts_for(TextKind::PointLabel), crate::text_style::TextStyle { height, ..label_style }, &value);
                fixed.push(crate::text_style::text_box(w, height, at, halign, valign));
            }
            out.push(PageText {
                text: DerivedText { rep: rid, of, value, at, height, halign, valign, aci: None },
                shown: labels_shown,
                moved,
                overlaps: false,
            });
        }
        // Brief 045 (design 045 §7.2): a card body's caption at its anchor, under the body.
        if let Some(text) = crate::plc::caption_text(doc, c, rid, r, &pl) {
            out.push(text);
        }
        // Brief 045 (design 045 §7.1): a channel cell's number, address and description.
        out.extend(crate::plc::channel_texts(doc, c, rid, r, &pl, plc, &mut plc_blocks));
        // Brief 048 (design 048 §2.3): a symbol with text slots places each device text at its
        // slot instead of in a tag block.
        if !def.texts.is_empty() {
            out.extend(slot_texts(doc, c, rid, r, def, &pl, sizes, &shown, &mut boxes, &look, &mut fixed));
            continue;
        }
        // The tag block (design 031 §6.1): the tag, then the description and part number.
        let Some((lo, hi)) = placed_box(def, &pl) else { continue };
        // Brief 026: a provisional tag is drawn in `ecad.provisional`.
        let prov = tag_of(doc, c).is_some_and(tag::is_provisional);
        let mut lines: Vec<(TextOf, String, f64, bool, Option<u8>)> = Vec::new();
        // Brief 044 (design 044 §5.2): a connector symbol shows its half's tag and fields.
        let half = crate::connectors::rep_side(doc, r);
        let tag_text = match half {
            Some(side) => Some(crate::connectors::half_tag(doc, c, side)),
            None => tag_of(doc, c).map(str::to_string),
        };
        if let Some(t) = tag_text.map(|t| crate::locations::shown_tag(doc, c, r, &t, &mut boxes)) {
            lines.push((TextOf::Tag, t, sizes.tag, shown(TextKind::Tag), prov.then_some(PROVISIONAL_ACI)));
        }
        for (of, key, kind) in
            [(TextOf::Description, "description", TextKind::Description), (TextOf::PartNumber, "part_number", TextKind::PartNumber)]
        {
            let value = match half {
                Some(side @ (crate::symbols::Side::Socket | crate::symbols::Side::Plug)) => crate::connectors::half_prop(doc, c, side, key),
                Some(crate::symbols::Side::Both) => None,
                None => props.and_then(|p| p.get(key)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
            };
            if let Some(v) = value {
                lines.push((of, v, text_style(doc, kind).height, shown(kind), None));
            }
        }
        if lines.is_empty() {
            continue;
        }
        // Shown lines first; the size is theirs (all of them when none is shown, for Show Hidden
        // Text).
        lines.sort_by_key(|l| !l.3);
        let any = lines.iter().any(|l| l.3);
        let sized: Vec<&(TextOf, String, f64, bool, Option<u8>)> = lines.iter().filter(|l| l.3 || !any).collect();
        let w = sized
            .iter()
            .map(|l| width_in(look.fonts_for(kind_of(l.0)), crate::text_style::TextStyle { height: l.2, ..text_style(doc, kind_of(l.0)) }, &l.1))
            .fold(0.0, f64::max);
        // Line tops are `text.line_pitch` × each line's height apart.
        let h = sized.iter().map(|l| pitch * l.2).sum::<f64>() - sized.last().map_or(0.0, |l| (pitch - 1.0) * l.2);
        pending.push(Pending { block: Block { rep: rid, component: c, lo, hi, vertical: vertical(&pl), placement: pl, w, h }, lines });
    }
    let placed = crate::text_style::place_blocks(
        doc,
        page,
        &pending.iter().filter(|p| p.lines.iter().any(|l| l.3)).map(|p| p.block.clone()).collect::<Vec<_>>(),
        &fixed,
    );
    for p in pending {
        let place = placed.get(&p.block.rep).copied().unwrap_or_else(|| {
            // Nothing shown: the first candidate, for Show Hidden Text only.
            let gap = sizes.gap;
            let (rect, halign) = crate::text_style::candidates(p.block.lo, p.block.hi, p.block.vertical, p.block.w, p.block.h, gap)[0];
            crate::text_style::BlockPlace { rect, halign, moved: false, overlaps: false }
        });
        let mut top = place.rect.1.y;
        for (of, value, height, shown, aci) in p.lines {
            let at = place.line_at(top);
            out.push(PageText {
                text: DerivedText { rep: p.block.rep, of, value, at, height, halign: place.halign, valign: VAlign::Top, aci },
                shown,
                moved: place.moved,
                overlaps: place.overlaps && shown,
            });
            top -= pitch * height;
        }
    }
    out
}

/// The texts a project symbol's slots place for representation `rid` of device `c` (brief 048,
/// design §2.3): the tag where its show rule says, every other slot's text whenever the device
/// has a value for it. Each is an obstacle for the page's tag blocks.
#[allow(clippy::too_many_arguments)]
fn slot_texts(
    doc: &Document, c: ComponentId, rid: RepresentationId, r: &Representation, def: &Definition, pl: &Placement, sizes: TagSizes,
    shown: &dyn Fn(crate::text_style::TextKind) -> bool, boxes: &mut crate::locations::BoxCache, look: &crate::text_style::Look,
    fixed: &mut Vec<(Pt, Pt)>,
) -> Vec<PageText> {
    use crate::text_style::{TextKind, text_style, width_in};
    let props = doc.components.get(&c).map(|x| &x.properties);
    let prop = |k: &str| props.and_then(|p| p.get(k)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let h = |a: ecad_model::TextHAlign| match a {
        ecad_model::TextHAlign::Left => HAlign::Left,
        ecad_model::TextHAlign::Center => HAlign::Center,
        ecad_model::TextHAlign::Right => HAlign::Right,
    };
    let v = |a: ecad_model::TextVAlign| match a {
        ecad_model::TextVAlign::Baseline => VAlign::Baseline,
        ecad_model::TextVAlign::Bottom => VAlign::Bottom,
        ecad_model::TextVAlign::Middle => VAlign::Middle,
        ecad_model::TextVAlign::Top => VAlign::Top,
    };
    let mut out = Vec::new();
    for (i, slot) in def.texts.iter().enumerate().take(usize::from(u8::MAX)) {
        let (of, kind, value, aci) = match slot.text.as_str() {
            "tag" => {
                let t = tag_of(doc, c).map(|t| crate::locations::shown_tag(doc, c, r, t, boxes));
                let prov = tag_of(doc, c).is_some_and(tag::is_provisional);
                (TextOf::Tag, TextKind::Tag, t, prov.then_some(PROVISIONAL_ACI))
            }
            "description" => (TextOf::Description, TextKind::Description, prop("description"), None),
            "part_number" => (TextOf::PartNumber, TextKind::PartNumber, prop("part_number"), None),
            other => (TextOf::Slot(u8::try_from(i).unwrap_or(u8::MAX)), TextKind::Description, prop(other), None),
        };
        let Some(value) = value else { continue };
        let style = text_style(doc, kind);
        let height = if of == TextOf::Tag { sizes.tag } else { style.height };
        let on = if of == TextOf::Tag { shown(kind) } else { true };
        let at = pl.apply(slot.at);
        if !at.is_finite() {
            continue;
        }
        let (ha, va) = (h(slot.h), v(slot.v));
        if on {
            let w = width_in(look.fonts_for(kind), crate::text_style::TextStyle { height, ..style }, &value);
            fixed.push(crate::text_style::text_box(w, height, at, ha, va));
        }
        out.push(PageText {
            text: DerivedText { rep: rid, of, value, at, height, halign: ha, valign: va, aci },
            shown: on,
            moved: false,
            overlaps: false,
        });
    }
    out
}

/// The key a connection point label's stored place is kept under (design 031 §6.4):
/// `text.<rep>.point_label.<terminal key>.at`, the terminal key being its definition key, else its
/// label.
pub fn point_label_key(doc: &Document, rep: RepresentationId, t: TerminalId) -> String {
    let term = doc.terminals.get(&t);
    let key = term.and_then(|x| x.key.clone()).or_else(|| term.and_then(|x| x.label.clone())).unwrap_or_else(|| t.hex());
    format!("{}point_label.{key}.at", crate::text_style::symbol_prefix(rep))
}

/// A derived text as an entity on [`TAG_LAYER`], in its kind's style from `look` (design 031 §3).
pub fn text_entity(look: &crate::text_style::Look, t: &DerivedText, handle: Handle) -> Entity {
    let mut e = text_entity_with(&t.value, t.at, t.height, t.halign, t.valign, handle, TAG_LAYER);
    look.apply(&mut e, kind_of(t.of));
    // Brief 045 (design 045 §7.1): a channel description reads upwards under its cell.
    if let (TextOf::ChannelText { rotated: true }, EntityKind::Text(x)) = (t.of, &mut e.kind) {
        x.rotation = std::f64::consts::FRAC_PI_2;
    }
    if let Some(aci) = t.aci {
        e.common.color = cadcraft_engine::doc::color::Color::Index(aci);
    }
    e
}

/// A justified text entity.
pub fn text_entity_with(value: &str, at: Pt, height: f64, halign: HAlign, valign: VAlign, handle: Handle, layer: &str) -> Entity {
    let p = Vec3::new(at.x, at.y, 0.0);
    let justified = !(halign == HAlign::Left && valign == VAlign::Baseline);
    Entity {
        handle,
        common: Common { layer: layer.into(), ..Common::default() },
        kind: EntityKind::Text(Text {
            insert: p,
            align_pt: justified.then_some(p),
            height,
            value: value.to_string(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign,
            valign,
        }),
    }
}

// ---------------------------------------------------------------- JSON (§8)

/// Whether device tag `t` matches a search `query` (design 018 §5.1): `-k` and `k1` find `-K01`,
/// `-K10`…
pub fn tag_matches(t: &str, query: &str) -> bool {
    let q = query.trim().trim_start_matches('-').to_uppercase();
    if q.is_empty() {
        return true;
    }
    match (tag::parse(t), tag::parse(&q)) {
        (Some((p, n)), Some((qp, qn))) => p == qp && n.to_string().starts_with(&qn.to_string()),
        (Some((p, _)), None) => p.starts_with(&q) && q.chars().all(|c| c.is_ascii_alphabetic()),
        _ => tag::key(t).starts_with(&q),
    }
}

/// Every device (a component with a tag) whose tag matches `query` (or every one), ordered by
/// prefix, then number, then ID.
pub fn devices(doc: &Document, query: Option<&str>) -> Vec<ComponentId> {
    // Through the tag index (brief 018 "Speed"): untagged components are never visited.
    let mut v: Vec<(String, u64, String, ComponentId)> = doc
        .tagged()
        .filter_map(|(_, id)| {
            let t = doc.components.get(&id)?.properties.get(TAG_KEY)?;
            if query.is_some_and(|q| !tag_matches(t, q)) {
                return None;
            }
            let (p, n) = tag::parse(t).unwrap_or_else(|| (tag::key(t), 0));
            Some((p, n, t.clone(), id))
        })
        .collect();
    v.sort();
    // Brief 044 (design 044 §5.1, tester D5): a typed half tag (`-X10.S`, `-X10.P:3`) finds its
    // connector.
    if v.is_empty()
        && let Some(Ok((c, Some(_), _))) = query.and_then(|q| crate::connectors::resolve(doc, q))
    {
        return vec![c];
    }
    v.into_iter().map(|(_, _, _, id)| id).collect()
}

/// `symbols {query?}`'s result (design 018 §8).
pub fn symbols_json(doc: &Document, query: Option<&str>) -> Value {
    let q = query.unwrap_or("");
    let symbols: Vec<Value> = symbols::search(q)
        .into_iter()
        .map(|s| {
            let (w, h) = s.size();
            json!({
                "key": s.key, "name": s.name, "aliases": s.aliases, "prefix": format!("-{}", prefix_for(doc, s)),
                "terminals": s.labels(), "box": [w, h], "joinable": s.joinable(),
            })
        })
        .collect();
    let found = if q.trim().is_empty() { devices(doc, None) } else { devices(doc, Some(q)) };
    let more = found.len().saturating_sub(MAX_LISTED);
    let devices: Vec<Value> = found
        .into_iter()
        .take(MAX_LISTED)
        .map(|c| {
            json!({
                "component": c.hex(), "tag": tag_words(doc, c), "description": description(doc, c),
                "representations": doc.representations_of(Subject::Component(c)).count(),
                "pages": page_numbers(doc, c),
            })
        })
        .collect();
    let mut r = json!({ "symbols": symbols, "devices": devices });
    if more > 0 {
        r["more"] = json!(more);
    }
    r
}

/// `devices {tag?}`'s result (design 018 §8).
pub fn devices_json(doc: &Document, query: Option<&str>) -> Value {
    let found = devices(doc, query);
    let more = found.len().saturating_sub(MAX_LISTED);
    let list: Vec<Value> = found.into_iter().take(MAX_LISTED).map(|c| device_json(doc, c)).collect();
    let mut r = json!({ "devices": list });
    if more > 0 {
        r["more"] = json!(more);
    }
    r
}

/// One device in JSON.
pub fn device_json(doc: &Document, c: ComponentId) -> Value {
    let props = doc.components.get(&c).map(|x| x.properties.clone()).unwrap_or_default();
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i + 1)).collect();
    let mut reps: Vec<(usize, RepresentationId, Value)> = doc
        .representations_of(Subject::Component(c))
        .map(|(rid, r)| {
            let labels: Vec<String> = shown_terminals(doc, c, r).iter().map(|(t, _)| label_of(doc, *t)).collect();
            let n = order.get(&r.page).copied().unwrap_or(usize::MAX);
            (
                n,
                rid,
                json!({
                    "representation": rid.hex(), "page": r.page.hex(), "page_number": order.get(&r.page),
                    "role": r.role, "symbol": rep_starter(doc, r).map(|s| s.key), "terminals": labels,
                }),
            )
        })
        .collect();
    reps.sort_by_key(|(n, rid, _)| (*n, *rid));
    let mut v = json!({ "component": c.hex() });
    for (key, _) in PROPERTIES {
        v[key] = props.get(key).map_or(Value::Null, |x| Value::String(x.clone()));
    }
    // Brief 024 (design 024 §3.4): the location shown and where it comes from, and each symbol's
    // context.
    let mut boxes = crate::locations::BoxCache::default();
    let loc = crate::locations::resolve_with(doc, c, &mut boxes);
    v["location_shown"] = loc.value.map_or(Value::Null, Value::String);
    v["location_from"] = json!(loc.source.word());
    for (_, rid, x) in &mut reps {
        if let Some(r) = doc.representations.get(rid) {
            x["context"] = crate::locations::rep_context(doc, r, &mut boxes).value.map_or(Value::Null, Value::String);
        }
    }
    v["representations"] = Value::Array(reps.into_iter().map(|(_, _, x)| x).collect());
    // Brief 026.
    v["tag_fixed"] = Value::Bool(ecad_model::numbering::is_fixed(&props));
    v["provisional"] = Value::Bool(props.get(TAG_KEY).is_some_and(|t| tag::is_provisional(t)));
    v
}

/// The device a starter symbol `key` names.
///
/// # Errors
/// T4: no such symbol.
pub fn starter(key: &str) -> Result<&'static Starter, String> {
    symbols::by_key(key).ok_or_else(|| format!("No symbol \"{}\". Type List to see them.", key.trim()))
}

/// A device property value as stored (design 018 §6.1): trimmed; `location` with a leading `+`,
/// `function` with a leading `=`; empty clears it. Tags go through [`ecad_model::tag::normalise`].
///
/// # Errors
/// T6: too long or a control character.
pub fn property_value(doc: &Document, key: &str, value: Option<&str>) -> Result<Option<String>, String> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if key == TAG_KEY {
        return tag::normalise(v, crate::defaults::print::tag_style(doc)).map(Some);
    }
    if v.chars().count() > MAX_PROPERTY_CHARS || v.chars().any(char::is_control) {
        return Err(format!("`{key}` can be at most {MAX_PROPERTY_CHARS} characters, with no control characters"));
    }
    Ok(Some(match key {
        "location" if !v.starts_with('+') => format!("+{v}"),
        "function" if !v.starts_with('=') => format!("={v}"),
        _ => v.to_string(),
    }))
}

/// Every starter symbol's key, for "List".
pub fn symbol_keys() -> Vec<&'static str> {
    STARTERS.iter().map(|s| s.key).collect()
}

// ---------------------------------------------------------------- hover (§6.4)

/// A symbol's short name in the lens: `coil`, `NO contact`…
fn short_name(s: &Starter) -> &'static str {
    match s.key {
        "coil" => "coil",
        _ => s.name,
    }
}

/// `NO contact 13/14` for a representation.
fn rep_words(doc: &Document, c: ComponentId, r: &Representation) -> String {
    let labels: Vec<String> = shown_terminals(doc, c, r).iter().map(|(t, _)| label_of(doc, *t)).collect();
    let name = rep_starter(doc, r).map_or("symbol", short_name);
    if labels.is_empty() { name.to_string() } else { format!("{name} {}", labels.join("/")) }
}

/// Most other symbols the lens's `Also` line lists before `+N more`.
pub const MAX_ALSO: usize = 4;

/// The lens of device `c` hovered through its representation `here` (design 018 §6.4).
pub fn hover_lines(doc: &Document, c: ComponentId, here: RepresentationId) -> Vec<String> {
    let props = doc.components.get(&c).map(|x| x.properties.clone()).unwrap_or_default();
    let get = |k: &str| props.get(k).cloned().unwrap_or_else(|| "?".into());
    let head = match props.get(TAG_KEY) {
        // Brief 026 (design §9): `(fixed)` or `(provisional)` after the tag.
        Some(t) if tag::is_provisional(t) => format!("Device {t} (provisional)   {}", description(doc, c)),
        Some(t) if ecad_model::numbering::is_fixed(&props) => format!("Device {t} (fixed)   {}", description(doc, c)),
        Some(t) => format!("Device {t}   {}", description(doc, c)),
        None => format!("Device (no tag)   {}", description(doc, c)),
    };
    let mut lines = vec![head, format!("Maker: {}   Part number: {}", get("maker"), get("part_number"))];
    // Brief 044 (design 044 §7, tester D3): a connector half says where its mate is.
    if let Some(side) = doc.representations.get(&here).and_then(|r| crate::connectors::rep_side(doc, r)) {
        let sides: &[crate::symbols::Side] = match side {
            crate::symbols::Side::Both => &[crate::symbols::Side::Socket, crate::symbols::Side::Plug],
            crate::symbols::Side::Socket => &[crate::symbols::Side::Socket],
            crate::symbols::Side::Plug => &[crate::symbols::Side::Plug],
        };
        lines.extend(sides.iter().filter_map(|s| crate::connectors::hover_words(doc, c, *s)));
    }
    // Brief 028 (design 028 §7.1): a terminal's sides and its strip's positions.
    if crate::strips::is_strip(doc, c) {
        lines.extend(crate::strips::terminal_hover(doc, c, here));
    }
    // Brief 024 (design 024 §3.3): the location shown, where it comes from, and this symbol's own
    // context when it differs.
    let mut boxes = crate::locations::BoxCache::default();
    let loc = crate::locations::resolve_with(doc, c, &mut boxes);
    let mut words = loc.shown(doc);
    if let Some(r) = doc.representations.get(&here) {
        let ctx = crate::locations::rep_context(doc, r, &mut boxes);
        if ctx.value.is_some() && ctx.value != loc.value {
            words.push_str(&format!("; this symbol is inside {}", ctx.text()));
        }
    }
    if loc.value.is_some() || props.contains_key("function") {
        lines.push(format!("Location: {words}   Function: {}", get("function")));
    }
    if let Some(r) = doc.representations.get(&here) {
        let drawable = rep_definition(doc, r).is_some_and(|(_, pl)| pl.is_valid());
        let cannot = if drawable { "" } else { " (cannot be drawn)" };
        lines.push(format!("This symbol: {}, {}{cannot}", rep_words(doc, c, r), crate::references::page_words(doc, r.page)));
        // Brief 031 (design §5.4): a tag hidden here is still data; the lens names it. (Descriptions
        // and part numbers are hidden by default, so naming them would add a line to every lens.)
        let hidden: Vec<String> = all_texts_on_page(doc, r.page)
            .into_iter()
            .filter(|t| t.text.rep == here && !t.shown && t.text.of == TextOf::Tag)
            .map(|t| format!("Tag {} (hidden here)", t.text.value))
            .collect();
        if !hidden.is_empty() {
            lines.push(hidden.join("; "));
        }
    }
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let mut others: Vec<(usize, RepresentationId, String)> = doc
        .representations_of(Subject::Component(c))
        .filter(|(id, _)| *id != here)
        .map(|(id, r)| {
            let n = order.get(&r.page).copied().unwrap_or(usize::MAX);
            (n, id, format!("{} {}", rep_words(doc, c, r), crate::references::page_words(doc, r.page)))
        })
        .collect();
    others.sort_by_key(|(n, id, _)| (*n, *id));
    if !others.is_empty() {
        let mut words: Vec<String> = others.iter().take(MAX_ALSO).map(|(_, _, w)| w.clone()).collect();
        if others.len() > MAX_ALSO {
            words.push(format!("+{} more", others.len() - MAX_ALSO));
        }
        lines.push(format!("Also: {}", words.join("; ")));
    }
    if let Some(r) = doc.representations.get(&here) {
        let ends: Vec<String> = shown_terminals(doc, c, r)
            .iter()
            .map(|(t, _)| {
                let label = label_of(doc, *t);
                match doc.ends_at(*t).first() {
                    Some((w, _)) => format!("{label} -> {}", crate::slash::wire_name(doc, *w)),
                    None => format!("{label} (not connected)"),
                }
            })
            .collect();
        if !ends.is_empty() {
            lines.push(format!("  {}", ends.join("        ")));
        }
    }
    lines.push(format!("id {}", c.hex()));
    lines
}

/// The tag the next new device of symbol `s` would get in project `p`, without using it up (for
/// prompts and pick lists).
pub fn next_tag_preview(p: &crate::project::Project, s: &Starter) -> Option<String> {
    let doc = p.ledger.document();
    p.tag_high.peek_auto(doc, &prefix_for(doc, s)).ok()
}
