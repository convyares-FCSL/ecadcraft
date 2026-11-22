//! The revision table's rules: the next rev, the column grammar, hostile settings, widths and dates.

use ecad_electrical::releases::revisions::*;
use ecad_model::Document;

#[test]
fn next_revs() {
    for (last, next) in [(None, "A"), (Some("A"), "B"), (Some("Z"), "AA"), (Some("AZ"), "BA"), (Some("0"), "1"), (Some("P3"), "P4")] {
        assert_eq!(next_rev(last), next, "{last:?}");
    }
}

#[test]
fn the_column_grammar() {
    let c = parse_columns("rev, date, ecr:ECR@20, description, validated:Validated").unwrap();
    assert_eq!(c.len(), 5);
    assert_eq!(c[2], ColumnSpec { field: "ecr".into(), header: Some("ECR".into()), width: Some(20.0) });
    assert_eq!(parse_columns("").unwrap(), Vec::new());
    for (bad, words) in [
        ("rev, ecr!, description", "\"ecr!\" is not a field name"),
        ("rev, date", "Rev and Description must be columns."),
        ("rev, released, description", "\"released\" is set by issuing."),
        ("rev, x@NaN, description", "is not a width"),
        ("rev, x@-5, description", "is not a width"),
        ("rev, x@1e308, description", "is not a width"),
        ("rev, rev, description", "a column twice"),
    ] {
        let e = parse_columns(bad).unwrap_err().to_string();
        assert!(e.contains(words), "{bad}: {e}");
    }
    let many = (0..1000).map(|i| format!("f{i}")).collect::<Vec<_>>().join(", ");
    assert!(parse_columns(&format!("rev, description, {many}")).is_err());
}

#[test]
fn hostile_settings_read_as_absent() {
    let mut doc = Document::default();
    for v in ["x".repeat(10_000), "rev, ecr@NaN, description".into(), (0..1000).map(|i| format!("f{i}")).collect::<Vec<_>>().join(","), "".into()] {
        doc.display.insert(COLUMNS_KEY.into(), v);
        let cols = columns(&doc);
        // Brief 055 (FP5): the built-in columns, ECR after Rev.
        assert_eq!(
            cols.iter().map(|c| c.field.as_str()).collect::<Vec<_>>(),
            ["rev", "ecr", "date", "description", "drawn", "checked", "approved"],
            "the built-in columns"
        );
    }
    for v in ["", "x", "STAR?"] {
        doc.display.insert(PENDING_KEY.into(), v.into());
        assert_eq!(PendingStyle::of(&doc), PendingStyle::Star);
    }
    doc.display.insert("tags.case".into(), "x".into());
    doc.display.insert("tags.dash".into(), "maybe".into());
    assert_eq!(ecad_electrical::defaults::print::tag_style(&doc), ecad_model::naming::tag::Style::from(doc.settings.tag_padding()));
}

#[test]
fn the_registry_and_ecad_model_read_the_tag_style_alike() {
    let mut doc = Document::default();
    for (dash, case) in [("yes", "upper"), ("no", "typed"), ("NO", "Typed"), ("", "")] {
        doc.display.insert("tags.dash".into(), dash.into());
        doc.display.insert("tags.case".into(), case.into());
        assert_eq!(ecad_electrical::defaults::print::tag_style(&doc), ecad_model::naming::tag::Style::of(&doc), "{dash} {case}");
    }
}

#[test]
fn widths_narrow_rather_than_refuse() {
    let cols = |w: &[Option<f64>]| w.iter().map(|w| Column { field: "f".into(), header: "F".into(), width: *w }).collect::<Vec<_>>();
    let (w, narrowed) = widths(&cols(&[Some(12.0), None, Some(22.0)]), 170.0);
    assert_eq!((w, narrowed), (vec![12.0, 136.0, 22.0], false));
    let (w, narrowed) = widths(&cols(&[Some(100.0), None, Some(100.0)]), 170.0);
    assert!(narrowed && (w[1] - MIN_DESCRIPTION_MM).abs() < 1e-9 && (w.iter().sum::<f64>() - 170.0).abs() < 1e-9, "{w:?}");
}

#[test]
fn dates_go_through_chrono() {
    for (ms, date) in [(0, "1970-01-01"), (951_782_400_000, "2000-02-29"), (1_794_441_600_000, "2026-11-12"), (4_107_542_399_999, "2100-02-28")] {
        assert_eq!(date_of_ms(ms).as_deref(), Some(date), "{ms}");
    }
    assert_eq!(date_of_ms(u64::MAX), None);
}
