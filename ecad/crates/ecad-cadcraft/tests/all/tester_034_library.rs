//! Tester 034 (component library build-up): acceptance checks from outside, written from brief 034
//! and design 034 revision 2 only, before the implementation existed. They drive the JSON commands
//! (`place`, `symbols`, `setprop`, `tagsettings`, `wire`, `bom`, `checks`) and the catalogue API
//! that 018 already had (`symbols::STARTERS`, `by_key`, `Starter::definition`, `bounds`), so the
//! file compiles before the worker's code lands. Tests of behaviour not built yet are
//!
//! The new API (`anchors`, `Family`, `Kind::ChangeOver`, quiet lists, the defaults registry) is
//! checked in `tester_034_api.rs`, which is added to `main.rs` once the build is merged.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::Session;
use ecad_cadcraft::checks::{self, Object};
use ecad_cadcraft::symbols::{self, STARTER_LIBRARY, STARTERS};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{ComponentId, Document, Pt, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// `def_id(0)` of the starter library (design 018 §4): definition IDs are this plus n.
const DEF_BASE: u128 = 0x0000_ecad_0018_5ea1_0000_0000_0001_0000;

/// One new catalogue entry as design 034 §3–§6 gives it.
#[derive(Debug, Clone)]
pub(crate) struct Sym {
    pub key: String,
    pub prefix: &'static str,
    /// A word the name contains (lower case).
    pub word: &'static str,
    /// Connection points in definition order: (key/label, x, y) in the local frame.
    pub points: Vec<(String, f64, f64)>,
    /// The box (width, height) when the design gives one.
    pub bx: Option<(f64, f64)>,
    /// Points whose `Unconnected` check is not raised (§11, question 8).
    pub quiet: Vec<String>,
    /// The ID block: 0x100 contacts and protection, 0x200 sensors and actuators, 0x300 connectors,
    /// 0x400 PLC card bodies (§2.2).
    pub block: u128,
}

fn sym(key: &str, prefix: &'static str, word: &'static str, points: &[(&str, f64, f64)], bx: Option<(f64, f64)>, quiet: &[&str], block: u128) -> Sym {
    Sym {
        key: key.into(),
        prefix,
        word,
        points: points.iter().map(|(k, x, y)| ((*k).to_string(), *x, *y)).collect(),
        bx,
        quiet: quiet.iter().map(|q| (*q).to_string()).collect(),
        block,
    }
}

fn row(n: usize, y: f64) -> Vec<(String, f64, f64)> {
    (1..=n).map(|i| (i.to_string(), 5.0 * (i as f64 - 1.0), y)).collect()
}

fn connector(key: String, word: &'static str, points: Vec<(String, f64, f64)>) -> Sym {
    let quiet = points.iter().map(|p| p.0.clone()).collect();
    Sym { key, prefix: "X", word, points, bx: None, quiet, block: 0x300 }
}

/// Every new symbol of design 034 §3–§6 (60 keys).
pub(crate) fn catalogue() -> Vec<Sym> {
    const S1: u128 = 0x100;
    const S2: u128 = 0x200;
    const NO: [(&str, f64, f64); 2] = [("13", 0.0, 10.0), ("14", 0.0, -10.0)];
    const NC: [(&str, f64, f64); 2] = [("11", 0.0, 10.0), ("12", 0.0, -10.0)];
    let co = [("11", 0.0, 10.0), ("12", 5.0, -10.0), ("14", 0.0, -10.0)];
    let rcd = [("1", 0.0, 10.0), ("2", 0.0, -10.0), ("N1", 10.0, 10.0), ("N2", 10.0, -10.0)];
    let mut v = vec![
        // Stage 1 (§3).
        sym("co", "K", "change-over", &co, Some((5.0, 20.0)), &[], S1),
        sym("swco", "S", "change-over", &co, Some((13.0, 20.0)), &[], S1),
        sym(
            "power4",
            "K",
            "4-pole",
            &[
                ("1", -15.0, 10.0),
                ("2", -15.0, -10.0),
                ("3", -5.0, 10.0),
                ("4", -5.0, -10.0),
                ("5", 5.0, 10.0),
                ("6", 5.0, -10.0),
                ("7", 15.0, 10.0),
                ("8", 15.0, -10.0),
            ],
            Some((34.0, 20.0)),
            &[],
            S1,
        ),
        sym("rcbo2", "Q", "rcbo", &rcd, Some((23.0, 20.0)), &[], S1),
        sym("rcd2", "Q", "rcd", &rcd, Some((23.0, 20.0)), &[], S1),
        sym(
            "rcd4",
            "Q",
            "rcd",
            &[
                ("1", 0.0, 10.0),
                ("2", 0.0, -10.0),
                ("3", 10.0, 10.0),
                ("4", 10.0, -10.0),
                ("5", 20.0, 10.0),
                ("6", 20.0, -10.0),
                ("N1", 30.0, 10.0),
                ("N2", 30.0, -10.0),
            ],
            Some((43.0, 20.0)),
            &[],
            S1,
        ),
        sym("ssrin", "K", "ssr", &[("A1", 0.0, 10.0), ("A2", 0.0, -10.0)], Some((12.0, 20.0)), &[], S1),
        sym("ssrout", "K", "ssr", &[("1", 0.0, 10.0), ("2", 0.0, -10.0)], Some((9.0, 20.0)), &[], S1),
        sym("fuseterm", "F", "fuse terminal", &[("1", 0.0, 10.0), ("2", 0.0, -10.0)], Some((7.0, 20.0)), &[], S1),
    ];
    for n in [4usize, 6, 8, 10, 12] {
        let xn = 10.0 * (n as f64 - 1.0);
        let mut points = vec![("L+".to_string(), -10.0, 20.0), ("M".to_string(), -10.0, -15.0)];
        points.extend((1..=n).map(|i| (i.to_string(), 10.0 * (i as f64 - 1.0), -15.0)));
        let mut quiet = vec!["M".to_string()];
        quiet.extend((1..=n).map(|i| i.to_string()));
        v.push(Sym { key: format!("fusebank{n}"), prefix: "F", word: "fuse bank", points, bx: Some((xn + 20.0, 35.0)), quiet, block: S1 });
    }
    // Stage 2 (§4).
    v.extend([
        sym("prox3", "B", "proximity", &[("1", -5.0, 10.0), ("3", -5.0, -10.0), ("4", 5.0, -10.0)], Some((20.0, 20.0)), &[], S2),
        sym("floatno", "B", "float", &NO, Some((9.5, 20.0)), &[], S2),
        sym("floatnc", "B", "float", &NC, Some((11.5, 20.0)), &[], S2),
        sym("rollerno", "B", "roller", &NO, Some((9.2, 20.0)), &[], S2),
        sym("rollernc", "B", "roller", &NC, Some((11.2, 20.0)), &[], S2),
        sym("interlocknc", "B", "interlock", &NC, Some((17.0, 20.0)), &[], S2),
        sym("interlockno", "B", "interlock", &NO, Some((13.0, 20.0)), &[], S2),
        sym("valve", "MB", "valve", &[("A1", 0.0, 10.0), ("A2", 0.0, -10.0)], Some((20.0, 20.0)), &[], S2),
        sym(
            "valve2",
            "MB",
            "valve",
            &[("A1", -10.0, 10.0), ("A2", -10.0, -10.0), ("B1", 10.0, 10.0), ("B2", 10.0, -10.0)],
            Some((32.0, 20.0)),
            &[],
            S2,
        ),
        sym("tc", "B", "thermocouple", &[("+", -5.0, 10.0), ("-", 5.0, 10.0)], Some((10.5, 15.5)), &[], S2),
        sym("heater", "E", "heater", &[("1", 0.0, 10.0), ("2", 0.0, -10.0)], Some((5.0, 20.0)), &[], S2),
    ]);
    // Stage 3a (§5): pin i at x = 5(i−1); sockets at y 5, plugs at y −5, bulkheads at y 0.
    for n in [2usize, 3, 4, 6, 8, 12] {
        v.push(connector(format!("socket{n}"), "socket", row(n, 5.0)));
        v.push(connector(format!("plug{n}"), "plug", row(n, -5.0)));
    }
    for n in [9usize, 15, 25] {
        v.push(connector(format!("dsub{n}s"), "d-sub", row(n, 5.0)));
        v.push(connector(format!("dsub{n}p"), "d-sub", row(n, -5.0)));
    }
    for n in [4usize, 5, 8] {
        v.push(connector(format!("m12s{n}"), "m12", row(n, 5.0)));
        v.push(connector(format!("m12p{n}"), "m12", row(n, -5.0)));
    }
    let mut rj = row(8, 0.0);
    rj.push(("SH".into(), 40.0, 0.0));
    v.push(connector("rj45".into(), "rj45", rj));
    for k in ["usba", "usbb"] {
        let mut u = row(4, 0.0);
        u.push(("SH".into(), 20.0, 0.0));
        v.push(connector(k.into(), "usb", u));
    }
    // Stage 3b (§6).
    let supply = [("L+", -5.0, 25.0), ("M", 5.0, 25.0)];
    v.push(sym("plccpu", "A", "plc", &supply, None, &[], 0x400));
    v.push(sym(
        "plcps",
        "A",
        "plc",
        &[("L", -10.0, 25.0), ("N", -5.0, 25.0), ("PE", 0.0, 25.0), ("L+", -5.0, -25.0), ("M", 5.0, -25.0)],
        None,
        &[],
        0x400,
    ));
    for k in ["plcdi", "plcdq", "plcai", "plcao", "plcrtd", "plciol"] {
        v.push(sym(k, "A", "plc", &supply, None, &["L+", "M"], 0x400));
    }
    v
}

/// The 21 starters of 018/022/028 with their definition IDs and prefixes, as on `dev` `6729c8f1`.
/// Brief 045's starters (design 045 §3.1, §4.1), not design 034's.
pub(crate) const BRIEF_045: &[&str] = &["plcim", "plcpm", "plccm", "chdi", "chdq", "chai", "chao", "chrtd", "chiol"];

const OLD: [(&str, u128, &str); 21] = [
    ("coil", 1, "K"),
    ("no", 2, "K"),
    ("nc", 3, "K"),
    ("power3", 4, "K"),
    ("mcb1", 5, "Q"),
    ("mcb2", 6, "Q"),
    ("fuse", 7, "F"),
    ("psu", 8, "G"),
    ("pbno", 9, "S"),
    ("pbnc", 10, "S"),
    ("sel", 11, "S"),
    ("lamp", 12, "H"),
    ("motor3", 13, "M"),
    ("motor1", 14, "M"),
    ("trafo", 15, "T"),
    ("terminal", 16, "X"),
    ("di", 17, "A"),
    ("do", 18, "A"),
    ("junction", 19, ""),
    ("continuation", 20, ""),
    ("terminalpe", 21, "X"),
];

// ---------------------------------------------------------------------------------------------
// helpers

fn place(h: &mut H, key: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": key, "at": [at.0, at.1]})).unwrap_or_else(|e| panic!("place {key}: {e}"))
}

fn join(h: &mut H, key: &str, at: (f64, f64), device: ComponentId) -> Result<Value, String> {
    h.run("place", json!({"symbol": key, "at": [at.0, at.1], "device": device.hex()}))
}

/// The labels of a `place` result, in order.
fn labels(v: &Value) -> Vec<String> {
    v["terminals"].as_array().map(|a| a.iter().map(|t| t["label"].as_str().unwrap_or("?").to_string()).collect()).unwrap_or_default()
}

/// A terminal of a `place` result by label.
fn term(v: &Value, label: &str) -> TerminalId {
    let t = v["terminals"].as_array().and_then(|a| a.iter().find(|t| t["label"] == label)).unwrap_or_else(|| panic!("no point {label} in {v}"));
    id(t, "terminal")
}

/// Every label of a device's points, sorted.
fn device_labels(d: &Document, c: ComponentId) -> BTreeSet<String> {
    d.terminals.values().filter(|t| t.component == c).filter_map(|t| t.label.clone()).collect()
}

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn prop(h: &mut H, c: ComponentId, key: &str) -> Option<String> {
    h.doc().components.get(&c).and_then(|x| x.properties.get(key).cloned())
}

fn tag(n: usize, prefix: &str) -> String {
    format!("-{prefix}{n:02}")
}

fn near(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// The `symbols` row with exactly this key.
fn symbol_row(h: &mut H, key: &str) -> Option<Value> {
    let v = h.run("symbols", json!({"query": key})).unwrap();
    v["symbols"].as_array().and_then(|a| a.iter().find(|s| s["key"] == key).cloned())
}

fn search_keys(h: &mut H, q: &str) -> BTreeSet<String> {
    let v = h.run("symbols", json!({"query": q})).unwrap();
    v["symbols"].as_array().map(|a| a.iter().filter_map(|s| s["key"].as_str().map(String::from)).collect()).unwrap_or_default()
}

fn net_of(d: &Document, t: TerminalId) -> Option<ecad_model::connectivity::NetId> {
    d.connectivity().net_of_terminal(t)
}

/// The definitions in `d` whose origin is the starter library's `key`.
fn embeds_of(d: &Document, key: &str) -> usize {
    let Some(s) = symbols::by_key(key) else { return 0 };
    d.definitions.values().filter(|x| x.origin.as_ref().is_some_and(|o| o.library == STARTER_LIBRARY && o.definition == s.id)).count()
}

/// The terminals raised as `Unconnected` in `d`.
fn unconnected(d: &Document) -> BTreeSet<TerminalId> {
    checks::all(d, 1e-6)
        .into_iter()
        .filter(|c| c.kind.name() == "unconnected")
        .filter_map(|c| match c.object {
            Object::Terminal(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// A refused write: an error containing `want` and no transaction.
fn refused(h: &mut H, cmd: &str, params: Value, want: &str) -> String {
    let head = h.head();
    let e = h.run(cmd, params.clone()).err().unwrap_or_else(|| panic!("{cmd} {params} should be refused"));
    assert!(e.contains(want), "{cmd} {params}: {e:?} should contain {want:?}");
    assert_eq!(h.head(), head, "{cmd} {params}: refused, head unchanged");
    e
}

fn setprop(h: &mut H, c: ComponentId, key: &str, value: Value) -> Result<Value, String> {
    h.run("setprop", json!({"component": c.hex(), "key": key, "value": value}))
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t034 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn close_all(h: &mut H) {
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
}

fn reopen(path: &str, seed: u64) -> H {
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, path, seed).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    h.trail.push(d);
    h
}

fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.components, &d.terminals, &d.conductors, &d.pages, &d.page_order, &d.representations, &d.items, &d.properties, &d.definitions)
}

/// Grid position of the i-th placement, far enough apart that no two symbols touch.
fn slot(i: usize) -> (f64, f64) {
    (200.0 * (i % 8) as f64, 100.0 * (i / 8) as f64 + 50.0)
}

// ---------------------------------------------------------------------------------------------
// AC 1: the catalogue

/// AC 1 (runs now): the 21 starters keep their definition IDs and prefixes; IDs are never reused.
#[test]
fn a0_the_21_old_starters_keep_their_ids_and_prefixes() {
    for (key, n, prefix) in OLD {
        let s = symbols::by_key(key).unwrap_or_else(|| panic!("{key} missing"));
        assert_eq!(s.id.0, DEF_BASE + n, "{key}: definition ID unchanged");
        assert_eq!(s.prefix(), prefix, "{key}: built-in prefix unchanged");
    }
}

/// AC 1: every new key is in the catalogue with the design's prefix, points in order, positions on
/// the 5 mm grid, box, ID block, unique ID and content hash = its origin's.
#[test]
fn a1_every_new_symbol_is_in_the_catalogue_as_designed() {
    let cat = catalogue();
    assert_eq!(cat.len(), 60, "the test table");
    let mut h = H::new(0x7034_0001);
    for e in &cat {
        let s = symbols::by_key(&e.key).unwrap_or_else(|| panic!("{} is not in the catalogue", e.key));
        assert_eq!(s.prefix(), e.prefix, "{}: prefix", e.key);
        assert!(s.name.to_lowercase().contains(e.word), "{}: name {:?} should contain {:?}", e.key, s.name, e.word);
        let d = s.definition().unwrap_or_else(|err| panic!("{}: {err}", e.key));
        // Points: keys in definition order at the design's positions, on the grid.
        let got: Vec<(String, f64, f64)> = d.terminals.iter().map(|t| (t.key.clone(), t.at.x, t.at.y)).collect();
        assert_eq!(got.len(), e.points.len(), "{}: points {got:?}", e.key);
        for ((gk, gx, gy), (wk, wx, wy)) in got.iter().zip(&e.points) {
            assert_eq!(gk, wk, "{}: point order {got:?}", e.key);
            assert!((gx - wx).abs() < 1e-9 && (gy - wy).abs() < 1e-9, "{}: point {gk} at ({gx},{gy}), design ({wx},{wy})", e.key);
            for c in [gx, gy] {
                let r = c / symbols::SYMBOL_GRID_MM;
                assert!((r - r.round()).abs() < 1e-9, "{}: point {gk} off the 5 mm grid", e.key);
            }
        }
        // Origin and content hash.
        let o = d.origin.as_ref().unwrap_or_else(|| panic!("{}: no origin", e.key));
        assert_eq!(o.library, STARTER_LIBRARY, "{}", e.key);
        assert_eq!(o.definition, s.id, "{}", e.key);
        assert_eq!(Some(o.content_hash), ecad_file::definition_hash(d).ok(), "{}: content hash is its origin's", e.key);
        // ID block (§2.2).
        let n = s.id.0.checked_sub(DEF_BASE).unwrap_or_else(|| panic!("{}: ID below the library base", e.key));
        assert!((e.block..e.block + 0x100).contains(&n), "{}: def_id({n:#x}) outside block {:#x}", e.key, e.block);
        // Box.
        if let Some((w, hh)) = e.bx {
            let (lo, hi) = symbols::bounds(d);
            let (gw, gh) = (hi.x - lo.x, hi.y - lo.y);
            assert!((gw - w).abs() < 0.05 && (gh - hh).abs() < 0.05, "{}: box {gw}×{gh}, design {w}×{hh}", e.key);
        }
        // Find Symbol's row: prefix and labels.
        let r = symbol_row(&mut h, &e.key).unwrap_or_else(|| panic!("symbols does not list {}", e.key));
        assert_eq!(r["prefix"], json!(format!("-{}", e.prefix)), "{}: {r}", e.key);
        let want: Vec<&str> = e.points.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(r["terminals"], json!(want), "{}: labels", e.key);
    }
    // IDs unique across the whole catalogue; the count is the old 21 plus the new ones.
    let ids: BTreeSet<u128> = STARTERS.iter().map(|s| s.id.0).collect();
    assert_eq!(ids.len(), STARTERS.len(), "definition IDs unique");
    let keys: BTreeSet<&str> = STARTERS.iter().map(|s| s.key).collect();
    assert_eq!(keys.len(), STARTERS.len(), "keys unique");
    // Brief 036's multi-level terminal starters (20) are design 036's.
    let extra: Vec<&str> = STARTERS
        .iter()
        .map(|s| s.key)
        .filter(|k| {
            !OLD.iter().any(|o| o.0 == *k)
                && !cat.iter().any(|e| e.key == *k)
                && !ecad_cadcraft::terminal_types::is_level_starter(k)
                && !BRIEF_045.contains(k)
        })
        // Brief 044's connector pins (design 044 §3.2).
        .filter(|k| !matches!(*k, "pinpair" | "pins" | "pinp"))
        .collect();
    assert!(extra.is_empty(), "keys not in design 034: {extra:?}");
    assert_eq!(STARTERS.len(), 21 + cat.len() + 20 + 3 + STARTERS.iter().filter(|s| BRIEF_045.contains(&s.key)).count());
}

/// §13 check 1 and §7.1: Find Symbol finds by family word, prefix and alias.
#[test]
fn a2_find_symbol_by_family_prefix_and_alias() {
    let mut h = H::new(0x7034_0002);
    let head = h.head();
    let prot = search_keys(&mut h, "protection");
    for k in ["rcbo2", "rcd2", "rcd4", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12"] {
        assert!(prot.contains(k), "protection finds {k}: {prot:?}");
    }
    let q = search_keys(&mut h, "-Q");
    for k in ["mcb1", "mcb2", "rcbo2", "rcd2", "rcd4"] {
        assert!(q.contains(k), "-Q finds {k}: {q:?}");
    }
    let conn = search_keys(&mut h, "connectors");
    for e in catalogue().iter().filter(|e| e.block == 0x300) {
        assert!(conn.contains(&e.key), "connectors finds {}: {conn:?}", e.key);
    }
    let plc = search_keys(&mut h, "plc");
    for e in catalogue().iter().filter(|e| e.block == 0x400) {
        assert!(plc.contains(&e.key), "plc finds {}", e.key);
    }
    let sensors = search_keys(&mut h, "sensors");
    for k in ["prox3", "floatno", "floatnc", "rollerno", "rollernc", "interlocknc", "interlockno", "tc"] {
        assert!(sensors.contains(k), "sensors finds {k}: {sensors:?}");
    }
    let act = search_keys(&mut h, "actuators");
    for k in ["valve", "valve2"] {
        assert!(act.contains(k), "actuators finds {k}: {act:?}");
    }
    let contacts = search_keys(&mut h, "contacts");
    for k in ["co", "swco", "power4", "ssrout"] {
        assert!(contacts.contains(k), "contacts finds {k}: {contacts:?}");
    }
    for (alias, k) in [("change-over", "co"), ("spdt", "co"), ("rccb", "rcd2"), ("profinet", "rj45"), ("io-link", "plciol"), ("sub-d", "dsub9s")] {
        assert!(search_keys(&mut h, alias).contains(k), "{alias} finds {k}");
    }
    // Searching never writes.
    assert_eq!(h.head(), head, "symbols records nothing");
}

// ---------------------------------------------------------------------------------------------
// AC 2: placing makes a device

/// AC 2, §12 item 1 in-process: every new key placed with `place` makes a new device with the
/// family's prefix and the next number, the design's labels at the design's positions, the
/// symbol's name as description, one transaction, and one embedded definition found by origin;
/// a second placement reuses it.
#[test]
#[ignore = "slow: places all 60 new symbols in one project (about 20 s at opt-level 0); library_contracts covers placement fast"]
fn b_each_new_symbol_places_as_a_numbered_device_and_embeds_once() {
    let mut h = H::new(0x7034_0003);
    let page = h.first_page();
    let mut next: BTreeMap<&str, usize> = BTreeMap::new();
    let cat = catalogue();
    for (i, e) in cat.iter().enumerate() {
        let name = symbols::by_key(&e.key).map(|s| s.name.to_string()).unwrap_or_default();
        for round in 0..2 {
            let at = slot(2 * i + round);
            let head = h.head();
            let defs = h.doc().definitions.len();
            let v = place(&mut h, &e.key, at);
            assert_eq!(h.head(), head + 1, "{}: one transaction", e.key);
            let n = next.entry(e.prefix).or_insert(0);
            *n += 1;
            assert_eq!(v["tag"], json!(tag(*n, e.prefix)), "{} round {round}: {v}", e.key);
            assert_eq!(v["joined"], json!(false), "{}", e.key);
            let want: Vec<&str> = e.points.iter().map(|p| p.0.as_str()).collect();
            assert_eq!(labels(&v), want, "{}: labels in definition order", e.key);
            let c: ComponentId = id(&v, "component");
            assert_eq!(prop(&mut h, c, "description").as_deref(), Some(name.as_str()), "{}: default description", e.key);
            let d = h.doc();
            let shown = ecad_model::derive::terminal_positions(&d, page);
            for (label, x, y) in &e.points {
                let t = term(&v, label);
                let got = shown.iter().find(|(x, _, _)| *x == t).map(|(_, _, p)| *p).unwrap_or_else(|| panic!("{} {label} not shown", e.key));
                assert!(near(got, Pt::new(at.0 + x, at.1 + y)), "{} {label}: at {got:?}", e.key);
            }
            assert_eq!(embeds_of(&d, &e.key), 1, "{} round {round}: embedded once, found by origin", e.key);
            assert_eq!(d.definitions.len(), defs + usize::from(round == 0), "{} round {round}: a second placement adds no definition", e.key);
        }
    }
}

/// AC 2: the embed is undone with the first placement and made again (once) on the next.
#[test]
fn b2_undo_takes_the_embed_back_and_redo_restores_it() {
    let mut h = H::new(0x7034_0004);
    let before = h.doc();
    place(&mut h, "fusebank8", (100.0, 150.0));
    assert_eq!(embeds_of(&h.doc(), "fusebank8"), 1);
    h.cmdline("u");
    assert_eq!(model_parts(&h.doc()), model_parts(&before), "u restores the document before the placement");
    assert_eq!(embeds_of(&h.doc(), "fusebank8"), 0);
    h.run("redo", Value::Null).unwrap();
    assert_eq!(embeds_of(&h.doc(), "fusebank8"), 1);
    place(&mut h, "fusebank8", (300.0, 150.0));
    assert_eq!(embeds_of(&h.doc(), "fusebank8"), 1, "reused by origin after redo");
}

// ---------------------------------------------------------------------------------------------
// AC 3: change-over contacts

/// AC 3, §13 check 3: `-K01` coil + `co` → 11/12/14; + `no` → 23/24; + `co` → 31/32/34, each one
/// transaction; `u` takes back one at a time.
#[test]
fn c_change_over_takes_the_next_sequence_digit() {
    let mut h = H::new(0x7034_0005);
    let k1: ComponentId = id(&place(&mut h, "coil", (50.0, 150.0)), "component");
    let mut docs = vec![h.doc()];
    for (key, x, want) in [("co", 80.0, vec!["11", "12", "14"]), ("no", 110.0, vec!["23", "24"]), ("co", 140.0, vec!["31", "32", "34"])] {
        let head = h.head();
        let v = join(&mut h, key, (x, 150.0), k1).unwrap();
        assert_eq!(h.head(), head + 1, "{key}: one transaction");
        assert_eq!(v["joined"], json!(true));
        assert_eq!(v["tag"], json!("-K01"));
        assert_eq!(labels(&v), want, "{key}");
        docs.push(h.doc());
    }
    assert_eq!(device_labels(&h.doc(), k1), set(&["A1", "A2", "11", "12", "14", "23", "24", "31", "32", "34"]));
    // Change-over positions: common on top, NC to the right below, NO below (§3).
    let page = h.first_page();
    let v = join(&mut h, "co", (170.0, 150.0), k1).unwrap();
    assert_eq!(labels(&v), ["41", "42", "44"]);
    assert!(near(h.terminal_pos(page, term(&v, "41")), Pt::new(170.0, 160.0)));
    assert!(near(h.terminal_pos(page, term(&v, "42")), Pt::new(175.0, 140.0)));
    assert!(near(h.terminal_pos(page, term(&v, "44")), Pt::new(170.0, 140.0)));
    docs.push(h.doc());
    for want in docs.iter().rev().skip(1) {
        h.cmdline("u");
        assert_eq!(model_parts(&h.doc()), model_parts(want), "u takes back one contact");
    }
}

/// §3 note: `-K01` after a `no` (13/14) gives a `co` 21/22/24; a manual change-over on `-S01`
/// follows the same rule; a `co` placed alone makes a new `-K` device with 11/12/14.
#[test]
fn c2_change_over_after_a_make_contact_and_on_a_switch() {
    let mut h = H::new(0x7034_0006);
    let k1: ComponentId = id(&place(&mut h, "coil", (50.0, 150.0)), "component");
    join(&mut h, "no", (80.0, 150.0), k1).unwrap();
    assert_eq!(labels(&join(&mut h, "co", (110.0, 150.0), k1).unwrap()), ["21", "22", "24"]);
    let s1v = place(&mut h, "pbno", (200.0, 150.0));
    assert_eq!(s1v["tag"], json!("-S01"));
    let s1: ComponentId = id(&s1v, "component");
    assert_eq!(labels(&join(&mut h, "swco", (230.0, 150.0), s1).unwrap()), ["21", "22", "24"]);
    let lone = place(&mut h, "co", (300.0, 150.0));
    assert_eq!(lone["tag"], json!("-K02"));
    assert_eq!(labels(&lone), ["11", "12", "14"]);
    let sw = place(&mut h, "swco", (350.0, 150.0));
    assert_eq!(sw["tag"], json!("-S02"));
    assert_eq!(labels(&sw), ["11", "12", "14"]);
}

/// AC 3, §13 check 4: a ninth contact as a `co` is 91/92/94; a tenth is refused with
/// `-K01 already has 9 contacts.` and nothing is written.
#[test]
fn d_after_the_ninth_contact_a_change_over_is_refused() {
    let mut h = H::new(0x7034_0007);
    let k1: ComponentId = id(&place(&mut h, "coil", (20.0, 150.0)), "component");
    for i in 0..8 {
        join(&mut h, "no", (50.0 + 30.0 * i as f64, 150.0), k1).unwrap();
    }
    assert_eq!(labels(&join(&mut h, "co", (300.0, 150.0), k1).unwrap()), ["91", "92", "94"]);
    refused(&mut h, "place", json!({"symbol": "co", "at": [330, 150], "device": k1.hex()}), "-K01 already has 9 contacts.");
    refused(&mut h, "place", json!({"symbol": "swco", "at": [330, 150], "device": k1.hex()}), "-K01 already has 9 contacts.");
    // Nine `no` first, then a `co`: the same refusal.
    let k2: ComponentId = id(&place(&mut h, "coil", (20.0, 50.0)), "component");
    for i in 0..9 {
        join(&mut h, "no", (50.0 + 30.0 * i as f64, 50.0), k2).unwrap();
    }
    refused(&mut h, "place", json!({"symbol": "co", "at": [330, 50], "device": k2.hex()}), "-K02 already has 9 contacts.");
}

// ---------------------------------------------------------------------------------------------
// Joining fixed and contact symbols

/// §2.3, §13 checks 5 and 8: SSR input + output is one `-K` device (A1, A2, 1, 2), a second
/// output is refused; `power4` joins a coil (1…8) once; heads are exclusive; field contacts take
/// the next sequence digit; a PLC card takes `di` channels.
#[test]
fn e_fixed_symbols_join_by_the_018_rules() {
    let mut h = H::new(0x7034_0008);
    let ssr = place(&mut h, "ssrin", (50.0, 150.0));
    assert_eq!(ssr["tag"], json!("-K01"));
    assert_eq!(labels(&ssr), ["A1", "A2"]);
    let k1: ComponentId = id(&ssr, "component");
    let out = join(&mut h, "ssrout", (80.0, 150.0), k1).unwrap();
    assert_eq!(labels(&out), ["1", "2"]);
    assert_eq!(device_labels(&h.doc(), k1), set(&["A1", "A2", "1", "2"]));
    refused(&mut h, "place", json!({"symbol": "ssrout", "at": [110, 150], "device": k1.hex()}), "already has connection point 1");

    let k2: ComponentId = id(&place(&mut h, "coil", (150.0, 150.0)), "component");
    assert_eq!(labels(&join(&mut h, "power4", (200.0, 150.0), k2).unwrap()), ["1", "2", "3", "4", "5", "6", "7", "8"]);
    refused(&mut h, "place", json!({"symbol": "power4", "at": [260, 150], "device": k2.hex()}), "already has connection point");
    refused(&mut h, "place", json!({"symbol": "power3", "at": [260, 150], "device": k2.hex()}), "already has connection point");

    // Exclusive heads: a second head on the same device is refused with 018's words.
    let q1: ComponentId = id(&place(&mut h, "rcbo2", (50.0, 50.0)), "component");
    refused(&mut h, "place", json!({"symbol": "rcbo2", "at": [100, 50], "device": q1.hex()}), "-Q01 already has a");
    refused(&mut h, "place", json!({"symbol": "rcd2", "at": [100, 50], "device": q1.hex()}), "-Q01 already has");
    let f1: ComponentId = id(&place(&mut h, "fusebank4", (200.0, 50.0)), "component");
    refused(&mut h, "place", json!({"symbol": "fuseterm", "at": [300, 50], "device": f1.hex()}), "-F01 already has");
    // A fault-signal contact on the bank is 13/14 (§3 notes).
    assert_eq!(labels(&join(&mut h, "no", (300.0, 50.0), f1).unwrap()), ["13", "14"]);

    // Limit switch with NO and NC: one device, sequence digits (§4 notes).
    let b = place(&mut h, "rollerno", (50.0, 250.0));
    assert_eq!(b["tag"], json!("-B01"));
    assert_eq!(labels(&b), ["13", "14"]);
    let b1: ComponentId = id(&b, "component");
    assert_eq!(labels(&join(&mut h, "rollernc", (80.0, 250.0), b1).unwrap()), ["21", "22"]);
    assert_eq!(labels(&join(&mut h, "interlocknc", (110.0, 250.0), b1).unwrap()), ["31", "32"]);

    // §13 check 8: a DI card takes two channels.
    let a = place(&mut h, "plcdi", (250.0, 250.0));
    assert_eq!(a["tag"], json!("-A01"));
    let a1: ComponentId = id(&a, "component");
    // Brief 045 (FP7): channels are labelled with pins.
    assert_eq!(labels(&join(&mut h, "di", (240.0, 200.0), a1).unwrap()), ["1"]);
    assert_eq!(labels(&join(&mut h, "di", (260.0, 200.0), a1).unwrap()), ["2"]);
    assert_eq!(device_labels(&h.doc(), a1), set(&["L+", "M", "1", "2"]));
}

// ---------------------------------------------------------------------------------------------
// Nets

/// §13 check 6: a fuse bank fed at `L+` with loads on outputs 1 and 2 gives three nets; the
/// fuses break them.
#[test]
fn f_fuse_bank_breaks_the_net_per_fuse() {
    let mut h = H::new(0x7034_0009);
    let v = place(&mut h, "fusebank8", (100.0, 150.0));
    // L+ (90,170), outputs 1 (100,135) and 2 (110,135).
    h.run("wire", json!({"points": pts(&[(90.0, 200.0), (90.0, 170.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(100.0, 135.0), (100.0, 100.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(110.0, 135.0), (110.0, 90.0)])})).unwrap();
    let d = h.doc();
    let (lp, o1, o2) = (term(&v, "L+"), term(&v, "1"), term(&v, "2"));
    for t in [lp, o1, o2] {
        assert!(d.has_ends(t), "a wire lands on {t:?}");
    }
    let (a, b, c) = (net_of(&d, lp), net_of(&d, o1), net_of(&d, o2));
    assert!(a.is_some() && b.is_some() && c.is_some());
    assert!(a != b && b != c && a != c, "supply, output 1 and output 2 are three nets: {a:?} {b:?} {c:?}");
}

/// §3 notes, question 1: a fuse terminal is two points, so the nets on either side differ.
#[test]
fn g_fuse_terminal_breaks_the_net() {
    let mut h = H::new(0x7034_000a);
    let v = place(&mut h, "fuseterm", (100.0, 150.0));
    assert_eq!(v["tag"], json!("-F01"));
    h.run("wire", json!({"points": pts(&[(100.0, 200.0), (100.0, 160.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(100.0, 140.0), (100.0, 100.0)])})).unwrap();
    let d = h.doc();
    let (a, b) = (term(&v, "1"), term(&v, "2"));
    assert!(d.has_ends(a) && d.has_ends(b));
    assert_ne!(net_of(&d, a), net_of(&d, b), "the fuse terminal breaks the net");
    // And it is not a strip position (question 1): no strip appears for it.
    let plan = h.run("stripplan", json!({})).unwrap();
    assert!(plan["strips"].as_array().is_none_or(|s| s.is_empty()), "{plan}");
}

/// §5, §13 check 7: a bulkhead pin is one point; wires from above and below are one net.
#[test]
fn h_bulkhead_pin_joins_both_sides() {
    let mut h = H::new(0x7034_000b);
    let v = place(&mut h, "rj45", (100.0, 150.0));
    assert_eq!(v["tag"], json!("-X01"));
    h.run("wire", json!({"points": pts(&[(100.0, 200.0), (100.0, 150.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(100.0, 150.0), (100.0, 100.0)])})).unwrap();
    let d = h.doc();
    let pin = term(&v, "1");
    let net = net_of(&d, pin).expect("pin 1 has a net");
    let conn = d.connectivity();
    assert_eq!(conn.net(net).map(|n| n.conductors.len()), Some(2), "both wires are on pin 1's net");
    let sh = term(&v, "SH");
    assert_ne!(net_of(&d, sh), Some(net), "the shield is its own point");
}

// ---------------------------------------------------------------------------------------------
// AC 4: ratings

/// AC 4, §7.3, §13 check 9: `rating` on any device; `rating.<n>` only on a fuse bank's fuse
/// outputs; 018 §6.1's value rules.
#[test]
fn i_ratings_set_and_refused_as_designed() {
    let mut h = H::new(0x7034_000c);
    let q1: ComponentId = id(&place(&mut h, "rcbo2", (50.0, 150.0)), "component");
    let f1: ComponentId = id(&place(&mut h, "fusebank8", (150.0, 150.0)), "component");
    let f2: ComponentId = id(&place(&mut h, "fuseterm", (300.0, 150.0)), "component");
    let e1: ComponentId = id(&place(&mut h, "heater", (50.0, 50.0)), "component");
    let b1: ComponentId = id(&place(&mut h, "tc", (100.0, 50.0)), "component");
    let k1: ComponentId = id(&place(&mut h, "coil", (150.0, 50.0)), "component");

    // `rating` on any device, one transaction each; trimmed; `null` and empty clear.
    for (c, value) in [(q1, "13A C 30mA"), (e1, "235W"), (b1, "K"), (f2, "6.3A T"), (k1, "24V DC"), (f1, "16A")] {
        let head = h.head();
        setprop(&mut h, c, "rating", json!(value)).unwrap();
        assert_eq!(h.head(), head + 1);
        assert_eq!(prop(&mut h, c, "rating").as_deref(), Some(value));
    }
    setprop(&mut h, e1, "rating", json!("  300W  ")).unwrap();
    assert_eq!(prop(&mut h, e1, "rating").as_deref(), Some("300W"), "trimmed");
    setprop(&mut h, e1, "rating", Value::Null).unwrap();
    assert_eq!(prop(&mut h, e1, "rating"), None, "null clears");
    setprop(&mut h, b1, "rating", json!("")).unwrap();
    assert_eq!(prop(&mut h, b1, "rating"), None, "empty clears");
    let max = "A".repeat(200);
    setprop(&mut h, k1, "rating", json!(max.as_str())).unwrap();
    assert_eq!(prop(&mut h, k1, "rating"), Some(max));

    // `rating.<n>` on the bank's fuse outputs 1…8.
    for n in 1..=8 {
        setprop(&mut h, f1, &format!("rating.{n}"), json!(format!("{n}A"))).unwrap();
    }
    assert_eq!(prop(&mut h, f1, "rating.3").as_deref(), Some("3A"));
    setprop(&mut h, f1, "rating.3", json!("2A")).unwrap();
    assert_eq!(prop(&mut h, f1, "rating.3").as_deref(), Some("2A"));
    setprop(&mut h, f1, "rating.3", Value::Null).unwrap();
    assert_eq!(prop(&mut h, f1, "rating.3"), None);

    // Refusals, each with the head unchanged.
    let f = f1.hex();
    refused(&mut h, "setprop", json!({"component": f, "key": "rating.13", "value": "2A"}), "-F01 has no fuse 13.");
    refused(&mut h, "setprop", json!({"component": f, "key": "rating.9", "value": "2A"}), "-F01 has no fuse 9.");
    for n in ["0", "L+", "M", "01", "abc", "-1", "1.5"] {
        let e = refused(&mut h, "setprop", json!({"component": f, "key": format!("rating.{n}"), "value": "2A"}), "-F01");
        assert!(!e.is_empty());
    }
    refused(&mut h, "setprop", json!({"component": q1.hex(), "key": "rating.3", "value": "2A"}), "is not a fuse bank");
    refused(&mut h, "setprop", json!({"component": q1.hex(), "key": "rating.1", "value": "2A"}), "-Q01 is not a fuse bank");
    refused(&mut h, "setprop", json!({"component": f2.hex(), "key": "rating.1", "value": "2A"}), "is not a fuse bank");
    refused(&mut h, "setprop", json!({"component": k1.hex(), "key": "rating", "value": "A".repeat(201)}), "");
    refused(&mut h, "setprop", json!({"component": k1.hex(), "key": "rating", "value": "13A\u{7}"}), "");
    refused(&mut h, "setprop", json!({"component": k1.hex(), "key": "rating", "value": "13A\nC"}), "");
    refused(&mut h, "setprop", json!({"component": f, "key": "rating.2", "value": "2A\u{0}"}), "");
    // A fuse bank joined to another symbol is still a fuse bank (its head).
    join(&mut h, "no", (250.0, 150.0), f1).unwrap();
    setprop(&mut h, f1, "rating.8", json!("1A")).unwrap();
}

// ---------------------------------------------------------------------------------------------
// Question 8: quiet spare points

/// §11, question 8: an unwired connector pin, a fuse bank's `M` and outputs and a card's supply
/// points are not flagged `Unconnected`; a fuse bank's `L+`, a CPU's and a power card's points and
/// every non-quiet point still are.
#[test]
#[ignore = "slow: places every quiet family in one project (about 4 s at opt-level 0); library_contracts covers quiet points fast"]
fn j_quiet_spare_points_are_not_flagged() {
    let mut h = H::new(0x7034_000d);
    let cat = catalogue();
    let mut placed: Vec<(Sym, Value)> = Vec::new();
    for (i, e) in cat.iter().enumerate() {
        let v = place(&mut h, &e.key, slot(i));
        placed.push((e.clone(), v));
    }
    let d = h.doc();
    let flagged = unconnected(&d);
    for (e, v) in &placed {
        for (label, _, _) in &e.points {
            let t = term(v, label);
            let quiet = e.quiet.contains(label);
            assert_eq!(!flagged.contains(&t), quiet, "{} {label}: quiet {quiet}", e.key);
        }
    }
    // A starter that was never quiet still is flagged.
    let k = place(&mut h, "coil", (100.0, 1000.0));
    assert!(unconnected(&h.doc()).contains(&term(&k, "A1")));
    // Wiring a quiet pin changes nothing else: its neighbour stays quiet.
    let (e, v) = placed.iter().find(|(e, _)| e.key == "socket4").unwrap();
    let at = slot(cat.iter().position(|x| x.key == e.key).unwrap());
    h.run("wire", json!({"points": pts(&[(at.0, at.1 + 5.0), (at.0, at.1 + 30.0)])})).unwrap();
    let f = unconnected(&h.doc());
    assert!(!f.contains(&term(v, "1")) && !f.contains(&term(v, "2")));
}

// ---------------------------------------------------------------------------------------------
// AC 5: BOM rows

/// AC 5, §9, §13 check 10: one BOM row entry per device; contacts, `power4` and `ssrout` add none;
/// no new column; the rating is not in the BOM.
#[test]
fn k_bom_one_row_per_device() {
    let mut h = fixture_all(0x7034_000e);
    let d = h.doc();
    let devices: BTreeSet<ComponentId> =
        d.components.iter().filter(|(_, c)| c.properties.get("tag").is_some_and(|t| !t.is_empty())).map(|(id, _)| *id).collect();
    let bom = h.run("bom", json!({"limit": 5000})).unwrap();
    let rows = bom["rows"].as_array().unwrap().clone();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    // Brief 047 added `unit` and `entries`.
    let allowed = ["flags", "location", "qty", "unit", "tags", "description", "maker", "part_number", "devices", "accessory", "strip", "entries"];
    for r in &rows {
        for k in r.as_object().unwrap().keys() {
            assert!(allowed.contains(&k.as_str()), "new BOM column {k}: {r}");
        }
        for c in r["devices"].as_array().unwrap() {
            *seen.entry(c.as_str().unwrap().to_string()).or_default() += 1;
        }
    }
    for c in &devices {
        assert_eq!(
            seen.get(&c.hex()).copied(),
            Some(1),
            "{} on one BOM row: {rows:?}",
            d.components[c].properties.get("tag").cloned().unwrap_or_default()
        );
    }
    assert_eq!(seen.len(), devices.len(), "nothing but devices on the BOM: {seen:?}");
    let qty: u64 = rows.iter().map(|r| r["qty"].as_u64().unwrap_or(0)).sum();
    assert_eq!(qty as usize, devices.len());
    assert!(!bom.to_string().contains("30mA"), "the rating is not in the BOM");
}

/// Every family on one page: `-K01` coil + co + no + co + power4; `-K02` ssrin + ssrout; RCBO,
/// RCDs, fuse terminal, fuse bank with ratings, sensors (a limit switch with NO and NC), valves,
/// thermocouple, heater, connector halves, bulkheads, a DI card with two channels, a CPU.
fn fixture_all(seed: u64) -> H {
    let mut h = H::new(seed);
    let k1: ComponentId = id(&place(&mut h, "coil", (20.0, 250.0)), "component");
    join(&mut h, "co", (50.0, 250.0), k1).unwrap();
    join(&mut h, "no", (80.0, 250.0), k1).unwrap();
    join(&mut h, "co", (110.0, 250.0), k1).unwrap();
    join(&mut h, "power4", (160.0, 250.0), k1).unwrap();
    let k2: ComponentId = id(&place(&mut h, "ssrin", (220.0, 250.0)), "component");
    join(&mut h, "ssrout", (250.0, 250.0), k2).unwrap();
    let q: ComponentId = id(&place(&mut h, "rcbo2", (300.0, 250.0)), "component");
    setprop(&mut h, q, "rating", json!("13A C 30mA")).unwrap();
    place(&mut h, "rcd2", (350.0, 250.0));
    place(&mut h, "rcd4", (400.0, 250.0));
    place(&mut h, "fuseterm", (20.0, 180.0));
    let f: ComponentId = id(&place(&mut h, "fusebank6", (70.0, 180.0)), "component");
    setprop(&mut h, f, "rating.2", json!("2A")).unwrap();
    place(&mut h, "prox3", (200.0, 180.0));
    let b: ComponentId = id(&place(&mut h, "rollerno", (250.0, 180.0)), "component");
    join(&mut h, "rollernc", (280.0, 180.0), b).unwrap();
    place(&mut h, "floatno", (320.0, 180.0));
    place(&mut h, "interlocknc", (360.0, 180.0));
    place(&mut h, "valve", (20.0, 110.0));
    place(&mut h, "valve2", (80.0, 110.0));
    let tc: ComponentId = id(&place(&mut h, "tc", (140.0, 110.0)), "component");
    setprop(&mut h, tc, "rating", json!("K")).unwrap();
    place(&mut h, "heater", (180.0, 110.0));
    place(&mut h, "socket4", (220.0, 110.0));
    place(&mut h, "plug4", (260.0, 110.0));
    place(&mut h, "dsub9s", (300.0, 110.0));
    place(&mut h, "m12p5", (370.0, 110.0));
    place(&mut h, "rj45", (20.0, 40.0));
    place(&mut h, "usba", (100.0, 40.0));
    let a: ComponentId = id(&place(&mut h, "plcdi", (180.0, 40.0)), "component");
    join(&mut h, "di", (170.0, 0.0), a).unwrap();
    join(&mut h, "di", (190.0, 0.0), a).unwrap();
    place(&mut h, "plccpu", (250.0, 40.0));
    place(&mut h, "plcps", (300.0, 40.0));
    h
}

// ---------------------------------------------------------------------------------------------
// §13 check 11: save and reopen

/// §13 check 11: every device, label, rating and embedded definition is unchanged after save,
/// close and reopen at a path with spaces and `ä`; opening adds no transaction.
#[test]
fn l_save_and_reopen_keeps_everything() {
    let dir = tmp_dir("l");
    let mut h = fixture_all(0x7034_000f);
    let before = h.doc();
    let head = h.head();
    let bom = h.run("bom", json!({"limit": 5000})).unwrap();
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("Bibliothek -K01 (Rev A).ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    close_all(&mut h);
    drop(h);
    let mut h2 = reopen(&path, 0x7034_0010);
    assert_eq!(h2.head(), head, "open adds no transaction");
    assert_eq!(model_parts(&h2.doc()), model_parts(&before));
    assert_eq!(h2.run("bom", json!({"limit": 5000})).unwrap(), bom);
    // Placing again after reopen reuses the embedded definitions (found by origin).
    let defs = h2.doc().definitions.len();
    h2.check();
    place(&mut h2, "fusebank6", (100.0, 300.0));
    place(&mut h2, "co", (150.0, 300.0));
    assert_eq!(h2.doc().definitions.len(), defs, "reused by origin after reopen");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// AC 11 (project side) and question 5: prefixes

/// Question 5, AC 11: the new keys list their built-in prefixes in Tag Settings, and a project's
/// own prefix still wins over the built-in one for a newly placed device.
#[test]
fn n_default_prefixes_and_a_project_override() {
    let mut h = H::new(0x7034_0011);
    let v = h.run("tagsettings", json!({})).unwrap();
    for e in catalogue() {
        assert_eq!(v["prefixes"][&e.key], json!(format!("-{}", e.prefix)), "{}: {v}", e.key);
    }
    h.run("tagsettings", json!({"prefixes": {"valve": "Y", "fusebank8": "FU"}})).unwrap();
    assert_eq!(place(&mut h, "valve", (50.0, 150.0))["tag"], json!("-Y01"));
    assert_eq!(place(&mut h, "valve2", (100.0, 150.0))["tag"], json!("-MB01"), "only the key changed follows");
    assert_eq!(place(&mut h, "fusebank8", (200.0, 150.0))["tag"], json!("-FU01"));
    h.run("tagsettings", json!({"prefixes": {"valve": null}})).unwrap();
    assert_eq!(place(&mut h, "valve", (50.0, 50.0))["tag"], json!("-MB02"), "back to the built-in prefix");
}

// ---------------------------------------------------------------------------------------------
// AC 9: hostile input

/// AC 9, §12 item 5: unknown keys, oversized or control-character ratings, `rating.<n>` with a huge
/// or non-numeric `<n>`: each an error, never a panic, nothing written.
#[test]
fn o_hostile_input_never_panics() {
    let mut h = H::new(0x7034_0012);
    let f1: ComponentId = id(&place(&mut h, "fusebank4", (100.0, 150.0)), "component");
    let k1: ComponentId = id(&place(&mut h, "coil", (200.0, 150.0)), "component");
    let f = f1.hex();
    let huge = format!("rating.{}", "9".repeat(10_000));
    for (cmd, params) in [
        ("place", json!({"symbol": "nosuchsymbol", "at": [10, 10]})),
        ("place", json!({"symbol": "fusebank7", "at": [10, 10]})),
        ("place", json!({"symbol": "fusebank", "at": [10, 10]})),
        ("place", json!({"symbol": "socket99", "at": [10, 10]})),
        ("place", json!({"symbol": "dsub9", "at": [10, 10]})),
        ("place", json!({"symbol": "x".repeat(10_000), "at": [10, 10]})),
        ("place", json!({"symbol": "co\u{0}", "at": [10, 10]})),
        ("place", json!({"symbol": "co", "at": [10, 10], "device": "ffffffffffffffffffffffffffffffff"})),
        ("place", json!({"symbol": "co", "at": [1e300, 10]})),
        ("place", json!({"symbol": "fusebank12", "at": [10, 10], "rotation": "x"})),
        ("setprop", json!({"component": f, "key": huge, "value": "2A"})),
        ("setprop", json!({"component": f, "key": "rating.", "value": "2A"})),
        ("setprop", json!({"component": f, "key": "rating.abc", "value": "2A"})),
        ("setprop", json!({"component": f, "key": "rating.1e3", "value": "2A"})),
        ("setprop", json!({"component": f, "key": "rating.18446744073709551616", "value": "2A"})),
        ("setprop", json!({"component": f, "key": "rating.\u{1F642}", "value": "2A"})),
        ("setprop", json!({"component": f, "key": "rating.\u{0}", "value": "2A"})),
        ("setprop", json!({"component": f, "key": "rating.1", "value": "A".repeat(10_000)})),
        ("setprop", json!({"component": f, "key": "rating.1", "value": "2A\u{1b}[31m"})),
        ("setprop", json!({"component": f, "key": "rating.1", "value": 5})),
        ("setprop", json!({"component": k1.hex(), "key": "rating", "value": "A".repeat(10_000)})),
        ("setprop", json!({"component": k1.hex(), "key": "rating", "value": "\u{7f}"})),
        ("setprop", json!({"component": k1.hex(), "key": huge, "value": "2A"})),
    ] {
        let head = h.head();
        let r = h.run(cmd, params.clone());
        assert!(r.is_err(), "{cmd} {}: should be an error, got {r:?}", params.to_string().chars().take(120).collect::<String>());
        assert_eq!(h.head(), head, "{cmd}: nothing written");
    }
    // Long or odd queries are read safely.
    for q in ["x".repeat(10_000), "\u{0}".into(), "-".into(), "--".into(), "\u{1F642}".into()] {
        h.run("symbols", json!({"query": q})).unwrap();
    }
    // The catalogue is complete and every definition builds.
    for s in STARTERS {
        assert!(s.definition().is_ok(), "{} builds", s.key);
    }
}

// ---------------------------------------------------------------------------------------------
// §13 check 12: the 21 starters are unchanged

/// §13 check 12: no content hash of an existing starter changes. The hashes are the ones the
/// starters have on `daeb0652` (= `dev` `6729c8f1` plus docs), computed by compiling that
/// commit's `symbols.rs` beside the build, not copied from the worker's unit test.
#[test]
fn p_old_starters_keep_their_content_hash() {
    const PINNED: &[(&str, &str)] = &[
        ("coil", "a38469fe64864a4a602f678c8bb2fae900c64b75c555a3b58b65b98029437db3"),
        ("no", "82317c626ba389fa1deb1ecbefc1457e0b958da637780813198b4969e6d4cc31"),
        ("nc", "8861d1f37368786878b2c860ce0bec1e98f4b8fb4b264fdf6b897cd01a38717c"),
        ("power3", "7554cf8d849ccf964912b5ceef2248e342c00351cd386f88bd00f27aff9c599b"),
        ("mcb1", "60a708b75ab297e22e8ee0e3e5b99163660dbf7f941ca0eba4d7009095042ecb"),
        ("mcb2", "907da0b24254cc9520f359285988b9706420730a46e626c73abf20a3434ddd73"),
        ("fuse", "92a1eb42f8036488fa6e127903f9381488777bb1314c272c97c7fb6c565f0f93"),
        ("psu", "7ab24232dfdaa94a6b28556544b3be0b128146f6cd314f7a2e4fef86df03bc6c"),
        ("pbno", "6fcb99af92cd7290c81fb4e05291bef164c4354e358a959490d4b1dece901148"),
        ("pbnc", "eb97ba69342ae1de571c128a9086531e499581436fb5cc8c0f223c98d881479f"),
        ("sel", "ee747f4614ce340e0500223ec3f23a1463c80ef9ec42f5e425be6449dedf3023"),
        ("lamp", "654a895a70d7a93b61a1dfd279243afcd45628a7835bc7ca7baf3419e6af3e5a"),
        ("motor3", "74fba3648ea65faed6d0f2d6122333c8ddbe82593551300b8b39b84e2783db94"),
        ("motor1", "97d78f1600e761449dda417ba7a0d8818d1fc0a7576bd56f5a68bd2665714dcb"),
        ("trafo", "7546ea16e7c4b9d68527ee6e022ad642a825844ab7441c699c2200015d736016"),
        ("terminal", "140b3e17fdb18a4889a5816b1f3aa34f9c9714643d25f518470cd800310290a7"),
        ("terminalpe", "950f2596fe76913d2ac50cc894697b3ca80e10cfb3bcbe136dccbc61af844a3c"),
        ("di", "691b84067c9c64dd0ab9de2a9a6bef23fb05c71a485264cf463f245ef39fc1eb"),
        ("do", "736535bf22162769b6420f790efd350383060df36447dcee6d29cf1f86f1ff92"),
        ("junction", "8a6e36eab66ded66ec4ef470682d2d5cecf536a0d1fe6c148c632154232a4868"),
        ("continuation", "7289a1f63b563b3f0ceb6959fd7fb34763daf4c1c6e56bb2366e01129ae7afaf"),
    ];
    assert_eq!(PINNED.len(), 21, "pin every old starter");
    for (key, want) in PINNED {
        let s = symbols::by_key(key).unwrap();
        let o = s.definition().unwrap().origin.clone().unwrap();
        assert_eq!(o.content_hash.hex(), *want, "{key}");
    }
}
