//! Tester 031: the shared getter and the defaults registry through their Rust API (design 031
//! §3.3, §3.5; brief AC 1 and AC 7). Written from the design before the implementation existed.

use ecad_cadcraft::defaults::{self, Scope};
use ecad_cadcraft::text_style::{self, TextKind, TextStyle, Weight};
use ecad_model::{Author, Document, ObjectRef, Op};

use crate::common::H;

const KINDS: &[(TextKind, f64, Weight)] = &[
    (TextKind::Tag, 2.5, Weight::Bold),
    (TextKind::PointLabel, 1.5, Weight::Regular),
    (TextKind::Description, 1.6, Weight::Regular),
    (TextKind::PartNumber, 1.6, Weight::Regular),
    (TextKind::ContinuationName, 2.0, Weight::Regular),
    (TextKind::Reference, 2.0, Weight::Regular),
    (TextKind::WireLabel, 1.6, Weight::Regular),
    (TextKind::CableText, 1.6, Weight::Regular),
    (TextKind::LocationLabel, 1.6, Weight::Regular),
    (TextKind::TitleValue, 2.5, Weight::Regular),
    (TextKind::TitleCaption, 1.8, Weight::Regular),
    (TextKind::TitleMain, 3.5, Weight::Bold),
    (TextKind::Grid, 3.0, Weight::Regular),
    (TextKind::Table, 2.5, Weight::Regular),
    (TextKind::TableHeader, 2.5, Weight::Bold),
];

fn doc_with(props: &[(&str, &str)]) -> Document {
    let mut h = H::new(0x7031_a001);
    let ops: Vec<Op> =
        props.iter().map(|(k, v)| Op::SetProperty { object: ObjectRef::Project, key: (*k).into(), before: None, after: Some((*v).into()) }).collect();
    if !ops.is_empty() {
        h.cfg(|p| p.ledger.commit(Author::User, "t", ops)).unwrap();
    }
    h.doc()
}

/// T2: every kind's §3.2 default on an empty project, with the style name its weight gives.
#[test]
fn getter_returns_the_design_defaults_for_every_kind() {
    let d = doc_with(&[]);
    for (kind, height, weight) in KINDS {
        let s: TextStyle = text_style::text_style(&d, *kind);
        assert_eq!(s.height, *height, "{kind:?}");
        assert_eq!(s.weight, *weight, "{kind:?}");
        let name = if *weight == Weight::Bold { "ECAD-BOLD" } else { "Standard" };
        assert_eq!(s.style_name, name, "{kind:?}");
    }
    assert_eq!(text_style::text_style(&d, TextKind::Free).style_name, "Standard");
}

/// T2: hostile stored heights and weights give the default; a value in range is used.
#[test]
fn getter_ignores_hostile_stored_values() {
    for bad in ["NaN", "abc", "0.1", "11", "1e308", "-1", "inf", "", " 3 x"] {
        let d = doc_with(&[("text.tag.height", bad), ("text.tag.weight", "heavy")]);
        let s = text_style::text_style(&d, TextKind::Tag);
        assert_eq!(s.height, 2.5, "{bad:?}");
        assert!(s.height.is_finite());
        assert_eq!((s.weight, s.style_name), (Weight::Bold, "ECAD-BOLD"), "{bad:?}");
    }
    let huge = "9".repeat(10_000);
    let d = doc_with(&[("text.tag.height", &huge)]);
    assert_eq!(text_style::text_style(&d, TextKind::Tag).height, 2.5);
    let d = doc_with(&[("text.tag.height", "0.5"), ("text.point_label.height", "10"), ("text.tag.weight", "regular")]);
    assert_eq!(text_style::text_style(&d, TextKind::Tag).height, 0.5);
    assert_eq!(text_style::text_style(&d, TextKind::PointLabel).height, 10.0);
    assert_eq!(text_style::text_style(&d, TextKind::Tag).style_name, "Standard");
}

/// AC 7: keys unique and of the key rule, built-ins valid for their own kind, every entry allows
/// the project level; a reader and its entry agree on an empty project.
#[test]
fn registry_keys_unique_builtins_valid_and_project_level_allowed() {
    let mut seen = std::collections::BTreeSet::new();
    let d = doc_with(&[]);
    for e in defaults::all() {
        assert!(seen.insert(e.key), "duplicate {}", e.key);
        assert!(e.key.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')), "{}", e.key);
        assert!(e.levels.iter().any(|l| matches!(l, defaults::Level::Project)), "{} allows Project", e.key);
        let r = defaults::resolve(&d, e, Scope::project());
        let _ = r;
    }
    assert!(seen.contains("text.tag.height") && seen.contains("text.run.gap") && seen.contains("text.font"));
}
