//! The rules of the commands: what `setlocation`, `setfunction`
//! (and `setprop` of an aspect property) write, the proposals a box's new value makes for the
//! devices inside, and what `conflicts` moves. A typed code is stored as its entry's key in the
//! project's list ([`crate::locations::designations`]); an unknown code adds its entry in the same
//! transaction. The engine crate's shims commit the ops, keep the waiting conflicts and answer in
//! JSON.

use std::collections::BTreeSet;

use ecad_model::{Aid, AidId, DeviceId, Document, IdAllocator, ObjectRef, Op};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::MAX_HANDLES;
use crate::commands::input::{Bounded, OrNull, nullable};
use crate::commands::run::{Cx, Generic, Outcome, Pages, Slot, State};
use crate::error::CmdError;
use crate::locations::designations::{self, Aspect};
use crate::symbols::proposals::{self, Conflict};

/// Most components one `setlocation` names (a list over it is refused before any
/// work).
pub(crate) const MAX_DEVICES: usize = 10_000;

/// The refusal for an override on a function.
pub(crate) const NO_FUNCTION_OVERRIDES: &str = "A function has no overrides: a device's parts share its function.";
/// The refusal for an `aspect` word that does not read.
pub(crate) const ASPECT_WORDS: &str = "`aspect` is \"location\" or \"function\"";
/// The refusal for an `answer` word that does not read.
pub(crate) const ANSWER_WORDS: &str = "`answer` is \"move\", \"half\" or \"leave\"";

/// The property `key` names when it is an aspect property, with the retired connector half key
/// `half.<s|p>.location` read as the override `location.socket` / `location.plug`.
pub(crate) fn aspect_property(key: &str) -> Option<String> {
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
pub(crate) fn property_for(a: Aspect, ty: Option<&str>) -> Result<String, CmdError> {
    match ty.map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(a.property().to_string()),
        Some(_) if a == Aspect::Function => Err(CmdError::refused(NO_FUNCTION_OVERRIDES)),
        Some(t) => Ok(format!("location.{}", t.to_ascii_lowercase())),
    }
}

/// The components named, each once, in the order first named (a device
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

/// The key stored for the code `text` of aspect `a` (`None`: cleared), noting it and its code in
/// `out` (a new entry's op goes in `out.ops`).
fn stored_key(
    doc: &Document, ids: &mut IdAllocator, a: Aspect, text: Option<&str>, pending: &mut designations::Pending, out: &mut Planned,
) -> Result<Option<String>, CmdError> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    let k = pending.store(doc, ids, a, t, &mut out.ops)?;
    let code = designations::code_of(doc, a, &k).or_else(|| pending.new_codes.last().map(|(_, c)| c.clone())).unwrap_or_default();
    out.value = Some((k.clone(), code));
    Ok(Some(k))
}

