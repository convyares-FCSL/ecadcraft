//! Brief 022's commands (design 022 §10): `nets`, `net`, `wirenumber`, `potential`,
//! `wiresettings`, `renumberwires`, `junction`, `checks`, and the continuation form of `place`.
//! Their JSON forms commit through [`Project::apply_edit`], which runs the numbering pass; reads
//! change nothing.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::Session;
use ecad_model::connectivity::{CONTINUATION, KIND, NAME};
use ecad_model::wiring::{self, Scheme};
use ecad_model::{ConductorId, Document, End, NetData, NetId, ObjectRef, Op, PageId, Placement, Pt, Subject, TerminalId};
use serde_json::{Value, json};

use crate::commands::{err, id, opt_id, plain_style, point, with_any_page, with_project};
use crate::project::Project;
use crate::symbols::Starter;
use crate::wires::{self, ON_WIRE};

/// `nets` lists at most this many by default.
pub const DEFAULT_LIMIT: usize = 200;
/// …and at most this many per call.
pub const MAX_LIMIT: usize = 2000;

/// Why `setprop` refuses a numbering key on a conductor.
pub const USE_WIRENUMBER: &str = "use wirenumber: a wire number belongs to the whole net";
pub const USE_POTENTIAL: &str = "use potential: a potential belongs to the whole net";

/// The net with ID `id` as JSON (design 022 §10).
pub fn net_json(doc: &Document, net: &NetData) -> Value {
    let pot = wiring::net_potential(doc, net);
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i + 1)).collect();
    // Per page (in page order): its shown number and the column of the first route start there.
    let mut pages: BTreeMap<usize, (String, Option<String>)> = BTreeMap::new();
    for c in &net.conductors {
        for (_, r) in doc.representations_of(Subject::Conductor(*c)) {
            let Some(n) = order.get(&r.page) else { continue };
            let col = ecad_model::derive::route_ends_on(doc, *c, r.page)
                .and_then(|(a, _)| ecad_model::wiring::place(doc, r.page, a))
                .map(|p| p.column_label);
            let slot = pages.entry(*n).or_insert_with(|| (ecad_model::book::page_number(doc, r.page), None));
            if slot.1.is_none() {
                slot.1 = col;
            }
        }
    }
    let points: Vec<Value> = net
        .terminals
        .iter()
        .filter(|t| {
            doc.terminals.get(t).is_none_or(|x| !matches!(wires::kind_of(doc, x.component), Some(ecad_model::connectivity::JUNCTION | CONTINUATION)))
        })
        .map(|t| json!({"terminal": t.hex(), "label": wires::point_words(doc, *t)}))
        .collect();
    let continuations: BTreeSet<String> = net
        .terminals
        .iter()
        .filter_map(|t| doc.terminals.get(t))
        .filter(|x| wires::kind_of(doc, x.component) == Some(CONTINUATION))
        .filter_map(|x| doc.components.get(&x.component).and_then(|c| c.properties.get(NAME)).cloned())
        .collect();
    json!({
        "number": wiring::net_number(doc, net),
        "fixed": wiring::net_fixed(net),
        "potential": pot.name,
        "conductors": net.conductors.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "points": points,
        "continuations": continuations,
        "pages": pages.into_values().map(|(p, c)| json!({"page": p, "column": c})).collect::<Vec<_>>(),
    })
}

/// A net's sort key in listings: its first conductor, else its first terminal.
fn sort_key(net: &NetData) -> (u8, u128) {
    match net.first_conductor() {
        Some(c) => (0, c.0),
        None => (1, net.terminals.first().map_or(0, |t| t.0)),
    }
}

fn opt_str<'a>(p: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(format!("`{key}` must be a string")),
    }
}

/// The net one of `conductor`, `terminal` or `number` names.
fn chosen_net(doc: &Document, p: &Value) -> Result<NetId, String> {
    let conn = doc.connectivity();
    if let Some(c) = opt_id::<ConductorId>(p, "conductor")? {
        return conn.net_of_conductor(c).ok_or_else(|| "No such wire.".into());
    }
    if let Some(t) = opt_id::<TerminalId>(p, "terminal")? {
        return conn.net_of_terminal(t).ok_or_else(|| "No such connection point.".into());
    }
    if let Some(n) = opt_str(p, "number")? {
        let n = n.trim();
        let mut nets: Vec<NetId> = conn.nets_numbered(n).map(|(id, _)| id).collect();
        nets.sort_by_key(|id| conn.net(*id).map(sort_key));
        return nets.first().copied().ok_or_else(|| format!("no net has number {n}"));
    }
    Err("one of `conductor`, `terminal` or `number` is required".into())
}

