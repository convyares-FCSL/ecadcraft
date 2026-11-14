//! Tester 057 (brief 057, design 057 §11 checks 1–18; written from the brief and design only,
//! before the implementation): the Datasheet, Table and Manual bases, volt drop and its limit,
//! rounding, the 40 °C panel and 30 °C field defaults, a missing bundling value, the protective
//! device reader and trace, Checks rows with fixes, update maps, rating tables and hostile values.
//!
//! **Every number here is made up** (owner, Q11): no value from IEC 60364-5-52, BS 7671 or any
//! other standard, and no temperature that would reproduce a standard's tabulated factor (the
//! design's own examples use 40 °C and 35 °C against a 30 °C reference, whose computed factors equal
//! published ones; this file uses a 20 °C reference, an 80 °C conductor and odd temperatures
//! instead). Every expected result is worked by hand below from design §3's formulas, not from the
//! code:
//!
//! - k_temp = round2(sqrt((θmax − θ) / (θmax − θref))), capped at 1.00 (Datasheet), uncapped
//!   (Table, computed ambient, θref 30).
//! - Iz = rating × k1 × k2 with the factors already rounded; displayed to 0.1 A half up, with an
//!   extra digit within 0.05 A of In.
//! - R_θ = R20 × (1 + 0.00393 × (θmax − 20)); mV/A/m = 2 R_θ (ac1, dc) or √3 R_θ (ac3);
//!   ΔU = mV/A/m × I × L / 1000; ΔU % = 100 ΔU / U.

use ecad_model::{ComponentId, ConductorId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};
use crate::plc_contracts::{checks_of, placed};

/// The tester's synthetic rating table (made-up numbers; not from any standard).
const T1: &str = "kind,a,b,c,d,e
# made-up values for ECAD tests; not from any standard
capacity,E,pvc,3,1.5,21.0
capacity,E,pvc,3,2.5,29.0
capacity,E,pvc,2,1.5,24.0
grouping,E,2,0.95
grouping,E,4,0.71
resistance,5,0.75,21.0
resistance,5,1.0,16.0
resistance,5,1.5,11.0
";

/// `T1` plus two made-up ambient rows for PVC.
const T1_AMBIENT: &str = "ambient,pvc,36,0.93\nambient,pvc,41,0.84\n";

const EPS: f64 = 1e-6;

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The first number found at one of `paths` (a number, or a string that starts with one).
fn num_at(v: &Value, paths: &[&[&str]]) -> Option<f64> {
    for p in paths {
        let mut x = v;
        for k in *p {
            x = &x[*k];
        }
        if let Some(n) = x.as_f64() {
            return Some(n);
        }
        if let Some(n) = x.as_str().and_then(|t| t.split_whitespace().next()).and_then(|t| t.parse::<f64>().ok()) {
            return Some(n);
        }
    }
    None
}

fn iz(v: &Value) -> f64 {
    num_at(v, &[&["iz"], &["iz", "value"], &["iz", "unrounded"], &["carries"], &["carries", "value"]]).unwrap_or_else(|| panic!("no Iz in {v}"))
}

fn device_in(v: &Value) -> Option<f64> {
    num_at(v, &[&["device", "in"], &["device", "amps"], &["device", "rating"], &["in"]])
}

fn vd_percent(v: &Value) -> Option<f64> {
    num_at(v, &[&["voltdrop", "percent"], &["voltdrop", "pct"], &["volt_drop", "percent"], &["voltdrop", "value"]])
}

fn vd_volts(v: &Value) -> Option<f64> {
    num_at(v, &[&["voltdrop", "volts"], &["voltdrop", "v"], &["volt_drop", "volts"]])
}

/// A factor of the result by name (`panel`, `bundling`, `ambient`, `grouping`).
fn factor(v: &Value, name: &str) -> Value {
    let fs = v["factors"].as_array().cloned().unwrap_or_default();
    fs.into_iter().find(|f| s(&f["name"]) == name).unwrap_or_else(|| panic!("no factor {name} in {v}"))
}

fn fval(v: &Value, name: &str) -> f64 {
    num_at(&factor(v, name), &[&["factor"], &["value"]]).unwrap_or_else(|| panic!("factor {name}: {v}"))
}

/// `ratings` for one subject.
fn rated(h: &mut H, which: Value) -> Value {
    let v = h.run("ratings", which.clone()).unwrap_or_else(|e| panic!("ratings {which}: {e}"));
    for k in ["subjects", "rows", "ratings"] {
        if let Some(first) = v[k].as_array().and_then(|a| a.first()) {
            return first.clone();
        }
    }
    v
}

fn wire_r(h: &mut H, c: ConductorId) -> Value {
    rated(h, json!({"wire": c.hex()}))
}

fn cable_r(h: &mut H, tag: &str) -> Value {
    rated(h, json!({"cable": tag}))
}

/// Every rating check row of `kind` (text of the rows, for word checks).
fn rows_text(h: &mut H, kind: &str) -> Vec<String> {
    checks_of(h, kind).iter().map(Value::to_string).collect()
}

const KINDS: [&str; 18] = [
    "rating-under",
    "rating-too-hot",
    "rating-load-over",
    "voltdrop-over",
    "rating-bundling-unset",
    "rating-device-missing",
    "rating-no-device",
    "rating-two-devices",
    "rating-device-unread",
    "rating-named-device",
    "rating-no-data",
    "rating-no-table",
    "rating-table-bad-row",
    "rating-table-odd",
    "rating-assumed",
    "rating-no-current",
    "rating-bad-value",
    "rating-unverified",
];

fn all_rating_rows(h: &mut H) -> usize {
    let v = h.run("checks", json!({"kinds": KINDS})).unwrap();
    v["checks"].as_array().map_or(0, Vec::len)
}

/// Run a write that must be refused: words, nothing recorded.
fn refused(h: &mut H, cmd: &str, p: Value) -> String {
    let head = h.head();
    let d = h.doc();
    let e = h.run(cmd, p.clone()).err().unwrap_or_else(|| panic!("{cmd} {p} should be refused"));
    assert_eq!(h.head(), head, "{cmd} {p}: nothing recorded");
    assert!(h.doc() == d, "{cmd} {p}: nothing changed");
    assert!(!e.is_empty());
    e
}

fn own_id(v: &Value) -> String {
    ["id", "style"].iter().map(|k| s(&v[*k])).find(|x| !x.is_empty()).unwrap_or_else(|| panic!("no style ID in {v}"))
}

/// The conductor with an end on `component`'s pin `label`.
fn conductor_at(h: &mut H, component: ComponentId, label: &str) -> ConductorId {
    let d = h.doc();
    let t = d
        .terminals_of(component)
        .find(|(_, t)| t.label.as_deref() == Some(label) || t.key.as_deref() == Some(label))
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("no pin {label}"));
    d.conductors.iter().find(|(_, c)| c.ends.contains(&Some(t))).map(|(i, _)| *i).unwrap_or_else(|| panic!("no conductor at {label}"))
}

/// Board 2.05's shape, made up: location +CAB1 over the panel; -Q01 (mcb1, pins 1 above, 2
/// below) at (100, 300) rated `4A C`; a wire from -Q01:2 down to lamp -H01:X1; terminal -X01 on it
/// at y 250, so the rated wire (-X01 → -H01) is traced through a terminal. Category `Synth power`:
/// 2.5 mm², rated 33 A at 20 °C, conductor 80 °C (all made up).
struct R {
    h: H,
    q01: ComponentId,
    cab: ComponentId,
    wire: ConductorId,
    category: String,
}

