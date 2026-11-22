//! The rules of brief 036's commands (design 036 §3.5, §4.2): `terminalblocklabel` (relabel a position, set
//! or clear one level's own label, move a level to another position) and `toposition` (turn a brief
//! 034 fuse terminal device into a fuse position of a strip). Each function reads the document and
//! returns the ops to commit and what to report; both commands are generic ([`crate::commands::run`]).

use std::collections::BTreeMap;

use ecad_model::{
    ConnectionPoint, ConnectionPointId, DeviceId, Document, IdAllocator, ItemKind, LevelIo, Noun, ObjectRef, Op, PointKey, Representation,
    RepresentationId, Subject,
};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::commands::input::nullable;
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;
use crate::terminal_strips::terminal_blocks as tt;

// ---------------------------------------------------------------- terminalblocklabel (§4.2)

/// Whether `l` is a position label `terminalblocklabel` accepts.
fn position_ok(l: &str) -> Result<(), CmdError> {
    if l.is_empty() || l.chars().count() > tt::MAX_POSITION_CHARS || l.chars().any(|c| c.is_whitespace() || c.is_control() || c == ',' || c == '-') {
        return Err(CmdError::refused(format!("A position label is 1 to {} characters, without spaces, commas or dashes.", tt::MAX_POSITION_CHARS)));
    }
    Ok(())
}

/// The ops relabelling `connection_points` to `to`: `RemoveConnectionPoint` + `AddConnectionPoint` of each ID.
fn relabel_ops(doc: &Document, connection_points: &[ConnectionPointId], to: &str) -> Vec<Op> {
    let mut ops = Vec::new();
    for t in connection_points {
        let Some(x) = doc.connection_points().get(t) else { continue };
        if x.label.as_deref() == Some(to) {
            continue;
        }
        let mut after = x.clone();
        after.label = Some(to.to_string());
        ops.push(Op::RemoveConnectionPoint { id: *t, connection_point: x.clone() });
        ops.push(Op::AddConnectionPoint { id: *t, connection_point: after });
    }
    ops
}

/// Where a level moves (`move_to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MoveTo {
    /// The level drawn twice: to the next position of its type that lacks it.
    Next,
    /// Another type's levels: to a new position.
    New,
}

/// What `terminalblocklabel` is asked to change (design 036 §4.2).
#[derive(Debug, Clone, Default)]
pub(crate) struct TerminalblocklabelInput {
    /// A terminal of the position (and its level).
    pub connection_point: Option<ConnectionPointId>,
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

/// A planned `terminalblocklabel` call.
#[derive(Debug, Clone, Default)]
pub(crate) struct Terminalblocklabel {
    /// Empty: nothing to change.
    pub ops: Vec<Op>,
    /// The transaction's label.
    pub label: &'static str,
    pub message: String,
    /// The link lines that follow a relabel.
    pub lines: Vec<String>,
}

/// Plan `terminalblocklabel` (design 036 §4.2).
///
/// # Errors
/// No such terminal, not a strip, a position or level the strip lacks, a label that does not read.
pub(crate) fn plan_terminal_block_label(doc: &Document, input: TerminalblocklabelInput) -> Result<Terminalblocklabel, CmdError> {
    let TerminalblocklabelInput { connection_point, strip, at, level, position, text, move_to } = input;
    // Which strip and position: a terminal, or a strip and a position (`at`, else `position`
    // when only `text` is given).
    let (c, pos, n) = match (connection_point, strip) {
        (Some(t), _) => {
            let x = doc.connection_points().get(&t).ok_or(CmdError::NotFound(Noun::TerminalBlock))?;
            let pos = x.label.clone().ok_or(CmdError::refused("That terminal has no position label."))?;
            (x.device().ok_or(CmdError::NotFound(Noun::TerminalBlock))?, pos, tt::level_of(doc, t).0)
        }
        (None, Some(c)) => {
            let pos = match (&at, &text) {
                (Some(a), _) => a.clone(),
                (None, Some(_)) => position.clone().ok_or(CmdError::input("Name the position with `at`."))?,
                (None, None) => {
                    return Err(CmdError::input("Name the position to relabel with `at` (its current label), or give `connection_point`."));
                }
            };
            let n = level.map_or(1, |l| u32::try_from(l).unwrap_or(u32::MAX));
            (c, pos, n)
        }
        (None, None) => return Err(CmdError::input("`connection_point` or `strip` (hex id) is required")),
    };
    if crate::terminal_strips::strip_of(doc, c).is_none() {
        return Err(CmdError::refused("That device is not a terminal strip."));
    }
    let tag = crate::devices::tags::tag_words(doc, c);
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
        let who: Vec<ConnectionPointId> = match (m, connection_point) {
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
                let ty = tt::type_of_connection_point(doc, t);
                here.connection_points().filter(|x| tt::type_of_connection_point(doc, *x) == ty).collect()
            }
            _ => return Err(CmdError::input("`move_to` is \"next\" or \"new\", with `connection_point`.")),
        };
        let ty = who.first().map(|t| tt::type_of_connection_point(doc, *t)).ok_or(CmdError::refused("Nothing to move."))?;
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
        let ts: Vec<ConnectionPointId> = here.connection_points().collect();
        let rel = relabel_ops(doc, &ts, q);
        let (follow, fl) = crate::terminal_strips::accessories::renumber_ops(doc, &rel);
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
        let before = doc.devices().get(&c).and_then(|x| x.properties.get(&key)).cloned();
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
    Ok(Terminalblocklabel { ops, label, message, lines })
}

