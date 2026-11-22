//! The selection half of the ECAD section of CADCraft's Properties palette (brief 019, design 019
//! §8): which devices, wires, cables and references the drawing's selected handles stand for, and
//! the view built from them by [`ecad_electrical::properties`], which holds the sections, their update
//! map and the edit lines.

use std::collections::BTreeSet;

use cadcraft_engine::doc::Handle;
use ecad_electrical::properties::{
    PropsView, Section, box_section, cable_section, device_section, jumper_bar_section, page_section, reference_section, wire_section,
};
use ecad_model::{CableId, DeviceId, ObjectRef, PageId, Subject, WireId};

use crate::project::{Mapped, Project};

/// What the selection is, by kind.
#[derive(Debug, Default)]
struct Picked {
    devices: BTreeSet<DeviceId>,
    symbol: Option<ecad_model::RepresentationId>,
    /// Continuation arrows (design 072 §6.2), and the first one's symbol.
    aids: BTreeSet<ecad_model::AidId>,
    aid_symbol: Option<ecad_model::RepresentationId>,
    /// Location boxes (design 072 §6.2).
    boxes: BTreeSet<ecad_model::AidId>,
    wires: BTreeSet<WireId>,
    cables: BTreeSet<CableId>,
    references: BTreeSet<ecad_model::RepresentationId>,
    /// Brief 030: link lines picked: (strip, link).
    jumper_bars: BTreeSet<(DeviceId, u32)>,
    plain: usize,
}

/// Most selected objects looked at (a bound on the work per build).
pub const MAX_LOOKED_AT: usize = 10_000;

fn pick(p: &Project, page: PageId, selection: &[Handle]) -> Picked {
    let doc = p.ledger.document();
    let mut out = Picked::default();
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return out };
    for h in selection.iter().take(MAX_LOOKED_AT) {
        let rep = match map.get(*h) {
            Some(Mapped::Item(i)) => doc.representation_of_item(i).map(|(id, _)| id),
            Some(Mapped::Marker(r, _) | Mapped::Tag(r) | Mapped::RefText(r) | Mapped::DeviceReference(r)) => Some(r),
            // Lane 072 D: a T's dot stands for the line that ends there.
            Some(Mapped::Tee(r)) => ecad_electrical::wires::routes::route_item(doc, r).map(|(rid, _, _)| rid),
            // Brief 030: a jumper bar shows its strip (Properties ▸ Jumper bar).
            Some(Mapped::JumperBar(c, n)) => {
                out.jumper_bars.insert((c, n));
                None
            }
            Some(Mapped::Fan(k)) => {
                match map.reference_of(*h) {
                    Some(r) => {
                        out.references.insert(r);
                    }
                    None => {
                        out.cables.insert(k);
                    }
                }
                continue;
            }
            Some(Mapped::Frame(_)) => continue,
            // A conductor label (brief 022) stands for its wire.
            Some(Mapped::Label(w)) => {
                out.wires.insert(w);
                continue;
            }
            None => None,
        };
        let Some((rid, r)) = rep.and_then(|id| doc.representations().get(&id).map(|r| (id, r))) else {
            out.plain += 1;
            continue;
        };
        match r.subject {
            _ if ecad_electrical::references::cables::is_reference(r) => {
                out.references.insert(rid);
            }
            Subject::Device(c) => {
                out.devices.insert(c);
                out.symbol.get_or_insert(rid);
            }
            Subject::Wire(w) => {
                out.wires.insert(w);
            }
            Subject::Cable(k) => {
                out.cables.insert(k);
            }
            // An aid is not a device, wire or cable (design 072 §6.2); a continuation has its own
            // section.
            Subject::Aid(a) if ecad_electrical::continuations::continuation(doc, a).is_some() => {
                out.aids.insert(a);
                out.aid_symbol.get_or_insert(rid);
            }
            Subject::Aid(a) if ecad_electrical::locations::box_values(doc, a).is_some() => {
                out.boxes.insert(a);
            }
            Subject::Aid(_) => {}
            // A drawn line stands for the wire it carries (design 072 §4.3).
            Subject::Route(x) => match ecad_electrical::wires::routes::wire_of_route(doc, x) {
                Some(w) => {
                    out.wires.insert(w);
                }
                None => out.plain += 1,
            },
        }
    }
    out
}