/// `nets {page?, number?, conductor?, terminal?, all?, limit?, after?}` (read only).
pub(crate) fn run_nets(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let limit = match params.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(v) => v.as_u64().map(|n| (n as usize).clamp(1, MAX_LIMIT)).ok_or_else(|| err("`limit` must be a number"))?,
    };
    let all = params.get("all").and_then(Value::as_bool).unwrap_or(false);
    let after = opt_str(params, "after").map_err(err)?.map(|a| a.trim().to_string());
    let page: Option<PageId> = opt_id(params, "page").map_err(err)?;
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let conn = doc.connectivity();
        let single = ["conductor", "terminal", "number"].iter().any(|k| params.get(*k).is_some_and(|v| !v.is_null()));
        let mut ids: Vec<NetId> = if single {
            if let Some(n) = opt_str(params, "number")? {
                conn.nets_numbered(n.trim()).map(|(id, _)| id).collect()
            } else {
                vec![chosen_net(doc, params)?]
            }
        } else if let Some(pg) = page {
            if !doc.pages.contains_key(&pg) {
                return Err(format!("page {pg} does not exist"));
            }
            let mut set = BTreeSet::new();
            for (_, r) in doc.representations_on(pg) {
                if let Subject::Conductor(c) = r.subject
                    && let Some(id) = conn.net_of_conductor(c)
                {
                    set.insert(id);
                }
            }
            set.into_iter().collect()
        } else {
            conn.nets().map(|(id, _)| id).collect()
        };
        ids.retain(|id| conn.net(*id).is_some_and(|n| all || !n.conductors.is_empty()));
        let mut keyed: Vec<((u8, u128), NetId)> = ids.into_iter().filter_map(|id| conn.net(id).map(|n| (sort_key(n), id))).collect();
        keyed.sort();
        if let Some(a) = &after {
            let cursor = u128::from_str_radix(a, 16).map_err(|_| "`after` must be a cursor from `next`".to_string())?;
            keyed.retain(|((_, k), _)| *k > cursor);
        }
        let more = keyed.len() > limit;
        let page_of: Vec<_> = keyed.into_iter().take(limit).collect();
        let next = more.then(|| page_of.last().map(|((_, k), _)| format!("{k:032x}"))).flatten();
        let nets: Vec<Value> = page_of.iter().filter_map(|(_, id)| conn.net(*id)).map(|n| net_json(doc, n)).collect();
        let mut r = json!({ "nets": nets });
        if let Some(n) = next {
            r["next"] = json!(n);
        }
        Ok(r)
    })
}

/// `net {conductor | terminal | number}` (read only).
pub(crate) fn run_net(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let id = chosen_net(doc, params)?;
        let net = doc.connectivity().net(id).ok_or("no such net")?;
        Ok(net_json(doc, net))
    })
}

