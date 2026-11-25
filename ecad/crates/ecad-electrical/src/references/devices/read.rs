//! Reading the derived references: the list, the JSON forms, go-to targets and the lines the panels show.

use crate::CmdError;
use crate::devices::traits::DeviceKind;
use ecad_model::look::Look;
use ecad_model::objects::devices::ContactTable;
use ecad_model::{CableId, Device as StoredDevice, DeviceId, DeviceType, Document, PageId, Pt, RepresentationId, Subject};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::classify::{Class, Device, Group, anchor, device};
use super::index::RefIndex;
use super::parse::{Placement, offer_of, placement_of};
use super::places::Places;
use super::table::{State, counts, drawn_table, enters_title_block, kind_words, rows, table_box, table_measured, unit};

/// Refusal of an edit of the text under a contact.
pub const TEXT_DERIVED: &str = "The reference text is derived; move the coil or the contact instead.";
/// Refusal of an edit of a contact table.
pub const TABLE_DERIVED: &str = "The contact table is derived; move it with Move Mirror Table, or set it in Properties.";

// ---------------------------------------------------------------- reading

/// One reference as `references` lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub source: RepresentationId,
    pub source_page: PageId,
    pub target: RepresentationId,
    pub target_page: PageId,
    /// `cable`, `contact` (a contact shows its coil), `mirror` (a table row shows a contact) or
    /// `continuation` (an arrow shows another arrow of its name).
    pub kind: &'static str,
    /// The contact pole it is about (`13/14`), for device references: one per pole.
    pub labels: Option<String>,
    pub text: String,
}

/// The references of the groups `groups`, in group order, then source, then target.
pub fn references(doc: &Document, idx: &RefIndex, groups: &BTreeSet<Group>) -> Vec<Reference> {
    let mut out = Vec::new();
    let mut places = Places::new(doc);
    for g in groups {
        let mut cable_texts: BTreeMap<RepresentationId, String> = BTreeMap::new();
        if let Group::Cable(k) = g {
            for r in ecad_model::connectivity::derive::cable_references_of(doc, &BTreeSet::from([*k])) {
                cable_texts.insert(r.reference, r.text);
            }
        }
        let dev = match g {
            Group::Device(c) => Some(device(doc, *c)),
            Group::Cable(_) | Group::Continuation(_) => None,
        };
        for s in idx.members(*g) {
            for t in idx.targets(s) {
                let (Some(sr), Some(tr)) = (doc.representations().get(&s), doc.representations().get(&t)) else { continue };
                let r = |kind, labels, text| Reference { source: s, source_page: sr.page, target: t, target_page: tr.page, kind, labels, text };
                // A device reference is listed once per pole: a table row per pole, and a
                // contact's text for each of its poles.
                let poles = |rep: RepresentationId| -> Vec<String> {
                    dev.iter().flat_map(|d| d.poles.iter()).filter(|p| p.rep == rep).map(|p| format!("{}/{}", p.labels[0], p.labels[1])).collect()
                };
                match (g, idx.member(s).map(|(_, c)| c)) {
                    (Group::Cable(_), _) => out.push(r("cable", None, cable_texts.get(&s).cloned().unwrap_or_default())),
                    // An arrow shows each other end's place.
                    (Group::Continuation(_), _) => out.push(r("continuation", None, places.text(tr.page, anchor(doc, tr)))),
                    // A connector half shows its mate half's place, a pin its
                    // mate pin's (labelled with the pin name).
                    (Group::Device(_), Some(Class::Half(_))) => out.push(r("half", None, places.text(tr.page, anchor(doc, tr)))),
                    (Group::Device(_), Some(Class::Pin(_))) => {
                        let name = sr.connection_points.keys().find_map(|t| doc.connection_points().get(t).and_then(|x| x.label.clone()));
                        out.push(r("pin", name, places.text(tr.page, anchor(doc, tr))));
                    }
                    (Group::Device(_), Some(Class::Head)) => {
                        let place = places.text(tr.page, anchor(doc, tr));
                        out.extend(poles(t).into_iter().map(|l| r("mirror", Some(l), place.clone())));
                    }
                    (Group::Device(_), _) => {
                        let place = places.text(tr.page, anchor(doc, tr));
                        out.extend(poles(s).into_iter().map(|l| r("contact", Some(l), place.clone())));
                    }
                }
            }
        }
    }
    out
}