/// The devices, cables and wires `selection` on `page` stands for, as hex IDs (brief 026: the
/// Renumber dialog's Sel scope).
pub fn selected_objects(p: &Project, page: PageId, selection: &[Handle]) -> Vec<String> {
    let k = pick(p, page, selection);
    k.devices.iter().map(|c| c.hex()).chain(k.cables.iter().map(|c| c.hex())).chain(k.wires.iter().map(|w| w.hex())).collect()
}

/// The ECAD section for `selection` on the active page `page` of `p`.
pub fn view(p: &Project, page: Option<PageId>, selection: &[Handle]) -> PropsView {
    let doc = p.ledger.document();
    let mut v = PropsView::default();
    let Some(page) = page.filter(|pg| doc.pages().contains_key(pg)) else { return v };
    if selection.is_empty() {
        // Project-level things are popouts of the Project panel (owner, fix round of 2026-11-08):
        // with nothing selected, Properties shows the page. Its Function and Location show the
        // project's defaults when the page has none, so the project is a dependency too.
        v.sections.push(page_section(doc, page));
        v.deps.insert(ObjectRef::Page(page));
        v.deps.insert(ObjectRef::Project);
        return v;
    }
    let k = pick(p, page, selection);
    // Brief 030: a link line alone selects its link.
    if k.devices.is_empty()
        && k.aids.is_empty()
        && k.boxes.is_empty()
        && k.wires.is_empty()
        && k.cables.is_empty()
        && k.references.is_empty()
        && let Some((c, n)) = k.jumper_bars.first().copied()
    {
        v.deps.insert(ObjectRef::Device(c));
        v.sections.push(jumper_bar_section(
            doc,
            c,
            n,
            p.kept_reports().and_then(|o| o.strips.get(c)).and_then(|d| d.acc.jumper_bars.iter().find(|l| l.n == n)).map(|l| l.bars),
        ));
        return v;
    }
    let kinds = [!k.devices.is_empty(), !k.aids.is_empty(), !k.boxes.is_empty(), !k.wires.is_empty(), !k.cables.is_empty(), !k.references.is_empty()];
    let n_kinds = kinds.iter().filter(|x| **x).count();
    if n_kinds == 0 {
        return v;
    }
    if n_kinds > 1 {
        let mut words = Vec::new();
        for (n, one, many) in [
            (k.devices.len(), "device", "devices"),
            (k.aids.len(), "continuation", "continuations"),
            (k.boxes.len(), "location box", "location boxes"),
            (k.wires.len(), "wire", "wires"),
            (k.cables.len(), "cable", "cables"),
            (k.references.len(), "reference", "references"),
        ] {
            if n > 0 {
                words.push(format!("{n} {}", if n == 1 { one } else { many }));
            }
        }
        let total = k.devices.len() + k.aids.len() + k.boxes.len() + k.wires.len() + k.cables.len() + k.references.len();
        v.sections.push(Section {
            title: format!("ECAD · {total} OBJECTS"),
            kind: ecad_electrical::properties::SectionKind::Objects,
            target: None,
            rows: Vec::new(),
            note: Some(format!("{}. Select one kind to edit.", words.join(", "))),
            buttons: Vec::new(),
        });
        return v;
    }
    if !k.devices.is_empty() {
        v.deps.extend(k.devices.iter().map(|c| ObjectRef::Device(*c)));
        v.deps.insert(ObjectRef::Page(page));
        // Brief 025: the Coil and Contacts rows name places on the device's pages.
        for c in &k.devices {
            v.deps.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| ObjectRef::Page(r.page)));
        }
        // Brief 052: codes come from the project's list, and an unset value's proposal from the
        // selected symbol's page (its boxes and values) and the project's.
        v.deps.insert(ObjectRef::Project);
        // Brief 045: a card's Rack row shows its rack's tag.
        v.deps.extend(k.devices.iter().filter_map(|c| ecad_electrical::plc::props_deps(doc, *c)));
        if let Some(r) = k.symbol.and_then(|s| doc.representations().get(&s)) {
            v.deps.insert(ObjectRef::Page(r.page));
        }
        v.sections.push(device_section(doc, p.kept_reports().map(|o| &o.plc), &k.devices, k.symbol));
    } else if !k.boxes.is_empty() {
        // Design 072 §6.2: an aid has no `ObjectRef`, so an aid op rebuilds the view
        // (`ecad_electrical::properties::view`). Devices inside and codes: its page and the list.
        v.deps.insert(ObjectRef::Project);
        for b in &k.boxes {
            v.deps.extend(doc.representations_of(Subject::Aid(*b)).map(|(_, r)| ObjectRef::Page(r.page)));
        }
        match k.boxes.iter().copied().collect::<Vec<_>>().as_slice() {
            [b] => v.sections.push(box_section(doc, *b)),
            many => v.sections.push(Section {
                title: format!("ECAD · {} LOCATION BOXES", many.len()),
                kind: ecad_electrical::properties::SectionKind::Objects,
                target: None,
                rows: Vec::new(),
                note: Some("Select one location box to edit.".into()),
                buttons: Vec::new(),
            }),
        }
    } else if !k.aids.is_empty() {
        // Design 072 §6.2: an aid has no `ObjectRef`, so an aid op rebuilds the view
        // (`ecad_electrical::properties::view`); brief 049: the Other ends row names its partners'
        // places, on their pages.
        for a in &k.aids {
            let partners = ecad_electrical::continuations::name_of(doc, *a).map(|n| doc.continuations_named(&n)).unwrap_or_default();
            for o in partners.into_iter().chain([*a]) {
                v.deps.extend(doc.representations_of(Subject::Aid(o)).map(|(_, r)| ObjectRef::Page(r.page)));
            }
        }
        v.sections.push(ecad_electrical::properties::continuation_section(doc, &k.aids, k.aid_symbol));
    } else if let [w] = k.wires.iter().copied().collect::<Vec<_>>().as_slice() {
        v.deps.insert(ObjectRef::Wire(*w));
        v.deps.extend(doc.cables_of(*w).into_iter().map(ObjectRef::Cable));
        // From and To name each end's device and connection point: a renamed device, or a
        // connection point added or removed, changes them.
        let ends = doc.wires().get(w).map(|c| c.ends.to_vec()).unwrap_or_default();
        v.deps.extend(ends.iter().filter_map(|t| doc.connection_points().get(t)).filter_map(|t| t.device()).map(ObjectRef::Device));
        // Brief 022: the number, potential and Net block read the whole net: its conductors and
        // the devices (junctions and continuations included) at its connection points.
        let conn = doc.connectivity();
        if let Some(net) = conn.net_of_conductor(*w).and_then(|id| conn.net(id)) {
            v.deps.extend(net.wires.iter().map(|c| ObjectRef::Wire(*c)));
            v.deps.extend(
                net.connection_points.iter().filter_map(|t| doc.connection_points().get(t)).filter_map(|t| t.device()).map(ObjectRef::Device),
            );
        }
        v.sections.push(wire_section(doc, *w));
    } else if let [c] = k.cables.iter().copied().collect::<Vec<_>>().as_slice() {
        v.deps.insert(ObjectRef::Cable(*c));
        v.sections.push(cable_section(doc, *c));
    } else if let [r] = k.references.iter().copied().collect::<Vec<_>>().as_slice() {
        if let Some(Subject::Cable(c)) = doc.representations().get(r).map(|x| x.subject) {
            v.deps.insert(ObjectRef::Cable(c));
            // "Other ends" name each partner's page number and its sheet's zones.
            let references = ecad_model::connectivity::derive::cable_references_of(doc, &BTreeSet::from([c]));
            v.deps.extend(references.iter().map(|x| ObjectRef::Page(x.page)));
        }
        v.deps.insert(ObjectRef::Page(page));
        v.sections.push(reference_section(doc, *r));
    } else {
        let n = k.wires.len() + k.cables.len() + k.references.len();
        v.sections.push(Section {
            title: format!("ECAD · {n} OBJECTS"),
            kind: ecad_electrical::properties::SectionKind::Objects,
            target: None,
            rows: Vec::new(),
            note: Some("Select one wire, cable or reference to see its properties.".into()),
            buttons: Vec::new(),
        });
    }
    v
}