/// A number in words for refusals: the net's first device point and page (`-K01:A1, /2.1`).
fn where_words(doc: &Document, net: &NetData) -> String {
    let point = wires::net_points(doc, net).into_iter().next();
    let place = net.first_conductor().and_then(|c| {
        doc.representations_of(Subject::Conductor(c)).find_map(|(_, r)| {
            let (a, _) = ecad_model::derive::route_ends_on(doc, c, r.page)?;
            Some(ecad_model::wiring::place(doc, r.page, a)?.text(doc.settings.reference_style))
        })
    });
    [point, place].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

/// The next free number in the scope of `value` (its prefix), with its padding; this net's own
/// next free number when `value` has no sequence.
fn next_like(doc: &Document, value: &str, net: &NetData) -> String {
    match wiring::sequence_of(value) {
        Some((prefix, _)) => {
            let digits = value.len().saturating_sub(prefix.len());
            let n = doc.connectivity().next_in(prefix);
            format!("{prefix}{n:0digits$}")
        }
        None => {
            let pot = wiring::net_potential(doc, net);
            wiring::Allocator::default().next(doc, net, &pot)
        }
    }
}

/// Whether merging nets `a` and `b` is electrically sensible: `Err` with the reason when not.
fn merge_sense(doc: &Document, a: &NetData, b: &NetData) -> Result<(), String> {
    let (pa, pb) = (wiring::net_potential(doc, a), wiring::net_potential(doc, b));
    if let (Some(x), Some(y)) = (&pa.name, &pb.name)
        && !x.eq_ignore_ascii_case(y)
    {
        return Err(format!("they carry different potentials ({x} and {y}), so joining them would short them"));
    }
    let fixed: BTreeSet<&String> = a.fixed.keys().chain(b.fixed.keys()).collect();
    if fixed.len() > 1 {
        return Err("both nets have fixed numbers of their own".into());
    }
    Ok(())
}

/// The loose end of a net to continue from: (conductor, end, page, point, direction away).
fn open_end(doc: &Document, net: &NetData) -> Option<(ConductorId, End, PageId, Pt, Pt)> {
    for c in &net.conductors {
        let cond = doc.conductors.get(c)?;
        for (_, r) in doc.representations_of(Subject::Conductor(*c)) {
            let Some((_, _, v)) = wires::route_vertices(doc, *c, r.page) else { continue };
            for end in End::BOTH {
                if cond.end(end).is_some() {
                    continue;
                }
                let (at, prev) = if end == End::A { (v.first(), v.get(1)) } else { (v.last(), v.iter().rev().nth(1)) };
                let (Some(at), Some(prev)) = (at, prev) else { continue };
                let d = Pt::new(at.x - prev.x, at.y - prev.y);
                return Some((*c, end, r.page, *at, d));
            }
        }
    }
    None
}

/// The rotation that points a continuation's arrow along `d` (snapped to 90°).
fn arrow_rotation(d: Pt) -> f64 {
    // The arrow runs to local (0, −5): rotation θ maps it to (sin θ, −cos θ).
    let theta = d.x.atan2(-d.y);
    let q = std::f64::consts::FRAC_PI_2;
    (theta / q).round() * q
}

/// `wirenumber {conductor | number, value: string | null, conflict?: "swap" | "another" | "merge"}`.
pub(crate) fn run_wirenumber(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    // `fixed` alone (the Properties panel's Fixed box): fix the net's current number, or release it.
    if params.get("value").is_none()
        && let Some(fixed) = params.get("fixed").and_then(Value::as_bool)
    {
        if !fixed {
            let mut p = params.clone();
            p["value"] = Value::Null;
            return run_wirenumber(s, &p);
        }
        let current = with_project(s, |p, _, _| {
            let doc = p.ledger.document();
            let id = chosen_net(doc, params)?;
            doc.connectivity().net(id).and_then(|n| wiring::net_number(doc, n)).ok_or_else(|| "That net has no number to fix yet.".to_string())
        })?;
        let mut p = params.clone();
        p["value"] = json!(current);
        return run_wirenumber(s, &p);
    }
    let value = match params.get("value") {
        Some(Value::Null) => None,
        Some(Value::String(v)) => Some(wiring::valid_number(v).map_err(err)?),
        Some(_) => return Err(err("`value` must be a string, or null to release a fixed number")),
        None => return Err(err("`value` is required (a wire number, or null to release a fixed number)")),
    };
    let conflict = opt_str(params, "conflict").map_err(err)?.map(str::to_ascii_lowercase);
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let id = chosen_net(doc, params)?;
        let net = doc.connectivity().net(id).ok_or("no such net")?.clone();
        if net.conductors.is_empty() {
            return Err("That net has no wire to number.".into());
        }
        let Some(v) = value else {
            let current = wiring::net_number(doc, &net).ok_or("That net has no number to release.")?;
            let ops = wiring::write_ops(doc, &net, &current, false);
            if ops.is_empty() {
                return Ok(json!({"number": current, "fixed": false, "conductors": net.conductors.len(), "changed": false}));
            }
            p.apply_edit(pg, "wirenumber", "Release wire number", ops)?;
            return Ok(json!({"number": current, "fixed": false, "conductors": net.conductors.len()}));
        };
        let conn = doc.connectivity();
        let exempt = doc.settings.wiring.potentials.iter().any(|x| !x.numbered && x.prefix == v);
        let others: Vec<NetId> = if exempt { Vec::new() } else { conn.nets_numbered(&v).map(|(n, _)| n).filter(|n| *n != id).collect() };
        if others.is_empty() {
            let ops = wiring::write_ops(doc, &net, &v, true);
            if !ops.is_empty() {
                p.apply_edit(pg, "wirenumber", "Fix wire number", ops)?;
            }
            return Ok(json!({"number": v, "fixed": true, "conductors": net.conductors.len()}));
        }
        let other_id = *others.first().ok_or("no other net")?;
        let other = conn.net(other_id).ok_or("no such net")?.clone();
        let next = next_like(doc, &v, &net);
        let sense = merge_sense(doc, &net, &other);
        match conflict.as_deref() {
            None => {
                let merge = match &sense {
                    Ok(()) => "\"merge\" (join the two nets with a continuation pair named for it)".to_string(),
                    Err(why) => format!("not \"merge\": {why}"),
                };
                let swap = match wiring::net_number(doc, &net) {
                    Some(mine) => format!("\"swap\" (that net takes {mine})"),
                    None => format!("\"swap\" (that net takes {next})"),
                };
                Err(format!(
                    "{v} is used by another net ({}). Choose: {swap}, \"another\" ({next}, the next free), or {merge}; give it as `conflict`.",
                    where_words(doc, &other)
                ))
            }
            Some("another") => {
                let mut ops = Vec::new();
                ops.extend(wiring::write_ops(doc, &net, &next, true));
                p.apply_edit(pg, "wirenumber", "Fix wire number", ops)?;
                Ok(json!({"number": next, "fixed": true, "conductors": net.conductors.len()}))
            }
            Some("swap") => {
                if others.len() > 1 {
                    return Err(format!("{v} is used by {} other nets; choose another number.", others.len()));
                }
                let theirs = wiring::net_number(doc, &net).unwrap_or_else(|| next.clone());
                let mut ops = wiring::write_ops(doc, &net, &v, true);
                ops.extend(wiring::write_ops(doc, &other, &theirs, wiring::net_fixed(&other)));
                p.apply_edit(pg, "wirenumber", "Swap wire numbers", ops)?;
                Ok(json!({"number": v, "fixed": true, "conductors": net.conductors.len(), "swapped": theirs}))
            }
            Some("merge") => {
                sense.map_err(|why| format!("The nets can't be merged: {why}."))?;
                let name =
                    wires::valid_name(&v).map_err(|_| format!("The nets can't be merged: {v} is not a continuation name ({}).", wires::NAME_RULE))?;
                let starter = wires::continuation_starter().ok_or("the continuation symbol is missing")?;
                let style = plain_style(p);
                let mut ops = wiring::write_ops(doc, &net, &v, true);
                let mut placed = Vec::new();
                for n in [&net, &other] {
                    let (_, _, page, at, d) = open_end(doc, n).ok_or_else(|| {
                        format!(
                            "The nets can't be merged: the net at {} has no open end to continue from; draw a continuation there.",
                            where_words(doc, n)
                        )
                    })?;
                    let mut props = ecad_model::Properties::new();
                    props.insert(KIND.into(), CONTINUATION.into());
                    props.insert(NAME.into(), name.clone());
                    let placement = Placement { origin: at, rotation: arrow_rotation(d), mirror: false, scale: 1.0 };
                    let t = wires::plan_tagless(doc, &mut p.ids, page, style.clone(), starter, placement, props)?;
                    // The plan attaches the loose end at the point.
                    placed.push(t.component.hex());
                    ops.extend(t.ops);
                }
                p.apply_edit(pg, "wirenumber", "Merge nets", ops)?;
                Ok(json!({"number": v, "fixed": true, "merged": true, "continuations": placed}))
            }
            Some(o) => Err(format!("`conflict` must be \"swap\", \"another\" or \"merge\", not {o:?}")),
        }
    })
}