/// `references {component?, cable?, page?, limit?, after?}`'s result.
///
/// Continuation rows come after the others, one per (arrow, other end), made only for
/// the window asked for: with `component`, that arrow's; with `page`, those of the arrows on the
/// page (a row whose source is elsewhere is listed from its source). O(arrows listed + rows
/// returned), never O(m²) for a name of m arrows.
///
/// # Errors
/// Unknown component, continuation, cable or page.
pub fn device_references_json(
    doc: &Document, idx: &RefIndex, component: Option<DeviceId>, aid: Option<ecad_model::AidId>, cable: Option<CableId>, page: Option<PageId>,
    limit: usize, after: usize,
) -> Result<Value, CmdError> {
    let groups: BTreeSet<Group> = match (component, cable) {
        // An arrow lists its other ends.
        (None, None) if let Some(a) = aid => {
            crate::continuations::continuation(doc, a).ok_or(CmdError::refused(crate::continuations::NO_SUCH_ARROW))?;
            crate::continuations::name_of(doc, a).and_then(|n| crate::continuations::group_key(doc, &n)).into_iter().collect()
        }
        (Some(c), _) => {
            if !doc.devices().contains_key(&c) {
                return Err(CmdError::NotFound(ecad_model::Noun::Device));
            }
            BTreeSet::from([Group::Device(c)])
        }
        (None, Some(k)) => {
            if !doc.cables().contains_key(&k) {
                return Err(CmdError::NotFound(ecad_model::Noun::Cable));
            }
            BTreeSet::from([Group::Cable(k)])
        }
        (None, None) => match page {
            Some(p) => {
                if !doc.pages().contains_key(&p) {
                    return Err(CmdError::NotFound(ecad_model::Noun::Page));
                }
                doc.representations_on(p).filter_map(|(r, _)| idx.member(r).map(|(g, _)| g)).collect()
            }
            None => idx.groups().collect(),
        },
    };
    // Continuation rows are made per source and only for the
    // window asked for, never all m(m−1) of a name; the other groups' rows as before.
    let (cont, other): (BTreeSet<Group>, BTreeSet<Group>) = groups.into_iter().partition(|g| matches!(g, Group::Continuation(_)));
    let all: Vec<Reference> =
        references(doc, idx, &other).into_iter().filter(|r| page.is_none_or(|p| r.source_page == p || r.target_page == p)).collect();
    // The arrows whose rows are listed: the given arrow's, the arrows on `page`, or every arrow.
    let sources: Vec<(RepresentationId, usize)> = match (component, page) {
        (Some(c), _) => doc.representations_of(Subject::Device(c)).map(|(r, _)| r).collect::<Vec<_>>(),
        (None, _) if let Some(a) = aid => doc.representations_of(Subject::Aid(a)).map(|(r, _)| r).collect::<Vec<_>>(),
        (None, Some(p)) => doc.representations_on(p).map(|(r, _)| r).collect(),
        (None, None) => cont.iter().flat_map(|g| idx.members(*g)).collect(),
    }
    .into_iter()
    .filter_map(|r| match idx.member(r) {
        Some((g @ Group::Continuation(_), _)) if cont.contains(&g) => Some((r, idx.group_len(g).saturating_sub(1))),
        _ => None,
    })
    .collect();
    let total = sources.iter().fold(all.len(), |n, (_, k)| n.saturating_add(*k));
    let row = |r: &Reference| {
        json!({
            "source": r.source.hex(), "source_page": crate::references::cables::page_number(doc, r.source_page),
            "target": r.target.hex(), "target_page": crate::references::cables::page_number(doc, r.target_page),
            "kind": r.kind, "labels": r.labels, "text": r.text,
        })
    };
    let end = after.saturating_add(limit);
    let mut list: Vec<Value> = all.iter().skip(after).take(limit).map(row).collect();
    // Walk the sources, making rows only for those that overlap [after, end).
    let mut at = all.len();
    let mut places = Places::new(doc);
    let mut ends: BTreeMap<String, Vec<crate::continuations::End>> = BTreeMap::new();
    for (s, k) in &sources {
        if at >= end {
            break;
        }
        let next = at.saturating_add(*k);
        if next > after
            && let Some(sr) = doc.representations().get(s)
            && let Subject::Aid(c) = sr.subject
            && let Some(name) = crate::continuations::name_of(doc, c)
        {
            let sorted = ends.entry(name.clone()).or_insert_with(|| crate::continuations::ends_named(doc, &mut places, &name));
            let skip = after.saturating_sub(at);
            let take = end.saturating_sub(at.max(after));
            for e in sorted.iter().filter(|e| e.rep != *s).skip(skip).take(take) {
                list.push(row(&Reference {
                    source: *s,
                    source_page: sr.page,
                    target: e.rep,
                    target_page: e.page,
                    kind: "continuation",
                    labels: None,
                    text: e.text.clone(),
                }));
            }
        }
        at = next;
    }
    let mut v = json!({ "references": list });
    if end < total {
        v["next"] = json!(end);
    }
    Ok(v)
}

