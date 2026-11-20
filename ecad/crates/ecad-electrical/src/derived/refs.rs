//! The references view (design 064 §4.1, `RefsView`): the kept reference index
//! ([`crate::xref::RefIndex`]), and per commit the pages whose references changed (cable
//! references, contact texts and mirror tables, connector halves and pins, continuation arrows)
//! and the line about references left without another end.
//!
//! # Update map
//! The map is [`crate::xref::Watch`]'s (design 025 §6, the table in [`crate::xref`]), taken on the
//! ops before the commit and finished on the committed transaction's ops after it:
//! - The cables the ops could touch ([`crate::references::cables_touched`]): their reference ends'
//!   pages when their texts changed, and the orphaned-reference line (design 010 §4.3) for the
//!   command line.
//! - The devices the ops name (items, representations, wires at their ends, connection points,
//!   `contacts` and `mirror`, definitions, a page's number, sheet, type or book position, a
//!   frame's grid): the pages where their texts or tables differ before and after.
//! - The continuation names the ops name or that stand on such a page: the pages where their
//!   arrows' texts or caption runs differ.
//! - The table text, the reference style, the sheet, the length unit or the contact mirror
//!   default: every page with a device member (the wide case); the continuation text settings:
//!   every page with an arrow.
//! - The index: the groups the ops name are made again; a page type's checks switched on or off
//!   rebuilds it (and is wide).
//! - Every other op changes nothing.
//!
//! What the last commit derived for its devices is kept and is the next commit's "before" (brief
//! 025): every change of the document goes through [`RefsView::take`] and [`RefsView::update`] or
//! a rebuild, so it is never stale; verify mode checks each reuse against a fresh derivation.
//!
//! It is the one view still mapped from ops, not effects: the pipeline hands it the ops directly.
//! Brief 064 step 5's port of `xref::Scope::collect`, `references::cables_touched` and
//! `continuations::names_named` to a `note` over `Effect` (with `Watch` as the scope, and its
//! internal rebuilds as `is_all` / `verify`) is still to do. Open, reload and branch switch build
//! it whole ([`RefsView::build`]).

use ecad_model::view::{Outcome, PageSet};
use ecad_model::{Document, Op};

use crate::text_style::fonts::TextMetrics;
use crate::xref::{RefIndex, Shown, Watch};

/// The references view: the kept index and what the last commit derived.
#[derive(Debug, Default)]
pub struct RefsView {
    index: RefIndex,
    /// The texts and tables the last commit derived after it, for the next commit's "before".
    shown: Shown,
}

/// What [`RefsView::take`] saw before the commit.
#[derive(Debug, Default)]
pub struct RefsScope {
    watch: Option<Watch>,
}

impl RefsView {
    /// The whole index of `doc`; nothing kept to reuse.
    pub fn build(doc: &Document) -> Self {
        RefsView { index: RefIndex::build(doc), shown: Shown::new() }
    }

    /// The kept reference index.
    pub fn index(&self) -> &RefIndex {
        &self.index
    }

    /// Before the commit: what `ops` could change, derived on `before`. Changes nothing.
    pub fn take(&self, before: &Document, ops: &[Op], m: &dyn TextMetrics) -> RefsScope {
        RefsScope { watch: Some(Watch::take(before, &self.index, ops, &self.shown, m)) }
    }

    /// After the commit: bring the index up to date on `after` and the transaction's ops, and
    /// say which pages' references changed. A scope not taken changes nothing.
    pub fn update(&mut self, after: &Document, tx_ops: &[Op], scope: RefsScope, m: &dyn TextMetrics) -> Outcome {
        let Some(watch) = scope.watch else { return Outcome::default() };
        let out = watch.finish(after, &mut self.index, tx_ops, m);
        self.shown = out.shown;
        Outcome { pages: PageSet::from_option(out.pages), echo: out.orphaned.into_iter().collect(), problems: out.problems, ..Outcome::default() }
    }

    /// Verify mode: when the kept index differs from a fresh build of `doc`, rebuild it and say so.
    pub fn check(&mut self, doc: &Document) -> Option<String> {
        let e = self.index.verify(doc).err()?;
        self.index = RefIndex::build(doc);
        self.shown = Shown::new();
        Some(format!("the reference index was out of step ({e}); rebuilt"))
    }
}