/// `potential {conductor, value: string | null}`: the potential of the whole net.
pub(crate) fn run_potential(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let c: ConductorId = id(params, "conductor").map_err(err)?;
    let value = match params.get("value") {
        Some(Value::Null) => None,
        Some(Value::String(v)) => wiring::valid_text("potential", Some(v)).map_err(err)?,
        Some(_) | None => return Err(err("`value` is required (a potential such as 24V, or null to clear it)")),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let conn = doc.connectivity();
        let net = conn.net_of_conductor(c).and_then(|id| conn.net(id)).ok_or("No such wire.")?;
        let mut ops = Vec::new();
        for w in &net.conductors {
            let before = doc.conductors.get(w).and_then(|x| x.properties.get(wiring::POTENTIAL_KEY)).cloned();
            if before != value {
                ops.push(Op::SetProperty { object: ObjectRef::Conductor(*w), key: wiring::POTENTIAL_KEY.into(), before, after: value.clone() });
            }
        }
        let n = net.conductors.len();
        if !ops.is_empty() {
            p.apply_edit(pg, "potential", "Set potential", ops)?;
        }
        let doc = p.ledger.document();
        let number = doc.connectivity().net_of_conductor(c).and_then(|id| doc.connectivity().net(id)).and_then(|n| wiring::net_number(doc, n));
        Ok(json!({"potential": value, "conductors": n, "number": number}))
    })
}