/// `contacts {component}`'s result.
///
/// # Errors
/// Unknown component.
pub fn contacts_json_measured(doc: &Document, c: DeviceId, m: &dyn Look) -> Result<Value, CmdError> {
    if !doc.devices().contains_key(&c) {
        return Err(CmdError::NotFound(ecad_model::Noun::Device));
    }
    let dev = device(doc, c);
    let offer = offer_of(doc, c);
    let stored = offer.as_ref().map(|(t, _)| t.clone());
    let mut places = Places::new(doc);
    let rows = rows(&mut places, &dev, offer.as_ref().map(|(_, o)| o.as_slice()));
    let over = rows.iter().filter(|r| r.state == State::Over).count();
    let (placement, own) = placement_of(doc, c);
    let head = dev.head.and_then(|h| doc.representations().get(&h).map(|r| (h, r)));
    Ok(json!({
        "component": c.hex(),
        "tag": crate::devices::tags::tag_shown_of(doc, c),
        "tag_state": doc.devices().get(&c).and_then(|d| d.tag.state()).map(ecad_model::naming::words::Word::word),
        "offer": stored,
        "head": head.map(|(h, r)| json!({"representation": h.hex(), "page": crate::references::cables::page_number(doc, r.page), "place": places.text(r.page, anchor(doc, r))})),
        "placement": placement.word(),
        "placement_from": if own { "device" } else { "project" },
        "rows": rows.iter().map(|r| json!({
            "labels": r.label_pair(), "kind": r.kind.upper(), "group": r.group_word(), "state": r.state.word(),
            "representation": r.rep.map(|x| x.hex()), "place": (r.state != State::Spare).then(|| r.place.clone()),
        })).collect::<Vec<_>>(),
        "over": over,
        "table": table_json(doc, &mut places, &dev, m),
    }))
}

/// The `contacts` reply's `table`: `{top, bottom, placement, shown}`, or null.
fn table_json(doc: &Document, places: &mut Places<'_>, dev: &Device, m: &dyn Look) -> Value {
    let Some(t) = drawn_table(doc, places, dev, m) else { return Value::Null };
    let k = unit(doc);
    json!({
        "top": [t.top.x / k, t.top.y / k],
        "bottom": t.bottom / k,
        "placement": t.placement.word(),
        "shown": t.shown.word(),
    })
}

