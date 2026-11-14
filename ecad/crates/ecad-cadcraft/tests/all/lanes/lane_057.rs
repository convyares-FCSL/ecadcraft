//! Lane 057 (wire and cable rating checks): design 057 §11, run through the commands.
//!
//! **Every table and datasheet value in this file is synthetic**, made up to exercise the
//! arithmetic; none is a value from a standard (owner, Q11). The expected numbers follow from them
//! by the formulas of design 057 §3.

use ecad_cadcraft::rating::dataset::{self, Insulation};
use ecad_cadcraft::rating::{self, parse};
use ecad_model::{CableId, ComponentId, ConductorId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// The synthetic rating table of design 057 §11 (not from any standard).
pub const SYNTHETIC: &str = "kind,a,b,c,d,value\n\
# synthetic values for tests; not from any standard\n\
capacity,E,pvc,3,1.5,20\n\
grouping,E,3,0.90\n\
resistance,5,0.75,20.0\n\
resistance,5,1.0,15.0\n\
resistance,5,1.5,10.0\n";

fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind], "limit": 2000})).unwrap_or_else(|e| panic!("checks {kind}: {e}"))["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn rating_rows(h: &mut H) -> Vec<Value> {
    let all = h.run("checks", json!({"limit": 2000})).unwrap()["checks"].as_array().cloned().unwrap_or_default();
    all.into_iter().filter(|c| c["kind"].as_str().is_some_and(|k| k.starts_with("rating-") || k == "voltdrop-over")).collect()
}

fn texts(rows: &[Value]) -> Vec<String> {
    rows.iter().map(|r| r["text"].as_str().unwrap_or_default().to_string()).collect()
}

fn place(h: &mut H, symbol: &str, at: [f64; 2], tag: &str) -> ComponentId {
    id(&h.run("place", json!({"symbol": symbol, "at": at, "tag": tag})).unwrap_or_else(|e| panic!("place {symbol}: {e}")), "component")
}

fn wire(h: &mut H, points: &[(f64, f64)]) -> ConductorId {
    id(&h.run("wire", json!({"points": pts(points)})).unwrap_or_else(|e| panic!("wire {points:?}: {e}")), "conductor")
}

fn setprop(h: &mut H, what: &str, idv: String, key: &str, value: Value) {
    h.run("setprop", json!({what: idv, "key": key, "value": value})).unwrap_or_else(|e| panic!("setprop {key}: {e}"));
}

fn ratings(h: &mut H, p: Value) -> Value {
    h.run("ratings", p.clone()).unwrap_or_else(|e| panic!("ratings {p}: {e}"))["ratings"][0].clone()
}

/// The fixture of design 057 §11 check 11, cut down: -Q01 (mcb1) → -K01 (power3, 3–4) → L2-03 →
/// -X01 (terminal) → -W01's core → -M01's terminal. -Q01 at (50, 100): pins 1 (50, 110), 2 (50, 90);
/// -K01 at (50, 50): pins 3 (50, 60), 4 (50, 40); -X01 at (50, 0); -X02 at (150, 0).
struct Fx {
    h: H,
    q01: ComponentId,
    l2: ConductorId,
    core: ConductorId,
    w01: CableId,
}

fn fixture() -> Fx {
    let mut h = H::new(57);
    let q01 = place(&mut h, "mcb1", [50.0, 100.0], "-Q01");
    let _k01 = place(&mut h, "power3", [50.0, 50.0], "-K01");
    let _x01 = place(&mut h, "terminal", [50.0, 0.0], "-X01");
    let _x02 = place(&mut h, "terminal", [150.0, 0.0], "-X02");
    setprop(&mut h, "component", q01.hex(), "rating", json!("4A C"));
    let _feed = wire(&mut h, &[(50.0, 90.0), (50.0, 60.0)]);
    let l2 = wire(&mut h, &[(50.0, 40.0), (50.0, 0.0)]);
    h.run("potential", json!({"conductor": l2.hex(), "value": "L2"})).unwrap();
    let core = wire(&mut h, &[(50.0, 0.0), (150.0, 0.0)]);
    let w01: CableId = id(&h.run("cableslash", json!({"from": [100, -5], "to": [100, 5], "tag": "-W01"})).unwrap(), "cable");
    Fx { h, q01, l2, core, w01 }
}

