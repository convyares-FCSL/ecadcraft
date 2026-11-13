//! The commands of brief 036 (design 036 §3.5, §4.2): `termlabel` (relabel a position, set or
//! clear one level's own label, move a level to another position) and `toposition` (turn a brief
//! 034 fuse terminal device into a fuse position of a strip).
//!
//! Every write is one transaction through [`crate::project::Project::apply_edit`]; refusals say
//! what to do instead and write nothing.

use std::collections::BTreeMap;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, Document, ItemKind, ObjectRef, Op, Representation, RepresentationId, Subject, Terminal, TerminalId};
use serde_json::{Value, json};

use crate::commands::{err, opt_id, opt_typed, with_project};
use crate::terminal_types as tt;

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("termlabel", "Terminal Label", run_termlabel)
            .params(
                "{terminal?: id | strip?: id + at?: position, level?: n, position?: new position label (relabels every level), \
text?: a level's own label (null clears it), move_to?: \"next\" | \"new\" (a level drawn twice, or another type's levels)} \
(design 036 §4.2; one transaction)",
            )
            .noundo(),
        CommandSpec::new("toposition", "Make Strip Position", run_toposition)
            .params(
                "{device: id (a fuse terminal device), strip?: id (default: the nearest strip on its page), position?: label} \
(the device becomes a fuse position of the strip, wires and rating kept; one transaction)",
            )
            .noundo(),
    ]
}

// ---------------------------------------------------------------- termlabel (§4.2)

/// Whether `l` is a position label `termlabel` accepts.
fn position_ok(l: &str) -> Result<(), String> {
    if l.is_empty() || l.chars().count() > tt::MAX_POSITION_CHARS || l.chars().any(|c| c.is_whitespace() || c.is_control() || c == ',' || c == '-') {
        return Err(format!("A position label is 1 to {} characters, without spaces, commas or dashes.", tt::MAX_POSITION_CHARS));
    }
    Ok(())
}

/// The ops relabelling `terminals` of strip `c` to `to`, with the keys and links that follow them
/// (FP7): `RemoveTerminal` + `AddTerminal` of each ID, then [`crate::accessories::renumber_ops`].
fn relabel_ops(doc: &Document, terminals: &[TerminalId], to: &str) -> (Vec<Op>, Vec<String>) {
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
    (ops, Vec::new())
}

