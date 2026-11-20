//! The rules of brief 036's commands (design 036 §3.5, §4.2): `termlabel` (relabel a position, set
//! or clear one level's own label, move a level to another position) and `toposition` (turn a brief
//! 034 fuse terminal device into a fuse position of a strip). Each function reads the document and
//! returns the ops to commit and what to report; the engine crate's shims read the parameters,
//! commit and answer in JSON.

use std::collections::BTreeMap;

use ecad_model::{
    DeviceId, Document, IdAllocator, ItemKind, LevelIo, Noun, ObjectRef, Op, PointKey, Representation, RepresentationId, Subject, Terminal,
    TerminalId,
};

use crate::error::CmdError;
use crate::strips::is_fuse_device;
use crate::terminal_types as tt;

// ---------------------------------------------------------------- termlabel (§4.2)

/// Whether `l` is a position label `termlabel` accepts.
fn position_ok(l: &str) -> Result<(), CmdError> {
    if l.is_empty() || l.chars().count() > tt::MAX_POSITION_CHARS || l.chars().any(|c| c.is_whitespace() || c.is_control() || c == ',' || c == '-') {
        return Err(CmdError::refused(format!("A position label is 1 to {} characters, without spaces, commas or dashes.", tt::MAX_POSITION_CHARS)));
    }
    Ok(())
}

/// The ops relabelling `terminals` to `to`: `RemoveTerminal` + `AddTerminal` of each ID.
fn relabel_ops(doc: &Document, terminals: &[TerminalId], to: &str) -> Vec<Op> {
    let mut ops = Vec::new();
    for t in terminals {
        let Some(x) = doc.terminals.get(t) else { continue };
        if x.label.as_deref() == Some(to) {
            continue;
        }
        let mut after = x.clone();
        after.label = Some(to.to_string());
        ops.push(Op::RemoveTerminal { id: *t, terminal: x.clone() });
        ops.push(Op::AddTerminal { id: *t, terminal: after });
    }
    ops
}

/// Where a level moves (`move_to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTo {
    /// The level drawn twice: to the next position of its type that lacks it.
    Next,
    /// Another type's levels: to a new position.
    New,
}

/// What `termlabel` is asked to change (design 036 §4.2).
#[derive(Debug, Clone, Default)]
pub struct TermlabelInput {
    /// A terminal of the position (and its level).
    pub terminal: Option<TerminalId>,
    /// With `at` (or `position`): the strip.
    pub strip: Option<DeviceId>,
    /// The position's current label.
    pub at: Option<String>,
    /// With `strip`: the level (default 1).
    pub level: Option<u64>,
    /// The position's new label (trimmed).
    pub position: Option<String>,
    /// A level's own label (`Some(None)` clears it; trimmed).
    pub text: Option<Option<String>>,
    /// Move the level to another position.
    pub move_to: Option<MoveTo>,
}

/// A planned `termlabel` call.
#[derive(Debug, Clone, Default)]
pub struct Termlabel {
    /// Empty: nothing to change.
    pub ops: Vec<Op>,
    /// The transaction's label.
    pub label: &'static str,
    pub message: String,
    /// The link lines that follow a relabel.
    pub lines: Vec<String>,
}

