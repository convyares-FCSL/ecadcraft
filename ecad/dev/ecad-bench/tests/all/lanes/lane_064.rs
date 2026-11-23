//! The references view (`ReferencesView`, its
//! update map over effects) against the whole document on every transaction of a generated
//! project's churned history: every page whose reference texts, tables, connector parts,
//! continuation arrows or cable reference texts differ is among its pages; its orphaned-reference
//! line is the whole document's; its index equals a build.

use std::collections::BTreeSet;

use ecad_bench::generate::{Tier, Variant, generate, tier};
use ecad_electrical::views::{self, EngineInputs};
use ecad_model::document::view::PageSet;
use ecad_model::{Author, Document, PageId};

/// What the references show on each page of `doc`, worked out over the whole document: the device
/// and connector parts, the continuation arrows, and the cable reference texts.
fn every_reference(doc: &Document, m: &dyn ecad_electrical::styles::text_style::fonts::TextMetrics) -> std::collections::BTreeMap<PageId, String> {
    use {ecad_electrical::continuations, ecad_electrical::references::devices};
    let mut out: std::collections::BTreeMap<PageId, String> = std::collections::BTreeMap::new();
    for p in doc.page_order() {
        let parts = devices::table::parts_on_page_measured(doc, *p, m);
        if !parts.is_empty() {
            out.insert(*p, format!("{parts:?}"));
        }
    }
    let names: BTreeSet<String> = doc.aids().keys().filter_map(|a| continuations::name_of(doc, *a)).collect();
    let mut places = devices::places::Places::new(doc);
    for (p, parts) in continuations::shown_measured(doc, &mut places, &names, m) {
        if !parts.is_empty() {
            out.entry(p).or_default().push_str(&format!("{parts:?}"));
        }
    }
    for r in ecad_model::connectivity::derive::cable_references(doc) {
        out.entry(r.page).or_default().push_str(&format!("{}:{};", r.reference.hex(), r.text));
    }
    out
}

/// The references view on every transaction of `tier`'s churned history with five undos and five
/// redos at its end, against the whole document. Returns how many transactions were compared.
fn references_view_agrees(tier: Tier) -> usize {
    use views::{Inputs, ReferencesView};
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let metrics = std::sync::Arc::new(ecad_electrical::styles::text_style::fonts::FixedWidth);
    let engine = EngineInputs { metrics: metrics.clone() };
    let inputs = Inputs { engine: &engine, frame_page: None };
    let mut view = ReferencesView::build(&Document::default());
    let mut doc = Document::default();
    let mut checked = 0;
    for tx in g.ledger.log() {
        let before = doc.clone();
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        let out = view.commit(&before, &doc, &tx.ops, &inputs);
        let what = format!("{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        let (shown_before, shown_after) = (every_reference(&before, &*metrics), every_reference(&doc, &*metrics));
        if let PageSet::Only(pages) = &out.pages {
            for p in shown_before.keys().chain(shown_after.keys()) {
                if shown_before.get(p) != shown_after.get(p) {
                    assert!(pages.contains(p), "{what}: page {} changed its references but was not named", p.hex());
                }
            }
        }
        let whole = ecad_electrical::references::cables::Snapshot::take(&before, None);
        let orphaned = whole.orphaned_line(&doc, &ecad_electrical::references::cables::Snapshot::take(&doc, None));
        assert_eq!(out.echo, orphaned.into_iter().collect::<Vec<_>>(), "{what}: orphaned line");
        assert_eq!(view.index(), &ecad_electrical::references::devices::index::RefIndex::build(&doc), "{what}: index");
        checked += 1;
    }
    checked
}

/// Step 5: `ReferencesView` on the small tier's recorded history.
#[test]
fn the_references_view_follows_the_whole_document_on_the_small_corpus() {
    let n = references_view_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// Step 5: `ReferencesView` on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_references_view_follows_the_whole_document_on_the_medium_corpus() {
    let n = references_view_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
}