fn run_termlabel(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let terminal: Option<TerminalId> = opt_id(params, "terminal").map_err(err)?;
    let strip: Option<ComponentId> = opt_id(params, "strip").map_err(err)?;
    let at = opt_typed(params, "at", "a position label", Value::as_str).map_err(err)?.map(str::to_string);
    let level = opt_typed(params, "level", "a level number", Value::as_u64).map_err(err)?;
    let position = opt_typed(params, "position", "a position label", Value::as_str).map_err(err)?.map(|x| x.trim().to_string());
    let move_to = opt_typed(params, "move_to", "\"next\" or \"new\"", Value::as_str).map_err(err)?.map(str::to_string);
    // `text`: a string sets, `null` clears, absent leaves it.
    let text: Option<Option<String>> = match params.get("text") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(t)) => Some(Some(t.trim().to_string())),
        Some(_) => return Err(err("`text` must be a string, or null to clear the level's own label")),
    };
    let v = with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        // Which strip and position: a terminal, or a strip and a position (`at`, else `position`
        // when only `text` is given).
        let (c, pos, n) = match (terminal, strip) {
            (Some(t), _) => {
                let x = doc.terminals.get(&t).ok_or("No such terminal.")?;
                let pos = x.label.clone().ok_or("That terminal has no position label.")?;
                (x.component, pos, tt::level_of(doc, t).0)
            }
            (None, Some(c)) => {
                let pos = match (&at, &text) {
                    (Some(a), _) => a.clone(),
                    (None, Some(_)) => position.clone().ok_or("Name the position with `at`.")?,
                    (None, None) => return Err("Name the position to relabel with `at` (its current label), or give `terminal`.".into()),
                };
                let n = level.map_or(1, |l| u32::try_from(l).unwrap_or(u32::MAX));
                (c, pos, n)
            }
            (None, None) => return Err("`terminal` or `strip` (hex id) is required".into()),
        };
        if !crate::strips::is_strip(doc, c) {
            return Err("That device is not a terminal strip.".into());
        }
        let tag = crate::devices::tag_words(doc, c);
        let positions = tt::positions(doc, c);
        let here = positions.iter().find(|x| x.label == pos).ok_or_else(|| format!("{tag} has no position {pos}."))?;
        if n == 0 || n > here.ty.count().max(1) && !here.levels.contains_key(&n) {
            return Err(format!("{tag}:{pos} has {} level{}.", here.ty.count(), if here.ty.count() == 1 { "" } else { "s" }));
        }
        let mut ops = Vec::new();
        let mut lines = Vec::new();
        let mut message = String::new();
        // Move one level (or another type's levels) to another position (Checks fixes, §8).
        if let Some(m) = &move_to {
            let who: Vec<TerminalId> = match (m.as_str(), terminal) {
                ("next", Some(t)) => {
                    // The drawing of `t`: itself, and its out point for a two-point level.
                    let mut v = vec![t];
                    if here.ty.breaks
                        && let Some(o) = here.levels.get(&n).and_then(|ts| ts.iter().find(|x| **x != t && tt::level_of(doc, **x).1 == Some('o')))
                    {
                        v.push(*o);
                    }
                    v
                }
                ("new", Some(t)) => {
                    let ty = tt::type_of_terminal(doc, t);
                    here.terminals().filter(|x| tt::type_of_terminal(doc, *x) == ty).collect()
                }
                _ => return Err("`move_to` is \"next\" or \"new\", with `terminal`.".into()),
            };
            let ty = who.first().map(|t| tt::type_of_terminal(doc, *t)).ok_or("Nothing to move.")?;
            let mut chooser = tt::Chooser::new(doc, Some(c));
            let to = if m == "next" { chooser.choose(doc, Some(c), ty, Some(n), None)? } else { chooser.choose(doc, Some(c), ty, None, None)? };
            ops.extend(relabel_ops(doc, &who, &to).0);
            message = format!("Moved {tag}:{pos} level {n} to position {to}.");
        } else if let Some(q) = &position {
            if q != &pos {
                position_ok(q)?;
                if positions.iter().any(|x| &x.label == q) {
                    return Err(format!("{tag} already has a position {q}. Pick another label, or swap them with Renumber."));
                }
                let ts: Vec<TerminalId> = here.terminals().collect();
                let (rel, _) = relabel_ops(doc, &ts, q);
                let (follow, fl) = crate::accessories::renumber_ops(doc, &rel);
                ops.extend(rel);
                ops.extend(follow);
                lines.extend(fl);
                let levels = here.levels.len();
                message = format!("{tag}:{pos} is now {tag}:{q} ({levels} level{}).", if levels == 1 { "" } else { "s" });
            }
        }
        if let Some(t) = &text {
            // After a relabel the level's key is under the new position.
            let p_now = position.clone().filter(|_| move_to.is_none()).unwrap_or_else(|| pos.clone());
            let key = tt::label_key(&p_now, n);
            if let Some(v) = t
                && (v.is_empty() || v.chars().count() > tt::MAX_LABEL_CHARS || v.chars().any(char::is_control))
            {
                return Err(format!("A level's label is 1 to {} characters, with no control characters.", tt::MAX_LABEL_CHARS));
            }
            let before = doc.components.get(&c).and_then(|x| x.properties.get(&key)).cloned();
            // A relabel above moved the old key; the new one is empty unless that move wrote it.
            let moved = ops.iter().rev().find_map(|o| match o {
                Op::SetProperty { key: k, after, .. } if *k == key => Some(after.clone()),
                _ => None,
            });
            let before = moved.unwrap_or(before);
            if before != *t {
                ops.push(Op::SetProperty { object: ObjectRef::Component(c), key, before, after: t.clone() });
            }
            let lvl = tt::level_name(doc, c, here.ty, n);
            let words = match t {
                Some(v) => format!("{tag}:{p_now} level {lvl} shows {v}."),
                None => format!("{tag}:{p_now} level {lvl} shows its derived label again."),
            };
            message = if message.is_empty() { words } else { format!("{message} {words}") };
        }
        if ops.is_empty() {
            return Ok(json!({"changed": false, "message": "Nothing to change."}));
        }
        let label = if position.is_some() && move_to.is_none() { "Relabel position" } else { "Terminal label" };
        p.apply_edit(pg, "termlabel", label, ops)?;
        Ok(json!({"changed": true, "message": message, "lines": lines}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- toposition (§3.5)

/// The starter key of a fuse terminal device (brief 034).
pub const FUSE_DEVICE: &str = "fuseterm";

/// Whether `c` is a brief 034 fuse terminal device (its own definition is `fuseterm`).
pub fn is_fuse_device(doc: &Document, c: ComponentId) -> bool {
    doc.components
        .get(&c)
        .and_then(|x| x.definition)
        .and_then(|d| doc.definitions.get(&d))
        .and_then(crate::symbols::of_definition)
        .is_some_and(|s| s.key == FUSE_DEVICE)
}

/// What `toposition` would do: the ops, the strip and position, and what goes with the device.
pub struct ToPosition {
    pub ops: Vec<Op>,
    pub strip: ComponentId,
    pub position: String,
    /// Properties that go with the device (not `tag`, `description`, `rating`, `part_number`,
    /// `maker`).
    pub lost: Vec<String>,
    /// The part number note, when the device's differs from the strip's fuse part.
    pub note: Option<String>,
    pub label: String,
}

/// Plan `toposition` (design 036 §3.5).
///
/// # Errors
/// Not a fuse terminal device, more than one symbol, no strip on its page, an unknown strip.
pub fn plan_toposition(
    doc: &Document, ids: &mut ecad_model::IdAllocator, device: ComponentId, strip: Option<ComponentId>, position: Option<&str>,
) -> Result<ToPosition, String> {
    let comp = doc.components.get(&device).ok_or("No such device.")?;
    let tag = crate::devices::tag_words(doc, device);
    if crate::strips::is_strip(doc, device) {
        return Err(format!("{tag} is a terminal strip, not a fuse terminal device."));
    }
    if !is_fuse_device(doc, device) {
        return Err(format!("{tag} is not a fuse terminal device. Make strip position turns a fuse terminal (fuseterm) into a strip position."));
    }
    let reps: Vec<(RepresentationId, &Representation)> = doc.representations_of(Subject::Component(device)).collect();
    let [(rid, rep)] = reps.as_slice() else {
        let other = reps.get(1).map(|(r, _)| r.hex().get(..8).unwrap_or_default().to_string()).unwrap_or_default();
        return Err(if reps.is_empty() {
            format!("{tag} is not drawn; place it on a page first.")
        } else {
            format!("{tag} has other symbols ({other}); move them first.")
        });
    };
    let (rid, rep) = (*rid, (*rep).clone());
    let page = rep.page;
    let item = *rep.items.first().ok_or("The device's symbol has no item.")?;
    let item_before = doc.items.get(&item).cloned().ok_or("The device's symbol has no item.")?;
    let ItemKind::Symbol(inst) = &item_before.kind else { return Err("The device is not drawn as a symbol.".into()) };
    let here = inst.placement.origin;
    // The strip: named, else the nearest strip with a terminal on this page.
    let strip = match strip {
        Some(c) => {
            if c == device {
                return Err(format!("{tag} cannot become a position of itself. Name a terminal strip."));
            }
            if !crate::strips::is_strip(doc, c) {
                return Err(format!("{} is not a terminal strip.", crate::devices::tag_words(doc, c)));
            }
            c
        }
        None => {
            let mut best: Option<(f64, ComponentId)> = None;
            let strips: std::collections::BTreeSet<ComponentId> = doc
                .representations_on(page)
                .filter_map(|(_, r)| match r.subject {
                    Subject::Component(c) if crate::strips::is_strip(doc, c) => Some(c),
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
            best.map(|(_, c)| c).ok_or("Name a strip: there is none on this page.")?
        }
    };
    let stag = crate::devices::tag_words(doc, strip);
    let starter = crate::devices::starter("termfuse")?;
    let ty = tt::by_code("fuse").ok_or("no fuse type")?;
    let mut ops = Vec::new();
    let def = crate::terminal_cmds::definition_for(doc, ids, starter, &mut ops)?;
    let mut chooser = tt::Chooser::new(doc, Some(strip));
    let pos = chooser.choose(doc, Some(strip), ty, Some(1), position)?;
    let (tin, tout): (TerminalId, TerminalId) = (ids.fresh(), ids.fresh());
    ops.push(Op::AddTerminal { id: tin, terminal: Terminal { component: strip, label: Some(pos.clone()), key: Some(".1i".into()) } });
    ops.push(Op::AddTerminal { id: tout, terminal: Terminal { component: strip, label: Some(pos.clone()), key: Some(".1o".into()) } });
    // The device's points by key: `1` → in, `2` → out.
    let old: Vec<(TerminalId, Terminal)> = doc.terminals_of(device).map(|(t, x)| (t, x.clone())).collect();
    for (t, x) in &old {
        let to = match x.key.as_deref() {
            Some("1") => tin,
            Some("2") => tout,
            _ => return Err(format!("{tag} has a connection point that is not 1 or 2; move its wires first.")),
        };
        for (w, end) in doc.ends_at(*t) {
            ops.push(Op::AttachEnd { conductor: w, end, before: Some(*t), after: Some(to) });
        }
    }
    ops.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
    let mut item_after = item_before.clone();
    if let ItemKind::Symbol(si) = &mut item_after.kind {
        si.definition = def;
    }
    ops.push(Op::ModifyItem { id: item, before: item_before, after: item_after });
    let d = starter.definition()?;
    let mut cps = BTreeMap::new();
    for td in &d.terminals {
        cps.insert(if td.key.ends_with('i') { tin } else { tout }, td.at);
    }
    let new_rep: RepresentationId = ids.fresh();
    ops.push(Op::LinkRepresentation {
        id: new_rep,
        rep: Representation {
            subject: Subject::Component(strip),
            page,
            form: rep.form,
            role: "termfuse".into(),
            items: rep.items.clone(),
            connection_points: cps,
        },
    });
    for (t, x) in &old {
        ops.push(Op::RemoveTerminal { id: *t, terminal: x.clone() });
    }
    let props = comp.properties.clone();
    if let Some(r) = props.get("rating").filter(|r| !r.trim().is_empty()) {
        let key = tt::rating_key(&pos);
        let before = doc.components.get(&strip).and_then(|x| x.properties.get(&key)).cloned();
        ops.push(Op::SetProperty { object: ObjectRef::Component(strip), key, before, after: Some(r.clone()) });
    }
    let keep = ["tag", "description", "rating", "part_number", "maker"];
    let lost: Vec<String> = props.keys().filter(|k| !keep.contains(&k.as_str()) && !k.starts_with("text.")).cloned().collect();
    let strip_part = doc.components.get(&strip).and_then(|x| x.properties.get(&crate::strips::type_key(ty, "part_number"))).cloned();
    let note = match (props.get("part_number"), &strip_part) {
        (Some(a), Some(b)) if a != b => {
            Some(format!("{tag}'s part {a} differs from {stag}'s fuse terminal part {b}; set {stag}'s part if {a} is right."))
        }
        _ => None,
    };
    ops.push(Op::DeleteComponent { id: device, component: comp.clone() });
    Ok(ToPosition { ops, strip, position: pos.clone(), lost, note, label: format!("{tag} \u{2192} {stag}:{pos}") })
}

fn run_toposition(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let device: ComponentId = crate::commands::id(params, "device").map_err(err)?;
    let strip: Option<ComponentId> = opt_id(params, "strip").map_err(err)?;
    let position = opt_typed(params, "position", "a position label", Value::as_str).map_err(err)?.map(str::to_string);
    let v = with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        let plan = plan_toposition(doc, &mut p.ids, device, strip, position.as_deref())?;
        let label = plan.label.clone();
        p.apply_edit(pg, "toposition", &label, plan.ops)?;
        let mut message = format!("{label}: the fuse terminal is now a fuse position.");
        if !plan.lost.is_empty() {
            message.push_str(&format!(" Not kept: {}.", plan.lost.join(", ")));
        }
        if let Some(n) = &plan.note {
            message.push(' ');
            message.push_str(n);
        }
        Ok(json!({"changed": true, "strip": plan.strip.hex(), "position": plan.position, "lost": plan.lost, "note": plan.note, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}
