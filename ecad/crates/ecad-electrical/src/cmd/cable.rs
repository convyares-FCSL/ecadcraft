//! The rules of brief 059's cable manager (design 059 §4): `cablenew` and `cableedit` (every value
//! checked first; any refusal writes nothing), `cableremove` (refused for a drawn cable), and the
//! assign rule a slash onto a cable not drawn follows. The engine crate's shims read the
//! parameters, commit and answer in JSON.
//!
//! # Semantics
//! - Every refusal is a [`CmdError`] whose words are design 059 §9's, every echoed value clipped
//!   ([`clip`]) and the whole cut at [`MAX_WORDS`] characters.
//! - Every key read or written is named by [`CableField::key`] (design 059 §2.3).
//!
//! Excludes: the pickers and the UI (the Cables tab).

use std::collections::VecDeque;

use ecad_model::words::Word;
use ecad_model::{CableId, Document, IdAllocator, Ledger, ObjectRef, Op, PageId, Properties, StyleRef};

use crate::cables::{CableField, MAX_LENGTH, MAX_TEXT, MAX_WORDS, MIN_LENGTH, Shield, Side, clip, drawn_pages, is_drawn, length_of, type_notation};
use crate::designations::{self, Aspect};
use crate::devices::TagHigh;
use crate::error::CmdError;
use crate::styles::{self, cable};

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

/// A refusal in `words`, cut at [`MAX_WORDS`] characters.
fn said(words: String) -> CmdError {
    if words.chars().nth(MAX_WORDS).is_some() {
        let cut: String = words.chars().take(MAX_WORDS).collect();
        CmdError::refused(format!("{cut}\u{2026}"))
    } else {
        CmdError::refused(words)
    }
}

