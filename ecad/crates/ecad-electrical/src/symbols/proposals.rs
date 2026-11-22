//! Proposals (brief 052d, design 052 §5.3–§5.6, §5.8): when a symbol is placed or moved, a box
//! drawn or given another entry, or a page's or the project's value changed, the context there
//! **proposes** a function and a location for the devices concerned.
//!
//! # Semantics
//! - A device with no value of an aspect takes the proposal: a `SetProperty` in the same
//!   transaction (or, after a move, in one follow-up transaction).
//! - A device with the same value: nothing.
//! - A device with another value: a **conflict**. It is listed and asked about, never changed
//!   silently. The default answer is always Leave (Q2). JSON calls never prompt: their
//!   `location` / `function` parameter decides (`propose`, the default: set unset devices and
//!   leave conflicts; `move`: also move conflicting devices; `keep`: propose nothing; a code: set
//!   that value).
//! - A strip proposes for the strip (its positions and levels take the strip's, §5.2).
//!
//! Excludes: the prompt itself (the command line and the prompt strip, `crate::interact`). This
//! module reads and plans ops only.

use std::collections::BTreeSet;

use ecad_model::{DeviceId, Document, IdAllocator, ObjectRef, Op, PageId, Pt};
use serde_json::{Value, json};

use crate::locations::designations::{self, Aspect, Pending};
use crate::error::CmdError;
use crate::locations::{self, LocBox};

/// What a JSON call asks for one aspect (design 052 §5.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    /// Set unset devices; leave conflicts (the default).
    #[default]
    Propose,
    /// Also move conflicting devices.
    Move,
    /// Propose nothing.
    Keep,
    /// Set this code.
    Value(String),
    /// A connector half or pin placed: set that half's own location (`location.socket` /
    /// `location.plug`) when the connector has another one (design 052 §5.3, **Half**).
    Half,
}

impl Mode {
    /// The mode `word` names for aspect `a` (absent: [`Mode::Propose`]).
    ///
    /// # Errors
    /// An empty word.
    pub fn of(word: Option<&str>, a: Aspect) -> Result<Mode, CmdError> {
        let Some(t) = word else { return Ok(Mode::Propose) };
        match t.trim().to_ascii_lowercase().as_str() {
            "" => Err(CmdError::refused(format!("`{}` is \"propose\", \"move\", \"keep\" or a code", a.word()))),
            "propose" => Ok(Mode::Propose),
            "move" => Ok(Mode::Move),
            "keep" => Ok(Mode::Keep),
            "half" if a == Aspect::Location => Ok(Mode::Half),
            _ => Ok(Mode::Value(t.trim().to_string())),
        }
    }

    /// Both aspects' modes, function first, from the words given for each.
    ///
    /// # Errors
    /// As [`Mode::of`].
    pub fn both(function: Option<&str>, location: Option<&str>) -> Result<[Mode; 2], CmdError> {
        Ok([Mode::of(function, Aspect::Function)?, Mode::of(location, Aspect::Location)?])
    }
}

/// One device whose stored value differs from what is proposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub component: DeviceId,
    pub aspect: Aspect,
    /// Its stored code (`?` for a key with no entry).
    pub from: String,
    /// The proposed code and key.
    pub to: String,
    pub to_key: String,
    /// Every one of its symbols is now in the proposal's context.
    pub all_symbols: bool,
    /// For a connector half or pin just placed: its side (socket or plug). The answer **Half**
    /// sets that half's `location.<side>` instead of the connector's location (design 052 §5.3).
    pub half: Option<crate::symbols::Side>,
}

/// What a planned proposal does.
#[derive(Debug, Default)]
pub struct Planned {
    /// `SetProperty` ops (and list entries for typed codes).
    pub ops: Vec<Op>,
    /// The devices set: (device, aspect, code).
    pub set: Vec<(DeviceId, Aspect, String)>,
    /// The devices left with another value (unless moved: see `moved`).
    pub conflicts: Vec<Conflict>,
    /// The conflicting devices moved (JSON `move`).
    pub moved: usize,
    /// The echo for entries a typed code added to the list.
    pub added: Option<String>,
}