/// Category Power L2 rated 27 A at 30 °C, 70 °C (synthetic); project panel 40 °C.
fn rate_category(h: &mut H) {
    h.run("setstyle", json!({"catalogue": "wire", "id": "l2", "fields": {"rated_current": "27", "rated_at": "30", "max_temperature": "70"}}))
        .unwrap();
}

// ------------------------------------------------------------------------------- parsing

#[test]
fn c10_device_ratings_read_as_the_design_says() {
    for (t, want) in [("4A C", Some(4.0)), ("C16", Some(16.0)), ("13A C 30mA", Some(13.0)), ("6.3 A T", Some(6.3))] {
        assert_eq!(parse::amps(t), want, "{t}");
    }
    for t in ["16", "K", "0 A", "-4 A", "1e9 A", "NaN A"] {
        assert_eq!(parse::amps(t), None, "{t}");
    }
    let start = std::time::Instant::now();
    assert_eq!(parse::amps(&"9".repeat(10_000)), None);
    assert!(start.elapsed().as_millis() < 100);
}

#[test]
fn hostile_values_are_refused() {
    for t in ["0", "-3", "NaN", "1e400", "inf", "6301"] {
        assert_eq!(parse::current(t), None, "{t}");
    }
    assert_eq!(parse::current("3,2 A"), Some(3.2));
    assert_eq!(parse::length_m("1200 mm"), Some(1.2));
    assert_eq!(parse::cross_section("2.5 mm\u{b2}"), Some(2.5));
    assert_eq!(parse::cross_section("18 AWG"), None);
    assert_eq!(parse::ambient("95"), None);
    assert_eq!(parse::bundling("3 bunched"), Some(parse::Bundling::Circuits(3)));
    assert_eq!(parse::percent("25"), None);
    assert_eq!(ecad_cadcraft::strips::notation_size("H07RN-F 4G1.5"), Some(1.5));
    assert_eq!(ecad_cadcraft::strips::notation_size("3x2.5"), Some(2.5));
    assert_eq!(ecad_cadcraft::strips::notation_size("LiYCY 4x2x0.25"), Some(0.25));
    assert_eq!(ecad_cadcraft::strips::notation_size("H07RN-F"), None);
}

#[test]
fn c15_a_table_reads_round_trips_and_refuses_bad_rows() {
    let d = dataset::parse_strict(SYNTHETIC).unwrap();
    assert_eq!(d.capacity("E", Insulation::Pvc, 3, 1.5), Some(20.0));
    assert_eq!(d.grouping("E", 3), Some((0.9, 3)));
    assert_eq!(d.grouping("E", 2), Some((0.9, 3)));
    assert_eq!(d.grouping("E", 4), None);
    assert_eq!(d.grouping("E", 1), Some((1.0, 1)));
    assert_eq!(dataset::parse_strict(&dataset::to_csv(&d)).unwrap().rows, d.rows);
    let bad = "capacity,Q,pvc,3,1.5,20\ngrouping,E,3,x\ncapacity,E,pvc,3,1.5,20\ncapacity,E,pvc,3,1.50,21\n";
    let e = dataset::parse_strict(bad).unwrap_err();
    assert!(e.contains("line 1") && e.contains("line 2") && e.contains("line 4") && e.contains("repeats"), "{e}");
    let kept = dataset::parse(bad);
    assert_eq!((kept.rows.len(), kept.bad_count), (1, 3));
    assert!(dataset::parse_strict(&"grouping,E,3,0.9\n".repeat(dataset::MAX_ROWS + 1)).unwrap_err().contains("more than"));
    assert!(dataset::parse_strict(&"#".repeat(300 * 1024)).unwrap_err().contains("KiB"));
    for hostile in ["capacity,E,pvc,3,1.5,NaN", "capacity,E,pvc,3,1.5,1e3", "capacity,E,pvc,3,1.5,-1", "ambient,pvc,95,0.5", "resistance,3,1.5,1"] {
        assert!(dataset::parse_strict(hostile).is_err(), "{hostile}");
    }
    let odd = dataset::parse("capacity,E,pvc,3,1.5,20\ncapacity,E,pvc,3,2.5,19\n");
    assert!(odd.odd().is_some_and(|w| w.contains("2.5")));
    assert_eq!(d.odd(), None);
}

