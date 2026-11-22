//! The engine side of the Checks list (design 065 §3): the from-scratch pass measuring text in
//! CADCraft's fonts (cut 10, option F2), and the drawn handles a check's go-to selects. The kept
//! list is `ecad_electrical::views::ChecksView`, in the project's views.

use ecad_model::{Document, PageId, Subject};

use crate::styles::draw::EngineMetrics;
use {ecad_electrical::checks::Check, ecad_electrical::checks::Object};

/// Every check of the project, worked out from scratch (the `checks` command reads on request).
pub fn all(doc: &Document, tol: f64) -> Vec<Check> {
    crate::project::engine_views().detached(doc).checks(doc, tol)
}

/// [`all`] with the report models already kept (brief 024: the project's, reused by `checks`).
pub fn all_with(doc: &Document, tol: f64, reports: &ecad_electrical::reports::kept::ReportModels) -> Vec<Check> {
    ecad_electrical::checks::all_measured(doc, tol, reports, &EngineMetrics)
}

/// The drawn handles of `object` on `page` (for a go-to's selection): a wire's lines, a route's line, a
/// component's symbols, a connection point's symbol.
pub fn handles_of(p: &crate::Project, page: PageId, object: Object) -> Vec<cadcraft_engine::doc::Handle> {
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Vec::new() };
    let doc = p.ledger.document();
    let subject = match object {
        // A wire's lines on the page: the routes drawing it (design 072 §4.3).
        Object::Wire(c) => {
            let routes: Vec<_> = ecad_electrical::wires::routes::routes_of_wire(doc, c)
                .into_iter()
                .filter(|r| doc.routes.get(r).is_some_and(|x| x.page == page))
                .collect();
            return routes
                .into_iter()
                .flat_map(|r| {
                    doc.representations_of(Subject::Route(r)).flat_map(|(_, x)| x.items.iter().filter_map(|i| map.item(*i))).collect::<Vec<_>>()
                })
                .collect();
        }
        Object::Route(r) => Subject::Route(r),
        Object::Device(c) => Subject::Device(c),
        Object::Aid(a) => Subject::Aid(a),
        Object::Reference(r) => {
            return doc
                .representations
                .get(&r)
                .filter(|x| x.page == page)
                .map(|x| x.items.iter().filter_map(|i| map.item(*i)).collect())
                .unwrap_or_default();
        }
        Object::Cable(k) => Subject::Cable(k),
        Object::ConnectionPoint(t) => match doc.connection_points.get(&t) {
            Some(x) => x.owner.subject(),
            None => return Vec::new(),
        },
        Object::None => return Vec::new(),
    };
    doc.representations_of(subject).filter(|(_, r)| r.page == page).flat_map(|(_, r)| r.items.iter().filter_map(|i| map.item(*i))).collect()
}
