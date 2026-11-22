//! The sheet numbers and pagination of generated pages.

use ecad_electrical::pages::generated::{paginate, rows_per_sheet, sheet_number};

#[test]
fn continuation_sheets_are_numbered_in_sequence() {
    for (n, i, want) in [
        ("400", 0, "400"),
        ("400", 1, "401"),
        ("400", 2, "402"),
        ("0090", 1, "0091"),
        ("B10", 1, "B11"),
        ("099", 1, "100"),
        ("BOM", 1, "BOM2"),
        ("BOM", 2, "BOM3"),
        ("18446744073709551615", 1, "184467440737095516152"),
    ] {
        assert_eq!(sheet_number(n, i), want, "{n} sheet {i}");
    }
}

#[test]
fn a_continued_heading_takes_a_row_of_its_sheet() {
    let mut heads = vec![Some("+CP1".to_string())];
    heads.extend(std::iter::repeat_n(None, 88));
    let sheets = paginate(&heads);
    assert_eq!(sheets.len(), 3);
    for (start, end, cont) in &sheets {
        assert!(end - start + usize::from(cont.is_some()) <= rows_per_sheet());
    }
    assert_eq!(sheets[1].2.as_deref(), Some("+CP1"));
    assert_eq!(paginate(&[]).len(), 1);
}
