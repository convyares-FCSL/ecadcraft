//! The rules of brief 052's commands (design 052 §5.5–§5.9): what `setlocation`, `setfunction`
//! (and `setprop` of an aspect property) write, the proposals a box's new value makes for the
//! devices inside, and what `conflicts` moves. A typed code is stored as its entry's key in the
//! project's list ([`crate::designations`]); an unknown code adds its entry in the same
//! transaction. The engine crate's shims commit the ops, keep the waiting conflicts and answer in
//! JSON.

use std::collections::BTreeSet;

use ecad_model::{DeviceId, Document, IdAllocator, ObjectRef, Op};

use crate::designations::{self, Aspect};
use crate::error::CmdError;
use crate::proposals::{self, Conflict};

/// Most components one `setlocation` names (tester 052: a list over it is refused before any
/// work).
pub const MAX_DEVICES: usize = 10_000;

/// The refusal for an override on a function.
pub const NO_FUNCTION_OVERRIDES: &str = "A function has no overrides: a device's parts share its function.";
/// The refusal for an `aspect` word that does not read.
pub const ASPECT_WORDS: &str = "`aspect` is \"location\" or \"function\"";
/// The refusal for an `answer` word that does not read.
pub const ANSWER_WORDS: &str = "`answer` is \"move\", \"half\" or \"leave\"";

/// The property `key` names when it is an aspect property, with the retired connector half key
/// `half.<s|p>.location` read as the override `location.socket` / `location.plug` (FP4).
pub fn aspect_property(key: &str) -> Option<String> {
    let key = key.trim();
    match key {
        "half.s.location" => return Some("location.socket".into()),
        "half.p.location" => return Some("location.plug".into()),
        _ => {}
    }
    Aspect::of_property(key).map(|_| key.to_string())
}

/// The aspect an `aspect` word names (`location` when none).
///
/// # Errors
/// A word that is neither.
pub fn aspect(word: Option<&str>) -> Result<Aspect, CmdError> {
    match word {
        None => Ok(Aspect::Location),
        Some(w) => Aspect::of_word(w).ok_or(CmdError::refused(ASPECT_WORDS)),
    }
}

/// The property a set of aspect `a` writes: the aspect's own, or with `ty` (a symbol type, trimmed;
/// blank is none) the location override `location.<type>`.
///
/// # Errors
/// An override on a function.
pub fn property_for(a: Aspect, ty: Option<&str>) -> Result<String, CmdError> {
    match ty.map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(a.property().to_string()),
        Some(_) if a == Aspect::Function => Err(CmdError::refused(NO_FUNCTION_OVERRIDES)),
        Some(t) => Ok(format!("location.{}", t.to_ascii_lowercase())),
    }
}

/// The components named, each once, in the order first named (tester 052, finding 4: a device
/// named twice is set once, not refused): `devices` when given, else `component`.
///
/// # Errors
/// Neither given.
pub fn objects(component: Option<DeviceId>, devices: Option<Vec<DeviceId>>) -> Result<Vec<DeviceId>, CmdError> {
    if let Some(cs) = devices {
        let mut seen = BTreeSet::new();
        return Ok(cs.into_iter().filter(|c| seen.insert(*c)).collect());
    }
    component.map(|c| vec![c]).ok_or(CmdError::input("`component` or `devices` (hex ids) is required"))
}

/// What a planned set does.
#[derive(Debug, Default)]
pub struct Planned {
    pub ops: Vec<Op>,
    /// The objects changed.
    pub changed: usize,
    /// The stored key and its code (`None` when cleared).
    pub value: Option<(String, String)>,
    /// The echo for entries added to the list.
    pub added: Option<String>,
}

