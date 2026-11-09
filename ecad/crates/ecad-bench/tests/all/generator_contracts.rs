//! The benchmark generator's shape (brief 012, acceptance 3 to 5). These are the only parts of the
//! benchmark that run in CI; they use a cut-down tier and the small tier's lean history so they
//! stay fast. The timings themselves never run here.

use std::collections::BTreeSet;

use ecad_bench::generate::{Generated, REFERENCE_ROLE, Tier, Variant, generate, tier};
use ecad_file::{Outcome, open_project};
use ecad_model::{Document, Subject};

const TINY: Tier = Tier { name: "tiny", pages: 3, components: 9, wires: 18, cables: 6 };

fn opened(g: &Generated) -> (Document, u64) {
    let bytes = g.file_bytes().unwrap();
    let o = open_project(&bytes, 5).unwrap();
    assert_eq!(o.report.outcome, Outcome::Clean, "{}", o.report.describe());
    (o.ledger.document().clone(), o.ledger.head())
}

/// Counts, attachments, nets, cores and continued cables of `doc` against what `g` intended.
fn check_shape(g: &Generated, doc: &Document) {
    let t = g.tier;
    assert_eq!(doc.page_order, g.pages);
    assert_eq!(doc.pages.len(), t.pages);
    assert_eq!(doc.components.len(), t.components);
    assert_eq!(doc.terminals.len(), 2 * t.components);
    assert_eq!(doc.conductors.len(), t.wires);
    assert_eq!(doc.cables.len(), t.cables);
    assert!(doc.conductors.values().all(|c| c.ends.iter().all(Option::is_some)), "every wire attached at both ends");
    assert!(ecad_model::derive::mismatches(doc, 1e-9).is_empty(), "every route ends on its terminals");

    let nets = ecad_model::derive::nets(doc);
    assert_eq!(nets.len(), g.nets.len());
    assert_eq!(nets.len(), t.components, "one net per pair of neighbouring components");
    for (got, want) in nets.iter().zip(&g.nets) {
        assert_eq!(got.terminals, want.terminals);
        assert_eq!(got.conductors, want.conductors);
    }

    let per_pair = t.wires / t.components;
    for cable in doc.cables.values() {
        assert_eq!(cable.members.len(), per_pair);
        let cores: Vec<_> = cable.members.iter().map(|m| m.core.clone()).collect();
        let want: Vec<_> = (1..=per_pair).map(|i| Some(i.to_string())).collect();
        assert_eq!(cores, want, "core labels 1, 2, …");
    }

    assert_eq!(g.continuations.len(), t.pages - 1, "one continued cable per page boundary");
    let references = doc.representations.values().filter(|r| r.role == REFERENCE_ROLE).count();
    assert_eq!(references, 2 * (t.pages - 1));
    // Brief 010's derivation recognises them: two partnered ends per continued cable, no orphans.
    let derived = ecad_model::derive::cable_references(doc);
    assert_eq!(derived.len(), 2 * (t.pages - 1), "every generated reference is derived");
    assert!(derived.iter().all(|r| !r.orphan && r.partner_count == 1), "each end has its partner");
    for c in &g.continuations {
        let [a, b] = c.references;
        let of = |rep| derived.iter().find(|r| r.reference == rep).expect("derived");
        assert_eq!((of(a).partners.as_slice(), of(b).partners.as_slice()), ([b].as_slice(), [a].as_slice()));
        let pages: BTreeSet<_> = doc
            .representations_of(Subject::Cable(c.cable))
            .filter(|(_, r)| r.role == REFERENCE_ROLE)
            .map(|(_, r)| r.page)
            .collect();
        assert_eq!(pages, BTreeSet::from([c.from, c.to]), "a reference end on each page");
        for (i, rep) in c.references.iter().enumerate() {
            let r = doc.representations.get(rep).unwrap();
            assert_eq!(r.page, if i == 0 { c.from } else { c.to });
        }
        for m in &doc.cables.get(&c.cable).unwrap().members {
            for page in [c.from, c.to] {
                assert!(
                    ecad_model::derive::route_on_page(doc, m.conductor, page).is_some(),
                    "a continued member is drawn on both pages"
                );
            }
        }
    }
}

#[test]
fn the_same_tier_gives_byte_identical_files() {
    for variant in Variant::BOTH {
        let a = generate(TINY, variant).unwrap().file_bytes().unwrap();
        let b = generate(TINY, variant).unwrap().file_bytes().unwrap();
        assert_eq!(a, b, "{variant:?}");
    }
}

#[test]
fn lean_and_churned_replay_to_the_same_intended_document() {
    let lean = generate(TINY, Variant::Lean).unwrap();
    let churned = generate(TINY, Variant::Churned).unwrap();
    let (lean_doc, lean_txs) = opened(&lean);
    let (churned_doc, churned_txs) = opened(&churned);
    assert_eq!(lean_doc, churned_doc, "history alone differs");
    assert_eq!(&lean_doc, lean.ledger.document());
    assert!(churned_txs > 4 * lean_txs && churned_txs <= 5 * lean_txs, "{lean_txs} lean, {churned_txs} churned");
    let labels: BTreeSet<&str> = churned.ledger.log().iter().map(|t| t.label.as_str()).collect();
    for l in ["Move", "Rename", "Rename page"] {
        assert!(labels.contains(l), "churned history holds {l}");
    }
    assert!(churned.ledger.log().iter().any(|t| matches!(t.kind, ecad_model::TxKind::Undo { .. })));
    assert!(churned.ledger.log().iter().any(|t| matches!(t.kind, ecad_model::TxKind::Redo { .. })));
    check_shape(&lean, &lean_doc);
}

#[test]
fn the_small_tier_has_the_shape_the_brief_names() {
    let t = tier("small").unwrap();
    assert_eq!((t.pages, t.components, t.wires, t.cables), (10, 100, 500, 50));
    let g = generate(t, Variant::Lean).unwrap();
    let (doc, _) = opened(&g);
    check_shape(&g, &doc);
}

#[test]
fn a_generated_project_opens_in_a_session_and_shows_every_page() {
    let dir = std::env::temp_dir().join(format!("ecad-bench-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let g = generate(TINY, Variant::Churned).unwrap();
    let path = dir.join("tiny.ecad");
    std::fs::write(&path, g.file_bytes().unwrap()).unwrap();

    // The whole measurement path, once.
    let (m, doc) = ecad_bench::measure::measure(&path, 1, &dir).unwrap();
    assert_eq!(m.transactions, g.ledger.head());
    assert_eq!(&doc, g.ledger.document());

    // The app opens version 2 files (brief 021): convert first.
    let v2 = dir.join("tiny.v2.ecad");
    ecad_bench::v2::convert(&path, &v2).unwrap();
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file_in(
        &mut s,
        v2.to_str().unwrap(),
        9,
        ecad_cadcraft::views::Settings::none(),
    )
    .unwrap();
    for page in &g.pages {
        project.open_page(&mut s, *page).unwrap();
        assert_eq!(project.read(&mut s, |p, _| p.active_page()).unwrap(), Some(*page));
    }
    // Brief 016: one page window, whatever was shown before.
    assert_eq!(project.read(&mut s, |p, _| p.open_pages()).unwrap(), 1);
    assert_eq!(s.docs.len(), 1);
    drop(project);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}