/// The target `refgoto` goes to from `source`: a cable end's partner, a
/// contact's head, or a head's given target (`target`, else its first child). `None` when it shows
/// nothing.
pub(crate) fn goto_target(doc: &Document, idx: &RefIndex, source: RepresentationId, target: Option<RepresentationId>) -> Option<RepresentationId> {
    match idx.member(source) {
        Some((Group::Cable(_), _)) => {
            let references = ecad_model::connectivity::derive::cable_references_of(
                doc,
                &BTreeSet::from([match doc.representations().get(&source)?.subject {
                    Subject::Cable(k) => k,
                    _ => return None,
                }]),
            );
            references.into_iter().find(|r| r.reference == source).and_then(|r| r.partner())
        }
        Some(_) => match target {
            Some(t) => idx.targets(source).find(|x| *x == t),
            None => idx.targets(source).next(),
        },
        None => None,
    }
}

/// Hover lines for contact `rep`: `Coil: 100-B-3  (page 100 "Supply")`.
pub(crate) fn contact_lines(doc: &Document, c: DeviceId, rep: RepresentationId) -> Vec<String> {
    let dev = device(doc, c);
    if !dev.children.contains(&rep) {
        return Vec::new();
    }
    match dev.head.and_then(|h| doc.representations().get(&h)) {
        Some(r) => {
            let mut places = Places::new(doc);
            let noun = doc.devices().get(&c).map_or("Head", |d| head_noun(&d.device_type));
            vec![format!("{noun}: {}  ({})", places.text(r.page, anchor(doc, r)), crate::references::cables::page_words(doc, r.page))]
        }
        None => vec!["Coil: not drawn".into()],
    }
}

/// What a contact's hover calls its device's head: by the device's stored type.
fn head_noun(t: &ecad_model::DeviceType) -> &'static str {
    use ecad_model::DeviceType as T;
    use ecad_model::objects::devices::{OperatorKind, ProtectionKind};
    match t {
        T::Switching(s) => match s.operator_kind() {
            OperatorKind::Coil => "Coil",
            OperatorKind::SolidState | OperatorKind::Manual => "Head",
        },
        T::Protection(p) => match p.kind() {
            ProtectionKind::Mcb => "MCB",
            ProtectionKind::Fuse | ProtectionKind::FuseTerminalBlock | ProtectionKind::Rcd | ProtectionKind::Rcbo | ProtectionKind::FuseBank => {
                "Head"
            }
        },
        T::PowerSupply(_)
        | T::Sensor(_)
        | T::Motor(_)
        | T::Load(_)
        | T::Indicator(_)
        | T::TerminalStrip(_)
        | T::Connector(_)
        | T::PlcRack(_)
        | T::PlcModule(_)
        | T::General => "Head",
    }
}

/// Hover lines for head `rep` of device `c`: `Contacts: 5 drawn, 1 spare (3P+1NO+1NC)` and up to
/// six rows.
pub fn head_lines(doc: &Document, c: DeviceId, rep: RepresentationId) -> Vec<String> {
    let dev = device(doc, c);
    if dev.head != Some(rep) {
        return Vec::new();
    }
    let offer = offer_of(doc, c);
    let mut places = Places::new(doc);
    let rows = rows(&mut places, &dev, offer.as_ref().map(|(_, o)| o.as_slice()));
    let n = |s: State| rows.iter().filter(|r| r.state == s).count();
    let mut head = format!("Contacts: {} drawn", n(State::Drawn) + n(State::Over));
    if n(State::Spare) > 0 {
        head.push_str(&format!(", {} spare", n(State::Spare)));
    }
    if n(State::Over) > 0 {
        head.push_str(&format!(", {} beyond the part", n(State::Over)));
    }
    if let Some((t, _)) = &offer {
        head.push_str(&format!(" ({t})"));
    }
    let mut lines = vec![head];
    for r in rows.iter().take(6) {
        lines.push(format!("  {} {} {} {}", r.label_pair(), r.kind.upper(), r.group_word(), r.place));
    }
    if rows.len() > 6 {
        lines.push(format!("  +{} more", rows.len() - 6));
    }
    lines
}