/// The ops setting property `prop` (an aspect property) of `objects` to the code `text` (`None`
/// clears it): devices, or boxes (`function` and `location` only).
///
/// # Errors
/// No such object; an override on a function, a box or a strip; an override type that is not one;
/// a box left with neither value; a code that is not valid.
pub fn plan(doc: &Document, ids: &mut IdAllocator, objects: &[DeviceId], prop: &str, text: Option<&str>) -> Result<Planned, CmdError> {
    // The type's own refusals first: `location.head` and `location.<bogus>` are no aspect
    // property at all.
    if let Some(t) = prop.strip_prefix("location.") {
        if t == "head" {
            return Err(CmdError::refused("The head is the device; set the device's location."));
        }
        if !crate::locations::override_type_ok(t) {
            return Err(CmdError::refused(format!("`type` is socket, plug or a symbol type such as terminal, not \"{t}\".")));
        }
    }
    let a = Aspect::of_property(prop).ok_or(CmdError::refused("not a function or location property"))?;
    let mut out = Planned::default();
    let mut pending = designations::Pending::default();
    let key = match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => {
            let k = pending.store(doc, ids, a, t, &mut out.ops)?;
            let code = designations::code_of(doc, a, &k).or_else(|| pending.new_codes.last().map(|(_, c)| c.clone())).unwrap_or_default();
            out.value = Some((k.clone(), code));
            Some(k)
        }
        None => None,
    };
    for c in objects {
        let comp = doc.devices.get(c).ok_or(CmdError::refused("no such device"))?;
        let is_box = crate::locations::is_box_component(comp);
        if prop != a.property() {
            if is_box {
                return Err(CmdError::refused("A box has a function and a location, not one per symbol type."));
            }
            if crate::strips::is_strip(doc, *c) {
                return Err(CmdError::refused(
                    "A strip's positions and levels are where the strip is; set the strip's location (a split strip is two strips).",
                ));
            }
        }
        if is_box && key.is_none() {
            let other = match a {
                Aspect::Function => Aspect::Location,
                Aspect::Location => Aspect::Function,
            };
            if !comp.properties.contains_key(other.property()) {
                return Err(CmdError::refused(crate::locations::NO_VALUE));
            }
        }
        let before = comp.properties.get(prop).cloned();
        if before != key {
            out.ops.push(Op::SetProperty { object: ObjectRef::Device(*c), key: prop.to_string(), before, after: key.clone() });
            out.changed += 1;
        }
    }
    out.added = pending.words();
    if out.changed == 0 {
        out.ops.clear();
    }
    Ok(out)
}

/// The transaction label: `Set location +CP1`, `Set function =F1 (3 devices)`, `Clear location`.
pub fn label(a: Aspect, value: Option<&(String, String)>, n: usize) -> String {
    let what = match value {
        Some((_, code)) => format!("Set {} {code}", a.word()),
        None => format!("Clear {}", a.word()),
    };
    if n > 1 { format!("{what} ({n} devices)") } else { what }
}

/// The aspect property `prop` belongs to (a `location.<type>` override is a location's).
///
/// # Errors
/// Not a function or location property.
pub fn aspect_of(prop: &str) -> Result<Aspect, CmdError> {
    (if prop.starts_with("location.") { Some(Aspect::Location) } else { Aspect::of_property(prop) })
        .ok_or(CmdError::refused("not a function or location property"))
}

/// A set with its follow-ups: the box's proposals for the devices inside (brief 052d, design 052
/// §5.6).
#[derive(Debug, Default)]
pub struct SetPlan {
    pub planned: Planned,
    /// The ops to record: the set's and the proposals'.
    pub ops: Vec<Op>,
    /// What a box's new value proposed (set and asked), when it is a box.
    pub proposals: Option<proposals::Planned>,
    /// The devices inside asked about (those at the box's old value).
    pub conflicts: Vec<Conflict>,
    pub label: String,
}

