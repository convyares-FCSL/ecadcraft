//! Merging devices and erasing their symbols (§3.3).

use super::*;

/// The terminals a representation shows, each with where it sits: in the definition's local frame
/// for a symbol, on the page for a drawn representation; in definition order for a symbol.
pub(crate) fn shown_connection_points(doc: &Document, c: DeviceId, r: &Representation) -> Vec<(ConnectionPointId, Pt)> {
    match r.form {
        Form::Drawn => r.connection_points.iter().map(|(t, p)| (*t, *p)).collect(),
        Form::Symbol => {
            let Some((def, _)) = rep_definition(doc, r) else { return Vec::new() };
            if r.connection_points.is_empty() {
                // A terminal no other symbol shows first (a joined terminal's key can equal the
                // definition key, review M1 2.1), else the first with the key.
                def.connection_points
                    .iter()
                    .filter_map(|td| {
                        let with_key: Vec<ConnectionPointId> =
                            doc.connection_points_of(c).filter(|(_, t)| t.key.as_ref() == Some(&td.key)).map(|(t, _)| t).collect();
                        let t = with_key.iter().find(|t| doc.representations_showing(**t).next().is_none()).or(with_key.first())?;
                        Some((*t, td.at))
                    })
                    .collect()
            } else {
                let mut v: Vec<(ConnectionPointId, Pt)> = r.connection_points.iter().map(|(t, p)| (*t, *p)).collect();
                let order = |p: &Pt| def.connection_points.iter().position(|td| td.at.near(*p, 1e-9)).unwrap_or(usize::MAX);
                v.sort_by_key(|(t, p)| (order(p), *t));
                v
            }
        }
    }
}

