//! The engine side of the Checks list (design 065 §3): the drawn handles a check's go-to selects.

use ecad_model::{PageId, Subject};

use crate::checks::Object;

/// The drawn handles of `object` on `page` (for a go-to's selection): a conductor's route, a
/// component's symbols, a connection point's symbol.
pub fn handles_of(p: &crate::Project, page: PageId, object: Object) -> Vec<cadcraft_engine::doc::Handle> {
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Vec::new() };
    let doc = p.ledger.document();
    let subject = match object {
        Object::Conductor(c) => Subject::Conductor(c),
        Object::Component(c) => Subject::Component(c),
        Object::Reference(r) => {
            return doc
                .representations
                .get(&r)
                .filter(|x| x.page == page)
                .map(|x| x.items.iter().filter_map(|i| map.item(*i)).collect())
                .unwrap_or_default();
        }
        Object::Cable(k) => Subject::Cable(k),
        Object::Terminal(t) => match doc.terminals.get(&t) {
            Some(x) => Subject::Component(x.component),
            None => return Vec::new(),
        },
        Object::None => return Vec::new(),
    };
    doc.representations_of(subject).filter(|(_, r)| r.page == page).flat_map(|(_, r)| r.items.iter().filter_map(|i| map.item(*i))).collect()
}
