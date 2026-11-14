//! The bench writes its projects straight to version 2 (fix/bench-direct-v2) instead of writing a
//! version 1 file and converting it. These tests prove the two routes give the same project: the
//! same document, the same number of pages, components, conductors and history entries.

use std::path::{Path, PathBuf};

use ecad_bench::generate::{Generated, Tier, Variant, generate, tier};
use ecad_file::v2::disk::DiskProjectV2;
use ecad_model::Document;

const TINY: Tier = Tier { name: "tiny", pages: 3, components: 9, wires: 18, cables: 6 };

/// What the equivalence compares: pages, components, conductors, history entries, and the
/// document itself.
#[derive(Debug, PartialEq)]
struct Shape {
    pages: usize,
    components: usize,
    conductors: usize,
    history: u64,
    document: Document,
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ecad-bench-v2-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn shape(path: &Path) -> Shape {
    let o = DiskProjectV2::open(path, 3, "lane_benchv2").unwrap();
    assert!(o.report.damage.is_none(), "{:?}", o.report.damage);
    let doc = o.ledger.document().clone();
    Shape { pages: doc.pages.len(), components: doc.components.len(), conductors: doc.conductors.len(), history: o.ledger.head(), document: doc }
}

/// The old route: a version 1 file, converted.
fn converted(g: &Generated, dir: &Path) -> PathBuf {
    let v1 = dir.join(format!("{}-{}.v1.ecad", g.tier.name, g.variant.name()));
    std::fs::write(&v1, g.file_bytes().unwrap()).unwrap();
    let out = dir.join(format!("{}-{}.converted.ecad", g.tier.name, g.variant.name()));
    ecad_file::v2::convert::convert_file(&v1, Some(&out), "lane_benchv2").unwrap();
    out
}

/// The new route: written straight to version 2.
fn direct(g: &Generated, dir: &Path) -> PathBuf {
    let out = dir.join(format!("{}-{}.ecad", g.tier.name, g.variant.name()));
    ecad_bench::v2::write(g, &out).unwrap();
    out
}

fn same_both_ways(t: Tier, tag: &str) -> Vec<Shape> {
    let d = dir(tag);
    let mut shapes = Vec::new();
    for variant in [Variant::Lean, Variant::Churned] {
        let g = generate(t, variant).unwrap();
        let old = shape(&converted(&g, &d));
        let new = shape(&direct(&g, &d));
        assert_eq!(new.pages, old.pages, "{} {}: pages", t.name, variant.name());
        assert_eq!(new.components, old.components, "{} {}: components", t.name, variant.name());
        assert_eq!(new.conductors, old.conductors, "{} {}: conductors", t.name, variant.name());
        assert_eq!(new.history, old.history, "{} {}: history entries", t.name, variant.name());
        assert!(new.document == old.document, "{} {}: the documents differ", t.name, variant.name());
        assert_eq!(new.history, g.ledger.head());
        eprintln!(
            "{} {}: {} pages, {} components, {} conductors, {} history entries (both routes)",
            t.name,
            variant.name(),
            new.pages,
            new.components,
            new.conductors,
            new.history
        );
        shapes.push(new);
    }
    let _ = std::fs::remove_dir_all(&d);
    shapes
}

#[test]
fn a_project_written_straight_to_version_2_matches_the_converted_one() {
    let shapes = same_both_ways(TINY, "tiny");
    assert_eq!(shapes.len(), 2);
}

#[test]
fn every_transaction_written_straight_to_version_2_matches_the_generated_history() {
    let d = dir("tx");
    let g = generate(TINY, Variant::Churned).unwrap();
    let path = direct(&g, &d);
    let o = DiskProjectV2::open(&path, 4, "lane_benchv2").unwrap();
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
    // A version 1 file from an older build at the bench's file name.
    let g = generate(TINY, Variant::Lean).unwrap();
    std::fs::write(&path, g.file_bytes().unwrap()).unwrap();
    ecad_bench::v2::write(&g, &path).unwrap();
    assert_eq!(ecad_file::v2::convert::sniff(&path).unwrap(), ecad_file::v2::convert::Major::V2);
    assert_eq!(shape(&path).history, g.ledger.head());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
#[ignore = "slow: generates the small and medium tiers and converts them the old way"]
fn the_small_and_medium_tiers_are_the_same_both_ways() {
    for name in ["small", "medium"] {
        same_both_ways(tier(name).unwrap(), name);
    }
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
