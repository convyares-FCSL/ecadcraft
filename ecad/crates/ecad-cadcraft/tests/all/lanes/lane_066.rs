//! Brief 066 (design 066 §8): built-in defaults are data. The files are complete and valid, the
//! loader refuses hostile files without a panic, and the values read are the data's.

use ecad_cadcraft::defaults::data::{self, DefaultsError, MAX_LISTED, load};
use ecad_cadcraft::defaults::{self, Kind, Value};
use ecad_cadcraft::styles;

/// The built-in files load with no problem: every entry and style field has a value, every key
/// is known, every value is valid for its kind.
#[test]
fn the_built_in_data_is_complete_and_valid() {
    assert_eq!(data::builtin_problems(), &[] as &[DefaultsError]);
    for (name, text) in data::FILES {
        assert_eq!(load(name, text).problems(), &[] as &[DefaultsError], "{name}");
    }
    for e in data::entries() {
        let v = e.builtin();
        assert!(e.accepts(&v), "{}: built-in {v:?} is not a valid {:?}", e.key, e.kind);
    }
}

/// Every style field's built-in and every built-in style's value is valid for its field.
#[test]
fn every_built_in_style_value_is_valid_for_its_field() {
    for c in styles::CATALOGUES {
        for f in c.fields {
            let v = f.builtin();
            assert!(matches!(v, Value::Text("")) || defaults::accepts(f.kind, &v), "{}.{}: {v:?}", c.word, f.key);
        }
        for b in c.builtins {
            for (k, v) in b.values() {
                let f = c.field(k).unwrap_or_else(|| panic!("{}.{} names no field {k}", c.word, b.id));
                assert!(matches!(v, Value::Text("")) || defaults::accepts(f.kind, v), "{}.{}.{k}: {v:?}", c.word, b.id);
            }
        }
    }
}

