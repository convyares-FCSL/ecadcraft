//! The generated pages' view (design 064 §4.1, `GeneratedView`): which generated pages a commit
//! redraws, by kind, through one exhaustive match ([`changed`], design 064 §4.2). It keeps the
//! wire categories in use while the book has a cover ([`crate::front::Uses`]).
//!
//! # Update map
//! After the commit, each effect of the transaction read on the document after it:
//! - Contents: every effect of `crate::generated::touched_by` for the kind (a page added, removed,
//!   moved, retitled, renumbered or refiled; a folder).
//! - Revisions: every effect of `crate::generated::touched_by` for the kind (a project property,
//!   a version, the revision rows or the project's info).
//! - Cover, while the book had one: the effects of [`crate::front::CoverNote`] (its own map), and
//!   of [`crate::front::UsesNote`] when they change the set of wire categories in use. A cover
//!   that comes with the transaction is drawn, and its categories are read then.
//! - The report kinds (BOM, terminals, cables, I/O list, rack) follow their models: their pages
//!   come from [`super::OutputsView`]'s outcome.
//! - Every other effect changes nothing.
//!
//! The pages of each changed kind are read from the page order only when some kind changed. The
//! kept frame is stale when the cover changed and it shows the cover. A cover changed, or a cover
//! page added, removed or moved, reports the cover's Checks rows ([`GeneratedView::take_cover`]).
//! Before the commit it notes nothing. Open, reload and branch switch forget the categories.

use ecad_model::effect::{AreaSet, Effect};
use ecad_model::view::{DerivedView, NoteCx, Outcome, PageSet, Phase, ViewScope};
use ecad_model::{Document, GenKind, PageId};

use super::Inputs;
use crate::front::{CoverNote, Uses, UsesNote};
use crate::generated::{kind_of, touched_by};

/// The generated pages' view.
#[derive(Debug, Default)]
pub struct GeneratedView {
    /// The wire categories in use, kept while the book has a cover.
    uses: Option<Uses>,
    /// The last commit changed the cover, or added, removed or moved a cover page.
    cover: bool,
}

impl GeneratedView {
    /// Whether the last commit changed the cover or added, removed or moved a cover page (the
    /// cover's Checks rows follow it), taken for the engine.
    pub fn take_cover(&mut self) -> bool {
        std::mem::take(&mut self.cover)
    }

    /// Forget the kept categories: read again on the next commit while there is a cover.
    pub fn forget(&mut self) {
        self.uses = None;
    }
}

/// What the generated pages' view found in one commit.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GeneratedScope {
    pub contents: bool,
    pub revisions: bool,
    /// The cover's own map, noted while a cover was kept.
    pub cover: CoverNote,
    /// The wire categories' map, noted while a cover was kept.
    pub uses: UsesNote,
    /// A cover page added, removed or moved.
    pub cover_page: bool,
}

impl ViewScope for GeneratedScope {
    fn is_empty(&self) -> bool {
        *self == GeneratedScope::default()
    }

    fn is_all(&self) -> bool {
        false
    }
}

/// What changed for each kind of generated page, worked out after the commit.
#[derive(Debug, Default, Clone, Copy)]
struct Kinds {
    cover: bool,
    contents: bool,
    revisions: bool,
}

/// Whether a generated page of `kind` must be drawn again (design 064 §4.2). No `_` arm: a new
/// kind does not compile until it says how it redraws.
fn changed(kind: GenKind, k: Kinds) -> bool {
    match kind {
        GenKind::Cover => k.cover,
        GenKind::Contents => k.contents,
        GenKind::Revisions => k.revisions,
        // Their models' pages come from `OutputsView`'s outcome.
        GenKind::BomLocation | GenKind::Terminals | GenKind::Cables | GenKind::PlcRack | GenKind::IoList => false,
    }
}

impl<'i> DerivedView<Inputs<'i>> for GeneratedView {
    type Scope = GeneratedScope;
    const NAME: &'static str = "generated";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut GeneratedScope) {
        if cx.phase != Phase::After {
            return;
        }
        scope.contents |= touched_by(effect, GenKind::Contents);
        scope.revisions |= touched_by(effect, GenKind::Revisions);
        if let Effect::Page { before, after, index_before, index_after, .. } = effect
            && (index_before.is_some() || index_after.is_some())
        {
            scope.cover_page |= before.iter().chain(after.iter()).any(|p| p.kind.generated_kind() == Some(GenKind::Cover));
        }
        // Brief 054b (design 054 §10): the cover's maps run only while a cover is kept; a new
        // cover is drawn and its categories read whole.
        if self.uses.is_some() {
            scope.cover.note(cx.doc, effect);
            scope.uses.note(cx.doc, effect);
        }
    }

    fn update(&mut self, doc: &Document, scope: GeneratedScope, cx: &Inputs<'i>) -> Outcome {
        let GeneratedScope { contents, revisions, cover, uses, cover_page } = scope;
        let has_cover = doc.page_order.iter().any(|p| kind_of(doc, *p) == Some(GenKind::Cover));
        // Brief 054b (design 054 §10.1): the legend redraws only when a category's count crosses
        // zero.
        let wires = match (&mut self.uses, has_cover) {
            (Some(u), true) => u.apply(doc, uses),
            (None, true) => {
                self.uses = Some(Uses::build(doc));
                true
            }
            (_, false) => {
                self.uses = None;
                false
            }
        };
        let k = Kinds { cover: has_cover && (wires || cover.changed(doc)), contents, revisions };
        self.cover = k.cover || cover_page;
        let mut pages = PageSet::Only(Default::default());
        if k.cover || k.contents || k.revisions {
            for p in doc.page_order.iter().copied().filter(|p| kind_of(doc, *p).is_some_and(|kind| changed(kind, k))) {
                pages.insert(p);
            }
        }
        let frame_stale = k.cover && cx.frame_page.is_some_and(|p: PageId| kind_of(doc, p) == Some(GenKind::Cover));
        Outcome { pages, frame_stale, ..Outcome::default() }
    }

    fn verify(&self, doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        let kept = self.uses.as_ref()?;
        (*kept != Uses::build(doc)).then(|| "the cover's wire categories in use differed from a fresh build".to_string())
    }
}