/// The ops setting property `prop` (an aspect property) of devices `objects` to the code `text`
/// (`None` clears it). A location box's values are its aid's ([`plan_box_set`]).
///
/// # Errors
/// No such device (a location box is named as one); an override on a function or a strip; an override type that is not one; a code
/// that is not valid.
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
    // A location box is a drawing aid: named, not "no such device".
    if let Some(c) = objects.iter().find(|c| !doc.devices().contains_key(c) && crate::locations::box_values(doc, AidId(c.0)).is_some()) {
        return Err(CmdError::refused(format!("{} is a location box; edit it with setprop {{aids}}", c.hex())));
    }
    let mut out = Planned::default();
    let mut pending = designations::Pending::default();
    let key = stored_key(doc, ids, a, text, &mut pending, &mut out)?;
    for c in objects {
        let comp = doc.devices().get(c).ok_or(CmdError::refused("no such device"))?;
        if prop != a.property() && crate::terminal_strips::strip_of(doc, *c).is_some() {
            return Err(CmdError::refused(
                "A strip's positions and levels are where the strip is; set the strip's location (a split strip is two strips).",
            ));
        }
        if prop == a.property() {
            // A typed value is set by hand: fixed.
            let after = designations::with_device_key(comp, a, key.as_deref(), true);
            if after != *comp {
                out.ops.push(Op::ModifyDevice { id: *c, before: comp.clone(), after });
                out.changed += 1;
            }
            continue;
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
pub(crate) fn aspect_of(prop: &str) -> Result<Aspect, CmdError> {
    (if ecad_model::naming::keys::in_area(prop, ecad_model::naming::keys::Area::Location) {
        Some(Aspect::Location)
    } else {
        Aspect::of_property(prop)
    })
    .ok_or(CmdError::refused("not a function or location property"))
}

/// A set with its follow-ups: the box's proposals for the devices inside
#[derive(Debug, Default)]
pub(crate) struct SetPlan {
    pub planned: Planned,
    /// The ops to record: the set's and the proposals'.
    pub ops: Vec<Op>,
    /// What a box's new value proposed (set and asked), when it is a box.
    pub proposals: Option<proposals::Planned>,
    /// The devices inside asked about (those at the box's old value).
    pub conflicts: Vec<Conflict>,
    pub label: String,
}

/// [`plan`] on devices, with its label.
///
/// # Errors
/// [`plan`]'s refusals.
pub(crate) fn plan_set(doc: &Document, ids: &mut IdAllocator, objects: &[DeviceId], prop: &str, text: Option<&str>) -> Result<SetPlan, CmdError> {
    let a = aspect_of(prop)?;
    let mut planned = plan(doc, ids, objects, prop, text)?;
    let out =
        SetPlan { label: label(a, planned.value.as_ref(), planned.changed), ops: std::mem::take(&mut planned.ops), planned, ..Default::default() };
    Ok(out)
}

/// The refusal of an aid that is not a location box.
pub(crate) const NOT_A_BOX: &str = "That is not a location box.";

/// `setprop {aids, key: function | location, value}`: location boxes `aids`
/// take the code `text` of aspect `a` (`None` clears it), one `ModifyAid` per box that changes.
/// When one box takes another entry, the devices inside are proposed it: unset ones take it in
/// the same transaction; those at the box's old value are asked.
///
/// # Errors
/// An aid that is not a box, a box left with neither value, or a code that is not valid.
pub(crate) fn plan_box_set(doc: &Document, ids: &mut IdAllocator, aids: &[AidId], a: Aspect, text: Option<&str>) -> Result<SetPlan, CmdError> {
    let mut planned = Planned::default();
    let mut pending = designations::Pending::default();
    let key = stored_key(doc, ids, a, text, &mut pending, &mut planned)?;
    let mut seen = BTreeSet::new();
    for b in aids.iter().filter(|b| seen.insert(**b)) {
        let Some(before @ Aid::Box(x)) = doc.aids().get(b) else { return Err(CmdError::refused(NOT_A_BOX)) };
        let after = designations::with_box_key(x, a, key.as_deref());
        if after.location.is_none() && after.function.is_none() {
            return Err(CmdError::refused(crate::locations::NO_VALUE));
        }
        if after != *x {
            planned.ops.push(Op::ModifyAid { id: *b, before: before.clone(), after: Aid::Box(after) });
            planned.changed += 1;
        }
    }
    planned.added = pending.words();
    if planned.changed == 0 {
        planned.ops.clear();
        return Ok(SetPlan { label: label(a, planned.value.as_ref(), 0), planned, ..Default::default() });
    }
    let n = planned.changed;
    let what = label(a, planned.value.as_ref(), 1);
    let mut out =
        SetPlan { label: if n > 1 { format!("{what} ({n} boxes)") } else { what }, ops: std::mem::take(&mut planned.ops), ..Default::default() };
    if let ([b], Some((key, code))) = (aids, planned.value.as_ref())
        && let Some(lb) = doc.representations_of(ecad_model::Subject::Aid(*b)).find_map(|(rid, r)| crate::locations::box_of_rep(doc, rid, r))
    {
        let spots = proposals::spots_inside(doc, &lb);
        let proposed = proposals::plan_change(doc, &out.ops, a, &spots, lb.key(a), key, code, &proposals::Mode::Propose);
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
pub(crate) fn plan_half(doc: &Document, waiting: &[Conflict], pick: &Pick) -> Result<(Vec<Op>, String), CmdError> {
    let chosen: Vec<Conflict> = proposals::still_valid(doc, waiting).into_iter().filter(|c| c.half.is_some() && pick.picks(c)).collect();
    let ops = proposals::half_ops(doc, &chosen);
    if ops.is_empty() {
        return Err(CmdError::refused("No connector half is waiting: Half answers a half or pin just placed."));
    }
    let label = match chosen.as_slice() {
        [c] => format!("Set {} half location {}", crate::devices::tags::tag_words(doc, c.component), c.to),
        _ => format!("Set half locations ({} halves)", chosen.len()),
    };
    Ok((ops, label))
}

/// What `conflicts {answer: "move"}` records: the ops (empty when nothing still differs), the
/// label, and the conflicts that still hold.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MovePlan {
    pub ops: Vec<Op>,
    pub label: String,
    pub still_valid: Vec<Conflict>,
}

/// `conflicts {answer: "move"}`: the chosen devices that still differ, moved to their box's value.
pub(crate) fn plan_move(doc: &Document, waiting: &[Conflict], pick: &Pick) -> MovePlan {
    let valid = proposals::still_valid(doc, waiting);
    let chosen: Vec<Conflict> = valid.iter().filter(|c| pick.picks(c)).cloned().collect();
    let ops = proposals::move_ops(doc, &chosen);
    let label = if ops.is_empty() { String::new() } else { proposals::move_label(&chosen) };
    MovePlan { ops, label, still_valid: valid }
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// A planned set of aspect `a` as the outcome: committed when it changes anything, with its
/// proposals, the words for the entries added and the devices asked about, which wait as the
/// conflicts (`count`: the word for the objects changed).
pub(crate) fn set_outcome(doc: &Document, a: Aspect, plan: SetPlan, count: &str, state: &mut Slot<'_>) -> Outcome {
    let planned = &plan.planned;
    let mut v = json!({
        "changed": planned.changed > 0,
        count: planned.changed,
        a.word(): planned.value.as_ref().map(|(_, c)| c.clone()),
        format!("{}_key", a.word()): planned.value.as_ref().map(|(k, _)| k.clone()),
    });
    if planned.changed == 0 {
        return Outcome::answer(v);
    }
    if let Some(proposed) = &plan.proposals {
        v["proposals"] = proposals::json(doc, proposed);
    }
    let mut message: Vec<String> = planned.added.iter().cloned().collect();
    message.extend(proposals::conflict_words(doc, &plan.conflicts));
    let echo = (!message.is_empty()).then(|| message.join("\n"));
    if let Some(m) = &echo {
        v["message"] = json!(m);
    }
    if let Slot::Conflicts { set, .. } = state {
        **set = Some(plan.conflicts);
    }
    Outcome::edit(plan.label, plan.ops, v).echo(echo)
}

/// The conflicts given to a command that declares [`State::Conflicts`]: those waiting, and the
/// ones to wait after it.
///
/// # Errors
/// A command that does not declare them.
pub(crate) fn conflicts_slot<'s, 'a>(state: &'s mut Slot<'a>) -> Result<(&'a [Conflict], &'s mut Option<Vec<Conflict>>), CmdError> {
    match state {
        Slot::Conflicts { waiting, set } => Ok((*waiting, &mut **set)),
        _ => Err(CmdError::refused("this command does not keep the location conflicts")),
    }
}

/// The aspect, objects and property a `setlocation` sets.
fn setlocation_target(p: &SetlocationParams) -> Result<(Aspect, Vec<DeviceId>, String), CmdError> {
    let a = aspect(p.aspect.as_deref())?;
    let objects = objects(p.component, p.devices.as_ref().map(|c| c.0.clone()))?;
    let prop = property_for(a, p.r#type.as_deref())?;
    Ok((a, objects, prop))
}

/// `setlocation {component | components, value, aspect?, type?}`: a device's location
/// or function, as a code of the project's list (one transaction; null clears it).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetlocationParams {
    /// One device.
    #[serde(default)]
    component: Option<DeviceId>,
    /// Several (at most 10000; one named twice is set once).
    #[serde(default)]
    devices: Option<Bounded<DeviceId, 0, MAX_DEVICES>>,
    /// The code, such as +CP1, or null to clear it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<String>")]
    value: Option<Option<String>>,
    /// location (the default) or function.
    #[serde(default)]
    aspect: Option<String>,
    /// Location only: an override for one symbol type (socket, plug, terminal or another type).
    #[serde(default)]
    r#type: Option<String>,
}

/// `setfunction {component | components, value}`: as `setlocation` with aspect function.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetfunctionParams {
    /// One device.
    #[serde(default)]
    component: Option<DeviceId>,
    /// Several (at most 10000; one named twice is set once).
    #[serde(default)]
    devices: Option<Bounded<DeviceId, 0, MAX_DEVICES>>,
    /// The code, such as =F1, or null to clear it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<String>")]
    value: Option<Option<String>>,
    /// Not taken: a function has no overrides (a device's parts share its function).
    #[serde(default)]
    r#type: Option<String>,
}

/// `conflicts {answer?, components?, aspect?}`: the waiting conflicts; Move,
/// Half or Leave. `devices` and `aspect` choose rows (Choose: one row is one device and one
/// aspect). Move and Half act only on the rows that still hold (an undo may have taken them away).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConflictsParams {
    /// move, half or leave; none lists the waiting conflicts.
    #[serde(default)]
    answer: Option<String>,
    /// Choose: only these devices.
    #[serde(default)]
    devices: Option<Bounded<DeviceId, 0, MAX_HANDLES>>,
    /// Choose: only this aspect, location or function.
    #[serde(default)]
    aspect: Option<String>,
}

impl Generic for SetlocationParams {
    const ID: &'static str = "setlocation";
    const TITLE: &'static str = "Set Location";
    const HELP: &'static str = "{component: id | devices: [id], value: code | null, aspect?: \"location\" (default) | \"function\", type?: \"socket\" | \"plug\" | \"terminal\" | a symbol type (an override, location only)} \
(one transaction; a code not in the project's list adds its entry; null clears it)";
    const STATE: State = State::Conflicts;

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        setlocation_target(self).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let (a, objects, prop) = setlocation_target(&self)?;
        let plan = plan_set(cx.doc, cx.ids, &objects, &prop, self.value.flatten().as_deref())?;
        Ok(set_outcome(cx.doc, a, plan, "devices", &mut cx.state))
    }
}