fn settings_json(w: &wiring::WiringSettings) -> Value {
    json!({
        "scheme": w.scheme.name(),
        "colour_style": w.colour_style.name(),
        "label_height": w.label_height(),
        "digits": w.digits,
        "unclassified_prefix": w.unclassified_prefix,
        "potentials": w.potentials.iter().map(|p| json!({
            "name": p.name, "prefix": p.prefix, "numbered": p.numbered, "colour": p.colour, "cross_section": p.cross_section,
        })).collect::<Vec<_>>(),
    })
}

/// `wiresettings {scheme?, digits?, unclassified_prefix?, potentials?}`: no parameters reads.
pub(crate) fn run_wiresettings(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let given = ["scheme", "digits", "unclassified_prefix", "potentials", "colour_style", "label_height"]
        .iter()
        .any(|k| params.get(*k).is_some_and(|v| !v.is_null()));
    with_project(s, |p, pg, _| {
        let before = p.ledger.document().settings.clone();
        if !given {
            return Ok(settings_json(&before.wiring));
        }
        let mut after = before.clone();
        let w = &mut after.wiring;
        if let Some(sc) = opt_str(params, "scheme")? {
            w.scheme = Scheme::parse(sc).ok_or("`scheme` must be \"potential\", \"page\" or \"page-column\"")?;
        }
        if let Some(d) = params.get("digits").filter(|v| !v.is_null()) {
            let d = d.as_u64().filter(|d| (1..=u64::from(wiring::MAX_DIGITS)).contains(d)).ok_or("`digits` must be 1 to 6")?;
            w.digits = d as u8;
        }
        if let Some(c) = opt_str(params, "colour_style")? {
            w.colour_style =
                wiring::ColourStyle::parse(c).ok_or("`colour_style` must be \"iec\" (BN, BU, GNYE) or \"three-letter\" (BRN, BLU, GRN/YEL)")?;
        }
        if let Some(hv) = params.get("label_height").filter(|v| !v.is_null()) {
            let hv = hv.as_f64().filter(|x| wiring::LABEL_HEIGHTS.contains(x)).ok_or("`label_height` must be 0.5 to 10 (millimetres)")?;
            w.label_height = hv;
        }
        if let Some(u) = opt_str(params, "unclassified_prefix")? {
            w.unclassified_prefix = wiring::valid_text("prefix", Some(u))?.unwrap_or_default();
        }
        if let Some(list) = params.get("potentials").filter(|v| !v.is_null()) {
            let a = list.as_array().ok_or("`potentials` must be the whole table: [{name, prefix, numbered, colour, cross_section}]")?;
            if a.len() > 200 {
                return Err("at most 200 potentials".into());
            }
            let mut rows = Vec::new();
            let mut names = BTreeSet::new();
            for r in a {
                let field = |k: &str| r.get(k).and_then(Value::as_str).map(str::trim).unwrap_or("").to_string();
                let name = wiring::valid_text("potential", Some(&field("name")))?.ok_or("every potential needs a `name`")?;
                if !names.insert(name.to_uppercase()) {
                    return Err(format!("potential {name} is listed twice"));
                }
                rows.push(wiring::PotentialDef {
                    prefix: wiring::valid_text("prefix", Some(&field("prefix")))?.unwrap_or_default(),
                    numbered: r.get("numbered").and_then(Value::as_bool).unwrap_or(true),
                    colour: wiring::valid_text("colour", Some(&field("colour")))?.unwrap_or_default(),
                    cross_section: wiring::valid_text("cross-section", Some(&field("cross_section")))?.unwrap_or_default(),
                    name,
                });
            }
            w.potentials = rows;
        }
        if after == before {
            return Ok(settings_json(&after.wiring));
        }
        let shown = settings_json(&after.wiring);
        p.apply_edit(pg, "wiresettings", "Wire settings", vec![Op::SetProjectSettings { before, after }])?;
        Ok(shown)
    })
}