#[test]
fn the_formulas_give_the_worked_numbers() {
    // §11 check 1: round2(√(30/40)) = 0.87; 27 × 0.87 × 0.80 = 18.792 → 18.8.
    let k = rating::k_temp(70.0, 30.0, 40.0, true).unwrap();
    assert_eq!(k, 0.87);
    assert!((27.0 * k * 0.80 - 18.792).abs() < 1e-9);
    assert_eq!(rating::fmt_amps(27.0 * k * 0.80, Some(4.0)), "18.8");
    // §11 check 9: the cap.
    assert_eq!(rating::k_temp(70.0, 30.0, 25.0, true), Some(1.0));
    assert_eq!(rating::k_temp(70.0, 30.0, 70.0, true), None);
    // §11 check 4: 10.0 Ω/km, 4 A, 12 m, 400 V, ac3 → 0.9948 V, 0.249 %.
    let (v, p) = rating::volt_drop(10.0, 70.0, true, 4.0, 12.0, 400.0).unwrap();
    assert!((v - 0.9948).abs() < 1e-3 && (p - 0.2487).abs() < 1e-3, "{v} {p}");
    assert_eq!(rating::fmt_percent(p), "0.2");
    // §11 check 5: 20.0 Ω/km, 0.5 A, 60 m, 24 V dc → 1.4358 V, 5.98 %.
    let (v, p) = rating::volt_drop(20.0, 70.0, false, 0.5, 60.0, 24.0).unwrap();
    assert!((v - 1.4358).abs() < 1e-3 && (p - 5.98).abs() < 1e-2, "{v} {p}");
    assert_eq!(rating::fmt_percent(p), "6.0");
    assert_eq!(rating::fmt_amps(4.04, Some(4.0)), "4.04");
}

// ------------------------------------------------------------------------------- checks

#[test]
fn c1_c2_datasheet_pass_then_under_then_undo() {
    let mut f = fixture();
    rate_category(&mut f.h);
    f.h.run("setdefault", json!({"key": "rating.panel_temperature", "level": "project", "value": "40"})).unwrap();
    f.h.run("rating", json!({"wire": f.l2.hex(), "bundling": "0.80"})).unwrap();
    let r = ratings(&mut f.h, json!({"wire": f.l2.hex()}));
    assert_eq!(r["basis"], json!("datasheet"), "{r}");
    assert!((r["iz"].as_f64().unwrap() - 18.792).abs() < 1e-9, "{r}");
    assert_eq!(r["summary"], json!("18.8 A \u{2265} 4 A"), "{r}");
    assert_eq!(r["device"]["tag"], json!("-Q01"));
    assert_eq!(r["device"]["how"], json!("traced"));
    let panel = r["factors"].as_array().unwrap().iter().find(|f| f["name"] == "panel").cloned().unwrap();
    assert_eq!(panel["factor"], json!(0.87));
    assert!(checks_of(&mut f.h, "rating-under").is_empty());
    setprop(&mut f.h, "component", f.q01.hex(), "rating", json!("20 A"));
    let under = texts(&checks_of(&mut f.h, "rating-under"));
    assert_eq!(under.len(), 1, "{under:?}");
    assert!(under[0].ends_with("carries 18.8 A; -Q01 is 20 A"), "{under:?}");
    f.h.cmdline("u");
    assert!(checks_of(&mut f.h, "rating-under").is_empty());
}

#[test]
fn c3_c4_c7_table_basis_and_volt_drop_with_the_synthetic_table() {
    let mut f = fixture();
    let t = f.h.run("ratingtable", json!({"name": "Test (synthetic)", "source": "made up for tests", "csv": SYNTHETIC})).unwrap();
    f.h.run("setdefault", json!({"key": "rating.table", "level": "project", "value": t["id"]})).unwrap();
    setprop(&mut f.h, "cable", f.w01.hex(), "type", json!("H07RN-F 4G1.5"));
    setprop(&mut f.h, "cable", f.w01.hex(), "length", json!("12 m"));
    f.h.run("rating", json!({"cable": f.w01.hex(), "basis": "table", "method": "E", "ambient": 35, "grouped": 3, "system": "ac3"})).unwrap();
    let r = ratings(&mut f.h, json!({"cable": f.w01.hex()}));
    assert!((r["iz"].as_f64().unwrap() - 16.92).abs() < 1e-9, "{r}");
    assert_eq!(r["iz_shown"], json!("16.9 A"));
    assert_eq!(r["table"]["name"], json!("Test (synthetic)"));
    let amb = r["factors"].as_array().unwrap().iter().find(|f| f["name"] == "ambient").cloned().unwrap();
    assert_eq!(amb["factor"], json!(0.94), "{r}");
    assert!(amb["from"].as_str().unwrap().contains("computed"), "{amb}");
    assert_eq!(r["device"]["tag"], json!("-Q01"));
    assert_eq!(r["voltdrop"]["shown"], json!("0.2 %"), "{r}");
    assert_eq!(r["voltdrop"]["why"], json!("-Q01 rating"));
    assert_eq!(checks_of(&mut f.h, "rating-assumed").len(), 1);
    // Check 7: a typed load over the device.
    f.h.run("rating", json!({"cable": f.w01.hex(), "load": "5"})).unwrap();
    assert_eq!(checks_of(&mut f.h, "rating-load-over").len(), 1);
    let r = ratings(&mut f.h, json!({"cable": f.w01.hex()}));
    assert_eq!(r["voltdrop"]["shown"], json!("0.3 %"), "{r}");
    assert_eq!(r["voltdrop"]["why"], json!("typed"));
    let _ = f.core;
}