/// One symbol a proposal is for: its device, page and anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    pub device: DeviceId,
    pub page: PageId,
    pub at: Option<Pt>,
}

/// The context of aspect `a` at `s`, with `extra` boxes (planned, not in `doc` yet) on its page.
fn context(doc: &Document, a: Aspect, s: &Spot, extra: &[LocBox], cache: &mut locations::BoxCache) -> locations::Located {
    let mut boxes: Vec<LocBox> = cache.on(doc, s.page).to_vec();
    boxes.extend(extra.iter().filter(|b| b.page == s.page).cloned());
    let mut ctx = locations::context_in(doc, a, s.page, s.at, &boxes);
    // A planned box may add its code to the list in the same ops: its entry is not in `doc` yet,
    // so its own code stands in.
    if ctx.value.is_none()
        && let locations::Source::Box { aid, .. } = ctx.source
        && let Some(b) = extra.iter().find(|b| b.aid == aid)
    {
        ctx.value = b.code(a).map(str::to_string);
    }
    ctx
}

/// Plan the proposals for the devices at `spots` (each device once; its first spot decides), with
/// `modes` per aspect (function, location) and `extra` planned boxes. `stored` overrides a
/// device's stored key per aspect. `base` are the caller's planned ops the proposal's follow (a
/// device created there is read from them).
///
/// # Errors
/// A typed code that is not valid.
pub fn plan(
    doc: &Document, base: &[Op], ids: &mut IdAllocator, spots: &[Spot], modes: &[Mode; 2], extra: &[LocBox], pending: &mut Pending,
) -> Result<Planned, CmdError> {
    let mut out = Planned::default();
    let mut cache = locations::BoxCache::default();
    let mut seen = BTreeSet::new();
    for s in spots {
        if !seen.insert(s.device) {
            continue;
        }
        for (i, a) in Aspect::ALL.into_iter().enumerate() {
            let mode = modes.get(i).cloned().unwrap_or_default();
            let stored = crate::devices::planned_device(doc, &[base, &out.ops].concat(), s.device).and_then(|x| designations::device_key(&x, a));
            let (key, code) = match &mode {
                Mode::Keep => continue,
                Mode::Value(t) => {
                    let k = pending.store(doc, ids, a, t, &mut out.ops)?;
                    let code = designations::code_of(doc, a, &k).unwrap_or_else(|| designations::normalise(doc, a, t).unwrap_or_default());
                    (k, code)
                }
                Mode::Propose | Mode::Move | Mode::Half => {
                    let ctx = context(doc, a, s, extra, &mut cache);
                    match (ctx.key, ctx.value) {
                        (Some(k), Some(c)) => (k, c),
                        _ => continue,
                    }
                }
            };
            if stored.as_deref() == Some(key.as_str()) {
                continue;
            }
            let set = match (&stored, &mode) {
                (None, _) | (_, Mode::Move | Mode::Value(_)) => true,
                (Some(_), _) => false,
            };
            if stored.is_some() && !matches!(mode, Mode::Value(_)) {
                let from = designations::shown_text(doc, a, stored.as_deref());
                let all_symbols = all_in(doc, a, s.device, &key, extra, &mut cache);
                out.conflicts.push(Conflict { component: s.device, aspect: a, from, to: code.clone(), to_key: key.clone(), all_symbols, half: None });
                if mode == Mode::Move {
                    out.moved += 1;
                }
            }
            if set {
                // A typed code is set by hand (fixed); a context's is not (design 072 §6.3).
                let fixed = matches!(mode, Mode::Value(_));
                crate::devices::edit_planned_on(doc, base, &mut out.ops, s.device, |d| {
                    *d = designations::with_device_key(d, a, Some(&key), fixed);
                });
                out.set.push((s.device, a, code));
            }
        }
    }
    out.added = pending.words();
    Ok(out)
}

