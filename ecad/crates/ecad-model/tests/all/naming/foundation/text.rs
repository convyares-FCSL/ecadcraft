//! A1: `Text<N>` and `LongText<N>` refuse every hidden and bidi character, and
//! keep to their length and line rules.

use ecad_model::naming::text::{LongText, Text, TextError};
use ecad_model::naming::words::{HIDDEN_RANGES, is_hidden};

#[test]
fn every_hidden_character_is_refused() {
    let mut hidden: Vec<char> = HIDDEN_RANGES.iter().flat_map(|(lo, hi)| (*lo..=*hi).filter_map(char::from_u32)).collect();
    hidden.extend(['\u{0}', '\t', '\u{1B}', '\u{7F}', '\u{85}', '\u{9F}']);
    for c in hidden {
        assert!(is_hidden(c), "U+{:04X} is hidden", u32::from(c));
        let text = format!("a{c}b");
        assert_eq!(Text::<8>::new(&text), Err(TextError::Hidden { at: 1, char: c }), "Text refuses U+{:04X}", u32::from(c));
        assert!(LongText::<8>::new(&text).is_err(), "LongText refuses U+{:04X}", u32::from(c));
        assert!(
            serde_json::from_str::<Text<8>>(&serde_json::to_string(&text).unwrap_or_default()).is_err(),
            "a file holding U+{:04X} is refused",
            u32::from(c)
        );
    }
    for c in ['a', 'é', 'Ω', '€', '中', ' ', '-', '/', '\u{2013}'] {
        assert!(!is_hidden(c), "{c:?} is not hidden");
    }
}

#[test]
fn lengths_and_lines() {
    let table: &[(&str, Result<&str, TextError>)] = &[
        ("  Motor  ", Ok("Motor")),
        ("12345678", Ok("12345678")),
        ("123456789", Err(TextError::TooLong { max: 8 })),
        ("ééééééé", Ok("ééééééé")),
        ("", Err(TextError::Empty)),
        ("   ", Err(TextError::Empty)),
        ("a\nb", Err(TextError::Lines)),
        ("a\r\nb", Err(TextError::Lines)),
    ];
    for (text, want) in table {
        assert_eq!(Text::<8>::new(text).as_ref().map(Text::as_str).map_err(|e| *e), *want, "Text::<8>::new({text:?})");
    }
    assert_eq!(LongText::<8>::new(" a\nb ").as_ref().map(LongText::as_str), Ok("a\nb"));
    assert!(LongText::<8>::new("a\r\nb").is_err(), "a carriage return is a control character");
    assert_eq!(Text::<8>::MAX, 8);
}

#[test]
fn files_hold_text_already_in_its_form() -> Result<(), Box<dyn std::error::Error>> {
    let t = Text::<16>::new("Pump")?;
    assert_eq!(serde_json::to_string(&t)?, "\"Pump\"");
    assert_eq!(serde_json::from_str::<Text<16>>("\"Pump\"")?, t);
    for bad in [r#"" Pump""#, r#""""#, r#""12345678901234567""#, r#""a\nb""#] {
        let err = serde_json::from_str::<Text<16>>(bad).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(!err.is_empty(), "{bad} is refused");
    }
    let err = serde_json::from_str::<Text<16>>(r#""a\u202eb""#).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("U+202E"), "the error names the character: {err}");
    Ok(())
}