fn fixture(seed: u64) -> R {
    let mut h = H::new(seed);
    let cab: ComponentId = id(&h.run("locationbox", json!({"location": "+CAB1", "corners": [[20, 150], [400, 420]]})).unwrap(), "component");
    let q01 = placed(&mut h, "mcb1", 100.0, 300.0);
    h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": "4A C"})).unwrap();
    let lamp = placed(&mut h, "lamp", 100.0, 200.0);
    h.run("wire", json!({"points": pts(&[(100.0, 290.0), (100.0, 210.0)])})).unwrap();
    let x = h.run("place", json!({"symbol": "terminal", "at": [100, 250], "tag": "-X01"})).unwrap();
    assert_eq!(x["joined"], true, "{x}");
    let wire = conductor_at(&mut h, lamp, "X1");
    let category = own_id(
        &h.run(
            "setstyle",
            json!({"catalogue": "wire", "new": true, "name": "Synth power", "fields": {"cross_section": 2.5, "rated_current": 33, "rated_at": 20, "max_temperature": 80}}),
        )
        .unwrap(),
    );
    h.run("category", json!({"conductor": wire.hex(), "value": category})).unwrap();
    R { h, q01, cab, wire, category }
}

fn table(h: &mut H, extra: &str) -> String {
    let v = h.run("ratingtable", json!({"name": "Synthetic T1", "source": "made up for ECAD tests", "csv": format!("{T1}{extra}")})).unwrap();
    let t = own_id(&v);
    h.run("setdefault", json!({"key": "rating.table", "level": "project", "value": t})).unwrap();
    t
}

/// A field cable `tag` slashed across a wire from its own MCB (`mcb1` at x, rated `dev_rating`)
/// to a lamp; cable `type` text `type_text`, `length` `len`. Returns the MCB.
fn cable(h: &mut H, x: f64, tag: &str, dev_rating: &str, type_text: &str, len: &str) -> ComponentId {
    let dev = placed(h, "mcb1", x, 120.0);
    h.run("setprop", json!({"component": dev.hex(), "key": "rating", "value": dev_rating})).unwrap();
    placed(h, "lamp", x, 20.0);
    h.run("wire", json!({"points": pts(&[(x, 110.0), (x, 30.0)])})).unwrap();
    let v = h.run("cableslash", json!({"from": [x - 5.0, 70.0], "to": [x + 5.0, 70.0], "tag": tag})).unwrap();
    let c = s(&v["cable"]);
    h.run("setprop", json!({"cable": c, "key": "type", "value": type_text})).unwrap();
    h.run("setprop", json!({"cable": c, "key": "length", "value": len})).unwrap();
    dev
}

// ---------------------------------------------------------------- Datasheet (§3.1, checks 1, 2, 9)

/// +CAB1 at 44 °C: k = round2(√((80 − 44)/(80 − 20))) = round2(√0.6) = round2(0.774597) = 0.77.
/// Bundling 0.85 (set here). Iz = 33 × 0.77 × 0.85 = 21.5985 → 21.6 A ≥ 4 A: pass, no row.
/// (Unrounded factors would give 33 × 0.774597 × 0.85 = 21.727 → 21.7: the 21.6 proves the
/// factors are rounded before multiplying, §3.6.)
#[test]
#[ignore = "tester 057: awaiting build"]
fn c01_datasheet_basis_worked_example() {
    let R { mut h, cab, wire, .. } = fixture(5701);
    h.run("setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": "44"})).unwrap();
    let head = h.head();
    h.run("rating", json!({"wire": wire.hex(), "bundling": 0.85})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let r = wire_r(&mut h, wire);
    assert!((fval(&r, "panel") - 0.77).abs() < EPS, "{r}");
    assert!(s(&factor(&r, "panel")).contains("+CAB1"), "the panel factor names its location: {r}");
    assert!((fval(&r, "bundling") - 0.85).abs() < EPS, "{r}");
    assert!((iz(&r) - 21.5985).abs() < 1e-9, "{r}");
    assert!(r.to_string().contains("21.6"), "shown 21.6 A: {r}");
    assert!(!r.to_string().contains("21.7"), "factors rounded before multiplying: {r}");
    assert_eq!(device_in(&r), Some(4.0), "{r}");
    assert!(r.to_string().contains("-Q01") && r.to_string().contains("traced"), "{r}");
    assert!(rows_text(&mut h, "rating-under").is_empty());
}

/// Same wire, -Q01 `25 A`: 21.5985 < 25 → Error `rating-under` "… carries 21.6 A; -Q01 is 25 A";
/// one more error; undo removes the row.
#[test]
#[ignore = "tester 057: awaiting build"]
fn c02_underrated_wire_is_an_error_with_a_fix_and_undo_clears_it() {
    let R { mut h, q01, cab, wire, .. } = fixture(5702);
    h.run("setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": "44"})).unwrap();
    h.run("rating", json!({"wire": wire.hex(), "bundling": 0.85})).unwrap();
    h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": "25 A"})).unwrap();
    let rows = checks_of(&mut h, "rating-under");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let t = rows[0].to_string();
    assert!(t.contains("carries 21.6 A; -Q01 is 25 A"), "{t}");
    assert!(t.to_lowercase().contains("error"), "an Error: {t}");
    assert!(t.contains("Show rating") || t.contains("ShowRating") || t.to_lowercase().contains("fix"), "a fix: {t}");
    h.cmdline("u");
    assert!(checks_of(&mut h, "rating-under").is_empty(), "undo: the row goes");
}

/// Display near In (§3.6): -Q01 `21.6 A`; Iz 21.5985 < 21.6 fails and must not read `21.6 A`
/// against `21.6 A`: within 0.05 A the extra digit shows (`21.60`). -Q01 `21.55 A`: passes.
#[test]
#[ignore = "tester 057: awaiting build"]
fn c03_rounding_near_the_device_rating_shows_the_extra_digit_and_compares_unrounded() {
    let R { mut h, q01, cab, wire, .. } = fixture(5703);
    h.run("setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": "44"})).unwrap();
    h.run("rating", json!({"wire": wire.hex(), "bundling": 0.85})).unwrap();
    h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": "21.6 A"})).unwrap();
    let rows = rows_text(&mut h, "rating-under");
    assert_eq!(rows.len(), 1, "unrounded 21.5985 < 21.6: {rows:?}");
    assert!(rows[0].contains("21.60 A"), "the extra digit: {rows:?}");
    h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": "21.55 A"})).unwrap();
    assert!(rows_text(&mut h, "rating-under").is_empty(), "21.5985 ≥ 21.55");
    assert!(wire_r(&mut h, wire).to_string().contains("21.60"), "within 0.05 A: two decimals");
}