/// Whether every symbol of device `c` has `key` as its context of aspect `a`.
fn all_in(doc: &Document, a: Aspect, c: DeviceId, key: &str, extra: &[LocBox], cache: &mut locations::BoxCache) -> bool {
    doc.representations_of(ecad_model::Subject::Device(c)).all(|(_, r)| {
        let s = Spot { device: c, page: r.page, at: locations::anchor(doc, r) };
        context(doc, a, &s, extra, cache).key.as_deref() == Some(key)
    })
}

/// Plan a changed value of aspect `a` where `candidates` are (a box given another entry, a page's
/// or the project's value changed; design 052 §5.6): an unset device takes `new_key`; a device at
/// `old_key` is a conflict (moved with [`Mode::Move`]); a device with any other value is not
/// listed. [`Mode::Keep`] proposes nothing.
/// `base` are the caller's planned ops these follow.
pub fn plan_change(
    doc: &Document, base: &[Op], a: Aspect, candidates: &[Spot], old_key: Option<&str>, new_key: &str, new_code: &str, mode: &Mode,
) -> Planned {
    let mut out = Planned::default();
    if *mode == Mode::Keep {
        return out;
    }
    let mut seen = BTreeSet::new();
    for s in candidates {
        if !seen.insert(s.device) {
            continue;
        }
        let stored = crate::devices::planned_device(doc, base, s.device).and_then(|x| designations::device_key(&x, a));
        let set = match stored.as_deref() {
            None => true,
            Some(k) if k == new_key => false,
            Some(k) if Some(k) == old_key => {
                out.conflicts.push(Conflict {
                    component: s.device,
                    aspect: a,
                    from: designations::shown_text(doc, a, Some(k)),
                    to: new_code.to_string(),
                    to_key: new_key.to_string(),
                    all_symbols: true,
                    half: None,
                });
                if *mode == Mode::Move {
                    out.moved += 1;
                }
                *mode == Mode::Move
            }
            Some(_) => false,
        };
        if set {
            crate::devices::edit_planned_on(doc, base, &mut out.ops, s.device, |d| {
                *d = designations::with_device_key(d, a, Some(new_key), false);
            });
            out.set.push((s.device, a, new_code.to_string()));
        }
    }
    out
}

/// The spots of the device symbols on `page` whose context for aspect `a` is the page's own value
/// (no box carrying `a` holds them): what a page's value proposes for.
pub fn spots_on_page(doc: &Document, a: Aspect, page: PageId) -> Vec<Spot> {
    let boxes = locations::boxes_on(doc, page);
    doc.representations_on(page)
        .filter_map(|(_, r)| {
            let ecad_model::Subject::Device(c) = r.subject else { return None };
            if !locations::is_device(doc, c) {
                return None;
            }
            let at = locations::anchor(doc, r);
            at.is_none_or(|p| locations::innermost_in(&boxes, a, p).is_none()).then_some(Spot { device: c, page, at })
        })
        .collect()
}

/// Every device symbol whose context for aspect `a` falls back to the project (no box carrying
/// `a`, no page value): what the project's value proposes for. The one project-wide case.
pub fn spots_of_project(doc: &Document, a: Aspect) -> Vec<Spot> {
    doc.pages.iter().filter(|(_, pg)| crate::locations::designations::page_key(pg, a).is_none()).flat_map(|(p, _)| spots_on_page(doc, a, *p)).collect()
}

/// A device symbol a CADCraft edit moves, with its contexts before the edit (function, location).
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub device: DeviceId,
    pub rep: ecad_model::RepresentationId,
    pub before: [Option<String>; 2],
}

