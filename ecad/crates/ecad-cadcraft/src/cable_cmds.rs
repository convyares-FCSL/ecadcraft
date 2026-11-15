//! Brief 059's commands (design 059 §4): the cable manager. `cablenew`, `cableedit` and
//! `cableremove` (write), and the assign rule `cableslash` and `cableadd` follow for a cable that is
//! not drawn yet.
//!
//! # Semantics
//! - A cable **not drawn** is a `Cable` with no members and no representation (FD1). It is built
//!   in Project ▸ Cables and assigned in the drawing later.
//! - Parameters are typed structs with `deny_unknown_fields`, parsed once; refusals are one enum,
//!   [`CableError`], whose `Display` gives the words of design 059 §9. Every refusal writes nothing.
//! - Every key this module reads or writes is named by [`CableField::key`] (design 059 §2.3): no
//!   other string literal names a cable key here.
//! - `cablenew` is one `CreateCable` (plus the project list entries a new planned code adds);
//!   `cableedit` is `SetProperty` ops in one transaction; `cableremove` is one `DeleteCable` and is
//!   refused for a drawn cable.
//!
//! This module excludes the UI (the Cables tab, the slash picker) and computed glands.
//!
//! Programmatic calls never prompt.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{
    CableId, CableMember, ConductorId, Document, Form, Item, ItemId, ItemKind, ObjectRef, Op, PageId, Primitive, Properties, Representation,
    RepresentationId, StyleRef, Subject,
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use crate::checks::{Check, Fix, Kind, Object};
use crate::commands::{err, with_project};
use crate::designations::{self, Aspect};
use crate::slash::MAX_CABLE_MEMBERS;
use crate::styles::{self, cable};

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("cablenew", "New Cable", run_cablenew)
            .params(
                "{tag?, cable_type?: id or name, type?, cores?, length?: m, shield?: \"none\"|\"overall\"|\"pairs\"|\"overall+pairs\", maker?, \
part_number?, description?, from?: {location?, function?, gland?}, to?: {…}} (a cable not drawn yet, the next free -W unless `tag`; one transaction)",
            )
            .noundo(),
        CommandSpec::new("cableedit", "Edit Cable", run_cableedit)
            .params(
                "{cable: id or tag, set: {tag?, cable_type?, type?, cores?, length?, shield?, maker?, part_number?, description?, \
from?: {location?, function?, gland?}, to?: {…}}} (null clears a field; one transaction; reducing cores below those in use is refused)",
            )
            .noundo(),
        CommandSpec::new("cableremove", "Remove Cable", run_cableremove)
            .params("{cable: id or tag} (deletes a cable that is not drawn; a drawn cable is refused with the pages it is drawn on)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------------------------------------
// Typed keys (design 059 §2.3)

/// A cable end: `From` is the cabinet end, `To` the field end (057's sides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    From,
    To,
}

impl Side {
    /// Both ends, cabinet first.
    pub const BOTH: [Side; 2] = [Side::From, Side::To];

    /// `from`, `to`: the JSON word and the stored key's part.
    pub fn word(self) -> &'static str {
        match self {
            Side::From => "from",
            Side::To => "to",
        }
    }

    /// `cabinet end`, `field end`, in words.
    pub fn words(self) -> &'static str {
        match self {
            Side::From => "cabinet end",
            Side::To => "field end",
        }
    }
}

/// A cable field the manager edits; [`CableField::key`] is its stored property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CableField {
    Tag,
    CableType,
    Type,
    Cores,
    Length,
    Gland(Side),
    Shield,
    Maker,
    PartNumber,
    Description,
    EndLocation(Side),
    EndFunction(Side),
}

impl CableField {
    /// Every field, in the Details pane's order.
    pub const ALL: [CableField; 15] = [
        CableField::Tag,
        CableField::CableType,
        CableField::Type,
        CableField::Cores,
        CableField::Shield,
        CableField::Maker,
        CableField::PartNumber,
        CableField::Description,
        CableField::Length,
        CableField::EndFunction(Side::From),
        CableField::EndLocation(Side::From),
        CableField::Gland(Side::From),
        CableField::EndFunction(Side::To),
        CableField::EndLocation(Side::To),
        CableField::Gland(Side::To),
    ];

    /// The stored property key.
    pub fn key(self) -> &'static str {
        match self {
            CableField::Tag => ecad_model::tag::TAG_KEY,
            CableField::CableType => cable::CABLE_TYPE,
            CableField::Type => "type",
            CableField::Cores => crate::strips::CORES_KEY,
            CableField::Length => "length",
            CableField::Gland(Side::From) => "gland.from",
            CableField::Gland(Side::To) => "gland.to",
            CableField::Shield => "shield",
            CableField::Maker => "maker",
            CableField::PartNumber => "part_number",
            CableField::Description => "description",
            CableField::EndLocation(Side::From) => "end.from.location",
            CableField::EndLocation(Side::To) => "end.to.location",
            CableField::EndFunction(Side::From) => "end.from.function",
            CableField::EndFunction(Side::To) => "end.to.function",
        }
    }

    /// The field a stored key names.
    pub fn parse(key: &str) -> Option<CableField> {
        CableField::ALL.into_iter().find(|f| f.key() == key)
    }

    /// The field's name in words (`Length`, `Gland, field end`).
    pub fn label(self) -> String {
        match self {
            CableField::Tag => "Tag".into(),
            CableField::CableType => "Cable type".into(),
            CableField::Type => "Type".into(),
            CableField::Cores => "Cores".into(),
            CableField::Length => "Length".into(),
            CableField::Shield => "Shield".into(),
            CableField::Maker => "Maker".into(),
            CableField::PartNumber => "Part number".into(),
            CableField::Description => "Description".into(),
            CableField::Gland(s) => format!("Gland, {}", s.words()),
            CableField::EndLocation(s) => format!("Planned location, {}", s.words()),
            CableField::EndFunction(s) => format!("Planned function, {}", s.words()),
        }
    }
}

