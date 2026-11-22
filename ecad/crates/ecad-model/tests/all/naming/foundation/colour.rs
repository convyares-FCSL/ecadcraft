//! Design 072 §4.6: every conductor colour has exactly one table of forms, the codes are unique,
//! the project stores the 2-letter code and any form reads back.

use std::collections::BTreeSet;

use ecad_model::geometry::colour::{COLOURS, ColourWords, ConductorColour, Rgb, load};

#[test]
fn every_colour_has_one_table_and_every_table_one_colour() {
    let table: toml::Table = toml::from_str(COLOURS).unwrap_or_default();
    let keys: BTreeSet<&str> = table.keys().map(String::as_str).collect();
    let colours: BTreeSet<&str> = ConductorColour::ALL.iter().map(|c| c.key()).collect();
    assert_eq!(keys, colours);
    assert_eq!(ConductorColour::ALL.len(), 14);
    let mut forms = BTreeSet::new();
    for c in ConductorColour::ALL {
        let f = c.forms();
        assert_eq!(f.code2.len(), 2, "{c:?}");
        assert_eq!(f.code3.len(), 3, "{c:?}");
        let own: BTreeSet<String> = [&f.code2, &f.code3, &f.name].iter().map(|w| w.to_ascii_uppercase()).collect();
        for w in own {
            assert!(forms.insert(w.clone()), "{w} names two colours");
        }
        if *c != ConductorColour::Grey {
            assert_ne!(f.hex, Rgb(0x80, 0x80, 0x80), "{c:?} read its own hex, not the fallback");
        }
    }
}

#[test]
fn the_table_is_today_s_codes() {
    let got: Vec<(&str, &str, &str)> =
        ConductorColour::ALL.iter().map(|c| (c.shown(ColourWords::Code2), c.shown(ColourWords::Code3), c.shown(ColourWords::Name))).collect();
    assert_eq!(got[0], ("BK", "BLK", "Black"));
    assert_eq!(got[6], ("BU", "BLU", "Blue"));
    assert_eq!(got[13], ("SR", "SLV", "Silver"));
    assert_eq!(ConductorColour::Turquoise.forms().hex, Rgb(0x40, 0xE0, 0xD0));
}

#[test]
fn stored_as_the_two_letter_code_read_in_any_form() -> Result<(), serde_json::Error> {
    for c in ConductorColour::ALL {
        let f = c.forms();
        assert_eq!(serde_json::to_string(c)?, format!("\"{}\"", f.code2));
        for form in [f.code2.clone(), f.code3.to_ascii_lowercase(), f.name.to_ascii_uppercase()] {
            assert_eq!(serde_json::from_str::<ConductorColour>(&format!("\"{form}\""))?, *c);
        }
    }
    let gnye: Vec<ConductorColour> = serde_json::from_str("[\"GN\",\"YE\"]")?;
    assert_eq!(gnye, [ConductorColour::Green, ConductorColour::Yellow]);
    let err = serde_json::from_str::<ConductorColour>("\"GNYE\"").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("`GNYE`") && err.contains("BU"), "{err}");
    Ok(())
}

#[test]
fn a_damaged_table_gives_safe_forms_never_a_crash() {
    for text in ["", "not toml [[[", "[blue]\ncode2 = 7\n"] {
        let forms = load(text);
        assert_eq!(forms.len(), 14);
        assert!(forms.iter().all(|f| !f.code2.is_empty()), "{text:?}");
    }
}