/// Plan merging device `from` into device `into` (design 018, owner decision 2026-11-08): every
/// representation and terminal of `from` moves to `into` (contacts, terminals and PLC channels
/// are renumbered as when joining), wires stay attached, and `from` is deleted. `into` keeps its
/// tag and properties.
///
/// # Errors
/// A label or exclusive symbol `into` already has (T3).
pub fn plan_merge(doc: &Document, ids: &mut IdAllocator, from: DeviceId, into: DeviceId) -> Result<Vec<Op>, CmdError> {
    if from == into {
        return Err(CmdError::refused("A device cannot be merged with itself."));
    }
    let from_c = doc.devices.get(&from).ok_or(CmdError::NotFound(Noun::Device))?.clone();
    if !doc.devices.contains_key(&into) {
        return Err(CmdError::NotFound(Noun::Device));
    }
    let mut assigned: BTreeSet<String> = BTreeSet::new();
    let mut new_connection_point: BTreeMap<ConnectionPointId, ConnectionPointId> = BTreeMap::new();
    let mut ops_add = Vec::new();
    let mut ops_reps = Vec::new();
    let mut exclusive_taken = crate::symbols::reps::head(doc, into).is_some();
    let who = tag_words(doc, into);
    // `into`'s point names, unlabelled points by their key (review of #92, 1).
    let into_labels: BTreeSet<String> = doc.connection_points_of(into).map(|(t, _)| label_of(doc, t)).collect();
    // The name and label `old` moves under as `label`: an unlabelled point keeping its own name
    // stays unlabelled, so it prints no label (bug 1); one whose name `into` already has is
    // renamed to the next free `<name>2`, `<name>3`… and still prints nothing. A labelled point
    // whose label is taken is refused.
    let name_for = |old: ConnectionPointId, label: String, assigned: &BTreeSet<String>| -> Result<(String, Option<String>), CmdError> {
        let taken = |n: &str| into_labels.contains(n) || assigned.contains(n);
        let unlabelled = doc.connection_points.get(&old).is_some_and(|x| x.label.is_none()) && label == label_of(doc, old);
        if !taken(&label) {
            let shown = (!unlabelled).then(|| label.clone());
            return Ok((label, shown));
        }
        if unlabelled && let Some(n) = (2..=MAX_RENAMED).map(|n| format!("{label}{n}")).find(|n| !taken(n)) {
            return Ok((n, None));
        }
        Err(CmdError::refused(format!("{who} already has connection point {label}.")))
    };
    let add = |old: ConnectionPointId,
               (name, label): (String, Option<String>),
               ops: &mut Vec<Op>,
               ids: &mut IdAllocator,
               map: &mut BTreeMap<ConnectionPointId, ConnectionPointId>|
     -> ConnectionPointId {
        let nt: ConnectionPointId = ids.fresh();
        ops.push(Op::AddConnectionPoint {
            id: nt,
            connection_point: ConnectionPoint { owner: into.into(), label, key: Some(PointKey::Def(name)), accepts: Default::default() },
        });
        map.insert(old, nt);
        nt
    };
    let reps: Vec<(RepresentationId, Representation)> = doc.representations_of(Subject::Device(from)).map(|(id, r)| (id, r.clone())).collect();
    let from_words = tag_words(doc, from);
    // Brief 031 (FP5): each symbol's own text keys move with it to its new representation.
    let mut renamed: Vec<(RepresentationId, RepresentationId)> = Vec::new();
    for (rid, r) in &reps {
        let shown = shown_connection_points(doc, from, r);
        // A symbol whose device's connection points do not match its definition (an old or
        // hand-edited file) would move without any, which the model refuses (I14) with words
        // nobody can act on: say what is wrong instead.
        if r.form == Form::Symbol && shown.is_empty() && rep_definition(doc, r).is_some_and(|(def, _)| !def.connection_points.is_empty()) {
            let page = doc.pages.get(&r.page).map_or_else(|| r.page.short(), |p| p.title.to_string());
            return Err(CmdError::refused(format!(
                "Cannot merge {from_words}: its symbol on page \"{page}\" is not linked to any of its connection points. Delete that symbol and place it again, then merge."
            )));
        }
        let starter = rep_starter(doc, r);
        let labels: Vec<String> = match starter {
            Some(s) if !matches!(s.kind, Kind::Fixed { .. }) => {
                let l = join_labels(doc, into, s, &assigned)?;
                if l.len() != shown.len() {
                    // A symbol showing fewer terminals than its definition: keep its own labels.
                    shown.iter().map(|(t, _)| label_of(doc, *t)).collect()
                } else {
                    l
                }
            }
            _ => {
                if let Some(s) = starter.filter(|s| matches!(s.kind, Kind::Fixed { exclusive: true })) {
                    if exclusive_taken {
                        return Err(CmdError::refused(format!("{who} already has a {}.", noun(s))));
                    }
                    exclusive_taken = true;
                }
                shown.iter().map(|(t, _)| label_of(doc, *t)).collect()
            }
        };
        let mut cps = BTreeMap::new();
        for ((old, at), label) in shown.iter().zip(labels) {
            let named = name_for(*old, label, &assigned)?;
            assigned.insert(named.0.clone());
            let nt = match new_connection_point.get(old) {
                Some(nt) => *nt,
                None => add(*old, named, &mut ops_add, ids, &mut new_connection_point),
            };
            cps.insert(nt, *at);
        }
        let role = r.role;
        let nr: RepresentationId = ids.fresh();
        renamed.push((*rid, nr));
        ops_reps.push((
            Op::UnlinkRepresentation { id: *rid, rep: r.clone() },
            Op::LinkRepresentation { id: nr, rep: Representation { subject: Subject::Device(into), role, connection_points: cps, ..r.clone() } },
        ));
    }
    // Terminals no representation shows move too, with their labels.
    for (t, _) in doc.connection_points_of(from) {
        if new_connection_point.contains_key(&t) {
            continue;
        }
        let named = name_for(t, label_of(doc, t), &assigned)?;
        assigned.insert(named.0.clone());
        add(t, named, &mut ops_add, ids, &mut new_connection_point);
    }
    let mut ops = ops_add;
    let (unlinks, links): (Vec<Op>, Vec<Op>) = ops_reps.into_iter().unzip();
    ops.extend(unlinks);
    ops.extend(links);
    ops.extend(crate::wires::routes::repoint_ops(doc, &new_connection_point.iter().map(|(old, nt)| (*old, *nt)).collect()));
    for (t, term) in doc.connection_points_of(from) {
        ops.push(Op::RemoveConnectionPoint { id: t, connection_point: term.clone() });
    }
    let into_props = doc.devices.get(&into).map(|c| &c.properties);
    for (old, new) in renamed {
        for (key, value) in crate::styles::text_style::symbol_text_keys(&from_c.properties, old) {
            let moved = crate::styles::text_style::rename_symbol_key(&key, old, new);
            let before = into_props.and_then(|p| p.get(&moved)).cloned();
            if before.as_ref() != Some(&value) {
                ops.push(Op::SetProperty { object: ecad_model::ObjectRef::Device(into), key: moved, before, after: Some(value) });
            }
        }
    }
    // Brief 045 (FP1 condition 2, review of #84, 1): a rack merged away hands its cards to `into`,
    // which takes its rack body (or already is a rack); otherwise they leave it, in this transaction.
    crate::plc::rack::merge_into(doc, &mut ops, from, into);
    // Design 072 §2.6: its links move to `into`.
    ops.extend(crate::devices::commands::device_links::repoint_ops(doc, from, into));
    ops.push(Op::DeleteDevice { id: from, device: from_c });
    Ok(ops)
}

