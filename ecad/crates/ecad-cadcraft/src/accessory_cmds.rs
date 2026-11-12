//! The commands of brief 030 (design 030 §2.2, §5.4, §5.5, §10): `accessory` places end stops,
//! end plates, partitions, strip labels and markers; `jumper` makes and edits links. Both write
//! ordinary strip properties, one transaction per call; a link edit's transaction also carries
//! the numbering pass's wire-number ops (FP7, through [`crate::project::Project::apply_edit`]).

use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::links::{self, Positions};
use ecad_model::{ComponentId, Document, ObjectRef, Op, TerminalId};
use serde_json::{Value, json};

use crate::accessories::{self as acc, Acc, Spot};
use crate::commands::{err, id_list, opt_id, with_project};

/// Most places one `accessory` call writes.
pub const MAX_AT: usize = 999;
/// Most terminals one `jumper` call picks.
pub const MAX_PICKS: usize = 999;

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("accessory", "Strip Accessory", run_accessory)
            .params(
                "{strip?: id, terminal?: id, at: \"start\"|\"end\"|\"all\"|label|[label…] (at most 999), on?: bool (markers on positions), \
items?: [item…]|null (replace), add?: [item…], remove?: [item…], clear?: [acc.… key]} items: end_stop, end_plate, partition, strip_label; on: marker",
            )
            .interactive(crate::interact::accessory)
            .noundo(),
        CommandSpec::new("jumper", "Jumper", run_jumper)
            .params(
                "make: {strip?: id, terminals?: [id…], positions?: \"1-4, 7\", skip?: bool} (ranges between consecutive picks unless skip); \
edit: {link: {strip: id, n}, add?: \"5,6\", remove?: \"5\", positions?: text, split_at?: label, drop_missing?: true, delete?: true}",
            )
            .interactive(crate::interact::jumper)
            .noundo(),
    ]
}

fn strip_words(doc: &Document, c: ComponentId) -> String {
    crate::devices::tag_words(doc, c)
}

/// The strip a call names (`strip`, or `terminal`'s), and the terminal's label.
fn strip_of(doc: &Document, strip: Option<ComponentId>, terminal: Option<TerminalId>) -> Result<(ComponentId, Option<String>), String> {
    let (c, label) = match (strip, terminal) {
        (Some(c), _) => (c, None),
        (None, Some(t)) => {
            let term = doc.terminals.get(&t).ok_or("No such terminal.")?;
            (term.component, Some(crate::interact::terminal_name(doc, t)))
        }
        (None, None) => return Err("`strip` or `terminal` (hex id) is required".into()),
    };
    if !doc.components.contains_key(&c) {
        return Err("No such strip.".into());
    }
    if !crate::strips::is_strip(doc, c) {
        return Err(format!("{} is not a terminal strip. Pick a terminal of the strip.", strip_words(doc, c)));
    }
    Ok((c, label))
}

/// `-X01 has no position 15; it has 1…12. Pick one of those, or start/end.`
fn no_position(doc: &Document, c: ComponentId, label: &str, order: &[String]) -> String {
    let has = match (order.first(), order.last()) {
        (Some(a), Some(b)) if a != b => format!("{a}\u{2026}{b}"),
        (Some(a), _) => a.clone(),
        _ => "no positions".into(),
    };
    let shown: String = label.chars().take(16).collect();
    format!("{} has no position {shown}; it has {has}. Pick one of those, or start/end.", strip_words(doc, c))
}

fn item_list(params: &Value, key: &str, on: bool) -> Result<Option<Vec<Acc>>, String> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(Vec::new())),
        Some(Value::Array(a)) => {
            if a.len() > acc::MAX_ITEMS {
                return Err(format!("A place holds at most {} accessories.", acc::MAX_ITEMS));
            }
            let words: Vec<&str> = a.iter().map(|v| v.as_str().ok_or(format!("`{key}` must be a list of item names"))).collect::<Result<_, _>>()?;
            acc::parse_items(&words.join(","), on).map(Some)
        }
        Some(Value::String(t)) => acc::parse_items(t, on).map(Some),
        Some(_) => Err(format!("`{key}` must be a list of item names, or null")),
    }
}

fn read_items(doc: &Document, c: ComponentId, key: &str, on: bool) -> Vec<Acc> {
    doc.components.get(&c).and_then(|x| x.properties.get(key)).and_then(|v| acc::parse_items(v, on).ok()).unwrap_or_default()
}