#[test]
fn c5_volt_drop_over_names_the_larger_size() {
    let mut f = fixture();
    let t = f.h.run("ratingtable", json!({"name": "Test (synthetic)", "csv": SYNTHETIC})).unwrap();
    f.h.run("setdefault", json!({"key": "rating.table", "level": "project", "value": t["id"]})).unwrap();
    let ty = f.h.run("setstyle", json!({"catalogue": "cable", "new": true, "name": "Sensor 0.75", "fields": {"cross_section": "0.75", "conductor_class": "5", "max_temperature": "70"}})).unwrap();
    f.h.run(
        "rating",
        json!({"cable": f.w01.hex(), "cable_type": ty["id"], "system": "dc", "voltage": 24, "load": "0.5", "basis": "manual", "manual": "10"}),
    )
    .unwrap();
    setprop(&mut f.h, "cable", f.w01.hex(), "length", json!("60 m"));
    let over = texts(&checks_of(&mut f.h, "voltdrop-over"));
    assert_eq!(over.len(), 1, "{over:?}");
    assert!(over[0].ends_with("drops 6.0 % at 0.5 A; limit 5 % (1 mm\u{b2} would drop 4.5 %)"), "{over:?}");
    f.h.run("rating", json!({"cable": f.w01.hex(), "vd_limit": 7})).unwrap();
    assert!(checks_of(&mut f.h, "voltdrop-over").is_empty());
}

#[test]
fn c6_c8_manual_and_refusals() {
    let mut f = fixture();
    let before = f.h.head();
    for bad in ["0", "-3", "NaN", "1e400"] {
        let e = f.h.run("rating", json!({"wire": f.l2.hex(), "basis": "manual", "manual": bad})).unwrap_err();
        assert!(e.contains("6300"), "{bad}: {e}");
    }
    assert_eq!(f.h.head(), before, "nothing written");
    f.h.run("rating", json!({"wire": f.l2.hex(), "basis": "manual", "manual": "3.5 A", "load": "3"})).unwrap();
    let under = texts(&checks_of(&mut f.h, "rating-under"));
    assert!(under.len() == 1 && under[0].ends_with("carries 3.5 A; -Q01 is 4 A"), "{under:?}");
    assert!(checks_of(&mut f.h, "rating-load-over").is_empty());
}

#[test]
fn c9_too_hot_and_the_range() {
    let mut f = fixture();
    rate_category(&mut f.h);
    f.h.run("rating", json!({"wire": f.l2.hex(), "bundling": "0.80"})).unwrap();
    f.h.run("setdefault", json!({"key": "rating.panel_temperature", "level": "project", "value": "25"})).unwrap();
    let r = ratings(&mut f.h, json!({"wire": f.l2.hex()}));
    assert!((r["iz"].as_f64().unwrap() - 21.6).abs() < 1e-9, "{r}");
    for hot in ["70", "75"] {
        f.h.run("setdefault", json!({"key": "rating.panel_temperature", "level": "project", "value": hot})).unwrap();
        assert_eq!(checks_of(&mut f.h, "rating-too-hot").len(), 1, "{hot}");
    }
    assert!(f.h.run("setdefault", json!({"key": "rating.panel_temperature", "level": "project", "value": "95"})).is_err());
}

