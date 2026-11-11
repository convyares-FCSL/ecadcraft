//! Brief 026 (design 026 §14): numbering from outside, through the commands' JSON forms: folder
//! and selection scopes, the drawn provisional colour and `24V-?` labels, the Checks rows kept
//! from each transaction's ops, save and reopen (tags, `tag_fixed`, the setting, the high-water
//! mark), and hostile requests. The MCP contract tests are in `ecad-mcp`'s `numbering_contracts`.

use crate::common;

use cadcraft_engine::Session;
use common::{H, id};
use ecad_cadcraft::devices::{self, PROVISIONAL_ACI, TextOf};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::sheet::Grid;
use ecad_model::{ComponentId, PageId};
use serde_json::{Value, json};

const SEED: u64 = 26;

/// The middle of column `col`, row `row` (0-based) of `page`'s frame.
fn cell(h: &mut H, page: PageId, col: u32, row: u32) -> [f64; 2] {
    h.p(|p| {
        let g = Grid::of(p.ledger.document(), page);
        [g.frame_left() + (f64::from(col) + 0.5) * g.column_width(), g.frame_top() - (f64::from(row) + 0.5) * g.row_height()]
    })
}

fn coil(h: &mut H, at: [f64; 2], tag: Option<&str>) -> ComponentId {
    let mut p = json!({"symbol": "coil", "at": at});
    if let Some(t) = tag {
        p["tag"] = json!(t);
    }
    id(&h.run("place", p).unwrap(), "component")
}

fn tag(h: &mut H, c: ComponentId) -> String {
    h.p(|p| devices::tag_words(p.ledger.document(), c))
}

/// Fixture N (design §14): page 1 has `-K07` at column A and `-K01` at column C; page 2, in folder
/// `Ctrl`, has `-K03` (fixed) at B, `-K02` at D and `-Q01`.
struct N {
    h: H,
    p1: PageId,
    k07: ComponentId,
    k01: ComponentId,
    k03: ComponentId,
    k02: ComponentId,
    q01: ComponentId,
    folder: String,
}

fn fixture() -> N {
    let mut h = H::new(SEED);
    let p1 = h.first_page();
    let (a, c) = (cell(&mut h, p1, 0, 1), cell(&mut h, p1, 2, 1));
    let k07 = coil(&mut h, a, Some("-K07"));
    let k01 = coil(&mut h, c, Some("-K01"));
    let folder = h.run("newfolder", json!({"name": "Ctrl"})).unwrap()["folder"].as_str().unwrap().to_string();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Control", "folder": folder})).unwrap(), "page");
    h.activate(p2);
    let (b, d, e) = (cell(&mut h, p2, 1, 1), cell(&mut h, p2, 3, 1), cell(&mut h, p2, 4, 2));
    let k03 = coil(&mut h, b, Some("-K03"));
    h.run("setprop", json!({"component": k03.hex(), "key": "tag_fixed", "value": "yes"})).unwrap();
    let k02 = coil(&mut h, d, Some("-K02"));
    let q01: ComponentId = id(&h.run("place", json!({"symbol": "mcb1", "at": e, "tag": "-Q01"})).unwrap(), "component");
    h.activate(p1);
    N { h, p1, k07, k01, k03, k02, q01, folder }
}

#[test]
fn a_renumber_devices_all_in_drawing_order_around_the_fixed_one() {
    let mut f = fixture();
    let pv = f.h.run("renumber", json!({"kind": "devices", "scope": "all", "preview": true})).unwrap();
    assert_eq!(pv["fixed"], json!(1), "{pv}");
    let head = f.h.head();
    f.h.run("renumber", json!({"kind": "devices", "scope": "all", "based_on": pv["change_count"]})).unwrap();
    assert_eq!(f.h.head(), head + 1, "one transaction");
    let tags: Vec<String> = [f.k07, f.k01, f.k03, f.k02, f.q01].iter().map(|c| tag(&mut f.h, *c)).collect();
    assert_eq!(tags, ["-K01", "-K02", "-K03", "-K04", "-Q01"]);
    // A new coil continues after the renumbered set.
    let at = cell(&mut f.h, f.p1, 5, 3);
    let k = coil(&mut f.h, at, None);
    assert_eq!(tag(&mut f.h, k), "-K05");
    f.h.run("u", Value::Null).unwrap();
    f.h.run("u", Value::Null).unwrap();
    let tags: Vec<String> = [f.k07, f.k01, f.k03, f.k02].iter().map(|c| tag(&mut f.h, *c)).collect();
    assert_eq!(tags, ["-K07", "-K01", "-K03", "-K02"], "u restores every tag");
    f.h.check();
}

#[test]
fn b_folder_and_prefix_scopes_change_only_their_items() {
    let mut f = fixture();
    f.h.run("renumber", json!({"kind": "devices", "scope": {"folder": f.folder}, "start": 1})).unwrap();
    // Page 2's -K02 is the only K to renumber there; -K01 (page 1) and -K03 (fixed) are taken.
    assert_eq!(tag(&mut f.h, f.k02), "-K02");
    assert_eq!((tag(&mut f.h, f.k07), tag(&mut f.h, f.k01)), ("-K07".into(), "-K01".into()));
    f.h.run("renumber", json!({"kind": "devices", "scope": {"prefix": "-K"}, "start": 10})).unwrap();
    assert_eq!(tag(&mut f.h, f.q01), "-Q01", "another prefix is never touched");
    assert_eq!(tag(&mut f.h, f.k07), "-K10");
    assert_eq!(tag(&mut f.h, f.k03), "-K03", "fixed");
    f.h.check();
}

