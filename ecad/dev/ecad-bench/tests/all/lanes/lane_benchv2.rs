//! The bench writes its projects straight to the project file. These tests
//! prove the file holds the generated project: the same document, pages, components, conductors
//! and history entries.

use std::path::{Path, PathBuf};

use ecad_bench::generate::{Generated, Tier, Variant, generate};
use ecad_file::disk::DiskProject;
use ecad_model::Document;

const TINY: Tier = Tier { name: "tiny", pages: 3, devices: 9, wires: 18, cables: 6 };

/// What the equivalence compares: pages, components, conductors, history entries, and the
/// document itself.
#[derive(Debug, PartialEq)]
struct Shape {
    pages: usize,
    devices: usize,
    wires: usize,
    history: u64,
    document: Document,
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ecad-bench-v2-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn shape(path: &Path) -> Shape {
    let o = DiskProject::open(path, 3, "lane_benchv2").unwrap();
    assert!(o.report.damage.is_none(), "{:?}", o.report.damage);
    let doc = o.ledger.document().clone();
    Shape { pages: doc.pages().len(), devices: doc.devices().len(), wires: doc.wires().len(), history: o.ledger.head(), document: doc }
}

/// Written straight to the project file, as the bench does.
fn direct(g: &Generated, dir: &Path) -> PathBuf {
    let out = dir.join(format!("{}-{}.ecad", g.tier.name, g.variant.name()));
    ecad_bench::container::write(g, &out).unwrap();
    out
}

#[test]
fn a_project_written_straight_to_the_file_matches_the_generated_one() {
    let d = dir("tiny");
    for variant in [Variant::Lean, Variant::Churned] {
        let g = generate(TINY, variant).unwrap();
        let s = shape(&direct(&g, &d));
        let doc = g.ledger.document();
        assert_eq!((s.pages, s.devices, s.wires), (doc.pages().len(), doc.devices().len(), doc.wires().len()), "{}", variant.name());
        assert_eq!(s.history, g.ledger.head(), "{}: history entries", variant.name());
        assert!(&s.document == doc, "{}: the documents differ", variant.name());
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn every_transaction_written_straight_to_version_2_matches_the_generated_history() {
    let d = dir("tx");
    let g = generate(TINY, Variant::Churned).unwrap();
    let path = direct(&g, &d);
    let o = DiskProject::open(&path, 4, "lane_benchv2").unwrap();
    let history = o.ledger.history().unwrap();
    for tx in g.ledger.log() {
        assert_eq!(&history.transaction(tx.seq).unwrap(), tx, "transaction {}", tx.seq);
    }
    drop(o);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn writing_over_a_stale_file_regenerates_it() {
    let d = dir("stale");
    let path = d.join("tiny-lean.ecad");
    // Something else from an older build at the bench's file name.
    let g = generate(TINY, Variant::Lean).unwrap();
    std::fs::write(&path, b"{\"stale\": true}\n").unwrap();
    ecad_bench::container::write(&g, &path).unwrap();
    assert!(ecad_file::disk::starts_like_project(&path).unwrap());
    assert_eq!(shape(&path).history, g.ledger.head());
    let _ = std::fs::remove_dir_all(&d);
}

/// The ccont row's own cable must not share a tag with a generated cable: on large, `-W932` was
/// one, and the row failed.
#[test]
fn the_continued_cable_tag_is_beyond_every_tier_s_cables() {
    let n: usize = ecad_bench::wires_nets::CCONT_TAG.trim_start_matches("-W").parse().unwrap();
    for t in ecad_bench::TIERS.iter().chain([&ecad_bench::PRINT_TIER]) {
        assert!(n > t.cables, "{} has a generated cable {}", t.name, ecad_bench::wires_nets::CCONT_TAG);
    }
}