impl Generic for SetfunctionParams {
    const ID: &'static str = "setfunction";
    const TITLE: &'static str = "Set Function";
    const HELP: &'static str = "{component: id | devices: [id], value: code | null} (as setlocation with aspect \"function\")";
    const STATE: State = State::Conflicts;

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        if self.r#type.is_some() {
            return Err(CmdError::refused(NO_FUNCTION_OVERRIDES));
        }
        objects(self.component, self.devices.as_ref().map(|c| c.0.clone())).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let objects = objects(self.component, self.devices.map(|c| c.0))?;
        let plan = plan_set(cx.doc, cx.ids, &objects, Aspect::Function.property(), self.value.flatten().as_deref())?;
        Ok(set_outcome(cx.doc, Aspect::Function, plan, "devices", &mut cx.state))
    }
}

/// The answer and the rows a `conflicts` names.
fn conflicts_pick(p: &ConflictsParams) -> Result<(Answer, Pick), CmdError> {
    let answer = answer(p.answer.as_deref())?;
    let aspect = match p.aspect.as_deref() {
        None => None,
        Some(w) => Some(Aspect::of_word(w).ok_or(CmdError::refused(ASPECT_WORDS))?),
    };
    Ok((answer, Pick { only: p.devices.as_ref().map(|c| c.0.clone()), aspect }))
}

