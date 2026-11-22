//! The revision table's rules: the next rev, the column grammar, hostile settings, widths and dates.

use ecad_electrical::releases::revisions::*;
use ecad_model::Document;

#[test]
fn the_pending_rev_follows_the_projects_scheme() {
    use ecad_model::{Rev, RevPrefix, RevScheme, Revision};
    let issued = |rev| Revision { rev: Some(rev), issued: true, ..Revision::default() };
    let mut doc = Document::default();
    // Letters (the default): A, B ... AA.
    assert_eq!(pending_rev(&doc), "A");
    doc.project.revisions = vec![issued(Rev::Letter(25)), issued(Rev::Letter(26))];
    assert_eq!(pending_rev(&doc), "AA");
    // With a dash first issue: dash, A, B ...
    doc.project.rev_scheme = RevScheme::Letters { dash_first: true };
    doc.project.revisions.clear();
    assert_eq!(pending_rev(&doc), ecad_model::document::rev::DASH);
    doc.project.revisions = vec![issued(Rev::Initial)];
    assert_eq!(pending_rev(&doc), "A");
    // Numbered: P01, P02 ...
    doc.project.rev_scheme = RevScheme::Numbered { prefix: RevPrefix::new("P").ok(), width: 2, start: 1 };
    doc.project.revisions.clear();
    assert_eq!(pending_rev(&doc), "P01");
    doc.project.revisions = vec![issued(Rev::Number(9))];
    assert_eq!(pending_rev(&doc), "P10");
}

#[test]
fn a_rev_typed_for_the_pending_row_is_read_in_the_scheme_and_refused_naming_the_problem() {
    use ecad_model::{Project, RevScheme};
    let mut project = Project::default();
    let ok = with_field(&project, 1, "rev", Some("B")).unwrap();
    assert_eq!(ok.revisions[0].rev, Some(ecad_model::Rev::Letter(2)));
    for (text, words) in [("b", "not a rev of this project"), ("P01", "not a rev of this project")] {
        let e = with_field(&project, 1, "rev", Some(text)).unwrap_err().to_string();
        assert!(e.contains(words), "{text}: {e}");
    }
    project.rev_scheme = RevScheme::Letters { dash_first: false };
    let e = with_field(&project, 1, "rev", Some("-")).unwrap_err().to_string();
    assert!(e.contains("start at A"), "{e}");
    // A date is a date: free text is refused by name.
    let e = with_field(&project, 1, "date", Some("next week")).unwrap_err().to_string();
    assert!(e.contains("not a date") && e.contains("YYYY-MM-DD"), "{e}");
    let row = with_field(&project, 1, "date", Some("2026-11-22")).unwrap();
    assert_eq!(row.revisions[0].date.map(|d| d.to_string()).as_deref(), Some("2026-11-22"));
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