/// Defaults: no location temperature and no project value → 40 °C, `project default`;
/// k = round2(√(40/60)) = round2(0.816497) = 0.82. Bundling not set → × 1.00 and Warning
/// `rating-bundling-unset` (count), never an invented factor. Iz = 33 × 0.82 × 1.00 = 27.06.
#[test]
#[ignore = "tester 057: awaiting build"]
fn c04_panel_default_is_40_and_missing_bundling_warns_with_one() {
    let R { mut h, wire, .. } = fixture(5704);
    let r = wire_r(&mut h, wire);
    assert!((fval(&r, "panel") - 0.82).abs() < EPS, "{r}");
    let p = factor(&r, "panel").to_string();
    assert!(p.contains("40") && p.to_lowercase().contains("project default"), "{p}");
    assert!((fval(&r, "bundling") - 1.0).abs() < EPS, "× 1.00 assumed: {r}");
    assert!((iz(&r) - 27.06).abs() < 1e-9, "{r}");
    let rows = checks_of(&mut h, "rating-bundling-unset");
    assert_eq!(rows.len(), 1, "one count row: {rows:?}");
    assert!(rows[0].to_string().to_lowercase().contains("warning"), "{rows:?}");
    // The project bundling clears it, and is used.
    h.run("setdefault", json!({"key": "rating.bundling", "level": "project", "value": "0.9"})).unwrap();
    assert!(checks_of(&mut h, "rating-bundling-unset").is_empty());
    let r = wire_r(&mut h, wire);
    assert!((iz(&r) - 33.0 * 0.82 * 0.9).abs() < 1e-9, "{r}");
    // The project panel temperature replaces the default.
    h.run("setdefault", json!({"key": "rating.panel_temperature", "level": "project", "value": 50})).unwrap();
    // k = round2(√(30/60)) = round2(0.707107) = 0.71.
    assert!((fval(&wire_r(&mut h, wire), "panel") - 0.71).abs() < EPS);
}

/// Cap and limits (check 9): 14 °C: √(66/60) = 1.0488 → 1.05 → capped 1.00; Iz = 33 × 1.00 × 0.85
/// = 28.05. 80 °C and 85 °C (θ ≥ θmax): k 0, Error `rating-too-hot`. 95 °C refused (−40 to 90).
#[test]
#[ignore = "tester 057: awaiting build"]
fn c05_the_datasheet_factor_is_capped_and_too_hot_is_an_error() {
    let R { mut h, cab, wire, .. } = fixture(5705);
    h.run("rating", json!({"wire": wire.hex(), "bundling": 0.85})).unwrap();
    h.run("setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": "14"})).unwrap();
    let r = wire_r(&mut h, wire);
    assert!((fval(&r, "panel") - 1.0).abs() < EPS, "capped at 1.00: {r}");
    assert!((iz(&r) - 28.05).abs() < 1e-9, "{r}");
    for t in ["80", "85"] {
        h.run("setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": t})).unwrap();
        let rows = rows_text(&mut h, "rating-too-hot");
        assert_eq!(rows.len(), 1, "{t} °C: {rows:?}");
        assert!(rows[0].contains("+CAB1") && rows[0].contains("80 °C"), "{rows:?}");
    }
    let e = refused(&mut h, "setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": "95"}));
    assert!(e.contains("90"), "names the range: {e}");
    // A style with θref ≥ θmax is refused when written.
    let e = h.run("setstyle", json!({"catalogue": "wire", "new": true, "name": "Bad", "fields": {"rated_at": 80, "max_temperature": 80}}));
    assert!(e.is_err(), "θref ≥ θmax refused: {e:?}");
}

/// A wire between two locations takes the hotter one (§4.4).
#[test]
#[ignore = "tester 057: awaiting build"]
fn c06_a_wire_across_two_locations_takes_the_hotter() {
    let R { mut h, cab, wire, .. } = fixture(5706);
    h.run("setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": "44"})).unwrap();
    // +CAB2 over the lamp end only (the lamp's X1 is at y 210).
    let cab2: ComponentId = id(&h.run("locationbox", json!({"location": "+CAB2", "corners": [[60, 160], [140, 230]]})).unwrap(), "component");
    h.run("setprop", json!({"component": cab2.hex(), "key": "rating.temperature", "value": "56"})).unwrap();
    // k = round2(√(24/60)) = round2(0.632456) = 0.63.
    let r = wire_r(&mut h, wire);
    assert!((fval(&r, "panel") - 0.63).abs() < EPS, "the hotter end: {r}");
}

// ---------------------------------------------------------------- Table (§3.2, checks 3, 14)

/// -W01 `H07RN-F 4G1.5` (no cable type: 1.5 mm², Cu, 70 °C, class 5 assumed), method E, ambient
/// 37 °C, 3 circuits, ac3 (3 loaded). T1 has no ambient rows: computed, θref 30, uncapped:
/// round2(√(33/40)) = round2(0.908295) = 0.91. Grouping 3: next higher row, 4 → 0.71.
/// Iz = 21.0 × 0.91 × 0.71 = 13.5681 → 13.6 A ≥ 4 A.
#[test]
#[ignore = "tester 057: awaiting build"]
fn t01_table_basis_worked_example_with_computed_ambient() {
    let R { mut h, .. } = fixture(5707);
    table(&mut h, "");
    cable(&mut h, 250.0, "-W01", "4 A", "H07RN-F 4G1.5", "12 m");
    let head = h.head();
    h.run("rating", json!({"cable": "-W01", "basis": "table", "method": "e", "ambient": 37, "grouped": 3})).unwrap();
    assert_eq!(h.head(), head + 1);
    let r = cable_r(&mut h, "-W01");
    assert!((fval(&r, "ambient") - 0.91).abs() < EPS, "{r}");
    assert!(factor(&r, "ambient").to_string().to_lowercase().contains("computed"), "{r}");
    assert!((fval(&r, "grouping") - 0.71).abs() < EPS, "the next higher count: {r}");
    assert!((iz(&r) - 13.5681).abs() < 1e-9, "{r}");
    assert!(r.to_string().contains("13.6"), "{r}");
    let t = r.to_string();
    assert!(t.contains("Synthetic T1") && t.contains("made up for ECAD tests"), "names the table and its source: {r}");
    assert_eq!(checks_of(&mut h, "rating-assumed").len(), 1, "no cable type: assumed, counted");
    // With made-up ambient rows 36 → 0.93 and 41 → 0.84: 37 °C takes the next higher row, 41.
    // Iz = 21.0 × 0.84 × 0.71 = 12.5244 → 12.5 A.
    table(&mut h, T1_AMBIENT);
    let r = cable_r(&mut h, "-W01");
    assert!((fval(&r, "ambient") - 0.84).abs() < EPS, "{r}");
    assert!((iz(&r) - 12.5244).abs() < 1e-9, "{r}");
    // Above the last row: no result, rating-no-table.
    h.run("rating", json!({"cable": "-W01", "ambient": 45})).unwrap();
    assert_eq!(rows_text(&mut h, "rating-no-table").len(), 1);
    // One circuit: 1.00 without a row. Iz = 21.0 × 0.93 × 1.00 at 36 °C = 19.53.
    h.run("rating", json!({"cable": "-W01", "ambient": 36, "grouped": 1})).unwrap();
    assert!((iz(&cable_r(&mut h, "-W01")) - 19.53).abs() < 1e-9);
}