/// What 066a moved reads the same as the Rust literals did (spot checks across areas and styles).
#[test]
fn the_data_holds_the_values_the_code_held() {
    use defaults::{connectors, plc, strip_pages, symbols, text, wires};
    let cases: &[(&defaults::Entry, Value<'static>)] = &[
        (&text::FONT, Value::Text(text::SHEET_FONT)),
        (&text::TAG_GAP, Value::Number(0.6)),
        (&text::RUN_GAP, Value::Length(15.0)),
        (&wires::WIRING_LABEL_RULE, Value::Choice("net")),
        (&wires::WIRING_LABEL_MIN_LENGTH, Value::Length(0.0)),
        (&symbols::POINT_GRID, Value::Length(ecad_cadcraft::symbols::POINT_GRID_MM)),
        (&strip_pages::PITCH, Value::Length(9.0)),
        (&plc::CHANNELS_DI, Value::Number(16.0)),
        (&connectors::SEPARATOR, Value::Text(".")),
    ];
    for (e, v) in cases {
        assert_eq!(e.builtin(), *v, "{}", e.key);
    }
    let text = &styles::text::CATALOGUE;
    let tag = text.builtins.iter().find(|b| b.id == "tag").map(|b| (b.value("height"), b.value("weight")));
    assert_eq!(tag, Some((Some(Value::Length(2.5)), Some(Value::Choice("bold")))));
    // The wire label style's height is the entry, one value in one place.
    let wl = text.builtins.iter().find(|b| b.id == "wire_label").and_then(|b| b.value("height"));
    assert_eq!(wl, Some(text::WIRE_LABEL_HEIGHT.builtin()));
    let height = text.field("height").map(|f| f.builtin());
    assert_eq!(height, Some(Value::Length(2.5)));
}

/// The problem a hostile file gives, by variant.
#[derive(Debug, PartialEq)]
enum Want {
    None,
    TooLarge,
    TooDeep,
    Syntax,
    WrongKind,
    OutOfRange,
    Unknown,
}

fn which(e: &DefaultsError) -> Want {
    match e {
        DefaultsError::TooLarge { .. } => Want::TooLarge,
        DefaultsError::TooDeep { .. } => Want::TooDeep,
        DefaultsError::Syntax { .. } => Want::Syntax,
        DefaultsError::WrongKind { .. } => Want::WrongKind,
        DefaultsError::OutOfRange { .. } => Want::OutOfRange,
        DefaultsError::UnknownKey { .. } | DefaultsError::MoreUnknownKeys { .. } => Want::Unknown,
        DefaultsError::Missing { .. } => Want::None,
    }
}

/// Design 066 §8, hostile files: each is refused with its error, never a panic, and gives no value,
/// so resolution falls through to the layer below.
#[test]
fn hostile_files_are_refused_with_their_error() {
    let height = &defaults::text::WIRE_LABEL_HEIGHT;
    let show = &defaults::text::TAG_SHOW;
    let font = &defaults::text::FONT;
    let mut junk: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    junk.retain(|b| *b != 0);
    let junk = String::from_utf8_lossy(&junk).into_owned();
    let huge = format!("[values]\n\"text.font\" = \"{}\"\n", "x".repeat(1 << 20));
    let deep = "[".repeat(10_000);
    let long = format!("[values]\n\"text.font\" = \"{}\"\n", "x".repeat(70_000));
    let dotted = format!("[values]\n{} = 1\n", vec!["a"; 10_000].join("."));
    let cases: Vec<(&str, String, Want, &defaults::Entry)> = vec![
        ("empty", String::new(), Want::None, height),
        ("binary junk", junk, Want::Syntax, height),
        ("1 MiB", huge, Want::TooLarge, font),
        ("10 000 deep", deep, Want::TooDeep, height),
        ("10 000 dotted", dotted, Want::TooDeep, height),
        ("number for a word", "[values]\n\"text.tag.show\" = 7\n".into(), Want::WrongKind, show),
        ("nan", "[values]\n\"text.wire_label.height\" = nan\n".into(), Want::OutOfRange, height),
        ("inf", "[values]\n\"text.wire_label.height\" = inf\n".into(), Want::OutOfRange, height),
        ("1e308", "[values]\n\"text.wire_label.height\" = 1e308\n".into(), Want::OutOfRange, height),
        ("-1", "[values]\n\"text.wire_label.height\" = -1\n".into(), Want::OutOfRange, height),
        ("text for a length", "[values]\n\"text.wire_label.height\" = \"2.5\"\n".into(), Want::WrongKind, height),
        ("control character", "[values]\n\"text.font\" = \"a\\u0007b\"\n".into(), Want::OutOfRange, font),
        ("70 000 characters", long, Want::OutOfRange, font),
        ("unknown word", "[values]\n\"text.tag.show\" = \"sideways\"\n".into(), Want::OutOfRange, show),
    ];
    for (what, text, want, entry) in &cases {
        let loaded = load(what, text);
        let got: Vec<Want> = loaded.problems().iter().map(which).collect();
        match want {
            Want::None => assert!(got.is_empty(), "{what}: {:?}", loaded.problems()),
            w => assert_eq!(got.first(), Some(w), "{what}: {:?}", loaded.problems()),
        }
        assert_eq!(loaded.value(entry), None, "{what}: no value, so the layer below is used");
    }
}

/// Unknown keys are listed up to a bound, then counted; good values beside them still load.
#[test]
fn unknown_keys_are_listed_then_counted() {
    let mut text = String::from("[values]\n\"text.tag.show\" = \"off\"\n");
    for i in 0..10_000 {
        text.push_str(&format!("\"text.nothing_{i}\" = 1\n"));
    }
    let loaded = load("many.toml", &text);
    let listed = loaded.problems().iter().filter(|p| matches!(p, DefaultsError::UnknownKey { .. })).count();
    assert_eq!(listed, MAX_LISTED);
    let more = loaded.problems().iter().find_map(|p| match p {
        DefaultsError::MoreUnknownKeys { count, .. } => Some(*count),
        _ => None,
    });
    assert_eq!(more, Some(10_000 - MAX_LISTED));
    assert_eq!(loaded.value(&defaults::text::TAG_SHOW), Some(Value::Choice("off")));
}

/// A syntax error names its line and column and the byte span the parser reports.
#[test]
fn a_syntax_error_says_where() {
    let loaded = load("bad.toml", "[values]\n\"text.font\" = = 2\n");
    match loaded.problems() {
        [DefaultsError::Syntax { file, line, column, span, .. }] => {
            assert_eq!((file.as_str(), *line), ("bad.toml", 2));
            assert!(*column > 1 && span.start >= 9, "{column} {span:?}");
        }
        other => panic!("{other:?}"),
    }
}

/// A value of the wrong kind or out of range carries its key and the span of the value.
#[test]
fn a_refused_value_names_its_key_and_span() {
    let text = "[values]\n\"text.wire_label.height\" = 99.0\n";
    match load("t.toml", text).problems() {
        [DefaultsError::OutOfRange { key, span, .. }] => {
            assert_eq!(key, "text.wire_label.height");
            assert_eq!(text.get(span.clone()), Some("99.0"));
        }
        other => panic!("{other:?}"),
    }
}

/// The safe value of each kind is inside its range or one of its words (design 066 §7).
#[test]
fn every_safe_value_is_valid_where_the_kind_allows_one() {
    for e in data::entries() {
        let v = data::safe_value(e.kind);
        match e.kind {
            Kind::Length { .. } | Kind::Number { .. } | Kind::Choice(_) | Kind::Style(_) => {
                assert!(e.accepts(&v), "{}: safe {v:?}", e.key);
            }
            Kind::Text { .. } | Kind::Parsed { .. } => assert_eq!(v, Value::Text("")),
        }
    }
}

/// A tested build has no `defaults-damaged` row.
#[test]
fn a_tested_build_reports_no_damaged_defaults() {
    let doc = ecad_model::Document::default();
    let rows = ecad_cadcraft::checks::all(&doc, 0.01);
    assert!(rows.iter().all(|c| c.kind != ecad_cadcraft::checks::Kind::DefaultsDamaged));
}