/// Hover lines for row `i` of head `head`'s table.
pub fn row_lines(doc: &Document, c: DeviceId, i: usize, m: &dyn Look) -> Option<Vec<String>> {
    let dev = device(doc, c);
    let mut places = Places::new(doc);
    let t = table_measured(doc, &mut places, &dev, m)?;
    let r = t.rows.get(i)?;
    let tag = crate::devices::tags::tag_words(doc, c);
    let what = if r.power { "power" } else { "auxiliary" };
    Some(vec![match (r.state, r.page) {
        (State::Spare, _) | (_, None) => format!("{tag} {} {}, {what}, offered, not drawn", r.label_pair(), r.kind.upper()),
        (s, Some(p)) => {
            let over = if s == State::Over { " (beyond the part's offer)" } else { "" };
            format!("{tag} {} {}, {what}, {} {}{over}", r.label_pair(), r.kind.upper(), crate::references::cables::page_words(doc, p), r.place)
        }
    }])
}

/// The Checks rows of device `c`: one `too-many-contacts` warning per kind drawn
/// beyond the offer, ringed at the first over pole.
pub(crate) fn count_checks(doc: &Document, c: DeviceId) -> Vec<(String, Option<PageId>, Option<Pt>)> {
    let Some((text, offer)) = offer_of(doc, c) else { return Vec::new() };
    let dev = device(doc, c);
    // A device with no head shows no references and gets no check.
    if dev.head.is_none() || dev.poles.is_empty() {
        return Vec::new();
    }
    let mut places = Places::new(doc);
    let rows = rows(&mut places, &dev, Some(&offer));
    let tag = crate::devices::tags::tag_words(doc, c);
    counts(&dev, &offer)
        .into_iter()
        .filter(|(_, (drawn, offered))| drawn > offered)
        .map(|((power, kind), (drawn, offered))| {
            let first = rows.iter().find(|r| r.state == State::Over && r.power == power && r.kind == kind);
            (
                format!("{tag}: {drawn} {} drawn, part offers {offered} ({text})", kind_words(power, kind)),
                first.and_then(|r| r.page),
                first.and_then(|r| r.at),
            )
        })
        .collect()
}

/// Whether a project key sizes every contact table's text, so every table is placed again (the
/// reference update map's wide case): the table text's style choice (its height
/// and weight are the style's) and the font.
pub fn table_text_key(key: &str) -> bool {
    key == ecad_model::look::TextKind::Table.style_key() || key == ecad_model::look::FONT_KEY
}

/// Whether a `ModifyDevice` from `before` to `after` changes what the device offers or where its
/// table goes.
pub(super) fn contacts_changed(before: &StoredDevice, after: &StoredDevice) -> bool {
    let of = |d: &StoredDevice| d.device_type.as_contacts().map(|x| (x.contacts().clone(), x.contact_table()));
    of(before) != of(after)
}

/// The devices whose table the user placed (`at dx,dy`): the ones a sheet change can move onto
/// or off the title block. O(components), on a project sheet change only.
pub(crate) fn placed_tables(doc: &Document) -> Vec<DeviceId> {
    doc.devices()
        .iter()
        .filter(|(_, c)| matches!(&c.device_type, DeviceType::Switching(s) if matches!(s.contact_table, Some(ContactTable::At { .. }))))
        .map(|(id, _)| *id)
        .collect()
}

/// The Checks row of a table the user placed over the title block: its words,
/// page and the table's top; `None` for an automatic placement (never moved onto it) or no table.
pub(crate) fn title_check(doc: &Document, c: DeviceId, m: &dyn Look) -> Option<(String, PageId, Pt)> {
    let (placement, _) = placement_of(doc, c);
    if !matches!(placement, Placement::At(_)) {
        return None;
    }
    let dev = device(doc, c);
    let mut places = Places::new(doc);
    let t = table_measured(doc, &mut places, &dev, m)?;
    if !enters_title_block(doc, t.page, table_box(doc, t.top, &t.rows, m)) {
        return None;
    }
    let which = format!(
        "The contact table of {} overlaps the title block on {}.",
        crate::devices::tags::tag_words(doc, c),
        crate::references::cables::page_number_words(doc, t.page)
    );
    Some((which, t.page, t.top))
}