/// `a, b and c`.
fn and_list(v: &[String]) -> String {
    match v {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `m` with each quoted value (between backticks) clipped.
fn clip_quoted(m: &str) -> String {
    let mut out = String::new();
    for (i, part) in m.split('`').enumerate() {
        if i > 0 {
            out.push('`');
        }
        if i % 2 == 1 { out.push_str(&clip(part)) } else { out.push_str(part) }
    }
    out
}

/// The refusal of `value` for `field` (design 059 §9).
pub fn bad_value(field: CableField, value: &str, reason: &BadReason) -> CmdError {
    let shown = clip(value);
    said(match (field, reason) {
        (CableField::Length, _) => format!("Length must be a number of metres from {MIN_LENGTH} to {MAX_LENGTH}."),
        (CableField::Cores | CableField::Type | CableField::Tag, BadReason::BadOffer(why)) => {
            format!("{}: {}", field.label(), clip_quoted(&why.replace(value, &shown)))
        }
        (CableField::Tag, BadReason::Empty) => "A cable always has a tag. Type a new tag (such as -W05) instead of clearing it.".to_string(),
        (_, BadReason::NotAWord) if ecad_model::text::has_hidden(value) => {
            format!("{} holds a control or invisible character; type it again without one.", field.label())
        }
        (CableField::CableType, BadReason::UnknownEntry) => format!("No cable type {shown:?} in this project's standards."),
        (CableField::Shield, _) => "Shield is none, overall, pairs or overall+pairs.".to_string(),
        (_, BadReason::TooLong(n)) => format!("{} is at most {n} characters.", field.label()),
        (_, BadReason::Empty) => format!("{} needs a value, or null to clear it.", field.label()),
        (_, BadReason::UnknownEntry) => format!("{}: no entry {shown:?}.", field.label()),
        (_, _) => format!("{}: {shown:?} is not a value ECAD reads.", field.label()),
    })
}

/// "No cable "-W9"." for the cable `t` does not name.
fn no_cable(t: &str) -> CmdError {
    said(format!("No cable {:?}.", clip(t)))
}

/// A cable end's plan and gland as given: absent (`None`), null (`Some(None)`) or text.
#[derive(Debug, Clone, Default)]
pub struct EndInput {
    pub location: Option<Option<String>>,
    pub function: Option<Option<String>>,
    pub gland: Option<Option<String>>,
}

/// The fields `cablenew` takes and `cableedit`'s `set` changes, as given: absent (`None`), null
/// (`Some(None)`, clears in `cableedit`) or the text written (a number as JSON writes it).
#[derive(Debug, Clone, Default)]
pub struct CableInput {
    pub tag: Option<Option<String>>,
    pub cable_type: Option<Option<String>>,
    pub kind: Option<Option<String>>,
    pub cores: Option<Option<String>>,
    pub length: Option<Option<String>>,
    pub shield: Option<Option<String>>,
    pub maker: Option<Option<String>>,
    pub part_number: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub from: Option<EndInput>,
    pub to: Option<EndInput>,
}

impl CableInput {
    /// Every given field with its value (`None`: null), in [`CableField::ALL`] order.
    fn entries(&self) -> Vec<(CableField, Option<&str>)> {
        let mut v: Vec<(CableField, Option<&str>)> = Vec::new();
        fn push<'a>(v: &mut Vec<(CableField, Option<&'a str>)>, f: CableField, x: &'a Option<Option<String>>) {
            if let Some(x) = x {
                v.push((f, x.as_deref()));
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

/// A value as stored text: `None` for null or blank.
///
/// # Errors
/// A control or invisible character.
fn as_text(field: CableField, v: Option<&str>) -> Result<Option<String>, CmdError> {
    match v {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        // Tester 059a: no control or invisible character in any value (a planned code, a maker…).
        Some(s) if ecad_model::text::has_hidden(s) => Err(bad_value(field, s, &BadReason::NotAWord)),
        Some(s) => Ok(Some(s.trim().to_string())),
    }
}

/// Check one value and give the text stored (`None` clears). A planned code not in the project's
/// list adds its entry (052's rule) through `pending`.
fn checked(
    doc: &Document, ids: &mut IdAllocator, pending: &mut designations::Pending, field: CableField, v: Option<&str>, ops: &mut Vec<Op>,
) -> Result<Option<String>, CmdError> {
    let Some(t) = as_text(field, v)? else { return Ok(None) };
    let bad = |reason: BadReason| bad_value(field, &t, &reason);
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
        CableField::Gland(_) if !cable::is_gland(&t) || ecad_model::text::has_hidden(&t) => Err(bad(BadReason::NotAWord)),
        CableField::Gland(_) => Ok(Some(t)),
        CableField::Shield => <Shield as Word>::parse(&t).map(|s| Some(s.word().to_string())).ok_or_else(|| bad(BadReason::NotAWord)),
        CableField::Maker | CableField::PartNumber | CableField::Description | CableField::Type => {
            if t.chars().count() > MAX_TEXT {
                return Err(bad(BadReason::TooLong(MAX_TEXT)));
            }
            if ecad_model::text::has_hidden(&t) {
                return Err(bad(BadReason::NotAWord));
            }
            Ok(Some(t))
        }
        CableField::Cores => crate::strips::cores_value(Some(&t)).map_err(|e| bad(BadReason::BadOffer(e.to_string()))),
        CableField::CableType => {
            styles::find(doc, ecad_model::StyleCatalogue::Cable, &t).map(|id| Some(id.text())).map_err(|_| bad(BadReason::UnknownEntry))
        }
        CableField::EndLocation(_) => pending.store(doc, ids, Aspect::Location, &t, ops).map(Some).map_err(|_| bad(BadReason::UnknownEntry)),
        CableField::EndFunction(_) => pending.store(doc, ids, Aspect::Function, &t, ops).map(Some).map_err(|_| bad(BadReason::UnknownEntry)),
    }
}

/// The cable `text` names: an ID or a tag.
///
/// # Errors
/// No such cable ("No cable "-W9".").
pub fn cable_named(doc: &Document, text: &str) -> Result<CableId, CmdError> {
    let t = text.trim();
    if let Some(k) = CableId::parse(t).filter(|k| doc.cables.contains_key(k)) {
        return Ok(k);
    }
    crate::references::cable_tagged(doc, t).ok_or_else(|| no_cable(&t.chars().take(32).collect::<String>()))
}

/// Whether core `m` is on a line: a stored unplaced core (design 072 §4.5) is offered, not in use.
fn placed(m: &ecad_model::Core) -> bool {
    m.position != ecad_model::CorePosition::Unplaced
}

/// The cores cable `k`'s members use, by label (their matched or stored labels, else their place).
fn cores_in_use(doc: &Document, k: CableId) -> Vec<String> {
    let Some(cab) = doc.cables.get(&k) else { return Vec::new() };
    let count = crate::strips::count_cores(cable::offer(doc, k), &cab.cores);
    cab.cores
        .iter()
        .enumerate()
        .filter(|(_, m)| placed(m))
        .map(|(i, m)| count.matched.get(i).cloned().flatten().or_else(|| m.label_str().map(str::to_string)).unwrap_or_else(|| (i + 1).to_string()))
        .collect()
}

/// Q6: a new offer (from `props`) must hold every core cable `k`'s members use.
fn check_offer(doc: &Document, k: CableId, props: &Properties) -> Result<(), CmdError> {
    let Some(cab) = doc.cables.get(&k) else { return Ok(()) };
    if !cab.cores.iter().any(placed) {
        return Ok(());
    }
    let Some((offer, _, text)) = cable::offer_of_props(doc, props) else { return Ok(()) };
    let count = crate::strips::count_cores(Some((offer.clone(), crate::strips::OfferSource::Cores, text.clone())), &cab.cores);
    let lacks_label =
        cab.cores.iter().filter(|m| placed(m)).any(|m| m.label_str().is_some_and(|l| !offer.iter().any(|o| o.eq_ignore_ascii_case(l.trim()))));
    if count.over.is_empty() && !lacks_label {
        return Ok(());
    }
    let tag = crate::cables::cable_title(doc, k);
    let in_use = cores_in_use(doc, k);
    let n = offer.len();
    Err(said(format!(
        "{tag} has wires on cores {}; {text} has {n} core{}. Remove wires on their page first, or choose a type with at least {} cores.",
        and_list(&in_use),
        if n == 1 { "" } else { "s" },
        in_use.len()
    )))
}

/// The refusal when another device or cable holds `typed`.
fn refuse_taken(doc: &Document, typed: &str, except: Option<CableId>) -> Result<(), CmdError> {
    let Ok(t) = ecad_model::tag::normalise(typed, crate::defaults::print::tag_style(doc)) else { return Ok(()) };
    if ecad_model::tag::provisional(&t).is_some() {
        return Ok(());
    }
    match crate::devices::tag_holder(doc, &t, except.map(ObjectRef::Cable)) {
        Some(h) => {
            let by = match h {
                crate::devices::Holder::Device(c) => format!("{} (device)", crate::devices::tag_of(doc, c).unwrap_or("?")),
                crate::devices::Holder::Cable(k) => format!("{} (cable)", crate::cables::cable_title(doc, k)),
            };
            Err(said(format!("{} is already the tag of {}. Choose another tag.", clip(&t), clip(&by))))
        }
        None => Ok(()),
    }
}

/// `cable` with the shield moved from its properties (where the checked inputs put it) to its
/// field (design 072 §4.5): the field is cleared when the properties hold none.
fn take_shield(mut cable: ecad_model::Cable) -> ecad_model::Cable {
    cable.shield = cable.properties.remove(CableField::Shield.key()).and_then(|w| <Shield as Word>::parse(&w));
    cable
}

/// A planned `cablenew`.
#[derive(Debug, Clone)]
pub struct NewCable {
    pub id: CableId,
    pub tag: String,
    /// The `CreateCable` (after the project list entries a new planned code adds).
    pub ops: Vec<Op>,
    /// The words of the entries added, if any.
    pub added: Option<String>,
}

/// `cablenew` (design 059 §4.1): a cable not drawn, the next free -W unless `tag`.
///
/// # Errors
/// A bad value or a taken tag; nothing is written.
pub fn plan_new(ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, fields: &CableInput) -> Result<NewCable, CmdError> {
    let doc = ledger.document();
    let mut ops = Vec::new();
    let mut pending = designations::Pending::default();
    let mut props = Properties::new();
    let typed_tag = match &fields.tag {
        Some(v) => as_text(CableField::Tag, v.as_deref())?,
        None => None,
    };
    for (field, v) in fields.entries() {
        if field == CableField::Tag {
            continue;
        }
        if let Some(t) = checked(doc, ids, &mut pending, field, v, &mut ops)? {
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
    let tag = crate::devices::new_cable_tag(ledger, high, typed_tag.as_deref())?;
    props.insert(CableField::Tag.key().to_string(), tag.clone());
    let id: CableId = ids.fresh();
    let mut cable = take_shield(ecad_model::Cable { properties: props, ..ecad_model::Cable::default() });
    crate::styles::cable::sync_offered_cores(doc, &Properties::new(), &mut cable);
    ops.push(Op::CreateCable { id, cable });
    Ok(NewCable { id, tag, ops, added: pending.words() })
}

/// The ops of `cableedit` on `doc`: every value checked first; any refusal writes nothing.
/// Returns the ops, the fields given, and the words of project list entries added.
///
/// # Errors
/// A bad value, a taken tag, or cores in use.
pub fn edit_ops(
    doc: &Document, ids: &mut IdAllocator, tag_high: &mut TagHigh, k: CableId, set: &CableInput,
) -> Result<(Vec<Op>, Vec<CableField>, Option<String>), CmdError> {
    let cab = doc.cables.get(&k).ok_or_else(|| no_cable(&k.hex()))?;
    let mut ops = Vec::new();
    let mut pending = designations::Pending::default();
    let mut after = cab.properties.clone();
    let mut changed = Vec::new();
    for (field, v) in set.entries() {
        let value = if field == CableField::Tag {
            match as_text(field, v)? {
                // Tester 059a, rule 1: clearing the tag would leave the cable without one.
                None => return Err(bad_value(field, "", &BadReason::Empty)),
                Some(t) => {
                    let n = ecad_model::tag::normalise(&t, crate::defaults::print::tag_style(doc))
                        .map_err(|e| bad_value(field, &t, &BadReason::BadOffer(e.to_string())))?;
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
    // A typed tag is fixed (026), as Set property on `tag` makes it.
    if set.tag.is_some() {
        let fixed = after.get(CableField::Tag.key()).is_some_and(|t| !ecad_model::tag::is_provisional(t));
        match fixed {
            true => after.insert(ecad_model::tag::FIXED_KEY.into(), ecad_model::words::flag_word(true).to_string()),
            false => after.remove(ecad_model::tag::FIXED_KEY),
        };
    }
    let mut cable = take_shield(ecad_model::Cable { properties: after, ..cab.clone() });
    if set.shield.is_none() {
        cable.shield = cab.shield;
    }
    crate::styles::cable::sync_offered_cores(doc, &cab.properties, &mut cable);
    ops.extend(Op::modify_cable(k, cab, cable));
    Ok((ops, changed, pending.words()))
}

/// A drawn cable, which the list cannot remove (Q1): its title and the pages it is drawn on (the
/// UI's Go to links).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawn {
    pub tag: String,
    pub pages: Vec<(PageId, String)>,
}

impl Drawn {
    /// "-W01 is drawn on 3 and 5. Delete it on its page."
    pub fn words(&self) -> String {
        let names: Vec<String> = self.pages.iter().map(|(_, n)| n.clone()).collect();
        said(format!("{} is drawn on {}. Delete it on its page.", self.tag, and_list(&names))).to_string()
    }
}

/// Cable `k` when it is drawn: what refuses its removal from the list.
pub fn drawn(doc: &Document, k: CableId) -> Option<Drawn> {
    (doc.cables.contains_key(&k) && is_drawn(doc, k)).then(|| Drawn { tag: crate::cables::cable_title(doc, k), pages: drawn_pages(doc, k) })
}

/// Whether cable `k` may be removed from the list (Q1): only when it is not drawn.
///
/// # Errors
/// No such cable, or it is drawn ([`Drawn::words`]; [`drawn`] gives its pages).
pub fn check_remove(doc: &Document, k: CableId) -> Result<(), CmdError> {
    if !doc.cables.contains_key(&k) {
        return Err(no_cable(&k.hex()));
    }
    match drawn(doc, k) {
        Some(d) => Err(CmdError::refused(d.words())),
        None => Ok(()),
    }
}

/// The cores `n` new wires of not-drawn cable `k` take, in order: the offer's free cores, else
/// `1…n` with no offer.
///
/// # Errors
/// No such cable, or the offer has fewer free cores than `n` (Q6).
pub fn assign_cores(doc: &Document, k: CableId, n: usize) -> Result<Vec<Option<String>>, CmdError> {
    let Some(count) = crate::strips::cable_cores(doc, k) else { return Err(no_cable(&k.hex())) };
    if count.offer.is_none() {
        let start = doc.cables.get(&k).map_or(0, |c| c.cores.len());
        return Ok((0..n).map(|i| Some((start + i + 1).to_string())).collect());
    }
    if n > count.spare.len() {
        let free = &count.spare;
        return Err(said(format!(
            "{} has {} free core{}{}; the slash crosses {n} wires. Slash fewer wires, or add cores first.",
            crate::cables::cable_title(doc, k),
            free.len(),
            if free.len() == 1 { "" } else { "s" },
            if free.is_empty() { String::new() } else { format!(" ({})", free.join(", ")) }
        )));
    }
    let mut spare: VecDeque<String> = count.spare.into_iter().collect();
    Ok((0..n).map(|_| spare.pop_front()).collect())
}

/// The refusal of a slash whose wires are in a cable already (`words`), cut at [`MAX_WORDS`].
pub fn already_in_cable(words: String) -> CmdError {
    said(words)
}

/// The refusal of a slash crossing more than the most members a cable takes.
pub fn too_many() -> CmdError {
    said(crate::slash::too_many())
}