/// A `conflicts` answer, echoing its message when it has one.
fn said(v: Value) -> Outcome {
    let echo = v.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()).map(str::to_string);
    Outcome::answer(v).echo(echo)
}

impl Generic for ConflictsParams {
    const ID: &'static str = "conflicts";
    const TITLE: &'static str = "Location Conflicts";
    const HELP: &'static str = "{answer?: \"move\" | \"leave\", components?: [id] (Choose: only these)} (the devices the last placement, move or box left with another function or location; no answer lists them; `move` sets them in one transaction; `leave` writes nothing)";
    const PAGES: Pages = Pages::ReadAnywhere;
    const STATE: State = State::Conflicts;

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        conflicts_pick(self).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let (answer, pick) = conflicts_pick(&self)?;
        let doc = cx.doc;
        let (waiting, set) = conflicts_slot(&mut cx.state)?;
        Ok(match answer {
            Answer::List => {
                let listed = proposals::Planned { conflicts: waiting.to_vec(), ..Default::default() };
                Outcome::answer(json!({"conflicts": proposals::json(doc, &listed)}))
            }
            Answer::Leave => {
                let words = proposals::left_words(doc, waiting);
                *set = Some(Vec::new());
                said(json!({"changed": false, "message": words}))
            }
            Answer::Half => {
                let (ops, label) = plan_half(doc, waiting, &pick)?;
                let message = format!("{label}.");
                Outcome::edit(label, ops, json!({"changed": true, "message": message})).echo(Some(message))
            }
            Answer::Move => {
                let plan = plan_move(doc, waiting, &pick);
                if plan.ops.is_empty() {
                    *set = Some(plan.still_valid);
                    return Ok(said(json!({"changed": false, "message": "Nothing to move: those devices no longer differ, or the entry is gone."})));
                }
                let message = format!("{}.", plan.label);
                Outcome::edit(plan.label, plan.ops, json!({"changed": true, "message": message})).echo(Some(message))
            }
        })
    }
}