// ---------------------------------------------------------------- toposition (§3.5)

/// What `toposition` would do: the ops, the strip and position, and what goes with the device.
#[derive(Debug, Clone)]
pub(crate) struct ToPosition {
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
pub(crate) fn plan_toposition(
    doc: &Document, ids: &mut IdAllocator, device: DeviceId, strip: Option<DeviceId>, position: Option<&str>,
) -> Result<ToPosition, CmdError> {
    let comp = doc.devices().get(&device).ok_or(CmdError::NotFound(Noun::Device))?;
    let tag = crate::devices::tags::tag_words(doc, device);
    if crate::terminal_strips::strip_of(doc, device).is_some() {
        return Err(CmdError::refused(format!("{tag} is a terminal strip, not a fuse terminal device.")));
    }
    if crate::terminal_strips::fuse_terminal_block_of(doc, device).is_none() {
        return Err(CmdError::refused(format!(
            "{tag} is not a fuse terminal device. Make strip position turns a fuse terminal (fuseterminalblock) into a strip position."
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
    let item_before = doc.items().get(&item).cloned().ok_or_else(no_item)?;
    let ItemKind::Symbol(inst) = &item_before.kind else { return Err(CmdError::refused("The device is not drawn as a symbol.")) };
    let here = inst.placement.origin;
    // The strip: named, else the nearest strip with a terminal on this page.
    let strip = match strip {
        Some(c) => {
            if c == device {
                return Err(CmdError::refused(format!("{tag} cannot become a position of itself. Name a terminal strip.")));
            }
            if crate::terminal_strips::strip_of(doc, c).is_none() {
                return Err(CmdError::refused(format!("{} is not a terminal strip.", crate::devices::tags::tag_words(doc, c))));
            }
            c
        }
        None => {
            let mut best: Option<(f64, DeviceId)> = None;
            let strips: std::collections::BTreeSet<DeviceId> = doc
                .representations_on(page)
                .filter_map(|(_, r)| match r.subject {
                    Subject::Device(c) if crate::terminal_strips::strip_of(doc, c).is_some() => Some(c),
                    _ => None,
                })
                .collect();
            for c in strips {
                for (_, _, pg, at) in crate::terminal_strips::drawn_points(doc, c) {
                    let d = at.dist(here);
                    if pg == page && d.is_finite() && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, c));
                    }
                }
            }
            best.map(|(_, c)| c).ok_or(CmdError::refused("Name a strip: there is none on this page."))?
        }
    };
    let stag = crate::devices::tags::tag_words(doc, strip);
    let starter = crate::devices::json::starter("terminalblockfuse")?;
    let ty = tt::by_code("fuse").ok_or(CmdError::refused("no fuse type"))?;
    let mut ops = Vec::new();
    let def = crate::terminal_strips::commands::terminal_strips::definition_for(doc, ids, starter, &mut ops)?;
    let mut chooser = tt::Chooser::new(doc, Some(strip));
    let pos = chooser.choose(doc, Some(strip), ty, Some(1), position)?;
    let (tin, tout): (ConnectionPointId, ConnectionPointId) = (ids.fresh(), ids.fresh());
    ops.push(Op::AddConnectionPoint {
        id: tin,
        connection_point: ConnectionPoint {
            owner: strip.into(),
            label: Some(pos.clone()),
            key: Some(PointKey::Level { level: 1, io: Some(LevelIo::In) }),
            accepts: Default::default(),
        },
    });
    ops.push(Op::AddConnectionPoint {
        id: tout,
        connection_point: ConnectionPoint {
            owner: strip.into(),
            label: Some(pos.clone()),
            key: Some(PointKey::Level { level: 1, io: Some(LevelIo::Out) }),
            accepts: Default::default(),
        },
    });
    // The device's points by key: `1` → in, `2` → out.
    let old: Vec<(ConnectionPointId, ConnectionPoint)> = doc.connection_points_of(device).map(|(t, x)| (t, x.clone())).collect();
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
    ops.extend(crate::wires::routes::repoint_ops(doc, &moved));
    ops.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
    let mut item_after = item_before.clone();
    if let ItemKind::Symbol(si) = &mut item_after.kind {
        si.definition = def;
    }
    ops.push(Op::ModifyItem { id: item, before: item_before, after: item_after });
    let d = starter.definition()?;
    let mut cps = BTreeMap::new();
    for td in &d.connection_points {
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
        ops.push(Op::RemoveConnectionPoint { id: *t, connection_point: x.clone() });
    }
    let props = comp.properties.clone();
    let keep: [&str; 0] = [];
    // Everything else goes with the device and is listed (review M1: no silent loss), its own
    // text placements included.
    let mut lost: Vec<String> = props.keys().filter(|k| !keep.contains(&k.as_str())).cloned().collect();
    // Design 072 §2.2 and §2.5: the fuse's typed rating becomes the position's. A note is no fuse
    // rating and a position has no note: it goes with the device and is listed.
    let typed = match &comp.device_type {
        ecad_model::DeviceType::Protection(
            ecad_model::objects::devices::Protection::Fuse(f) | ecad_model::objects::devices::Protection::FuseTerminalBlock(f),
        ) if *f != ecad_model::objects::devices::Fuse::default() => Some(f),
        _ => None,
    };
    let label = ecad_model::objects::devices::PositionLabel::new(&pos).ok();
    let sd = doc.devices().get(&strip);
    if let (Some(f), Some(label), Some(sd)) = (typed, label, sd)
        && let ecad_model::DeviceType::TerminalStrip(s) = &sd.device_type
    {
        let mut s = s.clone();
        s.fuses.insert(label, f.clone());
        let after = ecad_model::Device { device_type: ecad_model::DeviceType::TerminalStrip(s), ..sd.clone() };
        ops.push(Op::ModifyDevice { id: strip, before: sd.clone(), after });
    } else if comp.rating_shown().is_some() {
        lost.push(ecad_model::objects::devices::RATING_KEY.into());
    }
    // Design 072 §2.1: the tag is a field of the device, and goes with it too.
    if !comp.tag.is_none() {
        lost.push(ecad_model::naming::tag::TAG_KEY.into());
    }
    lost.extend(ecad_model::TextField::ALL.into_iter().filter(|f| comp.text(*f).is_some()).map(|f| f.key().to_string()));
    lost.sort();
    let strip_part =
        doc.devices().get(&strip).and_then(|x| x.properties.get(&crate::terminal_strips::strip_model::type_key(ty, "part_number"))).cloned();
    let note = match (comp.part_number.as_ref().map(|t| t.to_string()), &strip_part) {
        (Some(a), Some(b)) if a != *b => {
            Some(format!("{tag}'s part {a} differs from {stag}'s fuse terminal part {b}; set {stag}'s part if {a} is right."))
        }
        _ => None,
    };
    ops.extend(crate::devices::commands::device_links::unlink_ops(doc, device));
    ops.push(Op::DeleteDevice { id: device, device: comp.clone() });
    Ok(ToPosition { ops, strip, position: pos.clone(), lost, note, label: format!("{tag} \u{2192} {stag}:{pos}") })
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// Where a level moves.
#[derive(Deserialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum MoveToParam {
    /// The next position of its type that lacks the level (a level drawn twice).
    Next,
    /// A new position (another type's levels).
    New,
}

/// `terminalblocklabel {terminal | strip + at, level?, position?, text?, move_to?}` (design 036 §4.2):
/// relabel a position, set or clear one level's own label, or move a level to another position.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalblocklabelParams {
    /// A terminal of the position (and its level).
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
    /// The strip (with `at`).
    #[serde(default)]
    strip: Option<DeviceId>,
    /// The position's current label.
    #[serde(default)]
    at: Option<String>,
    /// With `strip`: the level (default 1).
    #[serde(default)]
    level: Option<u64>,
    /// The position's new label (relabels every level).
    #[serde(default)]
    position: Option<String>,
    /// A level's own label; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    text: Option<Option<String>>,
    /// "next" or "new", with `connection_point`: move the level to another position.
    #[serde(default)]
    move_to: Option<MoveToParam>,
}

impl Generic for TerminalblocklabelParams {
    const ID: &'static str = "terminalblocklabel";
    const TITLE: &'static str = "Terminal Block Label";
    const HELP: &'static str = "{connection_point?: id | strip?: id + at?: position, level?: n, position?: new position label (relabels every level), text?: a level's own label (null clears it), move_to?: \"next\" | \"new\" (a level drawn twice, or another type's levels)} (design 036 §4.2; one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let input = TerminalblocklabelInput {
            connection_point: self.connection_point,
            strip: self.strip,
            at: self.at,
            level: self.level,
            position: self.position.map(|x| x.trim().to_string()),
            text: self.text.map(|t| t.map(|t| t.trim().to_string())),
            move_to: self.move_to.map(|m| match m {
                MoveToParam::Next => MoveTo::Next,
                MoveToParam::New => MoveTo::New,
            }),
        };
        let plan = plan_terminal_block_label(cx.doc, input)?;
        if plan.ops.is_empty() {
            return Ok(Outcome::answer(json!({"changed": false, "message": "Nothing to change."})).echo(Some("Nothing to change.")));
        }
        let answer = json!({"changed": true, "message": plan.message, "lines": plan.lines});
        Ok(Outcome::edit(plan.label, plan.ops, answer).echo(Some(plan.message)))
    }
}

// ---------------------------------------------------------------- toposition (§3.5)

/// `toposition {device, strip?, position?}` (design 036 §3.5): the fuse terminal device becomes a
/// fuse position of the strip, wires and rating kept; one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TopositionParams {
    /// The fuse terminal device.
    device: DeviceId,
    /// The strip (default: the nearest strip on its page).
    #[serde(default)]
    strip: Option<DeviceId>,
    /// The position's label (default the next free one).
    #[serde(default)]
    position: Option<String>,
}

impl Generic for TopositionParams {
    const ID: &'static str = "toposition";
    const TITLE: &'static str = "Make Strip Position";
    const HELP: &'static str = "{device: id (a fuse terminal device), strip?: id (default: the nearest strip on its page), position?: label} (the device becomes a fuse position of the strip, wires and rating kept; one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let plan = plan_toposition(cx.doc, cx.ids, self.device, self.strip, self.position.as_deref())?;
        let message = plan.message();
        let answer =
            json!({"changed": true, "strip": plan.strip.hex(), "position": plan.position, "lost": plan.lost, "note": plan.note, "message": message});
        Ok(Outcome::edit(plan.label, plan.ops, answer).echo(Some(message)))
    }
}