/// The device symbols whose context `ops` (about to be committed on `doc`) could change, with
/// their contexts now (design 052 §5.4): the symbols whose items the ops move, and the device
/// symbols on the page of a box whose outline they move or reshape. O(the moved symbols and the
/// boxes' pages' symbols).
pub fn before_move(doc: &Document, ops: &[Op]) -> Vec<Moved> {
    let mut reps: BTreeSet<ecad_model::RepresentationId> = BTreeSet::new();
    let mut box_pages: BTreeSet<PageId> = BTreeSet::new();
    for op in ops {
        let ids = match op {
            Op::ModifyItem { id, .. } => vec![*id],
            Op::ModifyRepresentation { after, .. } => after.items.clone(),
            _ => continue,
        };
        for id in ids {
            let Some((rid, r)) = doc.representation_of_item(id) else { continue };
            match r.subject {
                ecad_model::Subject::Aid(a) if locations::box_values(doc, a).is_some() => {
                    box_pages.insert(r.page);
                }
                ecad_model::Subject::Device(c) if locations::is_device(doc, c) => {
                    reps.insert(rid);
                }
                _ => {}
            }
        }
    }
    for p in box_pages {
        reps.extend(
            doc.representations_on(p)
                .filter(|(_, r)| matches!(r.subject, ecad_model::Subject::Device(c) if locations::is_device(doc, c)))
                .map(|(id, _)| id),
        );
    }
    let mut cache = locations::BoxCache::default();
    reps.into_iter()
        .filter_map(|rid| {
            let r = doc.representations.get(&rid)?;
            let ecad_model::Subject::Device(c) = r.subject else { return None };
            let before = Aspect::ALL.map(|a| locations::rep_context_in(doc, a, r, &mut cache).key);
            Some(Moved { device: c, rep: rid, before })
        })
        .collect()
}

/// The follow-up of a move (design 052 §5.4) on `doc` (after the move): for each aspect, the
/// moved symbols whose context changed to a value propose it. Leaving every box never prompts: a
/// context changed to none proposes nothing.
pub fn after_move(doc: &Document, ids: &mut IdAllocator, moved: &[Moved]) -> Result<Planned, CmdError> {
    let mut out = Planned::default();
    let mut cache = locations::BoxCache::default();
    let mut pending = Pending::default();
    for (i, a) in Aspect::ALL.into_iter().enumerate() {
        let spots: Vec<Spot> = moved
            .iter()
            .filter_map(|m| {
                let r = doc.representations.get(&m.rep)?;
                let now = locations::rep_context_in(doc, a, r, &mut cache).key;
                (now.is_some() && now != m.before.get(i).cloned().flatten()).then(|| Spot {
                    device: m.device,
                    page: r.page,
                    at: locations::anchor(doc, r),
                })
            })
            .collect();
        if spots.is_empty() {
            continue;
        }
        let mut modes = [Mode::Keep, Mode::Keep];
        if let Some(x) = modes.get_mut(i) {
            *x = Mode::Propose;
        }
        let p = plan(doc, &out.ops, ids, &spots, &modes, &[], &mut pending)?;
        out.ops.extend(p.ops);
        out.set.extend(p.set);
        out.conflicts.extend(p.conflicts);
    }
    Ok(out)
}

/// The label of a move's follow-up: `Set location +FIELD`, or `Set location (3 devices)`.
pub fn follow_up_label(p: &Planned) -> String {
    let devices: BTreeSet<DeviceId> = p.set.iter().map(|(c, _, _)| *c).collect();
    let values: BTreeSet<(Aspect, &str)> = p.set.iter().map(|(_, a, v)| (*a, v.as_str())).collect();
    let what = match values.iter().collect::<Vec<_>>().as_slice() {
        [(a, v)] => format!("Set {} {v}", a.word()),
        _ => "Set location and function".to_string(),
    };
    if devices.len() > 1 { format!("{what} ({} devices)", devices.len()) } else { what }
}

