//! The engine side of the Checks list (design 065 §3): the kept list and the from-scratch pass
//! measuring text in CADCraft's fonts (cut 10, option F2), and the drawn handles a check's go-to
//! selects.

use ecad_model::{Document, Op, PageId, Subject};

use crate::checks::{Check, Object, Scope, kept};
use crate::draw::text_style::EngineMetrics;

/// The kept Checks list ([`kept::Checks`]) with text measured in CADCraft's fonts.
#[derive(Debug, Clone, Default)]
pub struct Checks(pub kept::Checks);

impl std::ops::Deref for Checks {
    type Target = kept::Checks;

    fn deref(&self) -> &kept::Checks {
        &self.0
    }
}

impl std::ops::DerefMut for Checks {
    fn deref_mut(&mut self) -> &mut kept::Checks {
        &mut self.0
    }
}

impl Checks {
    /// The whole list, computed from scratch (open, reload, first show): [`kept::Checks::build_with`].
    pub fn build(doc: &Document, tol: f64) -> Self {
        Checks(kept::Checks::build_with(doc, tol, &EngineMetrics))
    }

    /// Re-check the subjects in `scope` on `doc`, the state after `ops`: [`kept::Checks::update_with`].
    pub fn update(&mut self, doc: &Document, ops: &[Op], scope: Scope, tol: f64) {
        self.0.update_with(doc, ops, scope, tol, &EngineMetrics);
    }
}

/// Every check of the project, worked out from scratch (the `checks` command reads on request).
pub fn all(doc: &Document, tol: f64) -> Vec<Check> {
    all_with(doc, tol, &crate::outputs::Outputs::build(doc))
}

/// [`all`] with the report models already kept (brief 024: the project's, reused by `checks`).
pub fn all_with(doc: &Document, tol: f64, outputs: &crate::outputs::Outputs) -> Vec<Check> {
    crate::checks::all_measured(doc, tol, outputs, &EngineMetrics)
}

/// The drawn handles of `object` on `page` (for a go-to's selection): a conductor's route, a
/// component's symbols, a connection point's symbol.
pub fn handles_of(p: &crate::Project, page: PageId, object: Object) -> Vec<cadcraft_engine::doc::Handle> {
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Vec::new() };
    let doc = p.ledger.document();
    let subject = match object {
        Object::Wire(c) => Subject::Wire(c),
        Object::Device(c) => Subject::Device(c),
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
            Some(x) => Subject::Device(x.component),
            None => return Vec::new(),
        },
        Object::None => return Vec::new(),
    };
    doc.representations_of(subject).filter(|(_, r)| r.page == page).flat_map(|(_, r)| r.items.iter().filter_map(|i| map.item(*i))).collect()
}