/// [`plan`], and when the one object is a box given another entry, the proposals for the devices
/// inside: unset ones take it in the same transaction; those at the box's old value are asked.
///
/// # Errors
/// [`plan`]'s refusals.
pub fn plan_set(doc: &Document, ids: &mut IdAllocator, objects: &[DeviceId], prop: &str, text: Option<&str>) -> Result<SetPlan, CmdError> {
    let a = aspect_of(prop)?;
    let mut planned = plan(doc, ids, objects, prop, text)?;
    let mut out = SetPlan { label: label(a, planned.value.as_ref(), planned.changed), ..Default::default() };
    if planned.changed == 0 {
        out.planned = planned;
        return Ok(out);
    }
    out.ops = std::mem::take(&mut planned.ops);
    if let ([b], Some((key, code))) = (objects, planned.value.as_ref())
        && prop == a.property()
        && let Some(lb) = doc.representations_of(ecad_model::Subject::Device(*b)).find_map(|(rid, r)| crate::locations::box_of_rep(doc, rid, r))
    {
        let spots = proposals::spots_inside(doc, &lb);
        let proposed = proposals::plan_change(doc, a, &spots, lb.key(a), key, code, &proposals::Mode::Propose);
        out.ops.extend(proposed.ops);
        out.conflicts = proposed.conflicts.clone();
        out.proposals = Some(proposals::Planned { set: proposed.set, conflicts: proposed.conflicts, ..Default::default() });
    }
    out.planned = planned;
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// conflicts

/// What `conflicts` does with the waiting conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// List them.
    List,
    /// Move the chosen devices to the box's value.
    Move,
    /// Set the chosen connector halves' locations.
    Half,
    /// Leave them (write nothing).
    Leave,
}

/// The answer an `answer` word gives (trimmed, any case; none lists).
///
/// # Errors
/// Another word.
pub fn answer(word: Option<&str>) -> Result<Answer, CmdError> {
    match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
        None => Ok(Answer::List),
        Some("move") => Ok(Answer::Move),
        Some("half") => Ok(Answer::Half),
        Some("leave") => Ok(Answer::Leave),
        Some(_) => Err(CmdError::refused(ANSWER_WORDS)),
    }
}

/// Which conflicts an answer acts on (Choose: one row is one device and one aspect).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pick {
    /// Only these devices.
    pub only: Option<Vec<DeviceId>>,
    /// Only this aspect.
    pub aspect: Option<Aspect>,
}

impl Pick {
    pub fn picks(&self, c: &Conflict) -> bool {
        self.only.as_ref().is_none_or(|o| o.contains(&c.component)) && self.aspect.is_none_or(|a| a == c.aspect)
    }
}

/// `conflicts {answer: "half"}`: the chosen halves that still hold, as one transaction.
///
/// # Errors
/// No half waiting.
pub fn plan_half(doc: &Document, waiting: &[Conflict], pick: &Pick) -> Result<(Vec<Op>, String), CmdError> {
    let chosen: Vec<Conflict> = proposals::still_valid(doc, waiting).into_iter().filter(|c| c.half.is_some() && pick.picks(c)).collect();
    let ops = proposals::half_ops(doc, &chosen);
    if ops.is_empty() {
        return Err(CmdError::refused("No connector half is waiting: Half answers a half or pin just placed."));
    }
    let label = match chosen.as_slice() {
        [c] => format!("Set {} half location {}", crate::devices::tag_words(doc, c.component), c.to),
        _ => format!("Set half locations ({} halves)", chosen.len()),
    };
    Ok((ops, label))
}

/// What `conflicts {answer: "move"}` records: the ops (empty when nothing still differs), the
/// label, and the conflicts that still hold.
#[derive(Debug, Clone, PartialEq)]
pub struct MovePlan {
    pub ops: Vec<Op>,
    pub label: String,
    pub still_valid: Vec<Conflict>,
}

/// `conflicts {answer: "move"}`: the chosen devices that still differ, moved to their box's value.
pub fn plan_move(doc: &Document, waiting: &[Conflict], pick: &Pick) -> MovePlan {
    let valid = proposals::still_valid(doc, waiting);
    let chosen: Vec<Conflict> = valid.iter().filter(|c| pick.picks(c)).cloned().collect();
    let ops = proposals::move_ops(doc, &chosen);
    let label = if ops.is_empty() { String::new() } else { proposals::move_label(&chosen) };
    MovePlan { ops, label, still_valid: valid }
}