/// Plan `termlabel` (design 036 §4.2).
///
/// # Errors
/// No such terminal, not a strip, a position or level the strip lacks, a label that does not read.
pub fn plan_termlabel(doc: &Document, input: TermlabelInput) -> Result<Termlabel, CmdError> {
    let TermlabelInput { terminal, strip, at, level, position, text, move_to } = input;
    // Which strip and position: a terminal, or a strip and a position (`at`, else `position`
    // when only `text` is given).
    let (c, pos, n) = match (terminal, strip) {
        (Some(t), _) => {
            let x = doc.terminals.get(&t).ok_or(CmdError::NotFound(Noun::TerminalDevice))?;
            let pos = x.label.clone().ok_or(CmdError::refused("That terminal has no position label."))?;
            (x.component, pos, tt::level_of(doc, t).0)
        }
        (None, Some(c)) => {
            let pos = match (&at, &text) {
                (Some(a), _) => a.clone(),
                (None, Some(_)) => position.clone().ok_or(CmdError::input("Name the position with `at`."))?,
                (None, None) => return Err(CmdError::input("Name the position to relabel with `at` (its current label), or give `terminal`.")),
            };
            let n = level.map_or(1, |l| u32::try_from(l).unwrap_or(u32::MAX));
            (c, pos, n)
        }
        (None, None) => return Err(CmdError::input("`terminal` or `strip` (hex id) is required")),
    };
    if crate::strips::strip_of(doc, c).is_none() {
        return Err(CmdError::refused("That device is not a terminal strip."));
    }
    let tag = crate::devices::tag_words(doc, c);
    let positions = tt::positions(doc, c);
    let here = positions.iter().find(|x| x.label == pos).ok_or_else(|| CmdError::refused(format!("{tag} has no position {pos}.")))?;
    if n == 0 || n > here.ty.count().max(1) && !here.levels.contains_key(&n) {
        return Err(CmdError::refused(format!("{tag}:{pos} has {} level{}.", here.ty.count(), if here.ty.count() == 1 { "" } else { "s" })));
    }
    let mut ops = Vec::new();
    let mut lines = Vec::new();
    let mut message = String::new();
    // Move one level (or another type's levels) to another position (Checks fixes, §8).
    if let Some(m) = move_to {
        let who: Vec<TerminalId> = match (m, terminal) {
            (MoveTo::Next, Some(t)) => {
                // The drawing of `t`: itself, and its out point for a two-point level.
                let mut v = vec![t];
                if here.ty.breaks
                    && let Some(o) = here.levels.get(&n).and_then(|ts| ts.iter().find(|x| **x != t && tt::level_of(doc, **x).1 == Some('o')))
                {
                    v.push(*o);
                }
                v
            }
            (MoveTo::New, Some(t)) => {
                let ty = tt::type_of_terminal(doc, t);
                here.terminals().filter(|x| tt::type_of_terminal(doc, *x) == ty).collect()
            }
            _ => return Err(CmdError::input("`move_to` is \"next\" or \"new\", with `terminal`.")),
        };
        let ty = who.first().map(|t| tt::type_of_terminal(doc, *t)).ok_or(CmdError::refused("Nothing to move."))?;
        let mut chooser = tt::Chooser::new(doc, Some(c));
        let to = match m {
            MoveTo::Next => chooser.choose(doc, Some(c), ty, Some(n), None),
            MoveTo::New => chooser.choose(doc, Some(c), ty, None, None),
        }?;
        ops.extend(relabel_ops(doc, &who, &to));
        message = format!("Moved {tag}:{pos} level {n} to position {to}.");
    } else if let Some(q) = &position
        && q != &pos
    {
        position_ok(q)?;
        if positions.iter().any(|x| &x.label == q) {
            return Err(CmdError::refused(format!("{tag} already has a position {q}. Pick another label, or swap them with Renumber.")));
        }
        let ts: Vec<TerminalId> = here.terminals().collect();
        let rel = relabel_ops(doc, &ts, q);
        let (follow, fl) = crate::accessories::renumber_ops(doc, &rel);
        ops.extend(rel);
        ops.extend(follow);
        lines.extend(fl);
        let levels = here.levels.len();
        message = format!("{tag}:{pos} is now {tag}:{q} ({levels} level{}).", if levels == 1 { "" } else { "s" });
    }
    if let Some(t) = &text {
        // After a relabel the level's key is under the new position.
        let p_now = position.clone().filter(|_| move_to.is_none()).unwrap_or_else(|| pos.clone());
        let key = tt::label_key(&p_now, n);
        if let Some(v) = t
            && (v.is_empty() || v.chars().count() > tt::MAX_LABEL_CHARS || v.chars().any(char::is_control))
        {
            return Err(CmdError::refused(format!("A level's label is 1 to {} characters, with no control characters.", tt::MAX_LABEL_CHARS)));
        }
        let before = doc.devices.get(&c).and_then(|x| x.properties.get(&key)).cloned();
        // A relabel above moved the old key; the new one is empty unless that move wrote it.
        let moved = ops.iter().rev().find_map(|o| match o {
            Op::SetProperty { key: k, after, .. } if *k == key => Some(after.clone()),
            _ => None,
        });
        let before = moved.unwrap_or(before);
        if before != *t {
            ops.push(Op::SetProperty { object: ObjectRef::Device(c), key, before, after: t.clone() });
        }
        let lvl = tt::level_name(doc, c, here.ty, n);
        let words = match t {
            Some(v) => format!("{tag}:{p_now} level {lvl} shows {v}."),
            None => format!("{tag}:{p_now} level {lvl} shows its derived label again."),
        };
        message = if message.is_empty() { words } else { format!("{message} {words}") };
    }
    let label = if position.is_some() && move_to.is_none() { "Relabel position" } else { "Terminal label" };
    Ok(Termlabel { ops, label, message, lines })
}

// ---------------------------------------------------------------- toposition (§3.5)