#[test]
fn e_provisional_tags_are_drawn_orange_listed_in_checks_and_finalised() {
    let mut f = fixture();
    f.h.run("numbering", json!({"provisional": true})).unwrap();
    let at = cell(&mut f.h, f.p1, 6, 2);
    let k = coil(&mut f.h, at, None);
    assert_eq!(tag(&mut f.h, k), "-K?1");
    let p1 = f.p1;
    let texts = f.h.p(|p| devices::texts_on_page(p.ledger.document(), p1));
    let t = texts.iter().find(|t| t.of == TextOf::Tag && t.value == "-K?1").expect("the tag is drawn");
    assert_eq!(t.aci, Some(PROVISIONAL_ACI));
    // A new net with a potential reads 24V-?, provisional.
    let w = f.h.run("wire", json!({"points": [[400, 20], [440, 20]], "potential": "24V"})).unwrap();
    let w: ecad_model::ConductorId = id(&w, "conductor");
    let label = f.h.p(|p| ecad_model::wiring::label(p.ledger.document(), w)).unwrap();
    assert!(label.text.starts_with("24V-?") && label.provisional, "{label:?}");
    // Checks: the provisional row, kept from the ops (equal to a fresh build).
    let kinds = |h: &mut H| {
        let rows = h.cfg(|p| {
            let doc = p.ledger.document().clone();
            p.checks().sorted(&doc)
        });
        rows.iter().map(|r| r.kind.name().to_string()).collect::<Vec<_>>()
    };
    let kept = kinds(&mut f.h);
    assert!(kept.contains(&"provisional-tag".to_string()) && kept.contains(&"provisional-wires".to_string()), "{kept:?}");
    let fresh = f.h.p(|p| ecad_cadcraft::checks::all(p.ledger.document(), p.tolerance).iter().map(|r| r.kind.name().to_string()).collect::<Vec<_>>());
    assert_eq!(kept, fresh);
    f.h.run("finalise", json!({})).unwrap();
    assert_eq!(tag(&mut f.h, k), "-K08", "after the highest in use");
    let label = f.h.p(|p| ecad_model::wiring::label(p.ledger.document(), w)).unwrap();
    assert!(!label.text.contains('?'), "{label:?}");
    let kept = kinds(&mut f.h);
    assert!(!kept.iter().any(|k| k.starts_with("provisional")), "{kept:?}");
    f.h.check();
}

#[test]
fn h_save_and_reopen_keep_tags_fixed_flags_the_setting_and_the_high_water_mark() {
    let dir = std::env::temp_dir().join(format!("ecad-026-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut f = fixture();
    f.h.cfg(|p| p.persistence = Persistence::File);
    f.h.run("renumber", json!({"kind": "devices", "scope": "all"})).unwrap();
    f.h.run("numbering", json!({"provisional": true})).unwrap();
    let path = dir.join("n.ecad").display().to_string();
    f.h.run("saveas", json!({"path": path})).unwrap();
    let live = f.h.doc();
    while !f.h.s.docs.is_empty() {
        f.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 78).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(h2.doc(), live);
    assert!(ecad_model::numbering::provisional_on(&h2.doc()));
    assert_eq!(h2.doc().components[&f.k03].properties.get("tag_fixed").map(String::as_str), Some("yes"));
    // The reset mark survives: the next real tag follows the renumbered set.
    h2.run("numbering", json!({"provisional": false})).unwrap();
    let page = h2.first_page();
    let at = cell(&mut h2, page, 7, 4);
    let k = coil(&mut h2, at, None);
    assert_eq!(tag(&mut h2, k), "-K05");
    h2.check();
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn l_hostile_requests_are_refused_without_writing() {
    let mut f = fixture();
    let head = f.h.head();
    for params in [
        json!({"start": 0}),
        json!({"start": 1_000_000_000_000u64}),
        json!({"digits": 9}),
        json!({"scope": {"folder": "ffffffffffffffffffffffffffffffff"}}),
        json!({"scope": "selection"}),
        json!({"scope": "selection", "objects": ["zz"]}),
        json!({"kind": "everything"}),
        json!({"scope": {"pages": "x"}}),
    ] {
        assert!(f.h.run("renumber", params.clone()).is_err(), "{params}");
    }
    assert_eq!(f.h.head(), head);
    // Many provisional tags: each typed `-K?` takes the next one.
    f.h.run("numbering", json!({"provisional": true})).unwrap();
    for i in 1..=30u32 {
        let at = [2000.0 + f64::from(i) * 20.0, 2000.0];
        let k = coil(&mut f.h, at, Some("-K?"));
        assert_eq!(tag(&mut f.h, k), format!("-K?{i}"));
    }
    assert!(f.h.run("setprop", json!({"component": f.k07.hex(), "key": "tag", "value": "-K?"})).is_ok());
    let e = f.h.run("setprop", json!({"component": f.k07.hex(), "key": "tag_fixed", "value": "yes"})).unwrap_err();
    assert!(e.contains("provisional"), "{e}");
    f.h.check();
}
