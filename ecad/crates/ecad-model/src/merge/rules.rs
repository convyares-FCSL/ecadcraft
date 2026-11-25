//! The rules the result must keep: needed objects, tags, page numbers, folder names.

use std::collections::{BTreeMap, BTreeSet};

use crate::document::op::Op;
use crate::naming::id::{CableId, DeviceId, FolderId, PageId};
use crate::naming::tag::{self, HighWater};
use crate::objects::route::RouteEnd;
use crate::{Cable, CorePosition, Device, Document, End, Form, ItemKind, Subject};

use super::{conflicts::*, state::*};
use crate::document::doc::cable::join_cables;

/// What `obj` in `m` refers to, and how.
pub(super) fn references(m: &Document, obj: Obj) -> Vec<(Obj, Use)> {
    let mut out = Vec::new();
    match obj {
        Obj::Item(i) => {
            if let Some(it) = m.items.get(&i) {
                out.push((Obj::Page(it.page), Use::Whole));
                out.push((Obj::Layer(it.style.layer), Use::Whole));
                if let ItemKind::Symbol(sym) = &it.kind {
                    out.push((Obj::Definition(sym.definition), Use::Whole));
                }
            }
        }
        Obj::Device(c) => {
            if let Some(d) = m.devices.get(&c).and_then(|c| c.definition) {
                out.push((Obj::Definition(d), Use::DeviceDefinition));
            }
        }
        Obj::ConnectionPoint(t) => {
            if let Some(t) = m.connection_points.get(&t) {
                out.push((owner_obj(t.owner), Use::Whole));
            }
        }
        Obj::Representation(r) => {
            if let Some(r) = m.representations.get(&r) {
                out.push((
                    match r.subject {
                        Subject::Device(c) => Obj::Device(c),
                        Subject::Wire(c) => Obj::Wire(c),
                        Subject::Cable(c) => Obj::Cable(c),
                        Subject::Route(c) => Obj::Route(c),
                        Subject::Aid(c) => Obj::Aid(c),
                    },
                    Use::Whole,
                ));
                out.push((Obj::Page(r.page), Use::Whole));
                out.extend(r.items.iter().map(|i| (Obj::Item(*i), Use::RepItem)));
                out.extend(r.connection_points.keys().map(|t| (Obj::ConnectionPoint(*t), Use::ConnectionPoint)));
            }
        }
        Obj::Wire(c) => {
            if let Some(c) = m.wires.get(&c) {
                for e in End::BOTH {
                    out.push((Obj::ConnectionPoint(c.end(e)), Use::End(e)));
                }
            }
        }
        Obj::Cable(c) => {
            if let Some(c) = m.cables.get(&c) {
                out.extend(c.cores.iter().filter_map(|x| match x.position {
                    CorePosition::Wire(w) => Some((Obj::Wire(w), Use::Member)),
                    CorePosition::Route { route, .. } => Some((Obj::Route(route), Use::Member)),
                    CorePosition::Unplaced => None,
                }));
            }
        }
        Obj::Page(p) => {
            if let Some(f) = crate::document::book::folder_of(m, p) {
                out.push((Obj::Folder(f), Use::PageFolder));
            }
        }
        Obj::Folder(f) => {
            if let Some(p) = m.folders.get(&f).and_then(|f| f.parent) {
                out.push((Obj::Folder(p), Use::FolderParent));
            }
        }
        Obj::Route(r) => {
            if let Some(r) = m.routes.get(&r) {
                out.push((Obj::Page(r.page), Use::Whole));
                for (i, e) in r.ends.iter().enumerate() {
                    match e {
                        RouteEnd::Point(t) => out.push((Obj::ConnectionPoint(*t), Use::RouteEnd(i))),
                        RouteEnd::On { route, .. } => out.push((Obj::Route(*route), Use::RouteEnd(i))),
                        RouteEnd::Free => {}
                    }
                }
            }
        }
        Obj::Layer(_) | Obj::Definition(_) | Obj::Aid(_) | Obj::LoosePart(_) | Obj::Project => {}
    }
    out
}

