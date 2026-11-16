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

// ---------------------------------------------------------------- 066b: the saved-file rule (F2)

use ecad_cadcraft::Persistence;
use ecad_cadcraft::defaults::{Scope, resolve};
use ecad_model::Document;
use serde_json::json;

use crate::common::{H, close_all, pts, reopen, tmp_dir};

/// Every entry's resolved value at project scope, as text, with where it came from.
fn resolved_all(doc: &Document) -> Vec<(&'static str, String, &'static str)> {
    data::entries().map(|e| (e.key, resolve(doc, e, Scope::project()).text(), resolve(doc, e, Scope::project()).from.as_str())).collect()
}

fn wire_label_height(doc: &Document) -> Value<'_> {
    resolve(doc, &defaults::text::WIRE_LABEL_HEIGHT, Scope::project()).value
}

/// A project with one labelled wire, the label height chosen (`Some`) or not.
fn project(seed: u64, height: Option<f64>) -> H {
    let mut h = H::new(seed);
    h.run("wire", json!({"points": pts(&[(20.0, 20.0), (80.0, 20.0)]), "potential": "24V"})).unwrap();
    if let Some(x) = height {
        h.run("wiresettings", json!({"label_height": x})).unwrap();
    }
    h
}

/// Save `h` as a file and open it in a fresh session.
fn save_and_reopen(mut h: H, tag: &str, seed: u64) -> H {
    h.cfg(|p| p.persistence = Persistence::File);
    let dir = tmp_dir(tag);
    let path = dir.join("p066").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    close_all(&mut h.s);
    let h = reopen(&format!("{path}.ecad"), seed);
    // The folder may still be open on Windows; a leftover temporary folder is harmless.
    let _ = std::fs::remove_dir_all(&dir);
    h
}

/// 066b: the first value changed as data. A project that chose no height draws 2.5 mm labels.
#[test]
fn a_project_that_chose_no_height_follows_the_built_in() {
    assert_eq!(defaults::text::WIRE_LABEL_HEIGHT.builtin(), Value::Length(2.5));
    let mut h = project(0x66_b1, None);
    let doc = h.doc();
    assert_eq!(doc.settings.wiring.label_height, None);
    assert_eq!(wire_label_height(&doc), Value::Length(2.5));
    let page = h.first_page();
    let labels = ecad_cadcraft::wires::labels_on_page(&doc, page);
    assert!(!labels.is_empty() && labels.iter().all(|l| (l.height - 2.5).abs() < 1e-9), "{:?}", labels.iter().map(|l| l.height).collect::<Vec<_>>());
}

/// A chosen value is kept, even one equal to the built-in; an unset one stays unset, through a
/// save and an open (the file format, brief 066 F2).
#[test]
fn chosen_and_unset_heights_survive_save_and_open() {
    for (i, chosen) in [Some(1.6), Some(2.5), None].into_iter().enumerate() {
        let seed = 0x66_b2 + i as u64;
        let mut h = project(seed, chosen);
        let before = h.doc();
        assert_eq!(before.settings.wiring.label_height, chosen, "{chosen:?}");
        let want = resolved_all(&before);
        let mut h = save_and_reopen(h, "066 chosen", seed);
        let after = h.doc();
        assert_eq!(after.settings.wiring.label_height, chosen, "{chosen:?}: kept as chosen or not chosen");
        assert_eq!(resolved_all(&after), want, "{chosen:?}: every entry resolves the same");
        let page = h.first_page();
        let height = chosen.unwrap_or(2.5);
        let labels = ecad_cadcraft::wires::labels_on_page(&after, page);
        assert!(labels.iter().all(|l| (l.height - height).abs() < 1e-9), "{chosen:?}");
    }
}

/// The v1 record of the wiring settings: a chosen value (also one equal to the built-in) is written,
/// an unchosen one is left out, and both read back as they were.
#[test]
fn the_v1_record_writes_only_chosen_values() {
    let cases: [(Option<f64>, Option<&str>, Option<f64>); 3] =
        [(None, None, None), (Some(2.5), Some("net"), Some(0.0)), (Some(1.6), Some("each"), Some(10.0))];
    for (height, rule, min) in cases {
        let mut w = ecad_model::wiring::WiringSettings::default();
        (w.label_height, w.label_rule, w.label_min_length) = (height, rule.map(str::to_string), min);
        let v = serde_json::to_value(ecad_file::v1::WiringSettings::from(&w)).unwrap();
        for (key, present) in [("label_height", height.is_some()), ("label_rule", rule.is_some()), ("label_min_length", min.is_some())] {
            assert_eq!(v.get(key).is_some(), present, "{key} in {v}");
        }
        let back: ecad_file::v1::WiringSettings = serde_json::from_value(v).unwrap();
        assert_eq!(ecad_model::wiring::WiringSettings::from(back), w);
    }
}

/// Two machines, same read: a project never reads a company file from the machine. Files named
/// like a company standard beside the project and in the working folder change nothing.
#[test]
fn a_project_reads_the_same_whatever_company_files_the_machine_holds() {
    let company = "[values]\n\"text.wire_label.height\" = 5.0\n\"text.tag.show\" = \"off\"\n";
    let mut reads = Vec::new();
    for (i, files) in [&[][..], &["company.toml", "defaults.toml"][..], &["text.toml", "standard.toml"][..]].into_iter().enumerate() {
        let seed = 0x66_b6 + i as u64;
        let mut h = project(seed, None);
        h.cfg(|p| p.persistence = Persistence::File);
        let dir = tmp_dir("066 machines");
        for f in files {
            std::fs::write(dir.join(f), company).unwrap();
            std::fs::create_dir_all(dir.join("defaults")).unwrap();
            std::fs::write(dir.join("defaults").join(f), company).unwrap();
        }
        let path = dir.join("p066").display().to_string();
        h.run("saveas", json!({"path": path})).unwrap();
        close_all(&mut h.s);
        let mut h = reopen(&format!("{path}.ecad"), seed);
        let doc = h.doc();
        let page = h.first_page();
        let heights: Vec<f64> = ecad_cadcraft::wires::labels_on_page(&doc, page).iter().map(|l| l.height).collect();
        reads.push((resolved_all(&doc), heights));
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(reads.windows(2).all(|w| w[0] == w[1]), "every machine reads the project the same");
}
