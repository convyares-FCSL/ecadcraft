//! The text style getter through its Rust API: built-in values, and hostile stored ones ignored.

use ecad_model::{Document, ObjectRef, Op};
use {ecad_model::look::TextKind, ecad_model::look::TextStyle, ecad_model::objects::styles::text::Weight};

const KINDS: &[(TextKind, f64, Weight)] = &[
    (TextKind::Tag, 2.5, Weight::Bold),
    (TextKind::PointLabel, 1.5, Weight::Regular),
    (TextKind::Description, 1.6, Weight::Regular),
    (TextKind::PartNumber, 1.6, Weight::Regular),
    (TextKind::ContinuationName, 2.0, Weight::Regular),
    (TextKind::Reference, 2.5, Weight::Regular),
    (TextKind::WireLabel, 2.5, Weight::Regular),
    (TextKind::CableText, 1.6, Weight::Regular),
    (TextKind::LocationLabel, 1.6, Weight::Regular),
    (TextKind::TitleValue, 2.5, Weight::Regular),
    (TextKind::TitleCaption, 1.8, Weight::Regular),
    (TextKind::TitleMain, 3.5, Weight::Bold),
    (TextKind::Grid, 3.0, Weight::Regular),
    (TextKind::Table, 2.5, Weight::Regular),
    (TextKind::TableHeader, 2.5, Weight::Bold),
];

/// A project with these values. Brief 050a: a kind's `text.<kind>.<field>` (height, weight, font)
/// is a field of the kind's built-in text style, written as the project's record of it.
fn doc_with(props: &[(&str, &str)]) -> Document {
    let mut doc = Document::default();
    let mut records: std::collections::BTreeMap<String, ecad_model::StyleRecord> = Default::default();
    let mut ops: Vec<Op> = Vec::new();
    for (k, v) in props {
        let style = k.strip_prefix("text.").and_then(|r| r.rsplit_once('.')).filter(|(_, f)| ["height", "weight", "font"].contains(f));
        match style {
            Some((kind, field)) => {
                records.entry(kind.to_string()).or_insert_with(|| ecad_model::StyleRecord::empty(ecad_model::StyleCatalogue::Text));
                // Design 072 §8: a value the field does not take cannot be stored; it stays absent.
                if let Some(ecad_model::objects::styles::StyleBody::Text(t)) = records.get_mut(kind).map(|r| &mut r.body) {
                    let _ = ecad_drawing_style::catalogues::text::set(t, field, Some(v));
                }
            }
            None => ops.push(Op::SetProperty { object: ObjectRef::Project, key: (*k).into(), before: None, after: Some((*v).into()) }),
        }
    }
    for (kind, r) in records {
        ops.push(Op::SetStyle {
            layer: ecad_model::StyleLayer::Project,
            catalogue: ecad_model::StyleCatalogue::Text,
            id: ecad_model::StyleRef::Builtin(kind),
            before: None,
            after: Some(r),
        });
    }
    for op in ops {
        op.apply(&mut doc).unwrap();
    }
    doc
}

/// T2: every kind's §3.2 default on an empty project, with the style name its weight gives.
#[test]
fn getter_returns_the_design_defaults_for_every_kind() {
    let d = doc_with(&[]);
    for (kind, height, weight) in KINDS {
        let s: TextStyle = ecad_drawing_style::text::text_style(&d, *kind);
        assert_eq!(s.height, *height, "{kind:?}");
        assert_eq!(s.weight, *weight, "{kind:?}");
        let name = if *weight == Weight::Bold { "ECAD-BOLD" } else { "Standard" };
        assert_eq!(s.style_name, name, "{kind:?}");
    }
    assert_eq!(ecad_drawing_style::text::text_style(&d, TextKind::Free).style_name, "Standard");
}

/// T2: hostile stored heights and weights give the default; a value in range is used.
#[test]
fn getter_ignores_hostile_stored_values() {
    for bad in ["NaN", "abc", "0.1", "11", "1e308", "-1", "inf", "", " 3 x"] {
        let d = doc_with(&[("text.tag.height", bad), ("text.tag.weight", "heavy")]);
        let s = ecad_drawing_style::text::text_style(&d, TextKind::Tag);
        assert_eq!(s.height, 2.5, "{bad:?}");
        assert!(s.height.is_finite());
        assert_eq!((s.weight, s.style_name), (Weight::Bold, "ECAD-BOLD"), "{bad:?}");
    }
    let huge = "9".repeat(10_000);
    let d = doc_with(&[("text.tag.height", &huge)]);
    assert_eq!(ecad_drawing_style::text::text_style(&d, TextKind::Tag).height, 2.5);
    let d = doc_with(&[("text.tag.height", "0.5"), ("text.point_label.height", "10"), ("text.tag.weight", "regular")]);
    assert_eq!(ecad_drawing_style::text::text_style(&d, TextKind::Tag).height, 0.5);
    assert_eq!(ecad_drawing_style::text::text_style(&d, TextKind::PointLabel).height, 10.0);
    assert_eq!(ecad_drawing_style::text::text_style(&d, TextKind::Tag).style_name, "Standard");
}
