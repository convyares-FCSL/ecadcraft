//! Design 072 §9.3: page places are typed: a page number is well formed or refused.

use ecad_model::{PageColumn, PageNo, PageRow};

#[test]
fn page_numbers_are_well_formed() -> Result<(), serde_json::Error> {
    let table: &[(&str, Option<&str>)] = &[
        ("100", Some("100")),
        (" 105a ", Some("105a")),
        ("A_01", Some("A_01")),
        ("123456789012", Some("123456789012")),
        ("1234567890123", None),
        ("", None),
        ("1.5", None),
        ("1-2", None),
        ("é1", None),
    ];
    for (text, want) in table {
        assert_eq!(PageNo::parse(text).as_ref().map(PageNo::as_str), *want, "PageNo::parse({text:?})");
    }
    assert_eq!(serde_json::to_string(&PageNo::parse("105a"))?, "\"105a\"");
    assert_eq!(serde_json::from_str::<PageNo>("\"105a\"")?.as_str(), "105a");
    for bad in ["\"1.5\"", "\" 105\"", "\"\"", "5"] {
        assert!(serde_json::from_str::<PageNo>(bad).is_err(), "{bad} is refused");
    }
    Ok(())
}

#[test]
fn grid_places_store_as_numbers() -> Result<(), serde_json::Error> {
    assert_eq!(serde_json::to_string(&(PageColumn(3), PageRow(4)))?, "[3,4]");
    assert!(PageColumn(2) < PageColumn(10));
    Ok(())
}