/// Most `<name>2`, `<name>3`… tried for an unlabelled point a merge renames.
const MAX_RENAMED: u32 = 999;

/// A terminal's name: its label, else its definition key; its ID only when it has neither. A
/// point drawn with an empty label (a project symbol's earth point) is named by its key, never by
/// its 32-digit ID (pack re-run, bug 1).
pub fn label_of(doc: &Document, t: ConnectionPointId) -> String {
    let term = doc.connection_points.get(&t);
    term.and_then(|x| x.label.clone()).or_else(|| term.and_then(|x| x.key.as_ref()).map(ToString::to_string)).unwrap_or_else(|| t.hex())
}

// ---------------------------------------------------------------- erasing (§3.3)

/// What erasing device symbols adds to a drawing edit (design 018 §3.3): `erased` are the
/// representations of starter-symbol devices the edit unlinks, read on the document before it.
/// A device keeps its other symbols and loses the terminals only the erased ones showed (their
/// wire ends are disconnected); a device whose last symbol goes is deleted. Representations of
/// other components keep today's behaviour (the component stays, DOC-06).
pub fn erase_followups(doc: &Document, erased: &[(RepresentationId, Representation)]) -> (Vec<Op>, Vec<String>) {
    let mut by_component: BTreeMap<DeviceId, Vec<&(RepresentationId, Representation)>> = BTreeMap::new();
    for e in erased {
        if let Subject::Device(c) = e.1.subject
            && rep_starter(doc, &e.1).is_some()
        {
            by_component.entry(c).or_default().push(e);
        }
    }
    let gone: BTreeSet<RepresentationId> = erased.iter().map(|(id, _)| *id).collect();
    let (mut ops, mut notes) = (Vec::new(), Vec::new());
    // A location box (brief 024) goes with its outline.
    let boxes: BTreeSet<ecad_model::AidId> = erased
        .iter()
        .filter_map(|(_, r)| if let Subject::Aid(a) = r.subject { Some(a) } else { None })
        .filter(|a| crate::locations::box_values(doc, *a).is_some())
        .collect();
    for a in boxes {
        if doc.representations_of(Subject::Aid(a)).all(|(id, _)| gone.contains(&id))
            && let Some(aid) = doc.aids.get(&a)
        {
            ops.push(Op::DeleteAid { id: a, aid: aid.clone() });
        }
    }
    for (c, reps) in by_component {
        let Some(component) = doc.devices.get(&c) else { continue };
        let remaining: Vec<&Representation> =
            doc.representations_of(Subject::Device(c)).filter(|(id, _)| !gone.contains(id)).map(|(_, r)| r).collect();
        let who = tag_words(doc, c);
        let drop: Vec<ConnectionPointId> = if remaining.is_empty() {
            doc.connection_points_of(c).map(|(t, _)| t).collect()
        } else {
            let kept: BTreeSet<ConnectionPointId> = remaining.iter().flat_map(|r| shown_connection_points(doc, c, r)).map(|(t, _)| t).collect();
            reps.iter()
                .flat_map(|(_, r)| shown_connection_points(doc, c, r))
                .map(|(t, _)| t)
                .filter(|t| !kept.contains(t))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        ops.extend(crate::wires::routes::free_points_ops(doc, drop.iter().copied()));
        for t in &drop {
            if let Some(term) = doc.connection_points.get(t) {
                ops.push(Op::RemoveConnectionPoint { id: *t, connection_point: term.clone() });
            }
        }
        if remaining.is_empty() {
            // Brief 045 (design 045 §2.2, FP1): a rack's cards leave it in the same transaction.
            let (released, n) =
                if crate::plc::edit::station_of(doc, c).is_some() { crate::plc::rack::release_cards(doc, c) } else { (Vec::new(), 0) };
            ops.extend(released);
            ops.extend(crate::devices::commands::device_links::unlink_ops(doc, c));
            ops.push(Op::DeleteDevice { id: c, device: component.clone() });
            if n > 0 {
                notes.push(format!("Deleted {who}; {}", crate::plc::rack::released_words(n)));
            }
            notes.push(format!("Deleted {who} with its last symbol; U brings it back."));
            notes.extend(crate::circuits::trace::cleared_words(doc, c).map(|w| format!("{who}: {w}")));
        } else {
            // Brief 036 (review M1): a level's own label and a fuse position's rating go with
            // the last terminal of their level or position, never left to come back later.
            let (key_ops, key_notes) = crate::terminal_strips::terminal_blocks::orphaned_key_ops(doc, c, &drop);
            ops.extend(key_ops);
            notes.extend(key_notes);
            // Brief 031 (FP5): an erased symbol's own text keys go with it.
            for (rid, _) in &reps {
                for (key, value) in crate::styles::text_style::symbol_text_keys(&component.properties, *rid) {
                    ops.push(Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key, before: Some(value), after: None });
                }
            }
            for (_, r) in &reps {
                let name = rep_starter(doc, r).map_or("symbol", |s| s.name);
                let labels: Vec<String> = shown_connection_points(doc, c, r).iter().map(|(t, _)| label_of(doc, *t)).collect();
                notes.push(format!(
                    "Removed {who}'s {name} {} ({}); {who} is still shown on {}.",
                    labels.join("/"),
                    crate::references::cables::page_number_words(doc, r.page),
                    shown_pages_words(doc, &remaining)
                ));
            }
        }
    }
    (ops, notes)
}

fn shown_pages_words(doc: &Document, reps: &[&Representation]) -> String {
    let pages: BTreeSet<PageId> = reps.iter().map(|r| r.page).collect();
    let n: Vec<String> = doc.page_order.iter().enumerate().filter(|(_, p)| pages.contains(p)).map(|(i, _)| (i + 1).to_string()).collect();
    if n.len() == 1 { format!("page {}", n.join(", ")) } else { format!("pages {}", n.join(", ")) }
}

/// The tag a copy of device `c` gets (design 018 §3.3): the next automatic number of its prefix;
/// `None` when its tag is not of the usual form (the copy then has no tag).
pub fn copy_tag(doc: &Document, high: &mut TagHigh, c: &Device) -> Option<tag::Given> {
    let (prefix, _) = tag::parse(c.tag()?)?;
    high.next_auto_in(doc, &prefix).ok()
}