/// Every object of `m` that refers to `y` (a scan: used only when a "remove what uses it"
/// choice removes an object whole).
pub(super) fn users_of(m: &Document, y: Obj) -> Vec<Obj> {
    let mut all: Vec<Obj> = Vec::new();
    match y {
        Obj::ConnectionPoint(t) => {
            all.extend(m.wires.iter().filter(|(_, c)| c.ends.contains(&t)).map(|(k, _)| Obj::Wire(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.connection_points.contains_key(&t)).map(|(k, _)| Obj::Representation(*k)));
            all.extend(m.routes.iter().filter(|(_, r)| r.ends.contains(&RouteEnd::Point(t))).map(|(k, _)| Obj::Route(*k)));
        }
        Obj::Aid(x) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Aid(x)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Route(x) => {
            all.extend(m.routes.iter().filter(|(_, r)| r.ends.iter().any(|e| e.route() == Some(x))).map(|(k, _)| Obj::Route(*k)));
            all.extend(m.cables.iter().filter(|(_, c)| c.cores.iter().any(|mm| mm.route() == Some(x))).map(|(k, _)| Obj::Cable(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Route(x)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Device(c) => {
            all.extend(m.connection_points.iter().filter(|(_, t)| t.device() == Some(c)).map(|(k, _)| Obj::ConnectionPoint(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Device(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Wire(c) => {
            all.extend(m.cables.iter().filter(|(_, x)| x.cores.iter().any(|mm| mm.wire() == Some(c))).map(|(k, _)| Obj::Cable(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Wire(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Cable(c) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Cable(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Representation(_) | Obj::LoosePart(_) | Obj::Project => {}
        Obj::Item(i) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.items.contains(&i)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Page(p) => {
            all.extend(m.items.iter().filter(|(_, x)| x.page == p).map(|(k, _)| Obj::Item(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.page == p).map(|(k, _)| Obj::Representation(*k)));
            all.extend(m.routes.iter().filter(|(_, r)| r.page == p).map(|(k, _)| Obj::Route(*k)));
        }
        Obj::Layer(l) => all.extend(m.items.iter().filter(|(_, x)| x.style.layer == l).map(|(k, _)| Obj::Item(*k))),
        Obj::Definition(d) => {
            all.extend(m.items.iter().filter(|(_, x)| matches!(&x.kind, ItemKind::Symbol(s) if s.definition == d)).map(|(k, _)| Obj::Item(*k)));
            all.extend(m.devices.iter().filter(|(_, c)| c.definition == Some(d)).map(|(k, _)| Obj::Device(*k)));
        }
        Obj::Folder(f) => {
            all.extend(m.folders.iter().filter(|(_, x)| x.parent == Some(f)).map(|(k, _)| Obj::Folder(*k)));
            all.extend(m.pages.keys().filter(|p| crate::document::book::folder_of(m, **p) == Some(f)).map(|k| Obj::Page(*k)));
        }
    }
    all
}

/// Stop `x`'s use of `y` (it was not brought back): detach, drop the entry, or remove `x`.
pub(super) fn drop_use(m: &mut Document, x: Obj, y: Obj, how: Use, check: &mut BTreeSet<Obj>) {
    match (x, how) {
        // A wire runs between two connection points: without one it goes.
        (Obj::Wire(_), Use::End(_)) => remove_whole(m, x, check),
        (Obj::Route(r), Use::RouteEnd(i)) => {
            if let Some(e) = m.routes.get_mut(&r).and_then(|r| r.ends.get_mut(i)) {
                *e = RouteEnd::Free;
            }
        }
        (Obj::Representation(r), Use::ConnectionPoint) => {
            if let (Some(r), Obj::ConnectionPoint(t)) = (m.representations.get_mut(&r), y) {
                r.connection_points.remove(&t);
            }
        }
        (Obj::Cable(c), Use::Member) => match (m.cables.get_mut(&c), y) {
            (Some(c), Obj::Wire(w)) => off_line(&mut c.cores, |mm| mm.wire() == Some(w)),
            (Some(c), Obj::Route(r)) => off_line(&mut c.cores, |mm| mm.route() == Some(r)),
            _ => {}
        },
        (Obj::Page(p), Use::PageFolder) => {
            if let Some(p) = m.pages.get_mut(&p) {
                p.folder = None;
            }
        }
        (Obj::Folder(f), Use::FolderParent) => {
            if let Some(f) = m.folders.get_mut(&f) {
                f.parent = None;
            }
        }
        (Obj::Device(c), Use::DeviceDefinition) => {
            if let Some(c) = m.devices.get_mut(&c) {
                c.definition = None;
            }
        }
        (Obj::Representation(r), Use::RepItem) => {
            let symbol = m.representations.get(&r).is_some_and(|x| x.form == Form::Symbol);
            if symbol {
                remove_whole(m, x, check);
            } else if let (Some(rep), Obj::Item(i)) = (m.representations.get_mut(&r), y) {
                rep.items.retain(|k| *k != i);
            }
        }
        _ => remove_whole(m, x, check),
    }
}

/// Remove `x` from `m`; what used it is checked again.
pub(super) fn remove_whole(m: &mut Document, x: Obj, check: &mut BTreeSet<Obj>) {
    let users = users_of(m, x);
    let empty = Document::default();
    copy(m, &empty, x);
    check.extend(users);
}

/// Give every reference kept in `m` a target: each missing object is a **needed** conflict
/// (bring it back, or leave unconnected / remove what uses it).
pub(super) fn resolve_needed(ctx: &mut Ctx<'_, '_>, m: &mut Document, mut check: BTreeSet<Obj>) -> BTreeSet<Obj> {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    let mut changed: BTreeSet<Obj> = BTreeSet::new();
    // What was brought back here, and what could not be settled (contradicting answers).
    let mut brought: BTreeSet<Obj> = BTreeSet::new();
    let mut stuck: BTreeSet<Obj> = BTreeSet::new();
    for _ in 0..64 {
        let mut missing: BTreeMap<Obj, Vec<(Obj, Use)>> = BTreeMap::new();
        for x in &check {
            if !holds(m, *x) {
                continue;
            }
            for (y, how) in references(m, *x) {
                if !holds(m, y) && !stuck.contains(&y) {
                    missing.entry(y).or_default().push((*x, how));
                }
            }
        }
        if missing.is_empty() {
            break;
        }
        changed.extend(check.iter().copied());
        let mut next: BTreeSet<Obj> = BTreeSet::new();
        for (y, users) in missing {
            // Whoever still has it: the source, else the target, else the ancestor.
            let from = [s, t, a].into_iter().find(|d| holds(d, y));
            let docs = [t, s, a];
            let user_names: Vec<String> = users.iter().take(3).map(|(x, _)| name_of(&docs, *x)).collect();
            let more = users.len().saturating_sub(3);
            let removed_on = if holds(t, y) { ctx.s().to_string() } else { ctx.t().to_string() };
            let only_ends = users.iter().all(|(_, u)| matches!(u, Use::ConnectionPoint));
            let mut choices = Vec::new();
            if from.is_some() {
                choices.push(Choice::BringBack);
            }
            choices.push(Choice::Drop);
            let c = Conflict {
                id: format!("{}/needed", y.hex()),
                kind: ConflictKind::Needed,
                object: y,
                what: format!(
                    "{} · removed on {removed_on}; still used by {}{}",
                    name_of(&docs, y),
                    user_names.join(", "),
                    if more > 0 { format!(" and {more} more") } else { String::new() }
                ),
                target: if only_ends { "leave unconnected".into() } else { "remove what uses it".into() },
                source: "bring it back".into(),
                choices,
                chosen: None,
            };
            let chosen = ctx.ask(c);
            // Bringing something back that needs `y` while `y`'s answer removes what uses it
            // contradicts itself: say so, before Merge.
            if matches!(chosen, Some(Choice::Drop))
                && let Some((x, _)) = users.iter().find(|(x, _)| brought.contains(x))
            {
                ctx.problems.push(format!(
                    "Bringing back {} needs {}: choose Bring it back for {} too, or leave {} out, first.",
                    name_of(&docs, *x),
                    name_of(&docs, y),
                    name_of(&docs, y),
                    name_of(&docs, *x)
                ));
                stuck.insert(y);
                continue;
            }
            match (chosen, from) {
                (Some(Choice::Drop), _) | (_, None) => {
                    for (x, how) in users {
                        drop_use(m, x, y, how, &mut next);
                        next.insert(x);
                    }
                }
                // Brought back (also provisionally while unanswered, so what it needs shows).
                (_, Some(d)) => {
                    copy(m, d, y);
                    brought.insert(y);
                    next.insert(y);
                    changed.insert(y);
                }
            }
        }
        check = next;
    }
    changed.extend(check);
    changed
}

/// The next free tag with `prefix` (above `high` and every tag already given out here).
pub(super) fn next_tag(prefix: &str, high: &mut HighWater, style: tag::Style) -> String {
    let n = high.get(prefix).copied().unwrap_or(0).saturating_add(1);
    high.insert(prefix.to_string(), n);
    tag::format(prefix, n, style)
}

/// What holds a tag in the merged state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Holder {
    Device(DeviceId),
    Cable(CableId),
}

impl Holder {
    fn obj(self) -> Obj {
        match self {
            Holder::Device(c) => Obj::Device(c),
            Holder::Cable(c) => Obj::Cable(c),
        }
    }

    fn tag(self, d: &Document) -> Option<&str> {
        match self {
            Holder::Device(c) => d.devices.get(&c).and_then(|x| x.tag()),
            Holder::Cable(c) => d.cables.get(&c).and_then(|x| x.tag()),
        }
    }

    /// This holder's tag identity (its text and state).
    fn key(self, d: &Document) -> Option<tag::Key> {
        match self {
            Holder::Device(c) => d.devices.get(&c).and_then(Device::tag_key),
            Holder::Cable(c) => d.cables.get(&c).and_then(Cable::tag_key),
        }
    }

    /// This holder's tag state.
    fn state(self, d: &Document) -> tag::TagState {
        match self {
            Holder::Device(c) => d.devices.get(&c).and_then(|x| x.tag.state()),
            Holder::Cable(c) => d.cables.get(&c).and_then(|x| x.tag.state()),
        }
        .unwrap_or_default()
    }

    /// Give this holder bare tag text `t` in `state`.
    fn set_tag(self, m: &mut Document, t: String, state: tag::TagState) {
        match self {
            Holder::Device(c) => {
                if let Some(x) = m.devices.get_mut(&c) {
                    *x = x.with_tag(Some(&t), state);
                }
            }
            Holder::Cable(c) => {
                if let Some(x) = m.cables.get_mut(&c) {
                    x.tag = tag::TagSlot::of_text(Some(&t), state);
                }
            }
        }
    }
}

/// The highest provisional number of `prefix` in `m` (a scan: merges only).
pub(super) fn top_provisional(m: &Document, prefix: &str) -> u64 {
    m.devices
        .values()
        .map(|c| &c.tag)
        .chain(m.cables.values().map(|c| &c.tag))
        .filter(|s| s.is_provisional())
        .filter_map(|s| s.tag().and_then(tag::parse))
        .filter(|(p, _)| p == prefix)
        .map(|(_, n)| n)
        .max()
        .unwrap_or(0)
}

/// Two holders with one tag (one tag, one device; devices and cables share the namespace) after the
/// merge: a **tag** conflict per tag, renumbering the source's (the default suggestion)
/// or the target's, or making them one. Provisional tags added on both
/// sides are renumbered silently.
pub(super) fn resolve_tags(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &BTreeSet<Obj>) -> BTreeSet<Obj> {
    let t = ctx.inp.target;
    let mut high = ctx.inp.high.clone();
    let present: Vec<String> = m
        .devices
        .values()
        .filter(|c| !c.is_provisional())
        .filter_map(|c| c.tag())
        .chain(m.cables.values().filter(|c| !c.is_provisional()).filter_map(|c| c.tag()))
        .map(str::to_string)
        .collect();
    for x in &present {
        tag::note(&mut high, x);
    }
    // The merged project's tag style (padding, dash, case).
    let style = tag::Style::of(m);
    let mut out = BTreeSet::new();
    let mut done: BTreeSet<tag::Key> = BTreeSet::new();
    let ours: Vec<Holder> = objects
        .iter()
        .filter_map(|o| match o {
            Obj::Device(c) => Some(Holder::Device(*c)),
            Obj::Cable(c) => Some(Holder::Cable(*c)),
            _ => None,
        })
        .collect();
    // The merged state with a current index, built once, for the join.
    let mut indexed: Option<Document> = None;
    for h in ours.iter().copied() {
        let Some(tag_text) = h.tag(m).map(str::to_string) else { continue };
        let Some(key) = h.key(m) else { continue };
        // Only a tag the merge brought from the source.
        if h.key(t).as_ref() == Some(&key) {
            continue;
        }
        if done.contains(&key) {
            continue;
        }
        let same = |d: Holder| d != h && d.key(m).as_ref() == Some(&key);
        let mut others: Vec<Holder> = t
            .components_with(&key)
            .into_iter()
            .map(Holder::Device)
            .chain(t.cables_with(&key).into_iter().map(Holder::Cable))
            .filter(|d| same(*d))
            .collect();
        let more: Vec<Holder> = ours.iter().copied().filter(|d| same(*d) && !others.contains(d)).collect();
        others.extend(more);
        let Some(other) = others.first().copied() else { continue };
        done.insert(key.clone());
        // The source's holder and the target's: `h`'s tag came from the source (the loop skips
        // every holder whose tag the target already had), also when the target has `h` itself
        // under an older tag.
        let (src, tgt) = (h, other);
        // Provisional duplicates were never issued: the source's takes the next one, silently.
        if key.provisional
            && let Some((p, _)) = tag::parse(&tag_text)
        {
            let n = top_provisional(m, &p).saturating_add(1);
            src.set_tag(m, style.provisional(&p, n), tag::TagState::Provisional);
            // The indexed copy is stale now: rebuilt when next needed.
            indexed = None;
            out.insert(src.obj());
            ctx.provisional += 1;
            continue;
        }
        let prefix = tag::parse(&tag_text).map(|p| p.0);
        let fresh = match &prefix {
            Some(p) => next_tag(p, &mut high, style),
            None => format!("{tag_text}-2"),
        };
        let id = format!("rule/tag/{}", key.text);
        // Make them one: two devices or two cables, never a device and a cable.
        let one: Result<Vec<Op>, RuleError> = match (src, tgt) {
            (Holder::Device(a), Holder::Device(b)) => {
                let d = indexed.get_or_insert_with(|| {
                    let mut x = m.clone();
                    x.rebuild_index();
                    x
                });
                (ctx.inp.join)(d, a, b)
            }
            (Holder::Cable(a), Holder::Cable(b)) => {
                let d = indexed.get_or_insert_with(|| {
                    let mut x = m.clone();
                    x.rebuild_index();
                    x
                });
                join_cables(d, a, b).map_err(RuleError::from)
            }
            _ => Err(JoinError::CableAndDevice(tag_text.to_string()).into()),
        };
        let mut choices = vec![Choice::RenumberSource, Choice::RenumberTarget];
        match &one {
            Ok(_) => choices.push(Choice::MakeOne),
            Err(why) => {
                ctx.unavailable.insert(id.clone(), why.to_string());
            }
        }
        let noun = match h {
            Holder::Device(_) => "device",
            Holder::Cable(_) => "cable",
        };
        let conflict = Conflict {
            id: id.clone(),
            kind: ConflictKind::Tag,
            object: h.obj(),
            what: format!("{tag_text} · {} and {} each have a {noun} {tag_text}", ctx.t(), ctx.s()),
            target: format!("renumber {}'s to {fresh}", ctx.t()),
            source: format!("renumber {}'s to {fresh}", ctx.s()),
            choices,
            chosen: None,
        };
        let asked = ctx.ask(conflict);
        if asked.is_none()
            && ctx.choices.get(&id) == Some(&Choice::MakeOne)
            && let Some(why) = ctx.unavailable.get(&id).cloned()
        {
            ctx.problems.push(format!("{tag_text}: they cannot be one ({why}) Renumber one of them instead."));
        }
        let which = match asked {
            Some(Choice::RenumberSource) => Some(src),
            Some(Choice::RenumberTarget) => Some(tgt),
            Some(Choice::MakeOne) => {
                if let Ok(ops) = one {
                    for op in &ops {
                        if let Err(e) = op.apply(m) {
                            ctx.problems.push(format!("{tag_text}: they cannot be one ({e})."));
                            break;
                        }
                        if let Some(d) = indexed.as_mut() {
                            let _ = op.apply(d);
                        }
                    }
                    out.extend(objects_of(&ops));
                    out.insert(tgt.obj());
                }
                None
            }
            _ => None,
        };
        if let Some(d) = which {
            let state = d.state(m);
            d.set_tag(m, fresh, state);
            out.insert(d.obj());
            indexed = None;
        }
    }
    out
}

// The next free page number after `n`.
pub(super) fn next_number(n: &str, taken: &BTreeSet<String>, step: u32) -> String {
    if let Ok(v) = n.trim().parse::<u64>() {
        let mut x = v;
        for _ in 0..10_000 {
            x = x.saturating_add(u64::from(step.max(1)));
            if !taken.contains(&x.to_string()) {
                return x.to_string();
            }
        }
    }
    (2..10_000).map(|i| format!("{n}.{i}")).find(|c| !taken.contains(c)).unwrap_or_else(|| format!("{n}.x"))
}

/// Two pages with one number after the merge: a **page number** conflict, renumbering
/// the source's page or the target's to the next free number with the project's step.
pub(super) fn resolve_page_numbers(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &BTreeSet<Obj>) -> BTreeSet<Obj> {
    let (t, s) = (ctx.inp.target, ctx.inp.source);
    let mut out = BTreeSet::new();
    let mut taken: BTreeSet<String> = m.pages.keys().filter_map(|p| crate::document::book::stored_number(m, *p)).map(str::to_string).collect();
    let step = m.settings.page_number_step();
    let ours: Vec<PageId> = objects.iter().filter_map(|o| if let Obj::Page(p) = o { Some(*p) } else { None }).collect();
    for p in ours {
        let Some(num) = crate::document::book::stored_number(m, p).map(str::to_string) else { continue };
        if crate::document::book::stored_number(t, p) == Some(num.as_str()) {
            continue;
        }
        let Some(other) = m.pages.keys().copied().find(|q| *q != p && crate::document::book::stored_number(m, *q) == Some(num.as_str())) else {
            continue;
        };
        let fresh = next_number(&num, &taken, step);
        let c = Conflict {
            id: format!("rule/page/{num}"),
            kind: ConflictKind::PageNumber,
            object: Obj::Page(p),
            what: format!("Page {num} · {} and {} each have a page {num}", ctx.t(), ctx.s()),
            target: format!("renumber {}'s to {fresh}", ctx.t()),
            source: format!("renumber {}'s to {fresh}", ctx.s()),
            choices: vec![Choice::RenumberSource, Choice::RenumberTarget],
            chosen: None,
        };
        let src_is_p = s.pages.contains_key(&p) && !t.pages.contains_key(&p) || crate::document::book::stored_number(s, p) == Some(num.as_str());
        let which = match ctx.ask(c) {
            Some(Choice::RenumberSource) => Some(if src_is_p { p } else { other }),
            Some(Choice::RenumberTarget) => Some(if src_is_p { other } else { p }),
            _ => None,
        };
        if let Some(q) = which
            && let Some(page) = m.pages.get_mut(&q)
        {
            page.number = crate::document::book::PageNo::parse(&fresh);
            taken.insert(fresh);
            out.insert(Obj::Page(q));
        }
    }
    out
}

/// Two sibling folders with one name after the merge: a **folder name** conflict, renaming the
/// source's (`Spare (2)`) or the target's.
pub(super) fn resolve_folder_names(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &BTreeSet<Obj>) -> BTreeSet<Obj> {
    let (t, s) = (ctx.inp.target, ctx.inp.source);
    let mut out = BTreeSet::new();
    let ours: Vec<FolderId> = objects.iter().filter_map(|o| if let Obj::Folder(f) = o { Some(*f) } else { None }).collect();
    for f in ours {
        let Some(me) = m.folders.get(&f).cloned() else { continue };
        if t.folders.get(&f) == Some(&me) {
            continue;
        }
        let lower = me.name.to_lowercase();
        let Some(other) = m.folders.iter().find(|(k, x)| **k != f && x.parent == me.parent && x.name.to_lowercase() == lower).map(|(k, _)| *k) else {
            continue;
        };
        let fresh = (2..1_000)
            .map(|i| format!("{} ({i})", me.name))
            .find(|n| !m.folders.values().any(|x| x.parent == me.parent && x.name.to_lowercase() == n.to_lowercase()))
            .unwrap_or_else(|| format!("{} (new)", me.name));
        let c = Conflict {
            id: format!("rule/folder/{}", f.hex()),
            kind: ConflictKind::FolderName,
            object: Obj::Folder(f),
            what: format!("Folder \"{}\" · added on both", me.name),
            target: format!("rename {}'s to \"{fresh}\"", ctx.t()),
            source: format!("rename {}'s to \"{fresh}\"", ctx.s()),
            choices: vec![Choice::RenumberSource, Choice::RenumberTarget],
            chosen: None,
        };
        let src = s.folders.contains_key(&f) && !t.folders.contains_key(&f);
        let which = match ctx.ask(c) {
            Some(Choice::RenumberSource) => Some(if src { f } else { other }),
            Some(Choice::RenumberTarget) => Some(if src { other } else { f }),
            _ => None,
        };
        if let Some(q) = which
            && let Some(x) = m.folders.get_mut(&q)
        {
            x.name = fresh;
            out.insert(Obj::Folder(q));
        }
    }
    out
}