/// `renumberwires {scope?: "unnumbered" | "all"}`.
pub(crate) fn run_renumberwires(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let all = match opt_str(params, "scope").map_err(err)?.map(|x| x.trim().to_ascii_lowercase()) {
        None => false,
        Some(x) if x == "unnumbered" => false,
        Some(x) if x == "all" => true,
        Some(x) => return Err(err(format!("`scope` must be \"unnumbered\" or \"all\", not {x:?}"))),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let ops = wiring::renumber(doc, all);
        let nets: BTreeSet<NetId> = ops
            .iter()
            .filter_map(|op| match op {
                Op::SetProperty { object: ObjectRef::Conductor(c), .. } => doc.connectivity().net_of_conductor(*c),
                _ => None,
            })
            .collect();
        if ops.is_empty() {
            return Ok(json!({"renumbered": 0}));
        }
        p.apply_edit(pg, "renumberwires", "Renumber wires", ops)?;
        Ok(json!({"renumbered": nets.len()}))
    })
}

/// `junction {at: [x,y]}` on the active page (design 022 §3.1).
pub(crate) fn run_junction(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let at = params.get("at").and_then(point).ok_or_else(|| err("`at` [x,y] is required"))?;
    with_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        // Loose ends that only meet (no wire runs through the point): a junction joins them, with
        // nothing to split (the Checks row's Add junction, design 022 §8).
        let loose = wires::loose_ends_at(doc, page, at, ON_WIRE);
        let through = wires::wire_at(doc, page, at, ON_WIRE, None).filter(|on| {
            wires::route_vertices(doc, on.conductor, page)
                .is_some_and(|(_, _, v)| !v.first().is_some_and(|f| f.near(on.at, ON_WIRE)) && !v.last().is_some_and(|l| l.near(on.at, ON_WIRE)))
        });
        if through.is_none() && loose.len() >= 2 {
            let starter = wires::junction_starter().ok_or("the junction symbol is missing")?;
            let mut props = ecad_model::Properties::new();
            props.insert(KIND.into(), ecad_model::connectivity::JUNCTION.into());
            let snapped = loose
                .first()
                .and_then(|(c, e)| {
                    wires::route_vertices(doc, *c, page).and_then(|(_, _, v)| if *e == End::A { v.first().copied() } else { v.last().copied() })
                })
                .unwrap_or(at);
            let placement = Placement { origin: snapped, rotation: 0.0, mirror: false, scale: 1.0 };
            let t = wires::plan_tagless(doc, &mut p.ids, page, style, starter, placement, props)?;
            let r = json!({"junction": t.component.hex(), "terminal": t.terminal.hex(), "conductor": null, "at": [snapped.x, snapped.y], "attached": t.attached.len()});
            p.apply_edit(pg, "junction", "Junction", t.ops)?;
            return Ok(r);
        }
        let on = wires::wire_at(doc, page, at, ON_WIRE, None).ok_or("There is no wire there: click on a wire.")?;
        let j = wires::plan_junction(doc, &mut p.ids, page, style, on)?;
        let r = json!({
            "junction": j.tagless.component.hex(),
            "terminal": j.tagless.terminal.hex(),
            "conductor": j.second.hex(),
            "at": [j.at.x, j.at.y],
        });
        p.apply_edit(pg, "junction", "Junction", j.tagless.ops)?;
        Ok(r)
    })
}