/// Field default 30 °C (Q9): no ambient set → 30 °C `project default`; computed k = 1.00.
/// Grouping 2 → 0.95. Iz = 21.0 × 1.00 × 0.95 = 19.95 → 20.0 A (half up).
#[test]
#[ignore = "tester 057: awaiting build"]
fn t02_field_ambient_default_is_30() {
    let R { mut h, .. } = fixture(5708);
    table(&mut h, "");
    cable(&mut h, 250.0, "-W01", "4 A", "H07RN-F 4G1.5", "12 m");
    h.run("rating", json!({"cable": "-W01", "basis": "table", "method": "E", "grouped": 2})).unwrap();
    let r = cable_r(&mut h, "-W01");
    let a = factor(&r, "ambient").to_string();
    assert!(a.contains("30") && a.to_lowercase().contains("project default"), "{a}");
    assert!((fval(&r, "ambient") - 1.0).abs() < EPS);
    assert!((iz(&r) - 19.95).abs() < 1e-9, "{r}");
    assert!(r.to_string().contains("20.0"), "19.95 half up: {r}");
}

/// Check 14: no table chosen → "No rating table chosen" with Choose a table; 2.5 mm² ac1 (2
/// loaded) has no row → names the missing key; 5 circuits → no grouping factor; class 2 → no
/// resistance, no volt drop.
#[test]
#[ignore = "tester 057: awaiting build"]
fn t03_missing_table_data_says_what_is_missing() {
    let R { mut h, .. } = fixture(5709);
    cable(&mut h, 250.0, "-W01", "4 A", "H07RN-F 4G1.5", "12 m");
    h.run("rating", json!({"cable": "-W01", "basis": "table", "method": "E", "grouped": 2})).unwrap();
    let rows = rows_text(&mut h, "rating-no-table");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].contains("No rating table chosen") && rows[0].contains("Choose a table"), "{rows:?}");
    table(&mut h, "");
    h.run("setprop", json!({"cable": "-W01", "key": "type", "value": "H07RN-F 3G2.5"})).unwrap();
    h.run("rating", json!({"cable": "-W01", "system": "ac1", "voltage": 230})).unwrap();
    let rows = rows_text(&mut h, "rating-no-table");
    assert!(rows.iter().any(|r| r.contains("no capacity for 2.5 mm², E, pvc, 2 loaded")), "{rows:?}");
    h.run("setprop", json!({"cable": "-W01", "key": "type", "value": "H07RN-F 4G1.5"})).unwrap();
    h.run("rating", json!({"cable": "-W01", "system": "ac3", "voltage": 400, "grouped": 5})).unwrap();
    let rows = rows_text(&mut h, "rating-no-table");
    assert!(rows.iter().any(|r| r.contains("no grouping factor for 5 circuits, E")), "{rows:?}");
    // A cable type with class 2: no resistance row → no volt drop.
    let ct = own_id(
        &h.run(
            "setstyle",
            json!({"catalogue": "cable", "new": true, "name": "Synth class 2", "fields": {"cross_section": 1.5, "conductor_class": "2"}}),
        )
        .unwrap(),
    );
    h.run("rating", json!({"cable": "-W01", "grouped": 2, "cable_type": ct})).unwrap();
    let rows = rows_text(&mut h, "rating-no-table");
    assert!(rows.iter().any(|r| r.contains("no resistance for 1.5 mm², class 2")), "{rows:?}");
    assert_eq!(vd_percent(&cable_r(&mut h, "-W01")), None);
}

/// The Table basis on a panel wire uses B1 only (§2.1): T1 has no B1 rows → no result.
#[test]
#[ignore = "tester 057: awaiting build"]
fn t04_a_panel_wire_on_the_table_basis_uses_b1_only() {
    let R { mut h, wire, .. } = fixture(5710);
    table(&mut h, "");
    h.run("rating", json!({"wire": wire.hex(), "basis": "table"})).unwrap();
    let rows = rows_text(&mut h, "rating-no-table");
    assert!(rows.iter().any(|r| r.contains("B1")), "never a field method on a panel wire: {rows:?}");
    // With a made-up B1 row: 2.5 mm², 2 loaded, 30.0 A; panel 40 °C default on pvc (computed with
    // θref 30 for a table): the wire's category is 80 °C, not 70/90: looked up as insulation `80`.
    table(&mut h, "capacity,B1,80,2,2.5,30.0\n");
    h.run("setdefault", json!({"key": "rating.system", "level": "project", "value": "dc"})).unwrap();
    let r = wire_r(&mut h, wire);
    // k_panel = round2(√((80 − 40)/(80 − 30))) = round2(√0.8) = round2(0.894427) = 0.89; bundling
    // unset 1.00. Iz = 30.0 × 0.89 × 1.00 = 26.7.
    assert!((iz(&r) - 26.7).abs() < 1e-9, "{r}");
}

// ---------------------------------------------------------------- Manual (§3.3, checks 6, 8)

#[test]
#[ignore = "tester 057: awaiting build"]
fn m01_manual_basis_and_its_hostile_values() {
    let R { mut h, q01, wire, .. } = fixture(5711);
    h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": "16 A"})).unwrap();
    h.run("rating", json!({"wire": wire.hex(), "basis": "manual", "manual": "10 A"})).unwrap();
    let r = wire_r(&mut h, wire);
    assert!((iz(&r) - 10.0).abs() < EPS);
    assert!(r.to_string().contains("set by hand"), "{r}");
    let rows = rows_text(&mut h, "rating-under");
    assert!(rows.len() == 1 && rows[0].contains("carries 10") && rows[0].contains("-Q01 is 16 A"), "{rows:?}");
    for bad in ["0", "-3", "NaN", "1e400", "inf", "6301", "6300.0001", "10 kA", "", "ten", "3.2.1 A"] {
        let e = refused(&mut h, "rating", json!({"wire": wire.hex(), "manual": bad}));
        assert!(e.contains("6300") || e.contains("manual"), "{bad}: names the key or range: {e}");
    }
    h.run("rating", json!({"wire": wire.hex(), "manual": "3,2 A"})).expect("a decimal comma is read");
    assert!((iz(&wire_r(&mut h, wire)) - 3.2).abs() < EPS);
    h.run("rating", json!({"wire": wire.hex(), "manual": "6300"})).expect("the upper bound is allowed");
    // null clears; absent keys are untouched.
    h.run("rating", json!({"wire": wire.hex(), "manual": null})).unwrap();
    let d = h.doc();
    assert!(d.conductors.get(&wire).is_some_and(|c| !c.properties.contains_key("rating.manual")));
    assert!(d.conductors.get(&wire).is_some_and(|c| c.properties.get("rating.basis").map(String::as_str) == Some("manual")));
}

/// Check 8: Manual Iz 3.5, -Q01 4 A, load 3 → `rating-under` only (3 ≤ 4: no load-over).
/// Then load 5 > 4 → `rating-load-over` too.
#[test]
#[ignore = "tester 057: awaiting build"]
fn m02_ib_in_iz_at_once() {
    let R { mut h, wire, .. } = fixture(5712);
    h.run("rating", json!({"wire": wire.hex(), "basis": "manual", "manual": 3.5, "load": "3 A"})).unwrap();
    assert_eq!(checks_of(&mut h, "rating-under").len(), 1);
    assert!(checks_of(&mut h, "rating-load-over").is_empty());
    h.run("rating", json!({"wire": wire.hex(), "load": 5})).unwrap();
    let rows = rows_text(&mut h, "rating-load-over");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].contains("5 A") && rows[0].contains("-Q01") && rows[0].contains("4 A"), "{rows:?}");
}

