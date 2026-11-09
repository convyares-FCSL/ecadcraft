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
    Component, ComponentId, Definition, DefinitionId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Op,
    PageId, Placement, Pt, Representation, RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalId,
};
use serde_json::{Value, json};

use crate::symbols::{self, Kind, STARTERS, Starter};

/// Reserved layer of tag texts and terminal labels (design 018 §2.4).
pub const TAG_LAYER: &str = "ECAD-TAGS";
/// `ecad.tag-height`, millimetres.
pub const TAG_HEIGHT: f64 = 3.5;
/// `ecad.term-label-height`, millimetres.
pub const LABEL_HEIGHT: f64 = 2.0;
/// `ecad.tag-gap`, millimetres.
pub const TAG_GAP: f64 = 2.0;
/// Tag numbers are padded to this many digits unless the project says otherwise (owner, 2026-11-08).
pub const DEFAULT_PADDING: u8 = ecad_model::DEFAULT_TAG_PADDING;
/// Most letters in a tag prefix a project may set.
pub const MAX_PREFIX_LETTERS: usize = 10;
/// Refusal of an edit of a derived tag text or terminal label.
pub const TAG_DERIVED: &str = "The tag text is derived; change the tag with Device Properties.";
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
    ((1..=MAX_PREFIX_LETTERS).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphabetic()))
        .then(|| t.to_ascii_uppercase())
}

/// The tag prefix a new device of symbol `s` gets in this project (letters, without `-`): the
/// project's own (`tag_prefixes`), else the built-in IEC 81346 default. A value a hostile file
/// stored that is not a valid prefix is ignored.
pub fn prefix_for(doc: &Document, s: &Starter) -> String {
    doc.settings.tag_prefixes.get(s.key).and_then(|p| valid_prefix(p)).unwrap_or_else(|| s.prefix.to_string())
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
}

