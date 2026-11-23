//! Putting it together: the preview of changes and the plan (ops) from the answered conflicts.

use std::collections::{BTreeMap, BTreeSet};

use crate::document::diff;
use crate::document::invariant::Touched;
use crate::document::op::Op;
use crate::naming::id::{ConnectionPointId, DeviceId, PageId};
use crate::naming::tag::{self};
use crate::{Document, End, Properties, Subject};

use super::{conflicts::*, rules::*, state::*};

/// The source's changes since the ancestor, for the preview.
pub(super) fn changes(inp: &Inputs<'_>, objects: &[Obj]) -> (Vec<Change>, Vec<PageId>) {
    let (a, s) = (inp.ancestor, inp.source);
    let docs = [s, a, inp.target];
    let mut out = Vec::new();
    let mut seen: BTreeSet<Obj> = BTreeSet::new();
    for &obj in objects {
        if !differs(a, s, obj) {
            continue;
        }
        // A drawing item of a symbol is shown as that symbol (moving a symbol changes its item).
        let shown_as = match obj {
            Obj::Item(i) => s.representation_of_item(i).or_else(|| a.representation_of_item(i)).map_or(obj, |(r, _)| Obj::Representation(r)),
            _ => obj,
        };
        if seen.contains(&shown_as) {
            continue;
        }
        // A symbol of a wire, cable or device listed itself is shown through it.
        if let Obj::Representation(r) = shown_as
            && let Some(rep) = s.representations.get(&r).or_else(|| a.representations.get(&r))
        {
            let subject = match rep.subject {
                Subject::Device(c) => Obj::Device(c),
                Subject::Wire(c) => Obj::Wire(c),
                Subject::Cable(c) => Obj::Cable(c),
                Subject::Route(c) => Obj::Route(c),
                Subject::Aid(c) => Obj::Aid(c),
            };
            if objects.contains(&subject) && differs(a, s, subject) && holds(a, subject) == holds(a, shown_as) {
                continue;
            }
        }
        seen.insert(shown_as);
        let (sign, detail) = match (holds(a, obj), holds(s, obj)) {
            (false, true) => ('+', "added".to_string()),
            (true, false) => ('−', "removed".to_string()),
            _ => ('~', change_detail(a, s, obj)),
        };
        if obj == Obj::Project && detail.is_empty() {
            continue;
        }
        let (sign, detail) = if shown_as == obj { (sign, detail) } else { ('~', "moved or changed".to_string()) };
        let page = page_in(s, shown_as).or_else(|| page_in(a, shown_as));
        out.push(Change { sign, object: shown_as, text: format!("{} {detail}", name_of(&docs, shown_as)).trim().to_string(), page });
    }
    let order: BTreeMap<PageId, usize> = s.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    out.sort_by(|x, y| {
        let (px, py) = (x.page.and_then(|p| order.get(&p)), y.page.and_then(|p| order.get(&p)));
        (px.is_none(), px, &x.text).cmp(&(py.is_none(), py, &y.text))
    });
    let mut pages: Vec<PageId> = Vec::new();
    for c in &out {
        if let Some(p) = c.page
            && !pages.contains(&p)
        {
            pages.push(p);
        }
    }
    (out, pages)
}

