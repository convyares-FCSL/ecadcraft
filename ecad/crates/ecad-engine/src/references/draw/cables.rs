//! The engine half of [`crate::references`]: picking a reference end through CADCraft's fonts, and
//! the `cableslash` continue command run against a live [`crate::Project`].

use std::collections::BTreeMap;

use ecad_model::{CableId, ItemId, Op, PageId, Pt, Representation, RepresentationId, Subject};

use crate::styles::draw::EngineMetrics;

/// The cable reference end at `p` on `page` within `radius`, measuring flags with CADCraft's fonts
/// ([`crate::references::reference_at_measured`]).
pub fn reference_at(doc: &ecad_model::Document, page: PageId, p: Pt, radius: f64) -> Option<RepresentationId> {
    crate::references::reference_at_measured(doc, page, p, radius, &EngineMetrics)
}

/// `cableslash` with an existing cable's tag (design 072 §4.4, F5, superseding design 032 §3.3):
/// a slash representation of `cable` on this page over cores it already has here. One
/// transaction, `Cable slash`; no new cable, no new member.
///
/// # Errors
/// A slash already here, none of the cable's wires on this page, a crossed core not the cable's;
/// nothing is written then.
pub(crate) fn run_slash_continue(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, page: PageId, cable: CableId, from: Pt, to: Pt,
    found: &ecad_electrical::cables::slash::Crossings,
) -> Result<serde_json::Value, String> {
    let doc = p.ledger.document();
    let crossed: Vec<ecad_model::CorePosition> = found.members.iter().map(|m| m.position).collect();
    let plan = crate::references::plan_slash_continue(doc, page, cable, &crossed)?;
    let (item, rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
    let mut ops = Vec::new();
    let style = crate::commands::plain_style(p);
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
            role: ecad_model::Role::Slash,
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    p.apply_edit(pg, "cableslash", "Cable slash", ops)?;
    Ok(serde_json::json!({
        "cable": cable.hex(),
        "item": item.hex(),
        "representation": rep.hex(),
        "conductors": plan.iter().filter_map(|w| w.position.wire()).map(|c| c.hex()).collect::<Vec<_>>(),
        "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
        "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "continued": true,
        "cores": plan
            .iter()
            .map(|w| serde_json::json!({"conductor": w.position.wire().map(|c| c.hex()), "route": w.position.route().map(|r| r.hex()), "core": w.core}))
            .collect::<Vec<_>>(),
    }))
}