#[test]
fn c12_protected_by() {
    let mut f = fixture();
    rate_category(&mut f.h);
    let e = f.h.run("rating", json!({"wire": f.l2.hex(), "protected_by": "-Q09"})).unwrap_err();
    assert!(e.contains("-Q09"), "{e}");
    setprop(&mut f.h, "conductor", f.l2.hex(), "protected_by", json!("-Q09"));
    assert_eq!(checks_of(&mut f.h, "rating-device-missing").len(), 1);
}

#[test]
fn c13_no_data_no_nagging() {
    let mut f = fixture();
    setprop(&mut f.h, "cable", f.w01.hex(), "type", json!("H07RN-F 4G1.5"));
    assert!(rating_rows(&mut f.h).is_empty(), "{:?}", texts(&rating_rows(&mut f.h)));
    rate_category(&mut f.h);
    assert!(!rating_rows(&mut f.h).is_empty());
}

#[test]
fn c14_no_table() {
    let mut f = fixture();
    setprop(&mut f.h, "cable", f.w01.hex(), "type", json!("H07RN-F 4G1.5"));
    f.h.run("rating", json!({"cable": f.w01.hex(), "basis": "table"})).unwrap();
    let rows = texts(&checks_of(&mut f.h, "rating-no-table"));
    assert!(rows.iter().any(|t| t.contains("No rating table chosen")), "{rows:?}");
    let t = f.h.run("ratingtable", json!({"name": "Test (synthetic)", "csv": format!("{SYNTHETIC}ambient,pvc,35,0.50\n")})).unwrap();
    f.h.run("setdefault", json!({"key": "rating.table", "level": "project", "value": t["id"]})).unwrap();
    f.h.run("rating", json!({"cable": f.w01.hex(), "method": "E", "ambient": 35, "grouped": 3})).unwrap();
    let r = ratings(&mut f.h, json!({"cable": f.w01.hex()}));
    assert!((r["iz"].as_f64().unwrap() - 9.0).abs() < 1e-9, "{r}");
    f.h.run("rating", json!({"cable": f.w01.hex(), "grouped": 4})).unwrap();
    let rows = texts(&checks_of(&mut f.h, "rating-no-table"));
    assert!(rows.iter().any(|t| t.contains("no grouping factor for 4 circuits, E")), "{rows:?}");
}

#[test]
fn c15_import_refuses_whole_and_bad_rows_in_a_file_are_counted() {
    let mut f = fixture();
    let before = f.h.head();
    let e = f.h.run("ratingtable", json!({"name": "Bad", "csv": "capacity,Q,pvc,3,1.5,20\n"})).unwrap_err();
    assert!(e.contains("line 1"), "{e}");
    assert_eq!(f.h.head(), before);
    let dir = std::env::temp_dir().join(format!("ecad-057-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("synthetic.csv");
    std::fs::write(&path, SYNTHETIC).unwrap();
    let t = f.h.run("ratingtable", json!({"import": path.to_string_lossy()})).unwrap();
    assert_eq!(t["rows"], json!(5));
    let out = dir.join("out.csv");
    f.h.run("ratingtable", json!({"export": out.to_string_lossy(), "name": "synthetic"})).unwrap();
    let back = dataset::parse_strict(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(back.rows, dataset::parse(SYNTHETIC).rows);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c18_no_values_shipped() {
    assert!(ecad_cadcraft::styles::ratingtable::CATALOGUE.builtins.is_empty());
    assert!(ecad_cadcraft::styles::cable::CATALOGUE.builtins.is_empty());
    let h = &mut H::new(1);
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::styles::ratingtable::chosen(&doc), None);
    // No source file outside `tests/` holds a table row.
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                for kind in ["capacity,", "grouping,", "resistance,", "ambient,"] {
                    let row = text.lines().any(|l| {
                        let t = l.trim_start_matches([' ', '"', '/']);
                        t.starts_with(kind) && t[kind.len()..].chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
                    });
                    assert!(!row, "{} holds a {kind} row", p.display());
                }
            }
        }
    }
}