/// `· Description "Pump" → "Pump 1"` for the first changed property (and how many more).
pub(super) fn change_detail(a: &Document, s: &Document, obj: Obj) -> String {
    let props = |d: &Document| -> Option<Properties> {
        match obj {
            // A device's tag and text fields are listed as its keys were.
            Obj::Device(c) => d.devices.get(&c).map(|x| {
                let mut p = x.properties.clone();
                p.extend(text_bag(x));
                p.extend(x.tag().map(|t| (tag::TAG_KEY.to_string(), t.to_string())));
                p
            }),
            Obj::Wire(c) => d.wires.get(&c).map(|x| {
                let mut p = x.display.clone();
                p.extend(x.wire_type.as_ref().map(|t| (crate::connectivity::wiring::WIRE_CATEGORY.to_string(), t.text())));
                p
            }),
            Obj::Cable(c) => d.cables.get(&c).map(|x| {
                let mut p = x.properties.clone();
                p.extend(x.cable_type.as_ref().map(|t| (crate::document::doc::cable::CABLE_TYPE_KEY.to_string(), t.text())));
                p
            }),
            Obj::Page(p) => d.pages.get(&p).map(|x| x.display.clone()),
            Obj::Project => Some(d.display.clone()),
            _ => None,
        }
    };
    if let Obj::Wire(c) = obj
        && let (Some(x), Some(y)) = (a.wires.get(&c), s.wires.get(&c))
    {
        let docs = [s, a];
        for (e, word) in [(End::A, "end A"), (End::B, "end B")] {
            if x.end(e) != y.end(e) {
                let show = |t: ConnectionPointId| name_of(&docs, Obj::ConnectionPoint(t));
                return format!("· {word} {} → {}", show(x.end(e)), show(y.end(e)));
            }
        }
        let (vx, vy) = (x.values(), y.values());
        let name = |v: &[(&'static str, String)], k: &str| v.iter().find(|(n, _)| *n == k).map(|(_, t)| t.clone());
        let mut keys: Vec<&'static str> = vx.iter().chain(&vy).map(|(n, _)| *n).collect();
        keys.dedup();
        if let Some(k) = keys.into_iter().find(|k| name(&vx, k) != name(&vy, k)) {
            let q = |t: Option<String>| t.map_or_else(|| "(none)".into(), |t| format!("\"{t}\""));
            return format!("· {k} {} → {}", q(name(&vx, k)), q(name(&vy, k)));
        }
    }
    // A module moved to another station shows the stations' tags.
    if let Obj::Device(c) = obj
        && let (Some(x), Some(y)) = (a.devices.get(&c), s.devices.get(&c))
        && let (crate::DeviceType::PlcModule(mx), crate::DeviceType::PlcModule(my)) = (&x.device_type, &y.device_type)
        && (mx.rack, mx.slot) != (my.rack, my.slot)
    {
        let docs = [s, a];
        let at = |r: Option<DeviceId>, slot: Option<u16>| match (r, slot) {
            (Some(r), Some(n)) => format!("{} slot {n}", name_of(&docs, Obj::Device(r))),
            (Some(r), None) => name_of(&docs, Obj::Device(r)),
            (None, _) => "no rack".into(),
        };
        return format!("· rack {} → {}", at(mx.rack, mx.slot), at(my.rack, my.slot));
    }
    match (props(a), props(s)) {
        (Some(pa), Some(ps)) => {
            let keys: Vec<&String> = pa
                .keys()
                .chain(ps.keys())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter(|k| pa.get(*k) != ps.get(*k))
                .filter(|k| crate::naming::keys::NUMBERING_HIGH.parse(k).is_none())
                .collect();
            // `numbering.high.<P>` is internal: never listed.
            match keys.first() {
                Some(k) => {
                    // A value naming a component shows its tag.
                    let q = |v: Option<&String>| v.map_or_else(|| "(none)".into(), |v| format!("\"{}\"", value_words(&[s, a], v)));
                    let more = if keys.len() > 1 { format!(" (+{} more)", keys.len() - 1) } else { String::new() };
                    format!("· {} {} → {}{more}", key_words(k), q(pa.get(*k)), q(ps.get(*k)))
                }
                None if obj == Obj::Project => {
                    if a.settings != s.settings {
                        "· settings changed".into()
                    } else if a.standard != s.standard || a.styles != s.styles {
                        "· standard or styles changed".into()
                    } else {
                        String::new()
                    }
                }
                None => "changed".into(),
            }
        }
        _ => "changed".into(),
    }
}

/// Work out the merge of `inp` with `choices` (module docs).
pub fn plan(inp: &Inputs<'_>, choices: &BTreeMap<String, Choice>) -> Plan {
    let mut ctx =
        Ctx { inp, choices, conflicts: Vec::new(), seen: BTreeSet::new(), problems: Vec::new(), unavailable: BTreeMap::new(), provisional: 0 };
    let objects = scope_objects(inp.scope);
    let (changes, pages) = changes(inp, &objects);
    let mut m = inp.target.clone();
    merge_objects(&mut ctx, &mut m, &objects);
    let kept_devices = keep_devices(&mut ctx, &mut m);
    // Orders.
    let (a, t, s) = (inp.ancestor, inp.target, inp.source);
    if inp.scope.touched.page_order || a.page_order != s.page_order || a.page_order != t.page_order {
        let pages_in = m.pages.clone();
        m.page_order = merge_order(&mut ctx, "rule/pageorder", "Page order · changed on both", &a.page_order, &t.page_order, &s.page_order, &|p| {
            pages_in.contains_key(&p)
        });
    }
    if inp.scope.touched.folder_order || a.folder_order != s.folder_order || a.folder_order != t.folder_order {
        let folders_in = m.folders.clone();
        m.folder_order =
            merge_order(&mut ctx, "rule/folderorder", "Folder order · changed on both", &a.folder_order, &t.folder_order, &s.folder_order, &|f| {
                folders_in.contains_key(&f)
            });
    }
    // Rules.
    let mut all: BTreeSet<Obj> = objects.iter().copied().collect();
    all.extend(kept_devices);
    let needed = resolve_needed(&mut ctx, &mut m, all.clone());
    all.extend(needed);
    let renamed = resolve_tags(&mut ctx, &mut m, &all);
    all.extend(renamed);
    let renumbered = resolve_page_numbers(&mut ctx, &mut m, &all);
    all.extend(renumbered);
    let folders = resolve_folder_names(&mut ctx, &mut m, &all);
    all.extend(folders);
    // Page order lists every page exactly once.
    m.page_order.retain(|p| m.pages.contains_key(p));
    for p in m.pages.keys() {
        if !m.page_order.contains(p) {
            m.page_order.push(*p);
        }
    }
    m.folder_order.retain(|f| m.folders.contains_key(f));
    for f in m.folders.keys() {
        if !m.folder_order.contains(f) {
            m.folder_order.push(*f);
        }
    }
    let unanswered: Vec<String> = ctx.conflicts.iter().filter(|c| c.chosen.is_none()).map(|c| c.id.clone()).collect();
    // The same check the merge's transaction gets, on what it touches: every reference kept
    // has its object (a preview that says yes never ends in a refused Merge).
    if unanswered.is_empty() && ctx.problems.is_empty() {
        let docs = [inp.target, inp.source, inp.ancestor];
        'objects: for x in &all {
            if !holds(&m, *x) {
                continue;
            }
            for (y, _) in references(&m, *x) {
                if !holds(&m, y) {
                    ctx.problems.push(format!(
                        "{} would refer to {}, which the merge leaves out: choose differently for one of them first.",
                        name_of(&docs, *x),
                        name_of(&docs, y)
                    ));
                    break 'objects;
                }
            }
        }
    }
    let problem = ctx.problems.first().cloned();
    let ignored: Vec<String> = {
        let docs = [inp.target, inp.source, inp.ancestor];
        choices
            .keys()
            .filter(|id| !ctx.seen.contains(*id))
            .map(|id| {
                let what = id.split('/').next().and_then(|h| obj_by_hex(&docs, h)).map_or_else(|| id.clone(), |o| name_of(&docs, o));
                format!("{what}: the answer has no effect (that conflict is not raised with the other answers)")
            })
            .collect()
    };
    let ops = if unanswered.is_empty() && problem.is_none() {
        let mut scope = Touched::default();
        for o in &all {
            match *o {
                Obj::Page(i) => {
                    scope.pages.insert(i);
                }
                Obj::Folder(i) => {
                    scope.folders.insert(i);
                }
                Obj::Layer(i) => {
                    scope.layers.insert(i);
                }
                Obj::Definition(i) => {
                    scope.definitions.insert(i);
                }
                Obj::Device(i) => {
                    scope.devices.insert(i);
                }
                Obj::ConnectionPoint(i) => {
                    scope.connection_points.insert(i);
                }
                Obj::Representation(i) => {
                    scope.representations.insert(i);
                }
                Obj::Item(i) => {
                    scope.items.insert(i);
                }
                Obj::Wire(i) => {
                    scope.wires.insert(i);
                }
                Obj::Cable(i) => {
                    scope.cables.insert(i);
                }
                Obj::Route(i) => {
                    scope.routes.insert(i);
                }
                Obj::Aid(i) => {
                    scope.aids.insert(i);
                }
                Obj::LoosePart(i) => {
                    scope.loose_parts.insert(i);
                }
                Obj::Project => {}
            }
        }
        scope.page_order = true;
        scope.folder_order = true;
        diff::ops_between_in(inp.target, &m, inp.keep_project_key, Some(&scope), inp.scope.project || all.contains(&Obj::Project))
    } else {
        Vec::new()
    };
    Plan {
        changes,
        pages,
        conflicts: ctx.conflicts,
        unanswered,
        ops,
        problem,
        ignored,
        unavailable: ctx.unavailable,
        provisional_renumbered: ctx.provisional,
    }
}

/// The objects `ops` name.
pub fn objects_of(ops: &[Op]) -> BTreeSet<Obj> {
    let mut s = Scope::default();
    s.add_ops(ops);
    let t = &s.touched;
    let mut out = BTreeSet::new();
    out.extend(t.pages.iter().chain(&t.page_properties).map(|x| Obj::Page(*x)));
    out.extend(t.devices.iter().map(|x| Obj::Device(*x)));
    out.extend(t.wires.iter().map(|x| Obj::Wire(*x)));
    out.extend(t.cables.iter().map(|x| Obj::Cable(*x)));
    out.extend(t.representations.iter().map(|x| Obj::Representation(*x)));
    out.extend(t.items.iter().map(|x| Obj::Item(*x)));
    out.extend(t.connection_points.iter().map(|x| Obj::ConnectionPoint(*x)));
    out.extend(t.folders.iter().map(|x| Obj::Folder(*x)));
    if s.project {
        out.insert(Obj::Project);
    }
    out
}

/// A conflict kind's word in the merge preview's JSON.
pub fn kind_word(k: ConflictKind) -> &'static str {
    match k {
        ConflictKind::Value => "value",
        ConflictKind::Removed => "removed",
        ConflictKind::Needed => "needed",
        ConflictKind::Tag => "tag",
        ConflictKind::PageNumber => "page_number",
        ConflictKind::FolderName => "folder_name",
        ConflictKind::Order => "order",
    }
}