/// `checks {kinds?, limit?, after?}` (read only).
pub(crate) fn run_checks(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let kinds: Option<BTreeSet<crate::checks::Kind>> = match params.get("kinds") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => Some(
            a.iter()
                .map(|k| k.as_str().and_then(crate::checks::Kind::parse).ok_or_else(|| err(format!("unknown check kind {k}"))))
                .collect::<cadcraft_engine::Result<_>>()?,
        ),
        Some(_) => return Err(err("`kinds` must be a list of check kinds")),
    };
    let limit = match params.get("limit") {
        None | Some(Value::Null) => crate::checks::DEFAULT_LIMIT,
        Some(v) => v.as_u64().map(|n| (n as usize).clamp(1, crate::checks::MAX_LIMIT)).ok_or_else(|| err("`limit` must be a number"))?,
    };
    let after = params.get("after").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(0);
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let rows: Vec<_> = crate::checks::all(doc, p.tolerance).into_iter().filter(|c| kinds.as_ref().is_none_or(|k| k.contains(&c.kind))).collect();
        let shown: Vec<Value> = rows.iter().skip(after).take(limit).map(|c| c.to_json(doc)).collect();
        let mut r = json!({"checks": shown, "total": rows.len()});
        if rows.len() > after + limit {
            r["next"] = json!(after + limit);
        }
        Ok(r)
    })
}

/// `place {symbol: "continuation" | "junction", at, rotation?, mirror?, name?}`: a tagless
/// component (design 022 §3.2).
pub(crate) fn place_tagless(
    s: &mut Session, params: &Value, starter: &'static Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    if params.get("device").is_some_and(|v| !v.is_null()) || params.get("tag").is_some_and(|v| !v.is_null()) {
        return Err(err(format!("A {} has no tag and is not part of a device.", starter.name.to_lowercase())));
    }
    let name = match opt_str(params, "name").map_err(err)? {
        Some(n) => Some(wires::valid_name(n).map_err(err)?),
        None => None,
    };
    if starter.key == CONTINUATION && name.is_none() {
        return Err(err("`name` is required for a continuation (the potential or signal it carries, e.g. 24V)"));
    }
    with_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let mut props = ecad_model::Properties::new();
        props.insert(KIND.into(), starter.key.into());
        if let Some(n) = &name {
            props.insert(NAME.into(), n.clone());
        }
        let placement = Placement { origin: at, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        let t = wires::plan_tagless(doc, &mut p.ids, page, style, starter, placement, props)?;
        let label = if starter.key == CONTINUATION { "Continuation" } else { "Junction" };
        p.apply_edit(pg, "place", label, t.ops.clone())?;
        if let Some(n) = &name {
            p.last_continuation = Some(n.clone());
        }
        Ok(json!({
            "component": t.component.hex(),
            "representation": t.representation.hex(),
            "item": t.item.hex(),
            "definition": t.definition.hex(),
            "symbol": starter.key,
            "terminal": t.terminal.hex(),
            "name": name,
            "attached": t.attached.iter().map(|(c, e)| json!({"conductor": c.hex(), "end": format!("{e:?}")})).collect::<Vec<_>>(),
        }))
    })
}

/// The `setprop` rules of brief 022 for a conductor or a continuation: `Ok(Some(value))` to write
/// (checked), `Err` to refuse; `Ok(None)` when the key is not one of brief 022's.
pub(crate) fn setprop_rule(doc: &Document, object: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Option<String>>, String> {
    match object {
        ObjectRef::Conductor(_) => match key {
            k if k == wiring::WIRE_NUMBER_KEY || k == ecad_model::connectivity::WIRE_NUMBER_FIXED => Err(USE_WIRENUMBER.into()),
            k if k == wiring::POTENTIAL_KEY => Err(USE_POTENTIAL.into()),
            k if k == wiring::CROSS_SECTION => Ok(Some(wiring::valid_text("cross-section", value)?)),
            k if k == wiring::COLOUR => Ok(Some(wiring::valid_text("colour", value)?)),
            _ => Ok(None),
        },
        ObjectRef::Component(c) if wires::kind_of(doc, c).is_some() => match key {
            k if k == KIND => Err("A junction or continuation keeps its kind; erase it and place another.".into()),
            k if k == NAME && wires::kind_of(doc, c) == Some(CONTINUATION) => {
                let v = value.ok_or("A continuation needs a name.")?;
                Ok(Some(Some(wires::valid_name(v)?)))
            }
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

/// For the Wire tool: the potentials a person can choose (`auto` and the table's names).
pub(crate) fn potential_choices(p: &Project) -> Vec<String> {
    let mut v = vec!["auto".to_string()];
    v.extend(p.ledger.document().settings.wiring.potentials.iter().map(|x| x.name.clone()));
    v
}