/// A cable's shield (design 059 §2.1); the serde words are the stored words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Shield {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "overall")]
    Overall,
    #[serde(rename = "pairs")]
    Pairs,
    #[serde(rename = "overall+pairs")]
    Both,
}

impl Shield {
    pub const ALL: [Shield; 4] = [Shield::None, Shield::Overall, Shield::Pairs, Shield::Both];

    /// The stored word.
    pub fn word(self) -> &'static str {
        match self {
            Shield::None => "none",
            Shield::Overall => "overall",
            Shield::Pairs => "pairs",
            Shield::Both => "overall+pairs",
        }
    }

    /// The word a stored value names (case-insensitive).
    pub fn parse(text: &str) -> Option<Shield> {
        Shield::ALL.into_iter().find(|s| s.word().eq_ignore_ascii_case(text.trim()))
    }
}

/// Where a shown value comes from (design 059 §2.2): the hint `set here`, `from type`,
/// `from notation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Here,
    Type,
    Notation,
}

impl Source {
    pub fn hint(self) -> &'static str {
        match self {
            Source::Here => "set here",
            Source::Type => "from type",
            Source::Notation => "from notation",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Refusals (design 059 §4, §9)

/// Why a value is refused.
#[derive(Debug, Clone, PartialEq)]
pub enum BadReason {
    Empty,
    TooLong(usize),
    NotANumber,
    OutOfRange { min: f64, max: f64 },
    UnknownEntry,
    BadOffer(String),
    NotAWord,
}

/// A refusal of the cable manager's commands. Nothing is written.
#[derive(Debug, Clone, PartialEq)]
pub enum CableError {
    NoProject,
    ReadOnly,
    NoSuchCable(String),
    TagTaken {
        tag: String,
        by: String,
    },
    BadValue {
        field: CableField,
        value: String,
        reason: BadReason,
    },
    Drawn {
        tag: String,
        pages: Vec<(PageId, String)>,
    },
    CoresInUse {
        tag: String,
        offer: usize,
        offer_text: String,
        in_use: Vec<String>,
    },
    MoreWiresThanCores {
        tag: String,
        free: Vec<String>,
        wires: usize,
    },
    AlreadyInCable(String),
    SlashedHere {
        tag: String,
    },
    TooMany,
    /// A parameter shape serde refused (an unknown key, a wrong type).
    Params(String),
}

/// `a, b and c`.
fn and_list(v: &[String]) -> String {
    match v {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl fmt::Display for CableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CableError::NoProject => write!(f, "No ECAD project is open."),
            CableError::ReadOnly => write!(f, "This project is read-only."),
            CableError::NoSuchCable(t) => write!(f, "No cable {t:?}."),
            CableError::TagTaken { tag, by } => write!(f, "{tag} is already the tag of {by}. Choose another tag."),
            CableError::BadValue { field, value, reason } => {
                let shown: String = value.chars().take(32).collect();
                match (field, reason) {
                    (CableField::Length, _) => write!(f, "Length must be a number of metres from {MIN_LENGTH} to {MAX_LENGTH}."),
                    (CableField::Cores | CableField::Type, BadReason::BadOffer(why)) => write!(f, "{}: {why}", field.label()),
                    (CableField::CableType, BadReason::UnknownEntry) => write!(f, "No cable type {shown:?} in this project's standards."),
                    (CableField::Shield, _) => write!(f, "Shield is none, overall, pairs or overall+pairs."),
                    (_, BadReason::TooLong(n)) => write!(f, "{} is at most {n} characters.", field.label()),
                    (_, BadReason::Empty) => write!(f, "{} needs a value, or null to clear it.", field.label()),
                    (_, BadReason::UnknownEntry) => write!(f, "{}: no entry {shown:?}.", field.label()),
                    (_, _) => write!(f, "{}: {shown:?} is not a value ECAD reads.", field.label()),
                }
            }
            CableError::Drawn { tag, pages } => {
                let names: Vec<String> = pages.iter().map(|(_, n)| n.clone()).collect();
                write!(f, "{tag} is drawn on {}. Delete it on its page.", and_list(&names))
            }
            CableError::CoresInUse { tag, offer, offer_text, in_use } => write!(
                f,
                "{tag} has wires on cores {}; {offer_text} has {offer} core{}. Remove wires on their page first, or choose a type with at least {} cores.",
                and_list(in_use),
                if *offer == 1 { "" } else { "s" },
                in_use.len()
            ),
            CableError::MoreWiresThanCores { tag, free, wires } => write!(
                f,
                "{tag} has {} free core{}{}; the slash crosses {wires} wires. Slash fewer wires, or add cores first.",
                free.len(),
                if free.len() == 1 { "" } else { "s" },
                if free.is_empty() { String::new() } else { format!(" ({})", free.join(", ")) }
            ),
            CableError::AlreadyInCable(words) => write!(f, "{words}"),
            CableError::SlashedHere { tag } => write!(f, "{tag} already has a slash on this page."),
            CableError::TooMany => write!(f, "{}", crate::slash::too_many()),
            CableError::Params(m) => write!(f, "{m}"),
        }
    }
}

impl From<CableError> for String {
    fn from(e: CableError) -> String {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Bounds (design 059 §2.1)

/// Shortest and longest cable length stored, in metres.
pub const MIN_LENGTH: f64 = 0.1;
pub const MAX_LENGTH: f64 = 10_000.0;
/// Longest free text stored (`type`, maker, part number, description).
pub const MAX_TEXT: usize = 64;

/// A length read from text: metres, within bounds.
pub fn length_of(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_suffix('m').map_or(t, str::trim_end);
    t.parse::<f64>().ok().filter(|v| v.is_finite() && (MIN_LENGTH..=MAX_LENGTH).contains(v))
}

fn text_ok(t: &str, max: usize) -> bool {
    !t.trim().is_empty() && t.chars().count() <= max && !t.chars().any(char::is_control)
}

/// A stored value of cable `k` that ECAD reads: its property for `field`, when within bounds.
/// Entry keys (planned ends) and the cable type are given as stored; tags as stored.
pub fn read(doc: &Document, k: CableId, field: CableField) -> Option<String> {
    let v = doc.cables.get(&k)?.properties.get(field.key())?;
    valid(doc, field, v).then(|| v.trim().to_string())
}

/// Whether a stored value of `field` is within bounds.
fn valid(doc: &Document, field: CableField, v: &str) -> bool {
    match field {
        CableField::Length => length_of(v).is_some(),
        CableField::Gland(_) => cable::is_gland(v),
        CableField::Shield => Shield::parse(v).is_some(),
        CableField::Maker | CableField::PartNumber | CableField::Description | CableField::Type => text_ok(v, MAX_TEXT),
        CableField::Cores => crate::strips::parse_cores(v).is_ok(),
        CableField::EndLocation(_) => designations::code_of(doc, Aspect::Location, v.trim()).is_some(),
        CableField::EndFunction(_) => designations::code_of(doc, Aspect::Function, v.trim()).is_some(),
        CableField::CableType => StyleRef::parse(v).is_some_and(|id| styles::exists(doc, cable::WORD, &id)),
        CableField::Tag => !v.trim().is_empty(),
    }
}

/// The fields of §2.1 a file holds out of bounds (a file made elsewhere): `cable-bad-value`.
pub fn bad_values(doc: &Document, k: CableId) -> Vec<(CableField, String)> {
    let Some(c) = doc.cables.get(&k) else { return Vec::new() };
    let checked = [
        CableField::Length,
        CableField::Gland(Side::From),
        CableField::Gland(Side::To),
        CableField::Shield,
        CableField::Maker,
        CableField::PartNumber,
        CableField::Description,
        CableField::EndLocation(Side::From),
        CableField::EndLocation(Side::To),
        CableField::EndFunction(Side::From),
        CableField::EndFunction(Side::To),
    ];
    checked
        .into_iter()
        .filter_map(|f| {
            let v = c.properties.get(f.key())?;
            (!valid(doc, f, v)).then(|| (f, v.chars().take(24).filter(|c| !c.is_control()).collect()))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Reads

/// Whether cable `k` is drawn: it has a member or a representation (FD1).
pub fn is_drawn(doc: &Document, k: CableId) -> bool {
    doc.cables.get(&k).is_some_and(|c| !c.members.is_empty()) || doc.representations_of(Subject::Cable(k)).next().is_some()
}

/// The cables not drawn yet, in tag order. O(cables): the pickers build it once when opened.
pub fn not_drawn(doc: &Document) -> Vec<CableId> {
    let mut v: Vec<(String, CableId)> = doc.cables.keys().filter(|k| !is_drawn(doc, **k)).map(|k| (crate::views::cable_title(doc, *k), *k)).collect();
    v.sort_by(|a, b| crate::locations::natural_cmp(&a.0, &b.0));
    v.into_iter().map(|(_, k)| k).collect()
}

/// The cables whose planned ends store entry `key` (a rename shows the new code there).
/// O(cables), on a rename of the project's list only.
pub fn planning(doc: &Document, key: &str) -> Vec<CableId> {
    let fields = [Side::From, Side::To].into_iter().flat_map(|s| [CableField::EndLocation(s), CableField::EndFunction(s)]);
    let fields: Vec<&str> = fields.map(CableField::key).collect();
    doc.cables.iter().filter(|(_, c)| fields.iter().any(|f| c.properties.get(*f).is_some_and(|v| v.trim() == key))).map(|(k, _)| *k).collect()
}

/// Cable `k`'s planned code of `aspect` at `side` (`+CAB1`), when it names an entry.
pub fn planned(doc: &Document, k: CableId, side: Side, aspect: Aspect) -> Option<String> {
    let field = match aspect {
        Aspect::Location => CableField::EndLocation(side),
        Aspect::Function => CableField::EndFunction(side),
    };
    let key = doc.cables.get(&k)?.properties.get(field.key())?;
    designations::code_of(doc, aspect, key.trim())
}

/// Cable `k`'s plan at `side` in words (`=PUMP +CAB1`), when it has one.
pub fn plan_words(doc: &Document, k: CableId, side: Side) -> Option<String> {
    let parts: Vec<String> = [Aspect::Function, Aspect::Location].into_iter().filter_map(|a| planned(doc, k, side, a)).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Cable `k`'s cable type, when it names one that exists.
fn type_of(doc: &Document, k: CableId) -> Option<StyleRef> {
    cable::of(doc, k)
}

/// A text value of cable `k`: its own, else its cable type's field `type_key`.
fn own_or_type(doc: &Document, k: CableId, field: CableField, type_key: &str) -> Option<(String, Source)> {
    if let Some(v) = read(doc, k, field) {
        return Some((v, Source::Here));
    }
    let id = type_of(doc, k)?;
    cable::type_text(doc, &id, type_key).map(|v| (v, Source::Type))
}

/// Cable `k`'s gland at `side`: its own, else its cable type's (Q5, option B).
pub fn gland(doc: &Document, k: CableId, side: Side) -> Option<(String, Source)> {
    own_or_type(doc, k, CableField::Gland(side), cable::TYPE_GLAND).filter(|(g, _)| cable::is_gland(g))
}

/// Cable `k`'s shield: its own, else its cable type's.
pub fn shield(doc: &Document, k: CableId) -> Option<(Shield, Source)> {
    own_or_type(doc, k, CableField::Shield, cable::TYPE_SHIELD).and_then(|(w, s)| Shield::parse(&w).map(|x| (x, s)))
}

/// Cable `k`'s maker, part number or description: its own, else its cable type's (Q8).
pub fn part_value(doc: &Document, k: CableId, field: CableField) -> Option<(String, Source)> {
    let type_key = match field {
        CableField::Maker => "maker",
        CableField::PartNumber => "part_number",
        CableField::Description => "description",
        _ => return read(doc, k, field).map(|v| (v, Source::Here)),
    };
    own_or_type(doc, k, field, type_key)
}

/// Cable `k`'s core offer as written (`4G`, `BN,BK,GY`) and where it comes from.
pub fn cores_shown(doc: &Document, k: CableId) -> Option<(String, Source)> {
    let (_, src, text) = cable::offer(doc, k)?;
    let props = &doc.cables.get(&k)?.properties;
    let own = props.get(CableField::Cores.key()).is_some_and(|v| crate::strips::parse_cores(v).is_ok());
    Some((
        text,
        match src {
            crate::strips::OfferSource::Cores if own => Source::Here,
            crate::strips::OfferSource::Cores => Source::Type,
            crate::strips::OfferSource::Type => Source::Notation,
        },
    ))
}

/// The pages cable `k` is drawn on, with the place of its slash (`2/D1`), in book order: its
/// representations' pages, else its members' route pages.
pub fn drawn_pages(doc: &Document, k: CableId) -> Vec<(PageId, String)> {
    let style = doc.settings.reference_style;
    let mut out: BTreeMap<PageId, String> = BTreeMap::new();
    for (_, r) in doc.representations_of(Subject::Cable(k)) {
        let at = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
            ItemKind::Primitive(Primitive::Line { a, .. }) => Some(*a),
            ItemKind::Primitive(Primitive::Point { at }) => Some(*at),
            ItemKind::Primitive(Primitive::Polyline { vertices, .. }) => vertices.first().copied(),
            _ => None,
        });
        let words =
            at.and_then(|a| ecad_model::wiring::place(doc, r.page, a)).map_or_else(|| crate::references::page_number(doc, r.page), |p| p.text(style));
        out.entry(r.page).or_insert(words);
    }
    if out.is_empty()
        && let Some(c) = doc.cables.get(&k)
    {
        for m in c.members.iter().take(MAX_CABLE_MEMBERS) {
            for (_, r) in doc.representations_of(Subject::Conductor(m.conductor)) {
                out.entry(r.page).or_insert_with(|| crate::references::page_number(doc, r.page));
            }
        }
    }
    let mut v: Vec<(PageId, String)> = out.into_iter().collect();
    v.sort_by_key(|(p, _)| doc.page_order.iter().position(|x| x == p).unwrap_or(usize::MAX));
    v
}

/// The locations (codes) of the devices at cable `k`'s cores' ends, and whether a device there has
/// none. Strips and wire-only parts (terminals) count; `wires::kind_of` items do not.
fn drawn_locations(doc: &Document, k: CableId) -> (BTreeSet<String>, bool) {
    let mut out = BTreeSet::new();
    let mut missing = false;
    let Some(cab) = doc.cables.get(&k) else { return (out, missing) };
    for m in cab.members.iter().take(MAX_CABLE_MEMBERS) {
        let Some(cond) = doc.conductors.get(&m.conductor) else { continue };
        for t in cond.ends.iter().flatten() {
            let Some(c) = doc.terminals.get(t).map(|x| x.component) else { continue };
            if crate::wires::kind_of(doc, c).is_some() {
                continue;
            }
            match crate::locations::resolve(doc, c).value {
                Some(l) => {
                    out.insert(l);
                }
                None => missing = true,
            }
        }
    }
    (out, missing)
}

/// The ends of drawn cable `k` whose plan names a location none of its drawn ends has, with the
/// drawn locations not planned at the other end (design 059 §6, Q4): `cable-end-off-plan`. The two
/// ends differing from each other is normal and never raises. An end with no plan raises nothing.
pub fn off_plan(doc: &Document, k: CableId) -> Vec<(Side, String, String)> {
    if doc.cables.get(&k).is_none_or(|c| c.members.is_empty()) {
        return Vec::new();
    }
    let (drawn, _) = drawn_locations(doc, k);
    if drawn.is_empty() {
        return Vec::new();
    }
    let plans: Vec<(Side, Option<String>)> = Side::BOTH.into_iter().map(|s| (s, planned(doc, k, s, Aspect::Location))).collect();
    let planned_set: BTreeSet<String> = plans.iter().filter_map(|(_, p)| p.clone()).collect();
    let has = |code: &str| drawn.iter().any(|d| d.eq_ignore_ascii_case(code));
    let unplanned: Vec<String> = drawn.iter().filter(|d| !planned_set.iter().any(|p| p.eq_ignore_ascii_case(d))).cloned().collect();
    let mut out = Vec::new();
    for (side, plan) in plans {
        let Some(code) = plan else { continue };
        if has(&code) {
            continue;
        }
        let at = if unplanned.is_empty() { drawn.iter().cloned().collect::<Vec<_>>().join(", ") } else { unplanned.join(", ") };
        out.push((side, code, at));
    }
    out
}

/// The value rows of cable `k` (design 059 §6): `cable-bad-value` always; `cable-end-off-plan` and
/// `no-gland` (059d) once drawn.
pub fn value_checks(doc: &Document, k: CableId, drawn: bool) -> Vec<Check> {
    let mut out = Vec::new();
    if !doc.cables.contains_key(&k) {
        return out;
    }
    let name = crate::views::cable_title(doc, k);
    for (field, v) in bad_values(doc, k) {
        let what = match field {
            CableField::Length => format!("{name}'s length {v:?} is not a number of metres; it is ignored."),
            _ => format!("{name}'s {} {v:?} is not a value ECAD reads; it is ignored.", field.label().to_lowercase()),
        };
        out.push(Check {
            kind: Kind::CableBadValue,
            page: None,
            at: None,
            object: Object::Cable(k),
            which: what,
            fix: Some(Fix::ShowCableField(k, field)),
        });
    }
    if !drawn {
        return out;
    }
    let anchor = ecad_model::numbering::rep_anchor(doc, Subject::Cable(k));
    let (page, at) = anchor.map_or((None, None), |(p, a)| (Some(p), Some(a)));
    for (side, _, drawn_at) in off_plan(doc, k) {
        let plan = plan_words(doc, k, side).unwrap_or_default();
        out.push(Check {
            kind: Kind::CableEndOffPlan,
            page,
            at,
            object: Object::Cable(k),
            which: format!("{name}'s {} is planned {plan} but drawn at {drawn_at}.", side.words()),
            fix: Some(Fix::UseDrawing(k, side)),
        });
    }
    // 059d (design 059 §6): a drawn end with no gland size, its own or its cable type's.
    for side in Side::BOTH {
        if gland(doc, k, side).is_none() {
            out.push(Check {
                kind: Kind::NoGland,
                page,
                at,
                object: Object::Cable(k),
                which: format!("{name} has no gland size at the {}.", side.words()),
                fix: Some(Fix::ShowCableField(k, CableField::Gland(side))),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Parameters (design 059 §4.1, §4.2)

/// A field that may be `null`: absent is `None`, present (even `null`) is `Some`.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// An end's plan and gland.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndPlan {
    #[serde(default, deserialize_with = "present")]
    pub location: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub function: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub gland: Option<Value>,
}

/// The fields `cablenew` takes and `cableedit`'s `set` changes; `null` clears in `cableedit`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CableFields {
    #[serde(default, deserialize_with = "present")]
    pub tag: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub cable_type: Option<Value>,
    #[serde(default, deserialize_with = "present", rename = "type")]
    pub kind: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub cores: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub length: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub shield: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub maker: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub part_number: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    pub description: Option<Value>,
    #[serde(default)]
    pub from: Option<EndPlan>,
    #[serde(default)]
    pub to: Option<EndPlan>,
}

/// `cableedit`'s parameters.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditCable {
    pub cable: String,
    pub set: CableFields,
}

/// `cableremove`'s parameters.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveCable {
    pub cable: String,
}

fn parse<T: for<'de> Deserialize<'de>>(params: &Value) -> Result<T, CableError> {
    let p = if params.is_null() { json!({}) } else { params.clone() };
    serde_json::from_value(p).map_err(|e| CableError::Params(format!("{e}. Nothing was changed.")))
}

impl CableFields {
    /// Every given field with its JSON value, in [`CableField::ALL`] order.
    fn entries(&self) -> Vec<(CableField, &Value)> {
        let mut v: Vec<(CableField, &Value)> = Vec::new();
        fn push<'a>(v: &mut Vec<(CableField, &'a Value)>, f: CableField, x: &'a Option<Value>) {
            if let Some(x) = x {
                v.push((f, x));
            }
        }
        push(&mut v, CableField::Tag, &self.tag);
        push(&mut v, CableField::CableType, &self.cable_type);
        push(&mut v, CableField::Type, &self.kind);
        push(&mut v, CableField::Cores, &self.cores);
        push(&mut v, CableField::Shield, &self.shield);
        push(&mut v, CableField::Maker, &self.maker);
        push(&mut v, CableField::PartNumber, &self.part_number);
        push(&mut v, CableField::Description, &self.description);
        push(&mut v, CableField::Length, &self.length);
        for (side, end) in [(Side::From, &self.from), (Side::To, &self.to)] {
            if let Some(e) = end {
                push(&mut v, CableField::EndFunction(side), &e.function);
                push(&mut v, CableField::EndLocation(side), &e.location);
                push(&mut v, CableField::Gland(side), &e.gland);
            }
        }
        v
    }
}

/// A JSON value as text (`12` → `"12"`); `None` for null or an empty string.
fn as_text(field: CableField, v: &Value) -> Result<Option<String>, CableError> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s.trim().is_empty() => Ok(None),
        Value::String(s) => Ok(Some(s.trim().to_string())),
        Value::Number(n) => Ok(Some(n.to_string())),
        other => Err(CableError::BadValue { field, value: other.to_string(), reason: BadReason::NotAWord }),
    }
}

/// Check one value and give the text stored (`None` clears). A planned code not in the project's
/// list adds its entry (052's rule) through `pending`.
fn checked(
    doc: &Document, ids: &mut ecad_model::IdAllocator, pending: &mut designations::Pending, field: CableField, v: &Value, ops: &mut Vec<Op>,
) -> Result<Option<String>, CableError> {
    let Some(t) = as_text(field, v)? else { return Ok(None) };
    let bad = |reason: BadReason| CableError::BadValue { field, value: t.clone(), reason };
    match field {
        CableField::Tag => Ok(Some(t)),
        CableField::Length => match length_of(&t) {
            Some(_) => Ok(Some(t)),
            None if t.trim().trim_end_matches('m').trim().parse::<f64>().is_ok() => {
                Err(bad(BadReason::OutOfRange { min: MIN_LENGTH, max: MAX_LENGTH }))
            }
            None => Err(bad(BadReason::NotANumber)),
        },
        CableField::Gland(_) if t.chars().count() > cable::MAX_GLAND_CHARS => Err(bad(BadReason::TooLong(cable::MAX_GLAND_CHARS))),
        CableField::Gland(_) if !cable::is_gland(&t) => Err(bad(BadReason::NotAWord)),
        CableField::Gland(_) => Ok(Some(t)),
        CableField::Shield => Shield::parse(&t).map(|s| Some(s.word().to_string())).ok_or_else(|| bad(BadReason::NotAWord)),
        CableField::Maker | CableField::PartNumber | CableField::Description | CableField::Type => {
            if t.chars().count() > MAX_TEXT {
                return Err(bad(BadReason::TooLong(MAX_TEXT)));
            }
            if t.chars().any(char::is_control) {
                return Err(bad(BadReason::NotAWord));
            }
            Ok(Some(t))
        }
        CableField::Cores => crate::strips::cores_value(Some(&t)).map_err(|e| bad(BadReason::BadOffer(e))),
        CableField::CableType => styles::find(doc, cable::WORD, &t).map(|id| Some(id.text())).map_err(|_| bad(BadReason::UnknownEntry)),
        CableField::EndLocation(_) => pending.store(doc, ids, Aspect::Location, &t, ops).map(Some).map_err(|_| bad(BadReason::UnknownEntry)),
        CableField::EndFunction(_) => pending.store(doc, ids, Aspect::Function, &t, ops).map(Some).map_err(|_| bad(BadReason::UnknownEntry)),
    }
}

/// The cable `text` names: an ID or a tag.
pub fn cable_named(doc: &Document, text: &str) -> Result<CableId, CableError> {
    let t = text.trim();
    if let Some(k) = CableId::parse(t).filter(|k| doc.cables.contains_key(k)) {
        return Ok(k);
    }
    crate::references::cable_tagged(doc, t).ok_or_else(|| CableError::NoSuchCable(t.chars().take(32).collect()))
}

/// The notation a cable type gives `type` (`H07RN-F 4G1.5`): its name when it carries a core
/// notation, else its cores and cross-section (`4G1.5`).
pub fn type_notation(doc: &Document, id: &StyleRef) -> Option<String> {
    let name = styles::name(doc, cable::WORD, id);
    if crate::strips::offer_from_type(&name).is_some() {
        return Some(name);
    }
    let cores = cable::type_text(doc, id, cable::TYPE_CORES)?;
    let size = cable::type_text(doc, id, "cross_section").unwrap_or_default();
    Some(format!("{cores}{size}"))
}

/// The cores cable `k`'s members use, by label (their matched or stored labels, else their place).
fn cores_in_use(doc: &Document, k: CableId) -> Vec<String> {
    let Some(cab) = doc.cables.get(&k) else { return Vec::new() };
    let count = crate::strips::count_cores(cable::offer(doc, k), &cab.members);
    cab.members
        .iter()
        .enumerate()
        .map(|(i, m)| count.matched.get(i).cloned().flatten().or_else(|| m.core.clone()).unwrap_or_else(|| (i + 1).to_string()))
        .collect()
}

/// Q6: a new offer (from `props`) must hold every core cable `k`'s members use.
fn check_offer(doc: &Document, k: CableId, props: &Properties) -> Result<(), CableError> {
    let Some(cab) = doc.cables.get(&k) else { return Ok(()) };
    if cab.members.is_empty() {
        return Ok(());
    }
    let Some((offer, _, text)) = cable::offer_of_props(doc, props) else { return Ok(()) };
    let count = crate::strips::count_cores(Some((offer.clone(), crate::strips::OfferSource::Cores, text.clone())), &cab.members);
    let lacks_label = cab.members.iter().any(|m| m.core.as_deref().is_some_and(|l| !offer.iter().any(|o| o.eq_ignore_ascii_case(l.trim()))));
    if count.over.is_empty() && !lacks_label {
        return Ok(());
    }
    Err(CableError::CoresInUse { tag: crate::views::cable_title(doc, k), offer: offer.len(), offer_text: text, in_use: cores_in_use(doc, k) })
}

// ---------------------------------------------------------------------------------------------
// cablenew

fn run_cablenew(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let fields: CableFields = parse(params).map_err(|e| err(e.to_string()))?;
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        let mut pending = designations::Pending::default();
        let mut props = Properties::new();
        let typed_tag = match fields.tag.as_ref() {
            Some(v) => as_text(CableField::Tag, v)?,
            None => None,
        };
        for (field, v) in fields.entries() {
            if field == CableField::Tag {
                continue;
            }
            if let Some(t) = checked(doc, &mut p.ids, &mut pending, field, v, &mut ops)? {
                props.insert(field.key().to_string(), t);
            }
        }
        // A cable type fills `type` when none was given (design 059 §3).
        if !props.contains_key(CableField::Type.key())
            && let Some(id) = props.get(CableField::CableType.key()).and_then(|v| StyleRef::parse(v))
            && let Some(n) = type_notation(doc, &id)
        {
            props.insert(CableField::Type.key().to_string(), n);
        }
        if let Some(t) = typed_tag.as_deref() {
            refuse_taken(doc, t, None)?;
        }
        let tag = crate::devices::new_cable_tag(&p.ledger, &mut p.tag_high, typed_tag.as_deref())?;
        props.insert(CableField::Tag.key().to_string(), tag.clone());
        let id: CableId = p.ids.fresh();
        ops.push(Op::CreateCable { id, cable: ecad_model::Cable { properties: props, members: Vec::new() } });
        p.apply_edit(pg, "cablenew", "Create cable", ops)?;
        let mut message = format!("Created {tag} (not drawn). Fill it in on the right, then assign it in the drawing.");
        if let Some(w) = pending.words() {
            message = format!("{message} {w}");
        }
        Ok(json!({"cable": id.hex(), "tag": tag, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// [`CableError::TagTaken`] when another device or cable holds `typed`.
fn refuse_taken(doc: &Document, typed: &str, except: Option<CableId>) -> Result<(), CableError> {
    let Ok(t) = ecad_model::tag::normalise(typed, crate::defaults::print::tag_style(doc)) else { return Ok(()) };
    if ecad_model::tag::provisional(&t).is_some() {
        return Ok(());
    }
    match crate::devices::tag_holder(doc, &t, except.map(ObjectRef::Cable)) {
        Some(h) => {
            let what = match h {
                crate::devices::Holder::Device(c) => format!("{} (device)", crate::devices::tag_of(doc, c).unwrap_or("?")),
                crate::devices::Holder::Cable(k) => format!("{} (cable)", crate::views::cable_title(doc, k)),
            };
            Err(CableError::TagTaken { tag: t, by: what })
        }
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------
// cableedit

/// The ops of `cableedit` on `doc`: every value checked first; any refusal writes nothing.
///
/// # Errors
/// [`CableError`]: a bad value, a taken tag, cores in use.
pub fn edit_ops(
    doc: &Document, ids: &mut ecad_model::IdAllocator, tag_high: &mut crate::devices::TagHigh, k: CableId, set: &CableFields,
) -> Result<(Vec<Op>, Vec<CableField>, Option<String>), CableError> {
    let cab = doc.cables.get(&k).ok_or_else(|| CableError::NoSuchCable(k.hex()))?;
    let mut ops = Vec::new();
    let mut pending = designations::Pending::default();
    let mut after = cab.properties.clone();
    let mut changed = Vec::new();
    for (field, v) in set.entries() {
        let value = if field == CableField::Tag {
            match as_text(field, v)? {
                None => None,
                Some(t) => {
                    let n = ecad_model::tag::normalise(&t, crate::defaults::print::tag_style(doc)).map_err(|e| CableError::BadValue {
                        field,
                        value: t.clone(),
                        reason: BadReason::BadOffer(e),
                    })?;
                    refuse_taken(doc, &n, Some(k))?;
                    match ecad_model::tag::provisional(&n) {
                        Some((px, 0)) => Some(tag_high.next_provisional_in(doc, &px)),
                        _ => Some(n),
                    }
                }
            }
        } else {
            checked(doc, ids, &mut pending, field, v, &mut ops)?
        };
        match &value {
            Some(x) => after.insert(field.key().to_string(), x.clone()),
            None => after.remove(field.key()),
        };
        changed.push(field);
    }
    // Design 059 §3: choosing a cable type sets `type` from its notation when `type` was empty or
    // came from the old type (and `type` was not given here).
    if set.cable_type.is_some() && set.kind.is_none() {
        let old_type = cab.properties.get(CableField::CableType.key()).and_then(|v| StyleRef::parse(v));
        let old_notation = old_type.as_ref().and_then(|id| type_notation(doc, id));
        let current = cab.properties.get(CableField::Type.key()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        if current.is_none() || current == old_notation {
            let new = after.get(CableField::CableType.key()).and_then(|v| StyleRef::parse(v)).and_then(|id| type_notation(doc, &id));
            match new {
                Some(n) => after.insert(CableField::Type.key().to_string(), n),
                None if current.is_some() => after.remove(CableField::Type.key()),
                None => None,
            };
        }
    }
    // Q6: never fewer cores than those in use.
    check_offer(doc, k, &after)?;
    let object = ObjectRef::Cable(k);
    let keys: BTreeSet<&String> = cab.properties.keys().chain(after.keys()).collect();
    for key in keys {
        let (b, a) = (cab.properties.get(key), after.get(key));
        if b != a {
            ops.push(Op::SetProperty { object, key: key.clone(), before: b.cloned(), after: a.cloned() });
        }
    }
    // A typed tag is fixed (026), as Set property on `tag` makes it.
    if set.tag.is_some() {
        let fixed = after.get(CableField::Tag.key()).is_some_and(|t| !ecad_model::tag::is_provisional(t));
        let before = cab.properties.get(ecad_model::tag::FIXED_KEY).cloned();
        let a = fixed.then(|| "yes".to_string());
        if before != a {
            ops.push(Op::SetProperty { object, key: ecad_model::tag::FIXED_KEY.into(), before, after: a });
        }
    }
    Ok((ops, changed, pending.words()))
}

fn run_cableedit(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let edit: EditCable = parse(params).map_err(|e| err(e.to_string()))?;
    let v = with_project(s, |p, pg, _| {
        p.tag_high.catch_up(&p.ledger);
        let doc = p.ledger.document();
        let k = cable_named(doc, &edit.cable)?;
        let (ops, changed, words) = edit_ops(doc, &mut p.ids, &mut p.tag_high, k, &edit.set)?;
        let names: Vec<&str> = changed.iter().map(|f| f.key()).collect();
        if ops.is_empty() {
            return Ok(json!({"cable": k.hex(), "changed": names, "written": false}));
        }
        p.apply_edit(pg, "cableedit", "Edit cable", ops)?;
        Ok(json!({"cable": k.hex(), "changed": names, "written": true, "message": words}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------------------------------------
// cableremove

/// Whether cable `k` may be removed from the list (Q1): only when it is not drawn.
///
/// # Errors
/// [`CableError::Drawn`] with the pages it is drawn on (the UI's Go to links).
pub fn check_remove(doc: &Document, k: CableId) -> Result<(), CableError> {
    if !doc.cables.contains_key(&k) {
        return Err(CableError::NoSuchCable(k.hex()));
    }
    if is_drawn(doc, k) {
        return Err(CableError::Drawn { tag: crate::views::cable_title(doc, k), pages: drawn_pages(doc, k) });
    }
    Ok(())
}

fn run_cableremove(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let r: RemoveCable = parse(params).map_err(|e| err(e.to_string()))?;
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let k = cable_named(doc, &r.cable)?;
        check_remove(doc, k)?;
        let cable = doc.cables.get(&k).cloned().ok_or("no such cable")?;
        let tag = crate::views::cable_title(doc, k);
        p.apply_edit(pg, "cableremove", "Remove cable", vec![Op::DeleteCable { id: k, cable }])?;
        Ok(json!({"cable": k.hex(), "removed": true, "message": format!("Removed {tag}. Undo with Ctrl Z.")}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------------------------------------
// The assign rule (design 059 §4.3, FD5)

/// The cores `n` new wires of not-drawn cable `k` take, in order: the offer's free cores, else
/// `1…n` with no offer.
///
/// # Errors
/// [`CableError::MoreWiresThanCores`] when the offer has fewer free cores than `n` (Q6).
pub fn assign_cores(doc: &Document, k: CableId, n: usize) -> Result<Vec<Option<String>>, CableError> {
    let Some(count) = crate::strips::cable_cores(doc, k) else { return Err(CableError::NoSuchCable(k.hex())) };
    if count.offer.is_none() {
        let start = doc.cables.get(&k).map_or(0, |c| c.members.len());
        return Ok((0..n).map(|i| Some((start + i + 1).to_string())).collect());
    }
    if n > count.spare.len() {
        return Err(CableError::MoreWiresThanCores { tag: crate::views::cable_title(doc, k), free: count.spare, wires: n });
    }
    let mut spare: VecDeque<String> = count.spare.into_iter().collect();
    Ok((0..n).map(|_| spare.pop_front()).collect())
}

/// `cableslash` onto cable `k` that is not drawn (FD5): the crossed wires become its members in
/// crossing order with its free cores' labels, plus the slash. One transaction, `Cable slash`.
///
/// # Errors
/// A wire already in a cable, more wires than free cores, too many members; nothing is written.
pub(crate) fn run_slash_assign(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, page: PageId, k: CableId, from: ecad_model::Pt, to: ecad_model::Pt,
    found: &crate::slash::Crossings,
) -> Result<Value, String> {
    let doc = p.ledger.document();
    let blocked: Vec<(ConductorId, CableId)> = crate::slash::blocked(doc, &found.members).into_iter().map(|(m, c)| (m.conductor, c)).collect();
    if !blocked.is_empty() {
        return Err(CableError::AlreadyInCable(crate::slash::already_in_cable(doc, &blocked, "Nothing was changed.")).into());
    }
    if found.members.len() > MAX_CABLE_MEMBERS {
        return Err(CableError::TooMany.into());
    }
    let cores = assign_cores(doc, k, found.members.len())?;
    let (item, rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
    let mut ops = Vec::new();
    for (index, (m, core)) in found.members.iter().zip(cores.iter()).enumerate() {
        ops.push(Op::AddCableMember { cable: k, index, member: CableMember { conductor: m.conductor, core: core.clone() } });
    }
    let style = crate::commands::plain_style(p);
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Line { a: from, b: to }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Cable(k),
            page,
            form: Form::Drawn,
            role: "slash".into(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    p.apply_edit(pg, "cableslash", "Cable slash", ops)?;
    Ok(json!({
        "cable": k.hex(),
        "item": item.hex(),
        "representation": rep.hex(),
        "assigned": true,
        "conductors": found.members.iter().map(|m| m.conductor.hex()).collect::<Vec<_>>(),
        "cores": cores,
        "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
        "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
    }))
}

/// Board 2.06's **Add a core** (design 059 §3): cable `k`'s offer plus one core (`4G` → `5G`,
/// `7X` → `8X`; a list gains the next number), or its member count plus one with no offer.
pub fn next_offer(doc: &Document, k: CableId) -> String {
    let members = doc.cables.get(&k).map_or(0, |c| c.members.len());
    let Some((cores, _, text)) = cable::offer(doc, k) else { return (members + 1).to_string() };
    let t = text.trim();
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    let rest = &t[digits.len()..];
    if let Ok(n) = digits.parse::<usize>()
        && (rest.is_empty() || rest.eq_ignore_ascii_case("G") || rest.eq_ignore_ascii_case("X"))
    {
        return format!("{}{}", n.saturating_add(1), rest.to_ascii_uppercase());
    }
    let high = cores.iter().filter_map(|c| c.parse::<usize>().ok()).max().unwrap_or(0);
    let mut v = cores;
    v.push((high + 1).to_string());
    v.join(",")
}

/// One item of the slash and Add to cable pickers (design 059 §5.6).
#[derive(Debug, Clone, PartialEq)]
pub struct PickItem {
    pub cable: CableId,
    pub tag: String,
    /// `-W03  H07RN-F 4G1.5 · +CAB1 → +FIELD`.
    pub line: String,
    pub drawn: bool,
    /// A cable not drawn whose planned end location is this page's (or one of its location boxes').
    pub matches: bool,
}

/// The picker's cables on `page` (Q7): not drawn first (those whose planned end location matches
/// the page's location, or one of its location boxes', before the rest, marked), then drawn cables
/// not slashed on this page (032). Each group by tag. O(cables), built once when a picker opens.
pub fn picker(doc: &Document, page: PageId) -> Vec<PickItem> {
    let mut here: BTreeSet<String> = crate::locations::boxes_on(doc, page).into_iter().filter_map(|b| b.location).collect();
    let page_loc = doc.pages.get(&page).and_then(|x| x.properties.get(Aspect::Location.property()));
    if let Some(code) = designations::shown(doc, Aspect::Location, page_loc.map(String::as_str)) {
        here.insert(code);
    }
    let mut out: Vec<PickItem> = Vec::new();
    for k in doc.cables.keys() {
        let drawn = is_drawn(doc, *k);
        if drawn && doc.representations_of(Subject::Cable(*k)).any(|(_, r)| r.page == page && r.role == "slash") {
            continue;
        }
        let tag = crate::views::cable_title(doc, *k);
        let matches = !drawn
            && Side::BOTH.into_iter().any(|s| planned(doc, *k, s, Aspect::Location).is_some_and(|c| here.iter().any(|h| h.eq_ignore_ascii_case(&c))));
        let dash = "\u{2014}";
        let ends = format!(
            "{} \u{2192} {}",
            plan_words(doc, *k, Side::From).unwrap_or_else(|| dash.into()),
            plan_words(doc, *k, Side::To).unwrap_or_else(|| dash.into())
        );
        let line = match read(doc, *k, CableField::Type) {
            Some(t) => format!("{tag}  {t} \u{b7} {ends}"),
            None => format!("{tag}  {ends}"),
        };
        out.push(PickItem { cable: *k, tag, line, drawn, matches });
    }
    out.sort_by(|a, b| (a.drawn, !a.matches).cmp(&(b.drawn, !b.matches)).then_with(|| crate::locations::natural_cmp(&a.tag, &b.tag)));
    out
}
