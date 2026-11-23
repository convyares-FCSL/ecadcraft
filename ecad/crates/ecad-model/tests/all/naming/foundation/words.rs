//! A1: `one_boolean_rule`, `whole_numbers`: the word readers on tables.

use ecad_model::naming::words::{flag, flag_word, is_digits, uint, uint_canonical};

#[test]
fn one_boolean_rule() {
    let table: &[(&str, Option<bool>)] = &[
        ("yes", Some(true)),
        ("YES", Some(true)),
        (" true ", Some(true)),
        ("1", Some(true)),
        ("On", Some(true)),
        ("no", Some(false)),
        ("False", Some(false)),
        ("0", Some(false)),
        ("off", Some(false)),
        ("y", None),
        ("n", None),
        ("", None),
        ("2", None),
        ("yes please", None),
        ("enabled", None),
    ];
    for (text, want) in table {
        assert_eq!(flag(text), *want, "flag({text:?})");
    }
    assert_eq!(flag_word(true), "yes");
    assert_eq!(flag_word(false), "no");
    for v in [true, false] {
        assert_eq!(flag(flag_word(v)), Some(v), "the written word reads back");
    }
}

#[test]
fn whole_numbers() {
    let digits: &[(&str, bool)] = &[("0", true), ("0123", true), ("", false), ("1a", false), ("-1", false), (" 1", false), ("١", false)];
    for (text, want) in digits {
        assert_eq!(is_digits(text), *want, "is_digits({text:?})");
    }
    let lenient: &[(&str, u32, Option<u32>)] = &[
        ("7", 99, Some(7)),
        (" 07 ", 99, Some(7)),
        ("99", 99, Some(99)),
        ("100", 99, None),
        ("", 99, None),
        ("+1", 99, None),
        ("1.0", 99, None),
        ("1 000", 9999, None),
        ("4294967295", u32::MAX, Some(u32::MAX)),
        ("4294967296", u32::MAX, None),
        ("00000000001", u32::MAX, None),
    ];
    for (text, max, want) in lenient {
        assert_eq!(uint(text, *max), *want, "uint({text:?}, {max})");
    }
    let canonical: &[(&str, u32, Option<u32>)] =
        &[("0", 9, Some(0)), ("12", 99, Some(12)), ("012", 99, None), ("00", 99, None), (" 12", 99, None), ("12 ", 99, None), ("13", 12, None)];
    for (text, max, want) in canonical {
        assert_eq!(uint_canonical(text, *max), *want, "uint_canonical({text:?}, {max})");
    }
}