fn plural(a: Acc, n: usize) -> String {
    let w = a.words();
    let w = if n == 1 { w.to_string() } else { format!("{w}s") };
    let mut ch = w.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().chain(ch).collect(),
        None => w,
    }
}

/// `accessory` (design 030 §2.2): one `SetProperty` per place, all in one transaction.
fn run_accessory(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let strip: Option<ComponentId> = opt_id(params, "strip").map_err(err)?;
    let terminal: Option<TerminalId> = opt_id(params, "terminal").map_err(err)?;
    let on = match params.get("on") {
        None | Some(Value::Null) => false,
        Some(v) => v.as_bool().ok_or_else(|| err("`on` must be true or false"))?,
    };
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let (c, label) = strip_of(doc, strip, terminal)?;
        let at: Vec<String> = match params.get("at") {
            Some(Value::String(t)) => vec![t.trim().to_string()],
            Some(Value::Array(a)) => {
                if a.len() > MAX_AT {
                    return Err(format!("`at` names at most {MAX_AT} places."));
                }
                a.iter()
                    .map(|v| v.as_str().map(|t| t.trim().to_string()).ok_or("`at` must be a label or a list of labels"))
                    .collect::<Result<_, _>>()?
            }
            None | Some(Value::Null) => vec![label.clone().ok_or("`at` is required: \"start\", \"end\", a position label or a list of them")?],
            Some(_) => return Err("`at` must be a label or a list of labels".into()),
        };
        if at.is_empty() {
            return Err("`at` names no place.".into());
        }
        let (items, add, remove) = (item_list(params, "items", on)?, item_list(params, "add", on)?, item_list(params, "remove", on)?);
        if items.is_none() && add.is_none() && remove.is_none() {
            return Err("Give `items` (replace), `add` or `remove`.".into());
        }
        let order = acc::positions(doc, c);
        // `at: "all"` names every position of the strip (the plan's Marker on every position).
        let at: Vec<String> = if at.len() == 1 && at.first().is_some_and(|a| a.eq_ignore_ascii_case("all")) { order.clone() } else { at };
        let mut spots: Vec<Spot> = Vec::new();
        let mut seen = BTreeSet::new();
        for a in &at {
            let spot = match (a.to_ascii_lowercase().as_str(), on) {
                ("start" | "end", true) => return Err("Markers go on a position. Pick a terminal, not start or end.".into()),
                ("start", false) => Spot::Start,
                ("end", false) => Spot::End,
                _ => {
                    acc::check_label(a)?;
                    if !order.iter().any(|o| o == a) {
                        return Err(no_position(doc, c, a, &order));
                    }
                    if on { Spot::On(a.clone()) } else { Spot::After(a.clone()) }
                }
            };
            if seen.insert(spot.key()) {
                spots.push(spot);
            }
        }
        let mut ops = Vec::new();
        let mut keys = Vec::new();
        for spot in &spots {
            let key = spot.key();
            let mut list = match &items {
                Some(v) => v.clone(),
                None => read_items(doc, c, &key, on),
            };
            if let Some(a) = &add {
                list.extend(a.iter().copied());
            }
            if let Some(r) = &remove {
                for x in r {
                    if let Some(i) = list.iter().rposition(|y| y == x) {
                        list.remove(i);
                    }
                }
            }
            if list.len() > acc::MAX_ITEMS {
                return Err(format!("A place holds at most {} accessories.", acc::MAX_ITEMS));
            }
            let after = (!list.is_empty()).then(|| acc::items_text(&list));
            let before = doc.components.get(&c).and_then(|x| x.properties.get(&key)).cloned();
            keys.push(json!({"key": key, "items": list.iter().map(|a| a.key()).collect::<Vec<_>>()}));
            if before != after {
                ops.push(Op::SetProperty { object: ObjectRef::Component(c), key, before, after });
            }
        }
        // `clear`: placement keys emptied in the same transaction (the `accessory-orphan` fix,
        // Move to end).
        if let Some(v) = params.get("clear") {
            let list = v.as_array().ok_or("`clear` must be a list of placement keys")?;
            for k in list.iter().take(MAX_AT) {
                let k = k.as_str().ok_or("`clear` must be a list of placement keys")?;
                if !k.starts_with(acc::ACC_PREFIX) {
                    return Err(format!("`clear` takes placement keys (acc.…), not \"{k}\"."));
                }
                if keys.iter().any(|x| x["key"] == k) {
                    continue;
                }
                if let Some(before) = doc.components.get(&c).and_then(|x| x.properties.get(k)).cloned() {
                    ops.push(Op::SetProperty { object: ObjectRef::Component(c), key: k.to_string(), before: Some(before), after: None });
                }
            }
        }
        let tag = strip_words(doc, c);
        let place = |s: &Spot| match s {
            Spot::Start => format!("at the start of {tag}"),
            Spot::End => format!("at the end of {tag}"),
            Spot::After(l) => format!("after {tag}:{l}"),
            Spot::On(l) => format!("on {tag}:{l}"),
        };
        let where_ = match (spots.first(), spots.last()) {
            (Some(a), Some(b)) if spots.len() > 1 => match (a.label(), b.label()) {
                (Some(x), Some(y)) => format!("{} {tag}:{x}\u{2026}{y}", if on { "on" } else { "after" }),
                _ => format!("at {} places of {tag}", spots.len()),
            },
            (Some(a), _) => place(a),
            _ => tag.clone(),
        };
        let what = match (&items, &add, &remove) {
            (Some(v), _, _) if v.is_empty() => "Cleared".to_string(),
            (_, Some(v), _) | (Some(v), _, _) if !v.is_empty() => v.iter().map(|a| plural(*a, spots.len())).collect::<Vec<_>>().join(", "),
            (_, _, Some(v)) if !v.is_empty() => format!("Removed {}", v.iter().map(|a| a.words()).collect::<Vec<_>>().join(", ")),
            _ => "Accessories".to_string(),
        };
        let message = format!("{what} {where_}.");
        let changed = !ops.is_empty();
        if changed {
            let label = match spots.as_slice() {
                [one] => format!(
                    "Accessory {tag} {}",
                    match one {
                        Spot::Start => "start".to_string(),
                        Spot::End => "end".to_string(),
                        Spot::After(l) => format!("after {l}"),
                        Spot::On(l) => format!("on {l}"),
                    }
                ),
                many => format!("Accessories {tag} ({} places)", many.len()),
            };
            p.apply_edit(pg, "accessory", &label, ops)?;
        }
        Ok(json!({"strip": c.hex(), "keys": keys, "changed": changed, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- jumper (§5.4, §5.5)

/// The positions a link names in plan order, with the labels it names that the strip lacks.
fn link_labels(p: &Positions, order: &[String]) -> Vec<String> {
    let mut v: Vec<String> = p.select(order).into_iter().cloned().collect();
    for m in p.missing(order) {
        if !v.contains(&m) {
            v.push(m);
        }
    }
    v
}

fn canonical_of(labels: &[String], order: &[String]) -> String {
    let mut sorted: Vec<&String> = labels.iter().collect();
    sorted.sort_by_key(|l| (order.iter().position(|o| o == *l).unwrap_or(usize::MAX), links::natural_key(l)));
    sorted.dedup();
    links::canonical(&sorted, order)
}

/// Labels from a text (`5,6` or `5-8`) or a list.
fn labels_param(v: &Value, order: &[String]) -> Result<Vec<String>, String> {
    let text = match v {
        Value::String(t) => t.clone(),
        Value::Array(a) => {
            a.iter().map(|x| x.as_str().map(str::to_string).ok_or("positions must be labels")).collect::<Result<Vec<_>, _>>()?.join(",")
        }
        _ => return Err("positions must be text like \"5,6\" or a list of labels".into()),
    };
    Ok(link_labels(&links::parse(&text)?, order))
}

/// The next free link number on a strip.
fn next_link(doc: &Document, c: ComponentId) -> Option<u32> {
    let props = &doc.components.get(&c)?.properties;
    let high = props
        .range(links::PREFIX.to_string()..)
        .take_while(|(k, _)| k.starts_with(links::PREFIX))
        .filter_map(|(k, _)| {
            let rest = k.strip_prefix(links::PREFIX)?;
            rest.split('.').next()?.parse::<u32>().ok()
        })
        .filter(|n| *n <= links::MAX_LINK)
        .max()
        .unwrap_or(0);
    (high < links::MAX_LINK).then_some(high + 1)
}

fn set_op(doc: &Document, c: ComponentId, key: String, after: Option<String>) -> Option<Op> {
    let before = doc.components.get(&c).and_then(|x| x.properties.get(&key)).cloned();
    (before != after).then_some(Op::SetProperty { object: ObjectRef::Component(c), key, before, after })
}

/// The ops clearing a link and its own part keys.
fn delete_ops(doc: &Document, c: ComponentId, n: u32) -> Vec<Op> {
    let mut ops: Vec<Op> = set_op(doc, c, links::key(n), None).into_iter().collect();
    for f in acc::FIELDS {
        ops.extend(set_op(doc, c, format!("{}.{f}", links::key(n)), None));
    }
    ops
}

/// The positions from `a` to `b` in plan order (either way round).
fn range_between(order: &[String], a: &str, b: &str) -> Vec<String> {
    let (i, j) = (order.iter().position(|o| o == a), order.iter().position(|o| o == b));
    match (i, j) {
        (Some(i), Some(j)) => order.get(i.min(j)..=i.max(j)).unwrap_or_default().to_vec(),
        _ => [a, b].iter().map(|x| x.to_string()).collect(),
    }
}

/// `jumper` (design 030 §10): make a link, or edit one.
fn run_jumper(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let (c, n, before_text, labels, label, extra) = if let Some(l) = params.get("link") {
            let c: ComponentId = crate::commands::id(l, "strip")?;
            let (c, _) = strip_of(doc, Some(c), None)?;
            let n = l.get("n").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).filter(|n| (1..=links::MAX_LINK).contains(n));
            let n = n.ok_or("`link.n` must be a whole number from 1 to 999")?;
            let key = links::key(n);
            let cur_text = doc.components.get(&c).and_then(|x| x.properties.get(&key)).cloned();
            let cur_text = cur_text.ok_or_else(|| format!("{} has no link {n}.", strip_words(doc, c)))?;
            let order = acc::positions(doc, c);
            let cur = link_labels(&links::parse(&cur_text).unwrap_or_default(), &order);
            let tag = strip_words(doc, c);
            if params.get("delete").and_then(Value::as_bool) == Some(true) {
                let ops = delete_ops(doc, c, n);
                return finish(p, pg, c, n, ops, format!("Delete link {tag} {cur_text}"), format!("Deleted link {tag} {cur_text}."), None);
            }
            let mut next = cur.clone();
            if let Some(v) = params.get("positions") {
                next = labels_param(v, &order)?;
            }
            if let Some(v) = params.get("add") {
                for l in labels_param(v, &order)? {
                    if !next.contains(&l) {
                        next.push(l);
                    }
                }
            }
            if let Some(v) = params.get("remove") {
                let r = labels_param(v, &order)?;
                next.retain(|l| !r.contains(l));
            }
            if params.get("drop_missing").and_then(Value::as_bool) == Some(true) {
                next.retain(|l| order.contains(l));
            }
            let mut extra: Option<(u32, Vec<String>)> = None;
            if let Some(at) = params.get("split_at") {
                let at = at.as_str().ok_or("`split_at` must be a position label")?;
                let i = order.iter().position(|o| o == at).ok_or_else(|| no_position(doc, c, at, &order))?;
                let (first, rest): (Vec<String>, Vec<String>) =
                    next.iter().cloned().partition(|l| order.iter().position(|o| o == l).is_some_and(|j| j <= i));
                if first.is_empty() || rest.is_empty() {
                    return Err(format!("Link {cur_text} has no positions on both sides of {at}; pick a position inside it."));
                }
                // Review of PR #57, 3: a side of one position would be no link; never dropped.
                if first.len() < 2 || rest.len() < 2 {
                    let one = if first.len() < 2 { &first } else { &rest };
                    return Err(format!(
                        "Splitting link {cur_text} after {at} would leave {} alone. Shorten the link instead (remove {}), or split it at another position.",
                        one.join(", "),
                        one.join(", ")
                    ));
                }
                let m = next_link(doc, c).ok_or("This strip has 999 links; delete one first.")?;
                next = first;
                extra = Some((m, rest));
            }
            let verb = if extra.is_some() {
                "Split"
            } else if next.len() > cur.len() {
                "Extend"
            } else if next.len() < cur.len() {
                "Shorten"
            } else {
                "Change"
            };
            (c, n, Some(cur_text), next, format!("{verb} link {tag}"), extra)
        } else {
            // Make a new link.
            let strip: Option<ComponentId> = opt_id(params, "strip")?;
            let picks: Vec<TerminalId> = if params.get("terminals").is_some() { id_list(params, "terminals", MAX_PICKS)? } else { Vec::new() };
            let c = match (strip, picks.first()) {
                (Some(c), _) => c,
                (None, Some(t)) => doc.terminals.get(t).ok_or("No such terminal.")?.component,
                (None, None) => return Err("`strip` with `positions`, or `terminals`, is required".into()),
            };
            let (c, _) = strip_of(doc, Some(c), None)?;
            let order = acc::positions(doc, c);
            let skip = params.get("skip").and_then(Value::as_bool).unwrap_or(false);
            let mut labels: Vec<String> = Vec::new();
            let mut prev: Option<String> = None;
            for t in &picks {
                let term = doc.terminals.get(t).ok_or("No such terminal.")?;
                if term.component != c {
                    return Err(format!(
                        "{} is on another strip; a link stays on one strip. Pick a terminal of {}.",
                        crate::terminal_cmds::terminal_words(doc, *t),
                        strip_words(doc, c)
                    ));
                }
                let l = crate::interact::terminal_name(doc, *t);
                let add = match (&prev, skip) {
                    (Some(a), false) => range_between(&order, a, &l),
                    _ => vec![l.clone()],
                };
                for x in add {
                    if !labels.contains(&x) {
                        labels.push(x);
                    }
                }
                prev = Some(l);
            }
            if let Some(v) = params.get("positions") {
                for l in labels_param(v, &order)? {
                    if !labels.contains(&l) {
                        labels.push(l);
                    }
                }
            }
            let n = next_link(doc, c).ok_or("This strip has 999 links; delete one first.")?;
            (c, n, None, labels, format!("Link {}", strip_words(doc, c)), None)
        };
        let order = acc::positions(doc, c);
        let tag = strip_words(doc, c);
        let mut ops = Vec::new();
        let text = if labels.len() < 2 {
            if before_text.is_none() {
                return Err("A link joins at least 2 positions. Pick another terminal, or type positions like 1-4.".into());
            }
            ops.extend(delete_ops(doc, c, n));
            None
        } else {
            let t = canonical_of(&labels, &order);
            ops.extend(set_op(doc, c, links::key(n), Some(t.clone())));
            Some(t)
        };
        let mut extra_text = None;
        if let Some((m, rest)) = &extra
            && rest.len() >= 2
        {
            let t = canonical_of(rest, &order);
            ops.extend(set_op(doc, c, links::key(*m), Some(t.clone())));
            // Both halves keep the link's own part (review of PR #57, 3).
            for f in acc::FIELDS {
                let own = doc.components.get(&c).and_then(|x| x.properties.get(&format!("{}.{f}", links::key(n)))).cloned();
                if own.is_some() {
                    ops.extend(set_op(doc, c, format!("{}.{f}", links::key(*m)), own));
                }
            }
            extra_text = Some((*m, t));
        }
        let shown = text.clone().unwrap_or_else(|| "(removed)".into());
        let tx_label = match (&before_text, &extra_text) {
            (Some(b), Some((_, e))) => format!("{label} {b} \u{2192} {shown} + {e}"),
            (Some(b), None) => format!("{label} {b} \u{2192} {shown}"),
            (None, _) => format!("{label} {shown}"),
        };
        let message = match (&before_text, &text) {
            (Some(b), None) => format!("Removed link {tag} {b} (fewer than 2 positions)."),
            (Some(_), Some(_)) => format!("{tx_label}."),
            (None, _) => format!("Linked {tag} {shown}."),
        };
        finish(p, pg, c, n, ops, tx_label, message, extra_text.map(|(m, t)| json!({"link": m, "positions": t})))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

#[allow(clippy::too_many_arguments)]
fn finish(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, c: ComponentId, n: u32, ops: Vec<Op>, label: String, message: String,
    split: Option<Value>,
) -> Result<Value, String> {
    let changed = !ops.is_empty();
    if changed {
        p.apply_edit(pg, "jumper", &label, ops)?;
    }
    let doc = p.ledger.document();
    let positions = doc.components.get(&c).and_then(|x| x.properties.get(&links::key(n))).cloned();
    let bars = p.outputs().strips.get(c).and_then(|d| d.acc.links.iter().find(|l| l.n == n)).map_or(0, |l| l.bars);
    let mut v = json!({"strip": c.hex(), "link": n, "positions": positions, "bars": bars, "changed": changed, "message": message});
    if let Some(sp) = split {
        v["split"] = sp;
    }
    Ok(v)
}