/// The spots of the device symbols on `page` whose anchor lies inside box `b` (a planned or
/// existing box): what drawing it proposes for (design 052 §5.6).
pub fn spots_inside(doc: &Document, b: &LocBox) -> Vec<Spot> {
    doc.representations_on(b.page)
        .filter_map(|(_, r)| {
            let ecad_model::Subject::Device(c) = r.subject else { return None };
            if !locations::is_device(doc, c) {
                return None;
            }
            let at = locations::anchor(doc, r)?;
            locations::contains(&b.vertices, at).then_some(Spot { device: c, page: r.page, at: Some(at) })
        })
        .collect()
}

/// The JSON of a planned proposal (design 052 §5.3): per aspect `{value, set, conflicts}`, each
/// conflict `{component, tag, aspect, location|function, proposed, all_symbols}`.
pub fn json(doc: &Document, p: &Planned) -> Value {
    let mut out = serde_json::Map::new();
    for a in Aspect::ALL {
        let set: Vec<Value> = p.set.iter().filter(|(_, x, _)| *x == a).map(|(c, _, code)| json!({"component": c.hex(), "value": code})).collect();
        let conflicts: Vec<Value> = p
            .conflicts
            .iter()
            .filter(|x| x.aspect == a)
            .map(|x| {
                json!({
                    "component": x.component.hex(), "tag": crate::devices::tag_words(doc, x.component), "aspect": a.word(),
                    a.word(): x.from, "proposed": x.to, "all_symbols": x.all_symbols,
                })
            })
            .collect();
        out.insert(a.word().into(), json!({"set": set, "conflicts": conflicts}));
    }
    Value::Object(out)
}

/// The conflict list as the command line shows it (design 052 §5.5), and the echo of what stayed:
/// `Location: 1 device has another location.` then one row per device and aspect.
pub fn conflict_words(doc: &Document, conflicts: &[Conflict]) -> Vec<String> {
    if conflicts.is_empty() {
        return Vec::new();
    }
    let devices: BTreeSet<DeviceId> = conflicts.iter().map(|c| c.component).collect();
    let aspects: BTreeSet<Aspect> = conflicts.iter().map(|c| c.aspect).collect();
    let what = match aspects.iter().copied().collect::<Vec<_>>().as_slice() {
        [Aspect::Location] => "location",
        [Aspect::Function] => "function",
        _ => "location or function",
    };
    let n = devices.len();
    let head = if n == 1 { format!("1 device has another {what}.") } else { format!("{n} devices have another {what}.") };
    let mut out = vec![head];
    for c in conflicts {
        let note = if c.all_symbols { format!("    (all its symbols are now in {})", c.to) } else { String::new() };
        out.push(format!("  {}  {} → {}{note}", crate::devices::tag_words(doc, c.component), c.from, c.to));
    }
    out
}

/// The echo of a placement's proposal (design 052 §5.3): `Placed -K01 in +CP1.`, `Placed -K01 (no
/// location).`, `-K01 is now in +CP1.`, and the conflict rows with what stayed.
pub fn place_words(doc: &Document, c: DeviceId, joined: bool, p: &Planned) -> Vec<String> {
    let tag = crate::devices::tag_words(doc, c);
    let values: Vec<String> =
        Aspect::ALL.into_iter().filter_map(|a| p.set.iter().find(|(x, y, _)| *x == c && *y == a).map(|(_, _, code)| code.clone())).collect();
    let mut out = Vec::new();
    match (joined, values.is_empty()) {
        (false, true) if locations::resolve(doc, c).value.is_none() => out.push(format!("Placed {tag} (no location).")),
        (false, false) => out.push(format!("Placed {tag} in {}.", values.join(" "))),
        (true, false) => out.push(format!("{tag} is now in {}.", values.join(" "))),
        _ => {}
    }
    out.extend(conflict_words(doc, &p.conflicts));
    if !p.conflicts.is_empty() && p.moved == 0 {
        out.push(format!("Leave (default): {} Answer `conflicts {{\"answer\": \"move\"}}` to move them.", left_words(doc, &p.conflicts)));
    }
    out.extend(p.added.clone());
    out
}

