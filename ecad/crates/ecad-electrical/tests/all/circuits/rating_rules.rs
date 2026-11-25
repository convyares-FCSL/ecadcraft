//! The rules of the rating checks that need no project: how a device's rating, a
//! value and a rating table read, and the formulas. Every table and datasheet value here is
//! synthetic, made up to exercise the arithmetic; none is a value from a standard.

use ecad_electrical::circuits;
use ecad_electrical::devices::ratings::dataset::{self, Insulation};
use ecad_electrical::devices::ratings::parse;

/// The synthetic rating table of design 057 §11 (not from any standard).
pub(super) const SYNTHETIC: &str = "kind,a,b,c,d,value\n\
# synthetic values for tests; not from any standard\n\
capacity,E,pvc,3,1.5,20\n\
grouping,E,3,0.90\n\
resistance,5,0.75,20.0\n\
resistance,5,1.0,15.0\n\
resistance,5,1.5,10.0\n";

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
    assert_eq!(parse::bundling("3 bunched"), Some(ecad_model::objects::wire::Bundling::Circuits(3)));
    assert_eq!(parse::percent("25"), None);
    assert_eq!(ecad_electrical::terminal_strips::cores::notation_size("H07RN-F 4G1.5"), Some(1.5));
    assert_eq!(ecad_electrical::terminal_strips::cores::notation_size("3x2.5"), Some(2.5));
    assert_eq!(ecad_electrical::terminal_strips::cores::notation_size("LiYCY 4x2x0.25"), Some(0.25));
    assert_eq!(ecad_electrical::terminal_strips::cores::notation_size("H07RN-F"), None);
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
    let e = dataset::parse_strict(bad).unwrap_err().to_string();
    assert!(e.contains("line 1") && e.contains("line 2") && e.contains("line 4") && e.contains("repeats"), "{e}");
    let kept = dataset::parse(bad);
    assert_eq!((kept.rows.len(), kept.bad_count), (1, 3));
    assert!(dataset::parse_strict(&"grouping,E,3,0.9\n".repeat(dataset::MAX_ROWS + 1)).unwrap_err().to_string().contains("more than"));
    assert!(dataset::parse_strict(&"#".repeat(300 * 1024)).unwrap_err().to_string().contains("KiB"));
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
    let k = circuits::circuit::k_temp(70.0, 30.0, 40.0, true).unwrap();
    assert_eq!(k, 0.87);
    assert!((27.0 * k * 0.80 - 18.792).abs() < 1e-9);
    assert_eq!(circuits::circuit::fmt_amps(27.0 * k * 0.80, Some(4.0)), "18.8");
    // §11 check 9: the cap.
    assert_eq!(circuits::circuit::k_temp(70.0, 30.0, 25.0, true), Some(1.0));
    assert_eq!(circuits::circuit::k_temp(70.0, 30.0, 70.0, true), None);
    // §11 check 4: 10.0 Ω/km, 4 A, 12 m, 400 V, ac3 → 0.9948 V, 0.249 %.
    let (v, p) = circuits::circuit::volt_drop(10.0, 70.0, true, 4.0, 12.0, 400.0).unwrap();
    assert!((v - 0.9948).abs() < 1e-3 && (p - 0.2487).abs() < 1e-3, "{v} {p}");
    assert_eq!(circuits::circuit::fmt_percent(p), "0.2");
    // §11 check 5: 20.0 Ω/km, 0.5 A, 60 m, 24 V dc → 1.4358 V, 5.98 %.
    let (v, p) = circuits::circuit::volt_drop(20.0, 70.0, false, 0.5, 60.0, 24.0).unwrap();
    assert!((v - 1.4358).abs() < 1e-3 && (p - 5.98).abs() < 1e-2, "{v} {p}");
    assert_eq!(circuits::circuit::fmt_percent(p), "6.0");
    assert_eq!(circuits::circuit::fmt_amps(4.04, Some(4.0)), "4.04");
}