// ---------------------------------------------------------------- volt drop (§3.5, checks 4, 5, 7)

/// -W01 on T1 as t01, L 12 m, ac3, 400 V, θmax 70, no load → I = In = 4 A.
/// R70 = 11.0 × (1 + 0.00393 × 50) = 11.0 × 1.1965 = 13.1615; mV/A/m = √3 × 13.1615 = 22.79640;
/// ΔU = 22.79640 × 4 × 12 / 1000 = 1.094227 V; % = 0.273557 → 0.3 %, `at 4 A, -Q01 rating`.
/// Load 3 A (typed): ΔU = 0.820670 V = 0.205168 % → 0.2 %, `at 3 A, typed`.
/// 2 m at 1 A: ΔU = 0.0455928 V = 0.011398 % → shown to 0.01 %: 0.01 %.
#[test]
#[ignore = "tester 057: awaiting build"]
fn v01_volt_drop_worked_example_and_the_current_it_uses() {
    let R { mut h, .. } = fixture(5713);
    table(&mut h, "");
    cable(&mut h, 250.0, "-W01", "4 A", "H07RN-F 4G1.5", "12 m");
    h.run("rating", json!({"cable": "-W01", "basis": "table", "method": "E", "ambient": 37, "grouped": 3})).unwrap();
    let mv = 3f64.sqrt() * 11.0 * (1.0 + 0.00393 * 50.0);
    let r = cable_r(&mut h, "-W01");
    let want = mv * 4.0 * 12.0 / 1000.0;
    assert!((vd_volts(&r).unwrap_or(f64::NAN) - want).abs() < 1e-6, "{want} V: {r}");
    assert!((vd_percent(&r).unwrap_or(f64::NAN) - 100.0 * want / 400.0).abs() < 1e-6, "{r}");
    let t = r.to_string();
    assert!(t.contains("0.3 %") && t.contains("at 4 A") && t.contains("-Q01"), "{r}");
    h.run("rating", json!({"cable": "-W01", "load": "3 A"})).unwrap();
    let t = cable_r(&mut h, "-W01").to_string();
    assert!(t.contains("0.2 %") && t.contains("at 3 A") && t.contains("typed"), "{t}");
    h.run("setprop", json!({"cable": "-W01", "key": "length", "value": "2 m"})).unwrap();
    h.run("rating", json!({"cable": "-W01", "load": 1})).unwrap();
    assert!(cable_r(&mut h, "-W01").to_string().contains("0.01 %"), "below 0.1 %: two decimals");
    assert!(checks_of(&mut h, "voltdrop-over").is_empty());
    // Length in millimetres: 12 000 mm = 12 m.
    h.run("setprop", json!({"cable": "-W01", "key": "length", "value": "12000 mm"})).unwrap();
    h.run("rating", json!({"cable": "-W01", "load": null})).unwrap();
    assert!((vd_volts(&cable_r(&mut h, "-W01")).unwrap_or(f64::NAN) - want).abs() < 1e-6);
}

/// -W12, made-up cable type 0.75 mm² class 5, 70 °C; dc, 24 V, 60 m, load 0.5 A, no device.
/// R70 = 21.0 × 1.1965 = 25.1265; × 2 = 50.253; ΔU = 50.253 × 0.5 × 60 / 1000 = 1.50759 V
/// = 6.281625 % → 6.3 % > 5 %: Warning. Next size 1.0 mm²: 16.0 × 1.1965 × 2 = 38.288;
/// × 0.5 × 60 / 1000 = 1.14864 V = 4.786 % → 4.8 %. vd_limit 7 → the row goes.
#[test]
#[ignore = "tester 057: awaiting build"]
fn v02_volt_drop_over_the_limit_names_the_next_size() {
    let R { mut h, .. } = fixture(5714);
    table(&mut h, "");
    let ct = own_id(
        &h.run(
            "setstyle",
            json!({"catalogue": "cable", "new": true, "name": "Synth sensor 0.75", "fields": {"cross_section": 0.75, "conductor_class": "5", "max_temperature": 70}}),
        )
        .unwrap(),
    );
    placed(&mut h, "lamp", 300.0, 20.0);
    h.run("wire", json!({"points": pts(&[(300.0, 110.0), (300.0, 30.0)])})).unwrap();
    let v = h.run("cableslash", json!({"from": [295, 70], "to": [305, 70], "tag": "-W12"})).unwrap();
    let c = s(&v["cable"]);
    h.run("setprop", json!({"cable": c, "key": "length", "value": "60 m"})).unwrap();
    h.run("rating", json!({"cable": "-W12", "cable_type": ct, "system": "dc", "voltage": 24, "load": 0.5, "basis": "manual", "manual": 10})).unwrap();
    let r = cable_r(&mut h, "-W12");
    assert!((vd_volts(&r).unwrap_or(f64::NAN) - 1.50759).abs() < 1e-5, "{r}");
    let rows = rows_text(&mut h, "voltdrop-over");
    assert_eq!(rows.len(), 1, "Manual still shows the volt drop: {rows:?}");
    assert!(rows[0].contains("-W12 drops 6.3 % at 0.5 A; limit 5 % (1.0 mm² would drop 4.8 %)"), "{rows:?}");
    assert!(rows[0].contains("this cable only") || rows[0].to_lowercase().contains("warning"), "{rows:?}");
    h.run("rating", json!({"cable": "-W12", "vd_limit": 7})).unwrap();
    assert!(checks_of(&mut h, "voltdrop-over").is_empty());
    for bad in [json!(0.4), json!(21), json!("x"), json!("NaN")] {
        refused(&mut h, "rating", json!({"cable": "-W12", "vd_limit": bad}));
    }
    // Above 16 mm²: not computed.
    let big = own_id(&h.run("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth 25", "fields": {"cross_section": 25}})).unwrap());
    h.run("rating", json!({"cable": "-W12", "cable_type": big})).unwrap();
    let r = cable_r(&mut h, "-W12");
    assert!(r.to_string().contains("volt drop needs reactance; not computed"), "{r}");
    assert_eq!(vd_percent(&r), None);
}

/// No load and no device: no volt drop, Info `rating-no-current` (count).
#[test]
#[ignore = "tester 057: awaiting build"]
fn v03_no_current_no_volt_drop() {
    let R { mut h, .. } = fixture(5715);
    table(&mut h, "");
    placed(&mut h, "lamp", 300.0, 20.0);
    h.run("wire", json!({"points": pts(&[(300.0, 110.0), (300.0, 30.0)])})).unwrap();
    let v = h.run("cableslash", json!({"from": [295, 70], "to": [305, 70], "tag": "-W20"})).unwrap();
    let c = s(&v["cable"]);
    h.run("setprop", json!({"cable": c, "key": "type", "value": "4G1.5"})).unwrap();
    h.run("setprop", json!({"cable": c, "key": "length", "value": "10 m"})).unwrap();
    assert_eq!(checks_of(&mut h, "rating-no-current").len(), 1);
    assert_eq!(vd_percent(&cable_r(&mut h, "-W20")), None);
}

// ---------------------------------------------------------------- device rating reader (§4.1, check 10)

