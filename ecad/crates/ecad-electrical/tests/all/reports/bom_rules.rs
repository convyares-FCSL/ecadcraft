//! The rules of the BOM's inputs (brief 047, design 047 §7): quantities, part keys, `for` lists and
//! line numbers.

use std::collections::BTreeSet;

use ecad_electrical::reports::bom::{Field, Qty, expand_for, names_words, next_line, parse_part_key, parse_qty};

#[test]
fn quantities_parse_and_print() {
    assert_eq!(parse_qty("3.6", true), Ok(Qty(3600)));
    assert_eq!(parse_qty("1.50", true), Ok(Qty(1500)));
    assert_eq!(parse_qty("4", false), Ok(Qty(4000)));
    for bad in ["NaN", "-1", "1e308", "0.0001", "0", "10000", "", ".", "1.5.5", "inf"] {
        assert!(parse_qty(bad, true).is_err(), "{bad}");
    }
    assert!(parse_qty("1.5", false).is_err());
    assert!(parse_qty(&"9".repeat(10_000), true).is_err());
    assert_eq!(Qty(5600).to_string(), "5.6");
    assert_eq!(Qty(750).to_string(), "0.75");
    assert_eq!(Qty(4000).to_string(), "4");
}

#[test]
fn part_keys_parse() {
    assert_eq!(parse_part_key("part.1.maker"), Some(Ok((1, Field::Maker))));
    assert_eq!(parse_part_key("part.99.for"), Some(Ok((99, Field::For))));
    for bad in ["part.0.maker", "part.007.maker", "part.100.maker", "part.1.colour", "part.1", "part..maker"] {
        assert!(matches!(parse_part_key(bad), Some(Err(_))), "{bad}");
    }
    assert_eq!(parse_part_key("maker"), None);
}

#[test]
fn for_expands_and_is_bounded() {
    assert_eq!(expand_for("1, 5-6, 1").unwrap(), ["1", "5", "6"]);
    assert!(expand_for("1-99999").is_err());
    assert!(expand_for("4-1").is_err());
    assert_eq!(names_words(&["1".into(), "2".into(), "3".into(), "5".into(), "PE".into()]), "1-3, 5, PE");
}

#[test]
fn next_line_fills_after_the_highest() {
    assert_eq!(next_line(&BTreeSet::new()), Some(1));
    assert_eq!(next_line(&[1, 3].into_iter().collect()), Some(4));
    assert_eq!(next_line(&[2, 99].into_iter().collect()), Some(1));
    assert_eq!(next_line(&(1..=99).collect()), None);
}
