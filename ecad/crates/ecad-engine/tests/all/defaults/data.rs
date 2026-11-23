//! Built-in defaults are data. The files are complete and valid, the
//! loader refuses hostile files without a panic, and the values read are the data's.

use ecad_electrical::defaults::data::{self, DATA};
use ecad_electrical::defaults::{self};
use ecad_model::registry::data::DefaultsError;
use ecad_model::registry::data::{Loaded, MAX_LISTED, load_in};
use ecad_model::registry::{Kind, Value};

/// `text` loaded as one of the look's files (its keys are the look's: font, Shown, wire label height).
fn load<'i>(name: &str, text: &'i str) -> Loaded<'i> {
    load_in(&ecad_drawing_style::defaults::data::DATA, name, text)
}

/// The built-in files load with no problem: every entry and style field has a value, every key
/// is known, every value is valid for its kind.
#[test]
fn the_built_in_data_is_complete_and_valid() {
    for set in ecad_drawing_style::REGISTRY.sets() {
        assert_eq!(set.problems(), &[] as &[DefaultsError], "{set:?}");
        for (name, text) in set.files {
            assert_eq!(load_in(set, name, text).problems(), &[] as &[DefaultsError], "{name}");
        }
        for e in set.valued() {
            let v = e.builtin();
            assert!(e.accepts(&v), "{}: built-in {v:?} is not a valid {:?}", e.key, e.kind);
        }
    }
}

/// Every style field's built-in and every built-in style's value is valid for its field.
#[test]
fn every_built_in_style_value_is_valid_for_its_field() {
    for c in ecad_drawing_style::REGISTRY.catalogues() {
        for f in c.fields {
            let v = f.builtin();
            assert!(matches!(v, Value::Text("")) || ecad_model::registry::accepts(f.kind, &v), "{}.{}: {v:?}", c.catalogue, f.key);
        }
        for b in c.builtins {
            for (k, v) in b.values() {
                let f = c.field(k).unwrap_or_else(|| panic!("{}.{} names no field {k}", c.catalogue, b.id));
                assert!(matches!(v, Value::Text("")) || ecad_model::registry::accepts(f.kind, v), "{}.{}.{k}: {v:?}", c.catalogue, b.id);
            }
        }
    }
}

