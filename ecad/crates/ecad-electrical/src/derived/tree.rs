//! The Pages tree feed (design 064 §4.1, `TreeFeed`): whether a commit could change the Pages
//! list and tree (design 019 §9), so the change log tells the panel to read it again.
//!
//! # Update map
//! - A page added, removed, or removed and added again (a rename or move): the tree changes.
//! - A page changed in place (`ModifyPage`): the tree changes when its title, number or folder
//!   does; any other field (kind, sheet, display choices) leaves it.
//! - Any folder effect: the tree changes.
//! - Every other effect (the project's own data included: the tree shows none of it), and every property write (the feed reads no area), changes nothing.
//!
//! It keeps nothing: its scope is the change log's record.

use ecad_model::effect::{AreaSet, Effect};
use ecad_model::view::{DerivedView, NoteCx, Outcome, ViewScope};
use ecad_model::{Document, Page};

use super::Inputs;

/// The Pages tree feed.
#[derive(Debug, Default)]
pub struct TreeFeed;

/// Whether the tree must be read again.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TreeScope(pub bool);

impl ViewScope for TreeScope {
    fn is_empty(&self) -> bool {
        !self.0
    }

    fn is_all(&self) -> bool {
        false
    }
}

/// Whether a page change in place shows in the tree.
fn shown_changed(before: &Page, after: &Page) -> bool {
    before.title != after.title || before.number != after.number || before.folder != after.folder
}

impl<'i> DerivedView<Inputs<'i>> for TreeFeed {
    type Scope = TreeScope;
    const NAME: &'static str = "tree";
    const AREAS: AreaSet = AreaSet::NONE;

    fn note(&self, _cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut TreeScope) {
        let touched = match effect {
            Effect::DeviceLink { .. } => false,
            Effect::Page { before: Some(b), after: Some(a), index_before: None, index_after: None, .. } => shown_changed(b, a),
            Effect::Page { .. } | Effect::Folder { .. } => true,
            Effect::Layer { .. }
            | Effect::Item { .. }
            | Effect::Device { .. }
            | Effect::DeviceDefinition { .. }
            | Effect::Terminal { .. }
            | Effect::Rep { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Prop { .. }
            | Effect::Definition { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. }
            | Effect::Project { .. } => false,
        };
        scope.0 |= touched;
    }

    fn update(&mut self, _doc: &Document, scope: TreeScope, _cx: &Inputs<'i>) -> Outcome {
        Outcome { list: scope.0, ..Outcome::default() }
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