#[test]
#[ignore = "tester 057: awaiting build"]
fn d01_device_rating_text_is_read_or_reported_unread() {
    let R { mut h, q01, wire, .. } = fixture(5716);
    h.run("rating", json!({"wire": wire.hex(), "bundling": 0.85})).unwrap();
    let long = "9".repeat(10_000);
    let cases: Vec<(&str, Option<f64>)> = vec![
        ("4A C", Some(4.0)),
        ("C16", Some(16.0)),
        ("B6", Some(6.0)),
        ("13A C 30mA", Some(13.0)),
        ("6.3 A T", Some(6.3)),
        ("2 a", Some(2.0)),
        ("6300 A", Some(6300.0)),
        ("16", None),
        ("K", None),
        ("30mA", None),
        ("10kA", None),
        ("0 A", None),
        ("-4 A", None),
        ("1e9 A", None),
        ("NaN A", None),
        ("6301 A", None),
        (long.as_str(), None),
    ];
    for (text, want) in cases {
        h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": text})).unwrap();
        let r = wire_r(&mut h, wire);
        let unread = checks_of(&mut h, "rating-device-unread");
        match want {
            Some(a) => {
                assert_eq!(device_in(&r), Some(a), "{text:.20}: {r}");
                assert!(unread.is_empty(), "{text:.20}: {unread:?}");
            }
            None => {
                assert_eq!(unread.len(), 1, "{text:.20}: once per device: {unread:?}");
                assert!(device_in(&r).is_none(), "{text:.20}: {r}");
            }
        }
    }
    h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": "K"})).unwrap();
    let t = rows_text(&mut h, "rating-device-unread");
    assert!(t[0].contains("-Q01's rating 'K' has no current; write it as 16 A"), "{t:?}");
}

/// Fuse bank position (check 12): `protected_by "-F01:3"` with `rating.3 "2 A"` → In 2 A.
#[test]
#[ignore = "tester 057: awaiting build"]
fn d02_protected_by_a_fuse_of_a_bank_and_by_hand() {
    let R { mut h, wire, .. } = fixture(5717);
    let f01 = placed(&mut h, "fusebank4", 300.0, 300.0);
    h.run("setprop", json!({"component": f01.hex(), "key": "tag", "value": "-F01"})).ok();
    let tag = crate::plc_contracts::tag(&mut h, f01);
    h.run("setprop", json!({"component": f01.hex(), "key": "rating.3", "value": "2 A"})).unwrap();
    let head = h.head();
    h.run("rating", json!({"wire": wire.hex(), "protected_by": format!("{tag}:3")})).unwrap();
    assert_eq!(h.head(), head + 1);
    let r = wire_r(&mut h, wire);
    assert_eq!(device_in(&r), Some(2.0), "{r}");
    assert!(r.to_string().contains("named"), "found by hand: {r}");
    // Naming nothing is refused by `rating`; set by setprop (a file made elsewhere): a Warning.
    let e = refused(&mut h, "rating", json!({"wire": wire.hex(), "protected_by": "-Q09"}));
    assert!(e.contains("-Q09"), "{e}");
    h.run("setprop", json!({"conductor": wire.hex(), "key": "protected_by", "value": "-Q09"})).unwrap();
    let rows = rows_text(&mut h, "rating-device-missing");
    assert!(rows.len() == 1 && rows[0].contains("names -Q09 as its protection; there is no -Q09"), "{rows:?}");
    assert!(rows[0].contains("Clear Protected by") || rows[0].contains("ClearProtectedBy"), "the fix: {rows:?}");
    // A non-protective device named by hand: used, with an Info row.
    let k01 = placed(&mut h, "coil", 340.0, 300.0);
    h.run("setprop", json!({"component": k01.hex(), "key": "rating", "value": "3 A"})).unwrap();
    let k = crate::plc_contracts::tag(&mut h, k01);
    h.run("rating", json!({"wire": wire.hex(), "protected_by": k})).unwrap();
    assert_eq!(device_in(&wire_r(&mut h, wire)), Some(3.0));
    assert_eq!(checks_of(&mut h, "rating-named-device").len(), 1);
}

// ---------------------------------------------------------------- trace (§4.2, check 11)