/// The label of the transaction moving `conflicts`: `Set location +CP1 (2 devices)`.
pub fn move_label(conflicts: &[Conflict]) -> String {
    let devices: BTreeSet<DeviceId> = conflicts.iter().map(|c| c.component).collect();
    let n = devices.len();
    let what = match conflicts {
        [first, rest @ ..] if rest.iter().all(|c| c.aspect == first.aspect && c.to_key == first.to_key) => {
            format!("Set {} {}", first.aspect.word(), first.to)
        }
        _ => "Set location and function".to_string(),
    };
    if n == 1 { what } else { format!("{what} ({n} devices)") }
}

/// The words of the conflicts left as they are: `-K07 stays +CP2 (shown +CP2-K07 here).`
pub fn left_words(doc: &Document, conflicts: &[Conflict]) -> String {
    conflicts
        .iter()
        .map(|c| {
            let tag = crate::devices::tag_words(doc, c.component);
            format!("{tag} stays {} (shown {}{tag} here).", c.from, c.from)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The ops giving each connector half in `conflicts` its own location (the answer **Half**):
/// `location.socket` / `location.plug` set to the proposal.
pub fn half_ops(doc: &Document, conflicts: &[Conflict]) -> Vec<Op> {
    conflicts
        .iter()
        .filter_map(|c| {
            let side = c.half?;
            let key = format!("location.{}", if side == crate::symbols::Side::Plug { "plug" } else { "socket" });
            let before = doc.devices.get(&c.component)?.properties.get(&key).cloned();
            (before.as_deref() != Some(c.to_key.as_str())).then(|| Op::SetProperty {
                object: ObjectRef::Device(c.component),
                key,
                before,
                after: Some(c.to_key.clone()),
            })
        })
        .collect()
}

/// The `proposal` parameter of boxes, pages and the project: "propose" (default), "move" or
/// "keep"; anything else, a value that is not text included, is refused (tester 052d,
/// finding 3).
///
/// # Errors
/// Not one of the three words.
pub fn proposal_param(params: &Value) -> Result<Mode, CmdError> {
    const WORDS: &str = "`proposal` is \"propose\", \"move\" or \"keep\"";
    match params.get("proposal") {
        None | Some(Value::Null) => Ok(Mode::Propose),
        Some(Value::String(m)) => match m.trim().to_ascii_lowercase().as_str() {
            "propose" => Ok(Mode::Propose),
            "move" => Ok(Mode::Move),
            "keep" => Ok(Mode::Keep),
            _ => Err(CmdError::refused(WORDS)),
        },
        Some(_) => Err(CmdError::refused(WORDS)),
    }
}

/// The waiting `conflicts` that still hold on `doc` (tester 052d, finding 1): the device is still
/// there, the proposed entry is still in the list, and the device's value still differs. An
/// undo, a redo or another edit may have taken any of these away.
pub fn still_valid(doc: &Document, conflicts: &[Conflict]) -> Vec<Conflict> {
    conflicts
        .iter()
        .filter(|c| {
            let Some(comp) = doc.devices.get(&c.component) else { return false };
            designations::code_of(doc, c.aspect, &c.to_key).is_some()
                && designations::device_key(comp, c.aspect).as_deref() != Some(c.to_key.as_str())
        })
        .cloned()
        .collect()
}

/// The ops moving `conflicts` to their proposals (the answer **Move**).
pub fn move_ops(doc: &Document, conflicts: &[Conflict]) -> Vec<Op> {
    let mut ops = Vec::new();
    for c in conflicts {
        crate::devices::edit_planned(doc, &mut ops, c.component, |d| *d = designations::with_device_key(d, c.aspect, Some(&c.to_key), false));
    }
    ops
}