/// What `toposition` would do: the ops, the strip and position, and what goes with the device.
#[derive(Debug, Clone)]
pub struct ToPosition {
    pub ops: Vec<Op>,
    pub strip: DeviceId,
    pub position: String,
    /// Properties that go with the device: every one but `rating`, which moves to `rating.<p>`.
    pub lost: Vec<String>,
    /// The part number note, when the device's differs from the strip's fuse part.
    pub note: Option<String>,
    pub label: String,
}

impl ToPosition {
    /// The echo after the commit.
    pub fn message(&self) -> String {
        let mut message = format!("{}: the fuse terminal is now a fuse position.", self.label);
        if !self.lost.is_empty() {
            message.push_str(&format!(" Not kept: {}.", self.lost.join(", ")));
        }
        if let Some(n) = &self.note {
            message.push(' ');
            message.push_str(n);
        }
        message
    }
}

/// Plan `toposition` (design 036 §3.5).
///
/// # Errors
/// Not a fuse terminal device, more than one symbol, no strip on its page, an unknown strip.
pub fn plan_toposition(
    doc: &Document, ids: &mut IdAllocator, device: DeviceId, strip: Option<DeviceId>, position: Option<&str>,
) -> Result<ToPosition, CmdError> {
    let comp = doc.devices.get(&device).ok_or(CmdError::NotFound(Noun::Device))?;
    let tag = crate::devices::tag_words(doc, device);
    if crate::strips::strip_of(doc, device).is_some() {
        return Err(CmdError::refused(format!("{tag} is a terminal strip, not a fuse terminal device.")));
    }
    if !is_fuse_device(doc, device) {
        return Err(CmdError::refused(format!(
            "{tag} is not a fuse terminal device. Make strip position turns a fuse terminal (fuseterm) into a strip position."
        )));
    }
    let reps: Vec<(RepresentationId, &Representation)> = doc.representations_of(Subject::Device(device)).collect();
    let [(rid, rep)] = reps.as_slice() else {
        let other = reps.get(1).map(|(r, _)| r.hex().get(..8).unwrap_or_default().to_string()).unwrap_or_default();
        return Err(CmdError::refused(if reps.is_empty() {
            format!("{tag} is not drawn; place it on a page first.")
        } else {
            format!("{tag} has other symbols ({other}); move them first.")
        }));
    };
    let (rid, rep) = (*rid, (*rep).clone());
    let page = rep.page;
    let no_item = || CmdError::refused("The device's symbol has no item.");
    let item = *rep.items.first().ok_or_else(no_item)?;
    let item_before = doc.items.get(&item).cloned().ok_or_else(no_item)?;
    let ItemKind::Symbol(inst) = &item_before.kind else { return Err(CmdError::refused("The device is not drawn as a symbol.")) };
    let here = inst.placement.origin;
    // The strip: named, else the nearest strip with a terminal on this page.
    let strip = match strip {
        Some(c) => {
            if c == device {
                return Err(CmdError::refused(format!("{tag} cannot become a position of itself. Name a terminal strip.")));
            }
            if crate::strips::strip_of(doc, c).is_none() {
                return Err(CmdError::refused(format!("{} is not a terminal strip.", crate::devices::tag_words(doc, c))));
            }
            c
        }
        None => {
            let mut best: Option<(f64, DeviceId)> = None;
            let strips: std::collections::BTreeSet<DeviceId> = doc
                .representations_on(page)
                .filter_map(|(_, r)| match r.subject {
                    Subject::Device(c) if crate::strips::strip_of(doc, c).is_some() => Some(c),
                    _ => None,
                })
                .collect();
            for c in strips {
                for (_, _, pg, at) in crate::strips::drawn_points(doc, c) {
                    let d = at.dist(here);
                    if pg == page && d.is_finite() && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, c));
                    }
                }
            }
            best.map(|(_, c)| c).ok_or(CmdError::refused("Name a strip: there is none on this page."))?
        }
    };
    let stag = crate::devices::tag_words(doc, strip);
    let starter = crate::devices::starter("termfuse")?;
    let ty = tt::by_code("fuse").ok_or(CmdError::refused("no fuse type"))?;
    let mut ops = Vec::new();
    let def = super::terminal::definition_for(doc, ids, starter, &mut ops)?;
    let mut chooser = tt::Chooser::new(doc, Some(strip));
    let pos = chooser.choose(doc, Some(strip), ty, Some(1), position)?;
    let (tin, tout): (TerminalId, TerminalId) = (ids.fresh(), ids.fresh());
    ops.push(Op::AddTerminal {
        id: tin,
        terminal: Terminal {
            component: strip,
            label: Some(pos.clone()),
            key: Some(PointKey::Level { level: 1, io: Some(LevelIo::In) }),
            accepts: Default::default(),
        },
    });
    ops.push(Op::AddTerminal {
        id: tout,
        terminal: Terminal {
            component: strip,
            label: Some(pos.clone()),
            key: Some(PointKey::Level { level: 1, io: Some(LevelIo::Out) }),
            accepts: Default::default(),
        },
    });
    // The device's points by key: `1` → in, `2` → out.
    let old: Vec<(TerminalId, Terminal)> = doc.terminals_of(device).map(|(t, x)| (t, x.clone())).collect();
    let mut moved = std::collections::BTreeMap::new();
    for (t, x) in &old {
        let to = match x.key.as_ref().and_then(PointKey::as_def) {
            Some("1") => tin,
            Some("2") => tout,
            _ => return Err(CmdError::refused(format!("{tag} has a connection point that is not 1 or 2; move its wires first."))),
        };
        moved.insert(*t, to);
    }
    // Its routes land on the strip's points; the commit's rewrite moves the wires.
    ops.extend(crate::wires::repoint_ops(doc, &moved));
    ops.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
    let mut item_after = item_before.clone();
    if let ItemKind::Symbol(si) = &mut item_after.kind {
        si.definition = def;
    }
    ops.push(Op::ModifyItem { id: item, before: item_before, after: item_after });
    let d = starter.definition()?;
    let mut cps = BTreeMap::new();
    for td in &d.terminals {
        cps.insert(if matches!(td.key.level(), Some((_, Some(LevelIo::In)))) { tin } else { tout }, td.at);
    }
    let new_rep: RepresentationId = ids.fresh();
    ops.push(Op::LinkRepresentation {
        id: new_rep,
        rep: Representation {
            subject: Subject::Device(strip),
            page,
            form: rep.form,
            role: ecad_model::Role::FuseTerm,
            items: rep.items.clone(),
            connection_points: cps,
        },
    });
    for (t, x) in &old {
        ops.push(Op::RemoveTerminal { id: *t, terminal: x.clone() });
    }
    let props = comp.properties.clone();
    let keep: [&str; 0] = [];
    // Everything else goes with the device and is listed (review M1: no silent loss), its own
    // text placements included.
    let mut lost: Vec<String> = props.keys().filter(|k| !keep.contains(&k.as_str())).cloned().collect();
    // Design 072 §2.2 and §2.5: the fuse's typed rating becomes the position's. A note is no fuse
    // rating and a position has no note: it goes with the device and is listed.
    let typed = match &comp.device_type {
        ecad_model::DeviceType::Protection(ecad_model::device::Protection::Fuse(f) | ecad_model::device::Protection::FuseTerminal(f))
            if *f != ecad_model::device::Fuse::default() =>
        {
            Some(f)
        }
        _ => None,
    };
    let label = ecad_model::device::PositionLabel::new(&pos).ok();
    let sd = doc.devices.get(&strip);
    if let (Some(f), Some(label), Some(sd)) = (typed, label, sd)
        && let ecad_model::DeviceType::TerminalStrip(s) = &sd.device_type
    {
        let mut s = s.clone();
        s.fuses.insert(label, f.clone());
        let after = ecad_model::Device { device_type: ecad_model::DeviceType::TerminalStrip(s), ..sd.clone() };
        ops.push(Op::ModifyDevice { id: strip, before: sd.clone(), after });
    } else if comp.rating_shown().is_some() {
        lost.push(ecad_model::device::RATING_KEY.into());
    }
    // Design 072 §2.1: the tag is a field of the device, and goes with it too.
    if comp.tag.is_some() {
        lost.push(ecad_model::tag::TAG_KEY.into());
    }
    lost.extend(ecad_model::TextField::ALL.into_iter().filter(|f| comp.text(*f).is_some()).map(|f| f.key().to_string()));
    lost.sort();
    let strip_part = doc.devices.get(&strip).and_then(|x| x.properties.get(&crate::strips::type_key(ty, "part_number"))).cloned();
    let note = match (comp.part_number.as_ref().map(|t| t.to_string()), &strip_part) {
        (Some(a), Some(b)) if a != *b => {
            Some(format!("{tag}'s part {a} differs from {stag}'s fuse terminal part {b}; set {stag}'s part if {a} is right."))
        }
        _ => None,
    };
    ops.extend(crate::cmd::device_link::unlink_ops(doc, device));
    ops.push(Op::DeleteDevice { id: device, device: comp.clone() });
    Ok(ToPosition { ops, strip, position: pos.clone(), lost, note, label: format!("{tag} \u{2192} {stag}:{pos}") })
}