/// Line side, RCD crossed, two devices at one depth.
#[test]
#[ignore = "tester 057: awaiting build"]
fn tr01_trace_line_side_rcd_and_two_devices() {
    let R { mut h, category, .. } = fixture(5718);
    // Line side of -Q01: -Q00 above feeds it; a wire -Q00:2 → -Q01:1 is protected by -Q00.
    let q00 = placed(&mut h, "mcb1", 100.0, 370.0);
    h.run("setprop", json!({"component": q00.hex(), "key": "rating", "value": "10 A"})).unwrap();
    let w = id::<ConductorId>(&h.run("wire", json!({"points": pts(&[(100.0, 360.0), (100.0, 310.0)])})).unwrap(), "conductor");
    h.run("category", json!({"conductor": w.hex(), "value": category})).unwrap();
    let r = wire_r(&mut h, w);
    assert_eq!(device_in(&r), Some(10.0), "line side of -Q01: its feeder, not -Q01: {r}");
    // RCD -Q10 between -Q02 (6 A) and a lamp: crossed by pole.
    let q02 = placed(&mut h, "mcb1", 200.0, 340.0);
    h.run("setprop", json!({"component": q02.hex(), "key": "rating", "value": "6 A"})).unwrap();
    placed(&mut h, "rcd2", 200.0, 290.0);
    let lamp = placed(&mut h, "lamp", 200.0, 220.0);
    h.run("wire", json!({"points": pts(&[(200.0, 330.0), (200.0, 300.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(200.0, 280.0), (200.0, 230.0)])})).unwrap();
    let w2 = conductor_at(&mut h, lamp, "X1");
    h.run("category", json!({"conductor": w2.hex(), "value": category})).unwrap();
    let r = wire_r(&mut h, w2);
    assert_eq!(device_in(&r), Some(6.0), "the RCD is crossed: {r}");
    // Two MCBs on one net: -Q03 6 A and -Q04 10 A, both load sides joined.
    let q03 = placed(&mut h, "mcb1", 300.0, 350.0);
    let q04 = placed(&mut h, "mcb1", 340.0, 350.0);
    h.run("setprop", json!({"component": q03.hex(), "key": "rating", "value": "6 A"})).unwrap();
    h.run("setprop", json!({"component": q04.hex(), "key": "rating", "value": "10 A"})).unwrap();
    let w3 = id::<ConductorId>(
        &h.run("wire", json!({"points": pts(&[(300.0, 340.0), (300.0, 300.0), (340.0, 300.0), (340.0, 340.0)])})).unwrap(),
        "conductor",
    );
    h.run("category", json!({"conductor": w3.hex(), "value": category})).unwrap();
    let r = wire_r(&mut h, w3);
    assert_eq!(device_in(&r), Some(10.0), "the largest In: {r}");
    let rows = rows_text(&mut h, "rating-two-devices");
    assert!(rows.iter().any(|x| x.contains("-Q03") && x.contains("-Q04")), "{rows:?}");
}

/// A chain of 70 terminals: the trace stops after 64 nets, says so, never hangs.
#[test]
#[ignore = "tester 057: awaiting build"]
fn tr02_a_long_chain_stops_after_64_nets() {
    let mut h = H::new(5719);
    let q = placed(&mut h, "mcb1", 10.0, 400.0);
    h.run("setprop", json!({"component": q.hex(), "key": "rating", "value": "4 A"})).unwrap();
    let lamp = placed(&mut h, "lamp", 10.0, 20.0);
    h.run("wire", json!({"points": pts(&[(10.0, 390.0), (10.0, 30.0)])})).unwrap();
    for i in 0..70 {
        let y = 40.0 + 5.0 * f64::from(i);
        h.run("place", json!({"symbol": "terminal", "at": [10.0, y], "tag": "-X50"})).unwrap();
    }
    let w = conductor_at(&mut h, lamp, "X1");
    h.run("rating", json!({"wire": w.hex(), "basis": "manual", "manual": 10})).unwrap();
    let rows = rows_text(&mut h, "rating-no-device");
    assert!(rows.iter().any(|r| r.contains("trace stopped after 64 nets; set Protected by")), "{rows:?}");
}

// ---------------------------------------------------------------- no nagging (check 13, AC 2)

#[test]
#[ignore = "tester 057: awaiting build"]
fn n01_no_rating_data_no_rows_then_rows_only_for_rated_wires() {
    let mut h = H::new(5720);
    let q = placed(&mut h, "mcb1", 100.0, 300.0);
    h.run("setprop", json!({"component": q.hex(), "key": "rating", "value": "4 A"})).unwrap();
    let l1 = placed(&mut h, "lamp", 100.0, 200.0);
    h.run("wire", json!({"points": pts(&[(100.0, 290.0), (100.0, 210.0)])})).unwrap();
    let q2 = placed(&mut h, "mcb1", 200.0, 300.0);
    h.run("setprop", json!({"component": q2.hex(), "key": "rating", "value": "4 A"})).unwrap();
    placed(&mut h, "lamp", 200.0, 200.0);
    h.run("wire", json!({"points": pts(&[(200.0, 290.0), (200.0, 210.0)])})).unwrap();
    assert_eq!(all_rating_rows(&mut h), 0, "no rating data anywhere: no rows (no nagging)");
    let cat = own_id(&h.run("setstyle", json!({"catalogue": "wire", "new": true, "name": "Synth rated", "fields": {"rated_current": 2}})).unwrap());
    let w = conductor_at(&mut h, l1, "X1");
    h.run("category", json!({"conductor": w.hex(), "value": cat})).unwrap();
    // Rated 2 A × k(40 °C, default 30/70) — whatever the factor, ≤ 2 A < 4 A: under.
    assert_eq!(checks_of(&mut h, "rating-under").len(), 1);
    let nd = rows_text(&mut h, "rating-no-data");
    assert_eq!(nd.len(), 1, "one count row for the other protected wire: {nd:?}");
    assert!(nd[0].contains('1'), "{nd:?}");
}

// ---------------------------------------------------------------- rating tables (§5, check 15, 18)

#[test]
#[ignore = "tester 057: awaiting build"]
fn rt01_import_refuses_the_whole_file_listing_bad_lines() {
    let mut h = H::new(5721);
    let mut many = String::from("kind,a,b,c,d,e\n");
    for i in 0..4001 {
        many.push_str(&format!("resistance,5,{}.5,1.0\n", i % 900 + 1));
    }
    let huge = format!("# {}\n", "x".repeat(300 * 1024));
    let cases = [
        ("capacity,Q,pvc,3,1.5,21.0\n", "line"),
        ("grouping,E,3,x\n", "line"),
        ("capacity,E,pvc,3,1.5,21.0\ncapacity,E,pvc,3,1.5,22.0\n", "repeats capacity E pvc 3 1.5"),
        ("capacity,E,pvc,4,1.5,21.0\n", "line"),
        ("capacity,E,pvc,3,1.5,0\n", "line"),
        ("capacity,E,pvc,3,1.5,6301\n", "line"),
        ("grouping,E,101,0.5\n", "line"),
        ("grouping,E,3,2.01\n", "line"),
        ("ambient,pvc,91,0.5\n", "line"),
        ("ambient,glass,40,0.5\n", "line"),
        ("resistance,3,1.5,10\n", "line"),
        ("resistance,5,1.5,NaN\n", "line"),
        ("capacity,E,pvc,3,1.5,1e400\n", "line"),
        ("mystery,1,2,3\n", "line"),
        (many.as_str(), ""),
        (huge.as_str(), ""),
    ];
    for (csv, words) in cases {
        let e = refused(
            &mut h,
            "ratingtable",
            json!({"name": "Bad", "source": "made up", "csv": format!("kind,a,b,c,d,e\ncapacity,E,pvc,3,2.5,29.0\n{csv}")}),
        );
        assert!(e.contains(words), "{csv:.40}: {e}");
    }
    // At most the first 10 bad lines are listed.
    let bad: String = (0..20).map(|i| format!("capacity,Q,pvc,3,{}.5,1\n", i + 1)).collect();
    let e = refused(&mut h, "ratingtable", json!({"name": "Bad", "source": "x", "csv": bad}));
    assert!(e.matches("line").count() <= 11, "{e}");
    // Case-insensitive methods, comments, header and blank lines are fine.
    h.run("ratingtable", json!({"name": "Ok", "source": "made up", "csv": "kind,x\n# c\n\ncapacity,e,PVC,3,1.5,21.0\n"})).unwrap();
}

#[test]
#[ignore = "tester 057: awaiting build"]
fn rt02_odd_tables_and_bad_rows_in_a_file_are_info_rows() {
    let mut h = H::new(5722);
    let v =
        h.run("ratingtable", json!({"name": "Odd", "source": "made up", "csv": "capacity,E,pvc,3,1.5,21.0\ncapacity,E,pvc,3,2.5,18.0\n"})).unwrap();
    let t = own_id(&v);
    h.run("setdefault", json!({"key": "rating.table", "level": "project", "value": t.clone()})).unwrap();
    let rows = rows_text(&mut h, "rating-table-odd");
    assert!(rows.iter().any(|r| r.contains("capacity at 2.5 mm² is below 1.5 mm² (E, pvc, 3)")), "{rows:?}");
    // Bad rows in a file made elsewhere: written straight into the style field.
    let r = h.run(
        "setstyle",
        json!({"catalogue": "ratingtable", "id": t, "fields": {"rows": "capacity,E,pvc,3,1.5,21.0\ngrouping,E,3,x\ncapacity,Q,pvc,3,1.5,1\nresistance,5,1.5,11.0\n"}}),
    );
    if r.is_ok() {
        let rows = rows_text(&mut h, "rating-table-bad-row");
        assert!(rows.len() == 1 && rows[0].contains("2 rows not read") && rows[0].contains("line 2"), "{rows:?}");
    }
}

#[test]
#[ignore = "tester 057: awaiting build"]
fn rt03_export_then_import_gives_the_same_rows_and_import_writes_one_style() {
    let mut h = H::new(5723);
    let dir = std::env::temp_dir().join(format!("t057 Tabellen ä {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("synthetic in.csv");
    std::fs::write(&a, format!("{T1}{T1_AMBIENT}").replace('\n', "\r\n")).unwrap();
    let head = h.head();
    h.run("ratingtable", json!({"name": "Synthetic T1", "source": "made up", "import": a.display().to_string()})).unwrap();
    assert_eq!(h.head(), head + 1, "one SetStyle");
    let b = dir.join("synthetic out.csv");
    h.run("ratingtable", json!({"name": "Synthetic T1", "export": b.display().to_string()})).unwrap();
    let out = std::fs::read_to_string(&b).unwrap();
    h.run("ratingtable", json!({"name": "Synthetic T2", "source": "made up", "import": b.display().to_string()})).unwrap();
    let rows = |h: &mut H, n: &str| -> Value {
        let v = h.run("styles", json!({"catalogue": "ratingtable"})).unwrap();
        let list = v.as_array().cloned().or_else(|| v["styles"].as_array().cloned()).unwrap_or_default();
        list.into_iter().find(|r| s(&r["name"]) == n).map(|r| r["fields"]["rows"].clone()).unwrap_or(Value::Null)
    };
    assert_eq!(rows(&mut h, "Synthetic T1"), rows(&mut h, "Synthetic T2"), "round trip: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Check 18 and AC 3: no built-in tables, `rating.table` none, and no source file outside `tests/`
/// carries a rating row.
#[test]
#[ignore = "tester 057: awaiting build"]
fn rt04_no_table_values_are_shipped() {
    let mut h = H::new(5724);
    let v = h.run("styles", json!({"catalogue": "ratingtable"})).unwrap();
    let list = v.as_array().cloned().or_else(|| v["styles"].as_array().cloned()).unwrap_or_default();
    assert!(list.is_empty(), "no built-in rating tables: {v}");
    let d = h.run("defaults", json!({"group": "Rating"})).unwrap_or_else(|_| h.run("defaults", json!({})).unwrap());
    assert!(d.to_string().contains("rating.table"), "{d}");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut bad = Vec::new();
    let mut stack = vec![root.join("crates"), root.join("apps")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if p.is_dir() {
                if name != "tests" && name != "target" && !name.starts_with('.') {
                    stack.push(p);
                }
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            for (n, line) in text.lines().enumerate() {
                for k in ["capacity,", "grouping,", "resistance,", "ambient,"] {
                    if let Some(i) = line.find(k) {
                        let rest: String = line[i + k.len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == ',').collect();
                        if rest.matches(',').count() >= 2 && rest.split(',').next_back().is_some_and(|x| x.parse::<f64>().is_ok()) {
                            bad.push(format!("{}:{}: {line}", p.display(), n + 1));
                        }
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "rating rows outside tests/: {bad:#?}");
}

// ---------------------------------------------------------------- update maps (§10, check 16, AC 5)

/// A device rating change re-checks only what it protects; an unrelated edit nothing. The counter
/// is read through `ratings {stats: true}` or `checks`' `rechecked` (named in phase 2).
#[test]
#[ignore = "tester 057: awaiting build"]
fn u01_a_device_rating_change_rechecks_only_its_subjects() {
    let R { mut h, q01, category, .. } = fixture(5725);
    // A second, independent protected wire under -Q02.
    let q02 = placed(&mut h, "mcb1", 200.0, 300.0);
    h.run("setprop", json!({"component": q02.hex(), "key": "rating", "value": "4 A"})).unwrap();
    let l2 = placed(&mut h, "lamp", 200.0, 200.0);
    h.run("wire", json!({"points": pts(&[(200.0, 290.0), (200.0, 210.0)])})).unwrap();
    let w2 = conductor_at(&mut h, l2, "X1");
    h.run("category", json!({"conductor": w2.hex(), "value": category})).unwrap();
    let count = |h: &mut H| -> u64 {
        let v = h.run("ratings", json!({"stats": true})).unwrap();
        num_at(&v, &[&["stats", "rechecked_total"], &["rechecked_total"], &["stats", "rechecked"]]).map_or(u64::MAX, |n| n as u64)
    };
    let n = count(&mut h);
    h.run("setprop", json!({"component": q01.hex(), "key": "rating", "value": "6 A"})).unwrap();
    assert_eq!(count(&mut h) - n, 1, "protects[-Q01] only");
    let n = count(&mut h);
    h.run("setprop", json!({"component": l2.hex(), "key": "description", "value": "Lamp"})).unwrap();
    assert_eq!(count(&mut h), n, "an unrelated edit re-checks nothing");
    let n = count(&mut h);
    h.run("setstyle", json!({"catalogue": "wire", "id": category, "fields": {"rated_current": 34}})).unwrap();
    assert_eq!(count(&mut h) - n, 2, "a category change re-checks its users");
}

// ---------------------------------------------------------------- hostile values (§6.1)

#[test]
#[ignore = "tester 057: awaiting build"]
fn h01_hostile_values_are_refused_by_the_command_and_ignored_from_a_file() {
    let R { mut h, wire, cab, .. } = fixture(5726);
    cable(&mut h, 250.0, "-W01", "4 A", "H07RN-F 4G1.5", "12 m");
    let long = "1".repeat(10_000);
    for (k, v) in [
        ("load", json!("1e400")),
        ("load", json!(-1)),
        ("load", json!(long)),
        ("bundling", json!(0.29)),
        ("bundling", json!(1.01)),
        ("bundling", json!("21 bunched")),
        ("bundling", json!("0 bunched")),
        ("basis", json!("guess")),
        ("protected_by", json!("")),
    ] {
        let e = refused(&mut h, "rating", json!({"wire": wire.hex(), k: v}));
        assert!(e.contains(k) || e.contains("0.30") || e.contains("6300"), "{k} {v:.20}: {e}");
    }
    for (k, v) in [
        ("ambient", json!(-41)),
        ("ambient", json!(91)),
        ("grouped", json!(0)),
        ("grouped", json!(21)),
        ("voltage", json!(0)),
        ("voltage", json!(1001)),
        ("system", json!("ac2")),
        ("method", json!("Q")),
        ("cable_type", json!("#00000000000000000000000000000000")),
    ] {
        refused(&mut h, "rating", json!({"cable": "-W01", k: v}));
    }
    // The same values in a file made elsewhere (setprop): ignored, an Info row, never a panic.
    for (k, v) in [("rating.ambient", "hot"), ("rating.grouped", "999999999999"), ("load", "NaN"), ("rating.voltage", "-0")] {
        let _ = h.run("setprop", json!({"cable": "-W01", "key": k, "value": v}));
        let _ = h.run("ratings", json!({}));
    }
    assert!(!checks_of(&mut h, "rating-bad-value").is_empty());
    assert!(rows_text(&mut h, "rating-bad-value").iter().any(|r| r.contains("rating.ambient 'hot'")));
    for v in ["", "1e999", "NaN", "-273", "35 °F", &"9".repeat(10_000)] {
        let _ = h.run("setprop", json!({"component": cab.hex(), "key": "rating.temperature", "value": v}));
        let _ = h.run("ratings", json!({}));
    }
    // Wire-category fields out of range are refused when the style is written.
    for f in [
        json!({"rated_current": 0}),
        json!({"rated_current": 6301}),
        json!({"rated_at": -41}),
        json!({"max_temperature": 251}),
        json!({"conductor_class": "3"}),
        json!({"material": "al"}),
    ] {
        assert!(h.run("setstyle", json!({"catalogue": "wire", "new": true, "name": "Bad", "fields": f})).is_err(), "{f}");
    }
}