/// What 066a moved reads the same as the Rust literals did (spot checks across areas and styles).
#[test]
fn the_data_holds_the_values_the_code_held() {
    use defaults::{connectors, plc, strip_pages, symbols, text, wires};
    let cases: &[(&ecad_model::registry::Entry, Value<'static>)] = &[
        (&ecad_drawing_style::defaults::text::FONT, Value::Text(ecad_drawing_style::defaults::text::SHEET_FONT)),
        (&text::TAG_GAP, Value::Number(0.6)),
        (&text::RUN_GAP, Value::Length(15.0)),
        (&wires::WIRING_LABEL_RULE, Value::Choice("net")),
        (&wires::WIRING_LABEL_MIN_LENGTH, Value::Length(0.0)),
        (&symbols::POINT_GRID, Value::Length(ecad_electrical::symbols::graphics::POINT_GRID_MM)),
        (&strip_pages::PITCH, Value::Length(9.0)),
        (&plc::CHANNELS_DI, Value::Number(16.0)),
        (&connectors::SEPARATOR, Value::Text(".")),
    ];
    for (e, v) in cases {
        assert_eq!(e.builtin(), *v, "{}", e.key);
    }
    let text = &ecad_drawing_style::catalogues::text::CATALOGUE;
    let tag = text.builtins.iter().find(|b| b.id == "tag").map(|b| (b.value("height"), b.value("weight")));
    assert_eq!(tag, Some((Some(Value::Length(2.5)), Some(Value::Choice("bold")))));
    // The wire label style's height is the entry, one value in one place.
    let wl = text.builtins.iter().find(|b| b.id == "wire_label").and_then(|b| b.value("height"));
    assert_eq!(wl, Some(ecad_drawing_style::defaults::text::WIRE_LABEL_HEIGHT.builtin()));
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
    let height = &ecad_drawing_style::defaults::text::WIRE_LABEL_HEIGHT;
    let show = &ecad_drawing_style::defaults::text::TAG_SHOW;
    let font = &ecad_drawing_style::defaults::text::FONT;
    let mut junk: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    junk.retain(|b| *b != 0);
    let junk = String::from_utf8_lossy(&junk).into_owned();
    let huge = format!("[values]\n\"text.font\" = \"{}\"\n", "x".repeat(1 << 20));
    let deep = "[".repeat(10_000);
    let long = format!("[values]\n\"text.font\" = \"{}\"\n", "x".repeat(70_000));
    let dotted = format!("[values]\n{} = 1\n", vec!["a"; 10_000].join("."));
    let cases: Vec<(&str, String, Want, &ecad_model::registry::Entry)> = vec![
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
    let mut text = String::from("[values]\n\"text.tag.show\" = \"no\"\n");
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
    assert_eq!(loaded.value(&ecad_drawing_style::defaults::text::TAG_SHOW), Some(Value::Choice("no")));
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
    for e in DATA.valued() {
        let v = ecad_model::registry::data::safe_value(e.kind);
        match e.kind {
            Kind::Length { .. } | Kind::Number { .. } | Kind::Choice(_) | Kind::Flag | Kind::Style(_) => {
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
    let rows = ecad_engine::checks::all(&doc, 0.01);
    assert!(rows.iter().all(|c| c.kind != ecad_electrical::checks::Kind::DefaultsDamaged));
}

// ---------------------------------------------------------------- 066b: the saved-file rule (F2)

use ecad_engine::Persistence;
use ecad_model::Document;
use ecad_model::registry::{Scope, resolve};
use serde_json::json;

use crate::common::{H, close_all, pts, reopen, tmp_dir};

/// Every entry's resolved value at project scope, as text, with where it came from.
fn resolved_all(doc: &Document) -> Vec<(&'static str, String, &'static str)> {
    DATA.valued().map(|e| (e.key, resolve(doc, e, Scope::project()).text(), resolve(doc, e, Scope::project()).from.as_str())).collect()
}

fn wire_label_height(doc: &Document) -> Value<'_> {
    resolve(doc, &ecad_drawing_style::defaults::text::WIRE_LABEL_HEIGHT, Scope::project()).value
}

/// A project with one labelled wire, the label height chosen (`Some`) or not.
fn project(seed: u64, height: Option<f64>) -> H {
    let mut h = H::new(seed);
    h.points_at(&[(20.0, 20.0), (80.0, 20.0)]);
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
    assert_eq!(ecad_drawing_style::defaults::text::WIRE_LABEL_HEIGHT.builtin(), Value::Length(2.5));
    let mut h = project(0x66_b1, None);
    let doc = h.doc();
    assert_eq!(doc.settings().wiring.label_height, None);
    assert_eq!(wire_label_height(&doc), Value::Length(2.5));
    let page = h.first_page();
    let labels = ecad_engine::wires::draw::labels_on_page(&doc, page);
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
        assert_eq!(before.settings().wiring.label_height, chosen, "{chosen:?}");
        let want = resolved_all(&before);
        let mut h = save_and_reopen(h, "066 chosen", seed);
        let after = h.doc();
        assert_eq!(after.settings().wiring.label_height, chosen, "{chosen:?}: kept as chosen or not chosen");
        assert_eq!(resolved_all(&after), want, "{chosen:?}: every entry resolves the same");
        let page = h.first_page();
        let height = chosen.unwrap_or(2.5);
        let labels = ecad_engine::wires::draw::labels_on_page(&after, page);
        assert!(labels.iter().all(|l| (l.height - height).abs() < 1e-9), "{chosen:?}");
    }
}

/// The record of the wiring settings: a chosen value (also one equal to the built-in) is written,
/// an unchosen one is left out, and both read back as they were.
#[test]
fn the_record_writes_only_chosen_values() {
    let cases: [(Option<f64>, Option<&str>, Option<f64>); 3] =
        [(None, None, None), (Some(2.5), Some("net"), Some(0.0)), (Some(1.6), Some("each"), Some(10.0))];
    for (height, rule, min) in cases {
        let mut w = ecad_model::connectivity::wiring::WiringSettings::default();
        (w.label_height, w.label_rule, w.label_min_length) = (height, rule.map(str::to_string), min);
        let v = serde_json::to_value(ecad_file::records::WiringSettings::from(&w)).unwrap();
        for (key, present) in [("label_height", height.is_some()), ("label_rule", rule.is_some()), ("label_min_length", min.is_some())] {
            assert_eq!(v.get(key).is_some(), present, "{key} in {v}");
        }
        let back: ecad_file::records::WiringSettings = serde_json::from_value(v).unwrap();
        assert_eq!(ecad_model::connectivity::wiring::WiringSettings::from(back), w);
    }
}

/// Two machines, same read: a project never reads a company file from the machine. Files named
/// like a company standard beside the project and in the working folder change nothing.
#[test]
fn a_project_reads_the_same_whatever_company_files_the_machine_holds() {
    let company = "[values]\n\"text.wire_label.height\" = 5.0\n\"text.tag.show\" = \"no\"\n";
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
        let heights: Vec<f64> = ecad_engine::wires::draw::labels_on_page(&doc, page).iter().map(|l| l.height).collect();
        reads.push((resolved_all(&doc), heights));
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(reads.windows(2).all(|w| w[0] == w[1]), "every machine reads the project the same");
}

// ---------------------------------------------------------------- 066c: the other settings (F3)

/// No repeated values (owner, 2026-11-16): no key appears in both ecad-model's data and this
/// crate's files, and every registry entry takes its built-in from exactly one of them.
#[test]
fn every_built_in_lives_in_exactly_one_place() {
    let model = ecad_model::naming::builtins::KEYS;
    let files: Vec<_> = data::FILES.iter().map(|(name, text)| (*name, load(name, text))).collect();
    // ecad-model's own test checks its file holds exactly `KEYS`.
    for key in model {
        for (name, f) in &files {
            assert!(!f.names(key), "{key} is ecad-model's, yet {name} names it");
        }
    }
    for e in DATA.valued() {
        let in_files = files.iter().filter(|(_, f)| f.names(e.key)).count();
        let in_model = usize::from(ecad_model::registry::data::in_model(e.key));
        assert_eq!(in_files + in_model, 1, "{}: built-in in {in_files} file(s) and {in_model} model key(s)", e.key);
    }
    // Every model key but the structured sheet is a registry entry reading it.
    for key in model.iter().filter(|k| **k != ecad_model::naming::builtins::SHEET) {
        let e = ecad_drawing_style::REGISTRY.find(key).unwrap_or_else(|| panic!("{key} has no entry"));
        assert!(ecad_model::registry::data::in_model(e.key) && e.accepts(&e.builtin()), "{key}");
    }
    // And they are today's values: nothing moves.
    let b = ecad_model::naming::builtins::builtins();
    assert_eq!((b.tag_padding, b.wire_digits, b.page_number_step), (2, 2, 1));
    assert_eq!(b.sheet, ecad_model::SheetSpec::default());
}

/// Rows per sheet are computed from the row and header heights: 44 on A3, as before.
#[test]
fn rows_per_sheet_are_computed_and_unchanged() {
    use {
        ecad_electrical::pages::generated::header_mm, ecad_electrical::pages::generated::row_mm, ecad_electrical::pages::generated::rows_per_sheet,
    };
    assert_eq!((row_mm(), header_mm(), rows_per_sheet()), (5.0, 7.0, 44));
}

/// F3: a chosen setting is kept through a save and an open, even when it equals the built-in;
/// one not chosen stays not chosen and reads the built-in.
#[test]
fn chosen_and_unset_settings_survive_save_and_open() {
    let mut chosen = ecad_model::ProjectSettings {
        tag_padding: Some(2),
        page_number_step: Some(1),
        reference_style: Some(ecad_model::ReferenceStyle::PageZone),
        sheet: Some(ecad_model::SheetSpec::default()),
        ..Default::default()
    };
    chosen.wiring.digits = Some(2);
    chosen.wiring.colour_style = Some(ecad_model::geometry::colour::ColourWords::Code2);
    for (i, want) in [chosen, ecad_model::ProjectSettings::default()].into_iter().enumerate() {
        let seed = 0x66_c1 + i as u64;
        let mut h = project(seed, None);
        let before = h.doc().settings().clone();
        if before != want {
            h.project.apply_ops(&mut h.s, "settings", vec![ecad_model::Op::SetProjectSettings { before, after: want.clone() }]).unwrap();
        }
        let resolved = resolved_all(&h.doc());
        let mut h = save_and_reopen(h, "066 settings", seed);
        let doc = h.doc();
        assert_eq!(*doc.settings(), want, "{i}: kept as chosen or not chosen");
        assert_eq!(resolved_all(&doc), resolved, "{i}: every entry resolves the same");
        assert_eq!((doc.settings().tag_padding(), doc.settings().page_number_step()), (2, 1));
    }
}

/// `setdefault` writes a settings-backed entry at project level as a chosen value, and `reset`
/// clears it to not chosen.
#[test]
fn setdefault_writes_and_clears_a_settings_entry() {
    let mut h = H::new(0x66_c3);
    h.run("setdefault", json!({"key": "tags.padding", "level": "project", "value": "3"})).unwrap();
    assert_eq!(h.doc().settings().tag_padding, Some(3));
    h.run("setdefault", json!({"key": "tags.padding", "level": "project", "reset": true})).unwrap();
    assert_eq!(h.doc().settings().tag_padding, None);
    h.run("setdefault", json!({"key": "print.reference_style", "level": "project", "value": "page-column"})).unwrap();
    assert_eq!(h.doc().settings().reference_style(), ecad_model::ReferenceStyle::PageColumn);
}

// ---------------------------------------------------------------- 066d: drawn text reads its style

/// The heights of the text entities `value` is drawn with on `page`.
fn drawn_heights(h: &mut H, page: ecad_model::PageId, value: &str) -> Vec<f64> {
    h.pv(|p, pg| pg.entities_of(p, page))
        .iter()
        .filter_map(|e| match &e.kind {
            cadcraft_engine::doc::EntityKind::Text(t) if t.value == value => Some(t.height),
            _ => None,
        })
        .collect()
}

/// A contact's cross-reference is reference text: the `Reference` style's height (2.5 built in),
/// and editing that style moves it (owner, 2026-11-16).
#[test]
fn contact_cross_references_follow_the_reference_style() {
    let mut h = H::new(0x66_d1);
    let coil = h.run("place", json!({"symbol": "coil", "at": [75, 180]})).unwrap();
    let k1 = crate::common::hex(&coil, "component");
    let p2: ecad_model::PageId = crate::common::id(&h.run("newpage", json!({"title": "Contacts"})).unwrap(), "page");
    h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k1})).unwrap();
    let heights = |h: &mut H| -> Vec<f64> {
        let doc = h.doc();
        ecad_engine::references::draw::devices::parts_on_page(&doc, p2)
            .into_iter()
            .filter_map(|p| match p.shape {
                ecad_electrical::references::devices::table::Shape::Text { kind: ecad_model::look::TextKind::Reference, height, .. } => Some(height),
                _ => None,
            })
            .collect()
    };
    let before = heights(&mut h);
    assert!(!before.is_empty() && before.iter().all(|x| (x - 2.5).abs() < 1e-9), "{before:?}");
    h.run("setstyle", json!({"catalogue": "text", "id": "reference", "fields": {"height": 3.0}})).unwrap();
    let after = heights(&mut h);
    assert!(!after.is_empty() && after.iter().all(|x| (x - 3.0).abs() < 1e-9), "{after:?}");
}