#[test]
fn c11_two_devices_and_the_bound() {
    let mut f = fixture();
    rate_category(&mut f.h);
    // A second MCB feeding the same net as -Q01's load side.
    let q02 = place(&mut f.h, "mcb1", [80.0, 100.0], "-Q02");
    setprop(&mut f.h, "component", q02.hex(), "rating", json!("10 A"));
    setprop(&mut f.h, "component", f.q01.hex(), "rating", json!("6 A"));
    wire(&mut f.h, &[(80.0, 90.0), (50.0, 90.0)]);
    let r = ratings(&mut f.h, json!({"wire": f.l2.hex()}));
    assert_eq!(r["device"]["tag"], json!("-Q02"), "{r}");
    assert_eq!(checks_of(&mut f.h, "rating-two-devices").len(), 1);
}

#[test]
fn c16_update_map_rechecks_only_what_a_device_protects() {
    let mut f = fixture();
    rate_category(&mut f.h);
    f.h.run("rating", json!({"wire": f.l2.hex(), "bundling": "0.8"})).unwrap();
    let doc = f.h.doc();
    let mut k = rating::Kept::build(&doc);
    let before = doc.clone();
    setprop(&mut f.h, "component", f.q01.hex(), "rating", json!("20 A"));
    let after = f.h.doc();
    let ops: Vec<ecad_model::Op> = f.h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default());
    let mut touch = rating::Touch::default();
    touch.collect(&before, &ops);
    touch.collect(&after, &ops);
    let changed = k.update(&after, &touch);
    assert_eq!(changed.len(), k.protected_by(f.q01), "{changed:?}");
    assert_eq!(k.retraced, 0);
    // An unrelated op re-checks nothing.
    let mut none = rating::Touch::default();
    none.collect(&after, &[]);
    assert!(k.update(&after, &none).is_empty());
}

// ------------------------------------------------------------------------------- FP8 field cap

/// A synthetic table padded with a comment line to exactly `len` bytes.
fn padded(len: usize) -> String {
    let mut s = SYNTHETIC.to_string();
    let pad = len - s.len() - 2;
    s.push('#');
    s.push_str(&"x".repeat(pad));
    s.push('\n');
    assert_eq!(s.len(), len);
    s
}

#[test]
fn fp8_rows_at_the_cap_are_kept_and_one_over_is_refused_or_ignored() {
    use ecad_cadcraft::styles::{self, ratingtable};
    use ecad_model::style::{StyleLayer, StyleRecord, StyleRef};
    let cap = dataset::MAX_BYTES;
    assert_eq!(styles::field_cap(ratingtable::WORD, ratingtable::ROWS), cap);
    assert_eq!(styles::field_cap(ratingtable::WORD, ratingtable::SOURCE), ecad_model::style::MAX_FIELD_CHARS);
    assert_eq!(styles::field_cap("wire", "rows"), ecad_model::style::MAX_FIELD_CHARS);
    let mut f = fixture();
    // At the cap: written, read back, used.
    let t = f.h.run("ratingtable", json!({"name": "Big (synthetic)", "csv": padded(cap)})).unwrap();
    assert_eq!(t["rows"], json!(5));
    let id = StyleRef::parse(t["id"].as_str().unwrap()).unwrap();
    let doc = f.h.doc();
    assert_eq!(ratingtable::rows_text(&doc, &id).len(), cap);
    assert_eq!(ratingtable::read(&doc, &id).data.rows.len(), 5);
    assert!(checks_of(&mut f.h, "style-limits").is_empty());
    // One over: refused, nothing written.
    let head = f.h.head();
    let e = f.h.run("ratingtable", json!({"name": "Big (synthetic)", "csv": padded(cap + 1)})).unwrap_err();
    assert!(e.contains("KiB"), "{e}");
    assert_eq!(f.h.head(), head);
    // One over in a file made elsewhere: ignored with a Checks row, never a crash.
    let doc = f.h.doc();
    let before = styles::record(&doc, StyleLayer::Project, ratingtable::WORD, &id).cloned();
    let mut rec: StyleRecord = before.clone().unwrap();
    rec.fields.insert(ratingtable::ROWS.into(), padded(cap + 1));
    f.h.cfg(|p| {
        p.ledger.commit(
            ecad_model::Author::User,
            "hostile rows",
            vec![ecad_model::Op::SetStyle {
                layer: StyleLayer::Project,
                catalogue: ratingtable::WORD.into(),
                id: id.clone(),
                before,
                after: Some(rec),
            }],
        )
    })
    .unwrap();
    let doc = f.h.doc();
    assert_eq!(ratingtable::rows_text(&doc, &id), "");
    assert_eq!(checks_of(&mut f.h, "style-limits").len(), 1);
    let _ = rating::Kept::build(&doc);
}