impl TagHigh {
    /// Starting from `base`, the high-water mark at the ledger's base (empty for a ledger
    /// replayed from the root).
    pub fn starting_at(base: tag::HighWater) -> Self {
        Self { seen: 0, high: base.clone(), base }
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
        self.seen = log.len();
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
            let t = tag::format(&prefix, n, padding(doc));
            if doc.components_tagged(&t).is_empty() {
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
fn rep_definition<'a>(doc: &'a Document, r: &Representation) -> Option<(&'a Definition, Placement)> {
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
pub fn join_labels(
    doc: &Document,
    c: ComponentId,
    s: &Starter,
    extra: &BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let mut used = labels_of(doc, c);
    used.extend(extra.iter().cloned());
    let who = tag_words(doc, c);
    match s.kind {
        Kind::Contact { make } => {
            let highest = used
                .iter()
                .filter_map(|l| match l.as_bytes() {
                    [s @ b'1'..=b'9', b'1'..=b'4'] => Some(u32::from(s - b'0')),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let next = highest + 1;
            if next > 9 {
                return Err(format!("{who} already has 9 contacts."));
            }
            Ok(if make {
                vec![format!("{next}3"), format!("{next}4")]
            } else {
                vec![format!("{next}1"), format!("{next}2")]
            })
        }
        Kind::Terminal => {
            let highest =
                used.iter().filter_map(|l| l.parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0);
            Ok(vec![(highest + 1).to_string()])
        }
        Kind::Plc { letter } => {
            let highest = used
                .iter()
                .filter_map(|l| {
                    let rest = l.strip_prefix(letter)?;
                    let (a, b) = rest.split_once('.')?;
                    let (a, b) = (a.parse::<u32>().ok()?, b.parse::<u32>().ok()?);
                    (a < 10_000 && b < 8).then_some(a * 8 + b)
                })
                .max();
            let n = highest.map_or(0, |h| h + 1);
            Ok(vec![format!("{letter}{}.{}", n / 8, n % 8)])
        }
        Kind::Tagless => Err(format!("A {} is not part of a device; place it on its own.", s.name.to_lowercase())),
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
    ledger: &Ledger,
    ids: &mut IdAllocator,
    high: &mut TagHigh,
    page: PageId,
    style: Style,
    s: &Starter,
    placement: Placement,
    target: Target,
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
                Some(t) => t,
                None => high.next(ledger, &prefix_for(doc, s))?,
            };
            let c: ComponentId = ids.fresh();
            let mut properties = ecad_model::Properties::new();
            properties.insert(TAG_KEY.into(), t.clone());
            properties.insert("description".into(), s.name.into());
            ops.push(Op::CreateComponent { id: c, component: Component { properties, definition: Some(definition) } });
            for td in &def.terminals {
                let tid: TerminalId = ids.fresh();
                ops.push(Op::AddTerminal {
                    id: tid,
                    terminal: Terminal { component: c, label: td.label.clone(), key: Some(td.key.clone()) },
                });
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
            Ok(Placed {
                ops,
                label: "Place symbol".into(),
                component: c,
                representation,
                item,
                definition,
                tag: t,
                joined: false,
                terminals,
            })
        }
        Target::Join(c) => {
            if !doc.components.contains_key(&c) {
                return Err("No such device.".into());
            }
            let labels = join_labels(doc, c, s, &BTreeSet::new())?;
            let mut cps = BTreeMap::new();
            for (td, label) in def.terminals.iter().zip(&labels) {
                let tid: TerminalId = ids.fresh();
                ops.push(Op::AddTerminal {
                    id: tid,
                    terminal: Terminal { component: c, label: Some(label.clone()), key: Some(label.clone()) },
                });
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
            Ok(Placed {
                ops,
                label: format!("Add symbol to {t}"),
                component: c,
                representation,
                item,
                definition,
                tag: t,
                joined: true,
                terminals,
            })
        }
    }
}

// ---------------------------------------------------------------- merging

/// The terminals a representation shows, each with where it sits: in the definition's local frame
/// for a symbol, on the page for a drawn representation; in definition order for a symbol.
fn shown_terminals(doc: &Document, c: ComponentId, r: &Representation) -> Vec<(TerminalId, Pt)> {
    match r.form {
        Form::Drawn => r.connection_points.iter().map(|(t, p)| (*t, *p)).collect(),
        Form::Symbol => {
            let Some((def, _)) = rep_definition(doc, r) else { return Vec::new() };
            if r.connection_points.is_empty() {
                def.terminals
                    .iter()
                    .filter_map(|td| {
                        doc.terminals_of(c)
                            .find(|(_, t)| t.key.as_deref() == Some(td.key.as_str()))
                            .map(|(t, _)| (t, td.at))
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
pub fn plan_merge(
    doc: &Document,
    ids: &mut IdAllocator,
    from: ComponentId,
    into: ComponentId,
) -> Result<Vec<Op>, String> {
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
    let add = |old: TerminalId,
               label: String,
               ops: &mut Vec<Op>,
               ids: &mut IdAllocator,
               map: &mut BTreeMap<TerminalId, TerminalId>|
     -> TerminalId {
        let nt: TerminalId = ids.fresh();
        ops.push(Op::AddTerminal {
            id: nt,
            terminal: Terminal { component: into, label: Some(label.clone()), key: Some(label) },
        });
        map.insert(old, nt);
        nt
    };
    let reps: Vec<(RepresentationId, Representation)> =
        doc.representations_of(Subject::Component(from)).map(|(id, r)| (id, r.clone())).collect();
    let from_words = tag_words(doc, from);
    for (rid, r) in &reps {
        let shown = shown_terminals(doc, from, r);
        // A symbol whose device's connection points do not match its definition (an old or
        // hand-edited file) would move without any, which the model refuses (I14) with words
        // nobody can act on: say what is wrong instead.
        if r.form == Form::Symbol
            && shown.is_empty()
            && rep_definition(doc, r).is_some_and(|(def, _)| !def.terminals.is_empty())
        {
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
        let role = if r.role == "main" {
            starter.map_or_else(|| r.role.clone(), |s| s.key.to_string())
        } else {
            r.role.clone()
        };
        let nr: RepresentationId = ids.fresh();
        ops_reps.push((
            Op::UnlinkRepresentation { id: *rid, rep: r.clone() },
            Op::LinkRepresentation {
                id: nr,
                rep: Representation { subject: Subject::Component(into), role, connection_points: cps, ..r.clone() },
            },
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
    ops.push(Op::DeleteComponent { id: from, component: from_c });
    Ok(ops)
}

fn label_of(doc: &Document, t: TerminalId) -> String {
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
    for (c, reps) in by_component {
        let Some(component) = doc.components.get(&c) else { continue };
        let remaining: Vec<&Representation> = doc
            .representations_of(Subject::Component(c))
            .filter(|(id, _)| !gone.contains(id))
            .map(|(_, r)| r)
            .collect();
        let who = tag_words(doc, c);
        let drop: Vec<TerminalId> = if remaining.is_empty() {
            doc.terminals_of(c).map(|(t, _)| t).collect()
        } else {
            let kept: BTreeSet<TerminalId> =
                remaining.iter().flat_map(|r| shown_terminals(doc, c, r)).map(|(t, _)| t).collect();
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
            ops.push(Op::DeleteComponent { id: c, component: component.clone() });
            notes.push(format!("Deleted {who} with its last symbol; U brings it back."));
        } else {
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
    let n: Vec<String> = doc
        .page_order
        .iter()
        .enumerate()
        .filter(|(_, p)| pages.contains(p))
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    if n.len() == 1 { format!("page {}", n.join(", ")) } else { format!("pages {}", n.join(", ")) }
}

/// The tag a copy of device `c` gets (design 018 §3.3): the next automatic number of its prefix;
/// `None` when its tag is not of the usual form (the copy then has no tag).
pub fn copy_tag(doc: &Document, high: &mut TagHigh, c: &Component) -> Option<String> {
    let (prefix, _) = tag::parse(c.properties.get(TAG_KEY)?)?;
    high.next_in(doc, &prefix).ok()
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
fn placed_box(def: &Definition, pl: &Placement) -> Option<(Pt, Pt)> {
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
fn vertical(pl: &Placement) -> bool {
    let q = pl.rotation.rem_euclid(std::f64::consts::PI);
    q < std::f64::consts::FRAC_PI_4 || q > 3.0 * std::f64::consts::FRAC_PI_4
}

/// The texts of one symbol representation at `pl` (also used by the placing preview): its
/// device's tag and the labels `labels` at their page positions.
pub fn texts_for(
    def: &Definition,
    pl: &Placement,
    tag: Option<&str>,
    labels: &[(TextOf, String, Pt)],
) -> Vec<(TextOf, String, Pt, f64, HAlign, VAlign)> {
    let Some((lo, hi)) = placed_box(def, pl) else { return Vec::new() };
    let centre = Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    let vert = vertical(pl);
    let mut out = Vec::new();
    if let Some(t) = tag {
        let (at, h, v) = if vert {
            (Pt::new(lo.x - TAG_GAP, centre.y), HAlign::Right, VAlign::Middle)
        } else {
            (Pt::new(centre.x, hi.y + TAG_GAP), HAlign::Center, VAlign::Bottom)
        };
        out.push((TextOf::Tag, t.to_string(), at, TAG_HEIGHT, h, v));
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
        out.push((*of, label.clone(), p, LABEL_HEIGHT, h, v));
    }
    out
}

/// Every derived text on `page`: for each symbol representation of a component, its device's tag
/// (when it has one) and its terminals' labels. Hostile values (non-finite placements) draw
/// nothing for that representation.
pub fn texts_on_page(doc: &Document, page: PageId) -> Vec<DerivedText> {
    let mut by_rep: BTreeMap<RepresentationId, Vec<(TextOf, String, Pt)>> = BTreeMap::new();
    for (t, rid, at) in terminal_positions(doc, page) {
        if let Some(label) = doc.terminals.get(&t).and_then(|x| x.label.clone()) {
            by_rep.entry(rid).or_default().push((TextOf::Terminal(t), label, at));
        }
    }
    let mut out = Vec::new();
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
        // A continuation shows its name and its other places instead (brief 022).
        if let Some(texts) = crate::wires::continuation_texts(doc, c, rid, &pl) {
            out.extend(texts);
            continue;
        }
        for (of, value, at, height, halign, valign) in texts_for(def, &pl, tag_of(doc, c), &labels) {
            out.push(DerivedText { rep: rid, of, value, at, height, halign, valign, aci: None });
        }
    }
    out
}

/// A derived text as an entity on [`TAG_LAYER`].
pub fn text_entity(t: &DerivedText, handle: Handle) -> Entity {
    let mut e = text_entity_with(&t.value, t.at, t.height, t.halign, t.valign, handle, TAG_LAYER);
    if let Some(aci) = t.aci {
        e.common.color = cadcraft_engine::doc::color::Color::Index(aci);
    }
    e
}

/// A justified text entity.
pub fn text_entity_with(
    value: &str,
    at: Pt,
    height: f64,
    halign: HAlign,
    valign: VAlign,
    handle: Handle,
    layer: &str,
) -> Entity {
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
    v["representations"] = Value::Array(reps.into_iter().map(|(_, _, x)| x).collect());
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
        return tag::normalise(v, padding(doc)).map(Some);
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
        Some(t) => format!("Device {t}   {}", description(doc, c)),
        None => format!("Device (no tag)   {}", description(doc, c)),
    };
    let mut lines = vec![head, format!("Maker: {}   Part number: {}", get("maker"), get("part_number"))];
    if props.contains_key("location") || props.contains_key("function") {
        lines.push(format!("Location: {}   Function: {}", get("location"), get("function")));
    }
    if let Some(r) = doc.representations.get(&here) {
        let drawable = rep_definition(doc, r).is_some_and(|(_, pl)| pl.is_valid());
        let cannot = if drawable { "" } else { " (cannot be drawn)" };
        lines.push(format!(
            "This symbol: {}, {}{cannot}",
            rep_words(doc, c, r),
            crate::references::page_words(doc, r.page)
        ));
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
    p.tag_high.peek(doc, &prefix_for(doc, s)).ok()
}