/// The cable look is laid out from the `CableText` style: editing it moves the layout height.
#[test]
fn the_cable_draw_follows_the_cable_text_style() {
    let mut h = H::new(0x66_d2);
    let before = ecad_electrical::cables::words::label_height(&h.doc(), &ecad_engine::styles::draw::LOOK);
    assert!((before - 1.6).abs() < 1e-9, "{before}");
    h.run("setstyle", json!({"catalogue": "text", "id": "cable_text", "fields": {"height": 3.0}})).unwrap();
    let doc = h.doc();
    assert!((ecad_electrical::cables::words::label_height(&doc, &ecad_engine::styles::draw::LOOK) - 3.0).abs() < 1e-9);
    // A reference flag is laid out around text of that height: its rectangle is two heights tall.
    let f = ecad_electrical::cables::words::flag_shape(ecad_model::Pt::new(0.0, 0.0), 10.0, ecad_electrical::cables::words::FlagSide::Right, 3.0);
    assert!((f.hi.y - f.lo.y - 6.0).abs() < 1e-9, "{:?} {:?}", f.lo, f.hi);
}

/// Cover lines are the `Cover` style's height times their factors: editing the style moves them.
#[test]
fn cover_lines_follow_the_cover_style() {
    let mut h = H::new(0x66_d3);
    let cover: ecad_model::PageId = crate::common::id(&h.run("newpage", json!({"generated": "cover"})).unwrap(), "page");
    h.activate(cover);
    let before = drawn_heights(&mut h, cover, "No wires yet.");
    assert!(!before.is_empty(), "the wire colours block says so on a new project");
    h.run("setstyle", json!({"catalogue": "text", "id": "cover", "fields": {"height": 5.0}})).unwrap();
    let after = drawn_heights(&mut h, cover, "No wires yet.");
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(&after) {
        assert!((a * 2.0 - b).abs() < 1e-9, "the cover halved from 10 to 5 mm: {b} → {a}");
    }
}
