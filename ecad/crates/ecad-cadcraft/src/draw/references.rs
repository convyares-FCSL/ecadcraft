//! The engine half of [`crate::references`]: picking a reference end through CADCraft's fonts, and
//! the `cableslash` continue command run against a live [`crate::Project`].

use std::collections::BTreeMap;

use ecad_model::{CableId, ItemId, Op, PageId, Pt, Representation, RepresentationId, Subject, WireId};

use crate::draw::text_style::EngineMetrics;

/// The cable reference end at `p` on `page` within `radius`, measuring flags with CADCraft's fonts
/// ([`crate::references::reference_at_measured`]).
pub fn reference_at(doc: &ecad_model::Document, page: PageId, p: Pt, radius: f64) -> Option<RepresentationId> {
    crate::references::reference_at_measured(doc, page, p, radius, &EngineMetrics)
}

/// `cableslash` in continue mode (design 032 §3.3): `cable` gains the crossed wires as members with
/// their cores' labels, and a slash representation on this page. One transaction, `Cable slash`;
/// no new cable, no new tag.
///
/// # Errors
/// §3.3's refusals, a wire already in a cable, too many members; nothing is written then.
pub(crate) fn run_slash_continue(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, page: PageId, cable: CableId, from: Pt, to: Pt,
    found: &crate::slash::Crossings,
) -> Result<serde_json::Value, String> {
    let doc = p.ledger.document();
    let crossed: Vec<WireId> = found.members.iter().filter_map(|m| m.wire()).collect();
    if doc.representations_of(Subject::Cable(cable)).any(|(_, r)| r.page == page && r.role == "slash") {
        return Err(format!("{} already has a slash on this page.", crate::references::title(doc, cable)));
    }
    let blocked: Vec<(ecad_model::CorePosition, CableId)> =
        crate::slash::blocked(doc, &found.members).into_iter().map(|(m, k)| (m.position, k)).collect();
    if !blocked.is_empty() {
        return Err(crate::slash::already_in_cable(doc, &blocked, "Nothing was created."));
    }
    let plan = crate::references::plan_slash_continue(doc, page, cable, &crossed)?;
    let (item, rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
    let mut ops = Vec::new();
    // Design 072 §4.5: each continued wire takes its core, the stored unplaced one of that label.
    if let Some(before) = doc.cables.get(&cable) {
        let mut after = before.clone();
        for w in &plan {
            let label = w.core.as_deref().and_then(|c| ecad_model::CoreLabel::new(c).ok());
            ecad_electrical::styles::cable::place_core(&mut after, label, ecad_model::CorePosition::Wire(w.conductor));
        }
        ops.extend(Op::modify_cable(cable, before, after));
    }
    let style = crate::cmd::plain_style(p);
    ops.push(Op::AddItem {
        id: item,
        item: ecad_model::Item { page, style, kind: ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a: from, b: to }) },
    });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Cable(cable),
            page,
            form: ecad_model::Form::Drawn,
            role: "slash".into(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    p.apply_edit(pg, "cableslash", "Cable slash", ops)?;
    Ok(serde_json::json!({
        "cable": cable.hex(),
        "item": item.hex(),
        "representation": rep.hex(),
        "conductors": crossed.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
        "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "continued": true,
        "cores": plan.iter().map(|w| serde_json::json!({"conductor": w.conductor.hex(), "core": w.core, "continues": w.continues.hex()})).collect::<Vec<_>>(),
    }))
}
