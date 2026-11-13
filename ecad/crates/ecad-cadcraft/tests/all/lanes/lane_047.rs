//! Brief 047 (design 047 §12): BOM completeness. Part lines, items and article numbers through
//! `partline`, `bomitem`, `article` and `setprop`; the flat and nested layouts; the Checks rows;
//! the update map (the kept model equals a rebuild after every step); hostile input; save and
//! reopen.

use std::path::PathBuf;

use crate::common::{H, id};

use cadcraft_engine::Session;
use ecad_cadcraft::outputs::{BomView, Group, Outputs};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::ComponentId;
use serde_json::{Value, json};

/// The fixture of design 047 §12: contactors -K03…-K06, an 8-way fuse bank -F01, a strip -X05
/// with six positions (fuse positions 1, 5 and 6), project location +CP1.
struct F {
    h: H,
    k: Vec<ComponentId>,
    f01: ComponentId,
    x05: ComponentId,
}

fn place(h: &mut H, params: Value) -> ComponentId {
    id(&h.run("place", params).unwrap(), "component")
}

fn fixture(seed: u64) -> F {
    let mut h = H::new(seed);
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    let mut k = Vec::new();
    for (i, tag) in ["K03", "K04", "K05", "K06"].iter().enumerate() {
        let c = place(&mut h, json!({"symbol": "coil", "at": [40.0 + 30.0 * i as f64, 150], "tag": tag}));
        h.run("setprop", json!({"component": c.hex(), "key": "part_number", "value": "CT-9A"})).unwrap();
        h.run("setprop", json!({"component": c.hex(), "key": "maker", "value": "Generic"})).unwrap();
        h.run("setprop", json!({"component": c.hex(), "key": "description", "value": "Contactor"})).unwrap();
        k.push(c);
    }
    let f01 = place(&mut h, json!({"symbol": "fusebank8", "at": [100, 60], "tag": "F01"}));
    let x05 = place(&mut h, json!({"symbol": "termfuse", "at": [250, 200], "tag": "X05"}));
    for (i, sym) in ["terminal", "terminal", "terminal", "termfuse", "termfuse"].iter().enumerate() {
        place(&mut h, json!({"symbol": sym, "at": [270.0 + 20.0 * i as f64, 200], "device": x05.hex()}));
    }
    F { h, k, f01, x05 }
}

fn bom(h: &mut H, params: Value) -> Vec<Value> {
    h.run("bom", params).unwrap()["rows"].as_array().unwrap().clone()
}

fn row<'a>(rows: &'a [Value], pn: &str) -> &'a Value {
    rows.iter().find(|r| r["part_number"] == pn).unwrap_or_else(|| panic!("no row {pn}: {rows:#?}"))
}

fn checks(h: &mut H, kind: &str) -> Vec<Value> {
    let v = h.run("checks", json!({})).unwrap();
    v["rows"].as_array().or_else(|| v["checks"].as_array()).cloned().unwrap_or_default().into_iter().filter(|r| r["kind"] == kind).collect()
}

/// The kept model equals a fresh build in every layout (the update map left nothing behind).
fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    for article in [false, true] {
        for nested in [false, true] {
            let v = BomView { article, nested };
            for g in [Group::Location, Group::Part] {
                assert_eq!(kept.bom_table_with(g, v, false), built.bom_table_with(g, v, false), "{g:?} {v:?}");
                assert_eq!(kept.bom_table_with(g, v, true), built.bom_table_with(g, v, true), "{g:?} {v:?} csv");
            }
        }
    }
    assert_eq!(kept.bom_count(Group::Location), built.bom_count(Group::Location));
    assert_eq!(kept.bom_sheets(), built.bom_sheets());
    assert_eq!(kept.check_counts(), built.check_counts());
    let mut a = kept.checks();
    let mut b = built.checks();
    a.sort_by_key(|c| format!("{c:?}"));
    b.sort_by_key(|c| format!("{c:?}"));
    assert_eq!(a, b);
}

fn hexes(cs: &[ComponentId]) -> Vec<String> {
    cs.iter().map(|c| c.hex()).collect()
}

/// §12 1: with nothing new, 024's rows; `qty` numbers, `unit` pcs, no article field.
#[test]
fn a_plain_bom_reads_as_before_with_a_unit() {
    let mut f = fixture(0x47_01);
    let rows = bom(&mut f.h, json!({}));
    let r = row(&rows, "CT-9A");
    assert_eq!(r["qty"], 4);
    assert_eq!(r["unit"], "pcs");
    assert!(r.get("article_number").is_none(), "{r}");
    assert_eq!(r["tags"], "-K03, -K04, -K05, -K06");
    same_as_built(&mut f.h);
}

/// §12 2: two items of one part in one location add up in metres; the location is the
/// project's; each is one transaction; undo takes the second away.
#[test]
fn items_add_up_in_metres_and_undo() {
    let mut f = fixture(0x47_02);
    let head = f.h.head();
    let v = f.h.run("bomitem", json!({"description": "Slotted trunking 50x75", "qty": 3.6, "unit": "m", "part_number": "TR-5075"})).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert!(v["message"].as_str().unwrap().contains("3.6 m"), "{v}");
    f.h.run("bomitem", json!({"description": "Slotted trunking 50x75", "qty": "2", "unit": "m", "part_number": "TR-5075"})).unwrap();
    assert_eq!(f.h.head(), head + 2);
    let rows = bom(&mut f.h, json!({}));
    let r = row(&rows, "TR-5075");
    assert_eq!(r["qty"], json!(5.6));
    assert_eq!(r["unit"], "m");
    assert_eq!(r["location"], "+CP1");
    assert_eq!(r["tags"], "");
    assert_eq!(r["entries"].as_array().unwrap().len(), 2);
    assert_eq!(r["entries"][0]["item"], true);
    // The window shows metres in Qty; items after the tagged rows of their location.
    let t = f.h.cfg(|p| p.outputs().bom_table(Group::Location));
    let last = t.rows.iter().position(|c| c[5].text == "TR-5075").unwrap();
    assert_eq!(t.rows[last][1].text, "5.6 m");
    assert!(t.rows.iter().position(|c| c[2].text.contains("-K03")).unwrap() < last);
    same_as_built(&mut f.h);
    f.h.cmdline("u");
    let rows = bom(&mut f.h, json!({}));
    assert_eq!(row(&rows, "TR-5075")["qty"], json!(3.6));
    same_as_built(&mut f.h);
    // Pieces and metres of one part never add up.
    f.h.run("bomitem", json!({"description": "Slotted trunking 50x75", "qty": 2, "part_number": "TR-5075"})).unwrap();
    let rows = bom(&mut f.h, json!({}));
    assert_eq!(rows.iter().filter(|r| r["part_number"] == "TR-5075").count(), 2);
    same_as_built(&mut f.h);
}

/// §3: an item takes no tag and no part lines and keeps its description; edit and delete are
/// one transaction each.
#[test]
fn items_refuse_tags_and_lines_and_edit_and_delete() {
    let mut f = fixture(0x47_03);
    let item: ComponentId = id(&f.h.run("bomitem", json!({"description": "DIN rail 35x7.5", "qty": 2, "unit": "m"})).unwrap(), "item");
    let head = f.h.head();
    for (key, value, words) in [
        ("tag", json!("X9"), "An item has no tag"),
        ("part.1.maker", json!("Generic"), "An item has no part lines"),
        ("description", Value::Null, "An item needs a description"),
    ] {
        let e = f.h.run("setprop", json!({"component": item.hex(), "key": key, "value": value})).unwrap_err();
        assert!(e.contains(words), "{key}: {e}");
    }
    let e = f.h.run("partline", json!({"component": item.hex(), "add": {"part_number": "X"}})).unwrap_err();
    assert!(e.contains("partline is for devices and strips"), "{e}");
    assert_eq!(f.h.head(), head, "nothing written");
    // `setprop qty` follows the item's unit: 0.5 m is fine, 0.5 pieces is not.
    f.h.run("setprop", json!({"component": item.hex(), "key": "qty", "value": "0.5"})).unwrap();
    let e = f.h.run("bomitem", json!({"item": item.hex(), "set": {"unit": "pcs"}})).unwrap_err();
    assert!(e.contains("whole"), "{e}");
    f.h.run("bomitem", json!({"item": item.hex(), "set": {"qty": 3, "unit": "pcs", "location": "CP2"}})).unwrap();
    assert_eq!(f.h.head(), head + 2);
    let rows = bom(&mut f.h, json!({}));
    let r = rows.iter().find(|r| r["description"] == "DIN rail 35x7.5").unwrap();
    assert_eq!((r["qty"].clone(), r["unit"].clone(), r["location"].clone()), (json!(3), json!("pcs"), json!("+CP2")));
    same_as_built(&mut f.h);
    f.h.run("bomitem", json!({"item": item.hex(), "delete": true})).unwrap();
    assert!(!f.h.doc().components.contains_key(&item));
    assert!(bom(&mut f.h, json!({})).iter().all(|r| r["description"] != "DIN rail 35x7.5"));
    same_as_built(&mut f.h);
    f.h.cmdline("u");
    assert!(f.h.doc().components.contains_key(&item));
    same_as_built(&mut f.h);
}

/// §12 3: the same line on four contactors in one transaction is one row of four.
#[test]
fn one_line_on_four_devices_is_one_row() {
    let mut f = fixture(0x47_04);
    let head = f.h.head();
    let v =
        f.h.run("partline", json!({"components": hexes(&f.k), "add": {"part_number": "AB-1NO", "maker": "Generic", "description": "Aux block 1NO"}}))
            .unwrap();
    assert_eq!(f.h.head(), head + 1, "one transaction");
    assert_eq!(v["components"].as_array().unwrap().len(), 4);
    let label = f.h.p(|p| p.ledger.log().last().map(|t| t.label.clone())).unwrap();
    assert_eq!(label, "Add part line");
    let rows = bom(&mut f.h, json!({}));
    let r = row(&rows, "AB-1NO");
    assert_eq!(r["qty"], 4);
    assert_eq!(r["tags"], "-K03, -K04, -K05, -K06");
    assert_eq!(r["location"], "+CP1");
    // By part too.
    let rows = bom(&mut f.h, json!({"group": "part"}));
    assert_eq!(row(&rows, "AB-1NO")["location"], "+CP1 \u{d7}4");
    same_as_built(&mut f.h);
    // `setprop part.1.qty` is the same as `partline set`.
    f.h.run("setprop", json!({"component": f.k[0].hex(), "key": "part.1.qty", "value": "2"})).unwrap();
    assert_eq!(row(&bom(&mut f.h, json!({})), "AB-1NO")["qty"], 5);
    f.h.run("partline", json!({"component": f.k[0].hex(), "line": 1, "set": {"qty": null}})).unwrap();
    assert_eq!(row(&bom(&mut f.h, json!({})), "AB-1NO")["qty"], 4);
    // Remove takes every key of the line.
    f.h.run("partline", json!({"component": f.k[3].hex(), "line": 1, "remove": true})).unwrap();
    assert!(f.h.doc().components[&f.k[3]].properties.keys().all(|k| !k.starts_with("part.")));
    assert_eq!(row(&bom(&mut f.h, json!({})), "AB-1NO")["tags"], "-K03, -K04, -K05");
    same_as_built(&mut f.h);
    // A device's own part merges with another device's line of that part (Q6).
    f.h.run("setprop", json!({"component": f.k[3].hex(), "key": "part_number", "value": "AB-1NO"})).unwrap();
    f.h.run("setprop", json!({"component": f.k[3].hex(), "key": "description", "value": "Aux block 1NO"})).unwrap();
    let rows = bom(&mut f.h, json!({}));
    assert_eq!(row(&rows, "AB-1NO")["qty"], 4);
    same_as_built(&mut f.h);
}

/// §12 4 and 5, §9: a fuse insert line for three fuses; uncovered fuses counted once any insert
/// is listed; a fuse the bank lacks is refused in words.
#[test]
fn fuse_inserts_count_per_fuse_and_uncovered_fuses_are_checked() {
    let mut f = fixture(0x47_05);
    assert!(checks(&mut f.h, "fuse-no-insert").is_empty(), "not nagged before any insert is listed");
    let head = f.h.head();
    let e = f.h.run("partline", json!({"component": f.f01.hex(), "add": {"part_number": "FU-5T", "for": "13"}})).unwrap_err();
    assert!(e.contains("-F01 has no fuse 13. Its fuses are 1-8."), "{e}");
    let e = f.h.run("partline", json!({"component": f.k[0].hex(), "add": {"part_number": "FU-5T", "for": "1"}})).unwrap_err();
    assert!(e.contains("-K03 has nothing to name with for"), "{e}");
    assert_eq!(f.h.head(), head, "nothing written");
    f.h.run("partline", json!({"component": f.f01.hex(), "add": {"part_number": "FU-5T", "description": "Fuse 5A T 5x20", "for": "1,2,4"}})).unwrap();
    let rows = bom(&mut f.h, json!({}));
    let r = row(&rows, "FU-5T");
    assert_eq!(r["qty"], 3);
    assert_eq!(r["tags"], "-F01:1, -F01:2, -F01:4");
    assert_eq!(r["entries"][0]["sub"], json!(["1", "2", "4"]));
    let c = checks(&mut f.h, "fuse-no-insert");
    // -F01's other five, and -X05's fuse positions.
    let x05_fuses = f.h.p(|p| {
        let d = p.ledger.document();
        ecad_cadcraft::terminal_types::positions(d, f.x05).iter().filter(|p| p.ty.code == "fuse").count()
    });
    assert_eq!(x05_fuses, 3, "fixture: three fuse positions");
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["which"], "8 fuses have no insert");
    same_as_built(&mut f.h);
    // FP5 fallback: a strip line cannot name positions until renumbering keeps them; without
    // `for` it is a line of the strip as a whole.
    let head = f.h.head();
    let e = f.h.run("partline", json!({"component": f.x05.hex(), "add": {"part_number": "FU-5T", "for": "1"}})).unwrap_err();
    assert!(e.contains("-X05: a strip's part line cannot name positions with for yet"), "{e}");
    assert_eq!(f.h.head(), head);
    f.h.run("partline", json!({"component": f.x05.hex(), "add": {"part_number": "MK-1", "description": "Marker set"}})).unwrap();
    let r = row(&bom(&mut f.h, json!({})), "MK-1").clone();
    assert_eq!((r["qty"].clone(), r["tags"].clone()), (json!(1), json!("-X05")));
    same_as_built(&mut f.h);
    // `article` leaves strip parts alone and says so.
    f.h.run("setprop", json!({"component": f.x05.hex(), "key": "part_number", "value": "FU-5T"})).unwrap();
    assert!(bom(&mut f.h, json!({})).iter().any(|r| r["part_number"] == "FU-5T" && r["tags"] == "-X05"), "the strip's terminals use it");
    let v = f.h.run("article", json!({"part_number": "FU-5T", "article_number": "A-0400"})).unwrap();
    assert!(v["message"].as_str().unwrap().contains("Strip terminal, accessory and jumper parts with this part number are not changed yet"), "{v}");
}

/// §12 6 and 7, §4.3: the article column off by default, on by the registry; `article` sets
/// every entry of a part; two article numbers are flagged and fixed in one transaction.
#[test]
fn article_numbers_column_command_and_conflict() {
    let mut f = fixture(0x47_06);
    f.h.run("partline", json!({"components": hexes(&f.k), "add": {"part_number": "AB-1NO", "maker": "Generic", "description": "Aux block 1NO"}}))
        .unwrap();
    let rows = bom(&mut f.h, json!({}));
    assert!(row(&rows, "AB-1NO").get("article_number").is_none());
    // `columns` shows it for one call without writing anything.
    let head = f.h.head();
    assert_eq!(row(&bom(&mut f.h, json!({"columns": ["article"]})), "AB-1NO")["article_number"], "");
    assert_eq!(f.h.head(), head);
    f.h.run("setdefault", json!({"key": "bom.column.article", "level": "project", "value": "on"})).unwrap();
    let head = f.h.head();
    let v = f.h.run("article", json!({"part_number": "ab-1no", "article_number": "A-0110"})).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert_eq!(v["message"], "Set A-0110 on 4 parts (4 lines).");
    assert_eq!(row(&bom(&mut f.h, json!({})), "AB-1NO")["article_number"], "A-0110");
    let t = f.h.cfg(|p| p.outputs().bom_table(Group::Location));
    assert_eq!(t.headers.last(), Some(&"Article number"));
    same_as_built(&mut f.h);
    // A second value on one entry: flagged, checked, fixed.
    f.h.run("setprop", json!({"component": f.k[2].hex(), "key": "part.1.article_number", "value": "A-0111"})).unwrap();
    let r = row(&bom(&mut f.h, json!({})), "AB-1NO").clone();
    assert_eq!(r["article_number"], Value::Null, "flagged cells read null: {r}");
    assert!(r["flags"].as_array().unwrap().contains(&json!("article_number")));
    let t = f.h.cfg(|p| p.outputs().bom_table(Group::Location));
    let cell = t.rows.iter().find(|c| c[5].text == "AB-1NO").unwrap()[6].clone();
    assert_eq!(cell.text, "A-0110");
    assert_eq!(cell.flag.as_deref(), Some("-K03 and -K05 have different article numbers (A-0110, A-0111)."));
    let c = checks(&mut f.h, "article-conflict");
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["which"], "AB-1NO has two article numbers: A-0110 (3), A-0111 (1)");
    assert_eq!(c[0]["severity"], "warning");
    same_as_built(&mut f.h);
    let fixes = f.h.cfg(|p| {
        let o = p.outputs();
        let fix = o.checks().into_iter().find(|c| c.kind == ecad_cadcraft::checks::Kind::ArticleConflict).and_then(|c| c.fix).unwrap();
        ecad_cadcraft::outputs::accessory_fix_commands(o, fix)
    });
    assert_eq!(fixes.len(), 1);
    assert_eq!(fixes[0].0, "Use A-0110 for all");
    let head = f.h.head();
    f.h.run(fixes[0].1, fixes[0].2.clone()).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert!(checks(&mut f.h, "article-conflict").is_empty());
    assert_eq!(row(&bom(&mut f.h, json!({})), "AB-1NO")["article_number"], "A-0110");
    // A device's own article number, an item's; nothing matches: refused.
    f.h.run("bomitem", json!({"description": "Contactor spare", "part_number": "CT-9A", "maker": "Generic"})).unwrap();
    let v = f.h.run("article", json!({"part_number": "CT-9A", "maker": "generic", "article_number": "A-0102"})).unwrap();
    assert_eq!(v["message"], "Set A-0102 on 5 parts (4 devices, 1 item).");
    let e = f.h.run("article", json!({"part_number": "NOPE", "article_number": "A"})).unwrap_err();
    assert!(e.contains("No part NOPE in this project."), "{e}");
    // The CSV has the column; off again, it has none.
    let dir = std::env::temp_dir().join(format!("ecad-047-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bom.csv");
    f.h.run("bom", json!({"path": path.to_string_lossy()})).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("\u{feff}Location,Qty,Unit,Tags,Description,Maker,Part number,Article number\r\n"), "{text}");
    f.h.run("setdefault", json!({"key": "bom.column.article", "level": "project", "value": "off"})).unwrap();
    f.h.run("bom", json!({"path": path.to_string_lossy()})).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("\u{feff}Location,Qty,Unit,Tags,Description,Maker,Part number\r\n"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
    same_as_built(&mut f.h);
}

/// §12 8 and 9: a kit is one transaction; the nested layout lists a device's lines under it,
/// unmerged, with `↳` and the kit; By part ignores it; the BOM page's sheet count follows.
#[test]
fn kits_and_the_nested_layout() {
    let mut f = fixture(0x47_08);
    let head = f.h.head();
    let v =
        f.h.run(
            "partline",
            json!({"component": f.k[0].hex(), "kit": "Starter set", "add": [
                {"part_number": "SUP-1", "maker": "Generic", "description": "Suppressor"},
                {"part_number": "LBL-1", "maker": "Generic", "description": "Label", "qty": 2},
            ]}),
        )
        .unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert_eq!(v["lines"], json!([1, 2]));
    let label = f.h.p(|p| p.ledger.log().last().map(|t| t.label.clone())).unwrap();
    assert_eq!(label, "Add kit Starter set");
    let props = f.h.doc().components[&f.k[0]].properties.clone();
    assert_eq!(props.get("part.2.kit").map(String::as_str), Some("Starter set"));
    // The other contactors take the same suppressor line, so flat merges it.
    f.h.run("partline", json!({"components": hexes(&f.k[1..]), "add": {"part_number": "SUP-1", "maker": "Generic", "description": "Suppressor"}}))
        .unwrap();
    assert_eq!(row(&bom(&mut f.h, json!({})), "SUP-1")["qty"], 4);
    same_as_built(&mut f.h);
    let flat_rows = bom(&mut f.h, json!({})).len();
    f.h.run("setdefault", json!({"key": "bom.lines", "level": "project", "value": "nested"})).unwrap();
    let t = f.h.cfg(|p| p.outputs().bom_table(Group::Location));
    let texts: Vec<String> = t.rows.iter().map(|c| format!("{}|{}|{}", c[1].text, c[2].text, c[3].text)).collect();
    let k03 = texts.iter().position(|t| t.contains("|-K03|")).unwrap_or_else(|| panic!("{texts:#?}"));
    assert_eq!(texts[k03 + 1], "1||  \u{21b3} Suppressor (kit: Starter set)", "{texts:#?}");
    assert_eq!(texts[k03 + 2], "2||  \u{21b3} Label (kit: Starter set)", "{texts:#?}");
    let rows = bom(&mut f.h, json!({}));
    assert!(rows.len() > flat_rows);
    assert!(rows.iter().any(|r| r["line"] == "2" && r["kit"] == "Starter set"), "{rows:#?}");
    // By part stays flat.
    assert_eq!(row(&bom(&mut f.h, json!({"group": "part"})), "SUP-1")["qty"], 4);
    let count = f.h.cfg(|p| p.outputs().bom_count(Group::Location));
    assert_eq!(count, t.rows.len());
    same_as_built(&mut f.h);
}

/// §9: a `for` name the device stopped having is kept, counted 0, checked and removed by the fix;
/// unread `part.` keys from a file made elsewhere are listed.
#[test]
fn missing_for_names_and_bad_keys_are_checked() {
    let mut f = fixture(0x47_09);
    f.h.run("partline", json!({"component": f.f01.hex(), "add": {"part_number": "FU-5T", "for": "1,7,8"}})).unwrap();
    // A file made elsewhere: a name the bank lacks, and keys ECAD does not read.
    f.h.cfg(|p| {
        let c = f.f01;
        let ops = vec![
            ecad_model::Op::SetProperty {
                object: ecad_model::ObjectRef::Component(c),
                key: "part.1.for".into(),
                before: Some("1,7,8".into()),
                after: Some("1,9".into()),
            },
            ecad_model::Op::SetProperty {
                object: ecad_model::ObjectRef::Component(c),
                key: "part.100.maker".into(),
                before: None,
                after: Some("X".into()),
            },
        ];
        p.ledger.commit(ecad_model::Author::User, "a file made elsewhere", ops).unwrap();
    });
    let rows = bom(&mut f.h, json!({}));
    assert_eq!(row(&rows, "FU-5T")["qty"], 1);
    let c = checks(&mut f.h, "part-for-missing");
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0]["which"], "-F01 line 1 names fuse 9, which -F01 does not have");
    let c = checks(&mut f.h, "bad-part-key");
    assert_eq!(c[0]["which"], "-F01 has part.100.maker, which ECAD does not read");
    let fixes = f.h.cfg(|p| {
        let o = p.outputs();
        let fix = o.checks().into_iter().find(|c| c.kind == ecad_cadcraft::checks::Kind::PartForMissing).and_then(|c| c.fix).unwrap();
        ecad_cadcraft::outputs::accessory_fix_commands(o, fix)
    });
    assert_eq!(fixes[0].0, "Remove 9");
    f.h.run(fixes[0].1, fixes[0].2.clone()).unwrap();
    assert_eq!(f.h.doc().components[&f.f01].properties.get("part.1.for").map(String::as_str), Some("1"));
    assert!(checks(&mut f.h, "part-for-missing").is_empty());
}

/// §12 11 (update map, §10): a part line edit recomputes only its device; an unrelated edit
/// leaves the BOM entries alone.
#[test]
fn a_part_line_edit_recomputes_one_device() {
    let mut f = fixture(0x47_0b);
    f.h.run("partline", json!({"components": hexes(&f.k), "add": {"part_number": "AB-1NO"}})).unwrap();
    let _ = f.h.cfg(|p| p.outputs().bom_count(Group::Location));
    f.h.run("partline", json!({"component": f.k[1].hex(), "line": 1, "set": {"part_number": "AB-1NC"}})).unwrap();
    let work = f.h.cfg(|p| p.outputs().last_work);
    assert_eq!(work, 1, "only -K04");
    same_as_built(&mut f.h);
    let revision = f.h.cfg(|p| p.outputs().revision);
    f.h.run("setdefault", json!({"key": "bom.item.unit", "level": "project", "value": "m"})).unwrap();
    let (work, rev) = f.h.cfg(|p| (p.outputs().last_work, p.outputs().revision));
    assert_eq!(work, 0, "a report setting recomputes no row");
    assert!(rev > revision);
    // The new item takes the registry's unit.
    f.h.run("bomitem", json!({"description": "Cable tray", "qty": "1.5"})).unwrap();
    let rows = bom(&mut f.h, json!({}));
    assert_eq!(rows.iter().find(|r| r["description"] == "Cable tray").unwrap()["unit"], "m");
    same_as_built(&mut f.h);
}

/// §12 12 (§7.5 4): hostile values are refused in words, never a panic, and write nothing.
#[test]
fn hostile_values_are_refused_in_words() {
    let mut f = fixture(0x47_0c);
    let k = f.k[0].hex();
    let head = f.h.head();
    let long = "x".repeat(10_000);
    for q in [json!("NaN"), json!(-1), json!(1e308), json!("0.0001"), json!(long), json!("1.5")] {
        let e = f.h.run("partline", json!({"component": k, "add": {"part_number": "P", "qty": q}})).unwrap_err();
        assert!(!e.is_empty());
        let e = f.h.run("bomitem", json!({"description": "D", "qty": q})).unwrap_err();
        assert!(!e.is_empty());
    }
    let e = f.h.run("partline", json!({"component": f.f01.hex(), "add": {"part_number": "P", "for": "1-99999"}})).unwrap_err();
    assert!(e.contains("at most 999"), "{e}");
    for key in ["part.0.maker", "part.007.maker", "part.100.maker", "part.1.colour"] {
        let e = f.h.run("setprop", json!({"component": k, "key": key, "value": "X"})).unwrap_err();
        assert!(e.contains("is not a part line key"), "{key}: {e}");
    }
    for bad in
        [json!({"components": [k], "add": []}), json!({"component": k, "add": {"nope": 1}}), json!({"component": k, "line": 0, "remove": true})]
    {
        assert!(f.h.run("partline", bad).is_err());
    }
    let many: Vec<Value> = (0..21).map(|i| json!({"part_number": format!("P{i}")})).collect();
    assert!(f.h.run("partline", json!({"component": k, "add": many})).unwrap_err().contains("At most 20"));
    assert!(f.h.run("bomitem", json!({"description": long})).is_err());
    assert!(f.h.run("article", json!({"part_number": "CT-9A", "article_number": "x".repeat(65)})).is_err());
    assert_eq!(f.h.head(), head, "nothing written");
    // 99 lines, then the 100th is refused with the way on.
    for _ in 0..4 {
        let add: Vec<Value> = (0..20).map(|i| json!({"part_number": format!("P{i}")})).collect();
        f.h.run("partline", json!({"component": k, "add": add})).unwrap();
    }
    let add: Vec<Value> = (0..19).map(|i| json!({"part_number": format!("Q{i}")})).collect();
    f.h.run("partline", json!({"component": k, "add": add})).unwrap();
    let e = f.h.run("partline", json!({"component": k, "add": {"part_number": "R"}})).unwrap_err();
    assert!(e.contains("-K03 has 99 part lines; remove one first."), "{e}");
    same_as_built(&mut f.h);
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// §12 13: items, lines, kits, `for`, article numbers and the two report settings survive save
/// and reopen.
#[test]
fn everything_survives_save_and_reopen() {
    let mut f = fixture(0x47_0d);
    f.h.cfg(|p| p.persistence = Persistence::File);
    f.h.run(
        "partline",
        json!({"component": f.k[0].hex(), "kit": "Starter set", "add": [{"part_number": "SUP-1"}, {"part_number": "LBL-1", "qty": 2}]}),
    )
    .unwrap();
    f.h.run("partline", json!({"component": f.f01.hex(), "add": {"part_number": "FU-5T", "for": "1,2,4", "article_number": "A-0400"}})).unwrap();
    f.h.run("bomitem", json!({"description": "DIN rail", "qty": 2.5, "unit": "m", "article_number": "A-0301"})).unwrap();
    f.h.run("setdefault", json!({"key": "bom.column.article", "level": "project", "value": "on"})).unwrap();
    f.h.run("setdefault", json!({"key": "bom.lines", "level": "project", "value": "nested"})).unwrap();
    let before = bom(&mut f.h, json!({}));
    let dir = TempDir(std::env::temp_dir().join(format!("ecad-047-reopen-{}", ecad_store::disk::unique())));
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("bom047").display().to_string();
    f.h.run("saveas", json!({"path": path})).unwrap();
    while !f.h.s.docs.is_empty() {
        f.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &format!("{path}.ecad"), 0x47_0d).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let after = bom(&mut h, json!({}));
    let strip = |v: &[Value]| -> Vec<Value> {
        v.iter()
            .map(|r| {
                let mut r = r.clone();
                r.as_object_mut().unwrap().remove("devices");
                r.as_object_mut().unwrap().remove("entries");
                r
            })
            .collect()
    };
    assert_eq!(strip(&after), strip(&before));
    assert!(after.iter().any(|r| r["article_number"] == "A-0301" && r["qty"] == json!(2.5)));
}

/// Design 047 §6.3: the BOM-by-location page shows Qty with its unit, the Article column when it
/// is on, and nested lines indented; its sheet count follows the layout from kept counts.
#[test]
fn the_bom_page_shows_units_articles_and_nested_lines() {
    let mut f = fixture(0x47_0e);
    let page: ecad_model::PageId = id(&f.h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    f.h.run("bomitem", json!({"description": "DIN rail", "qty": 2.5, "unit": "m", "article_number": "A-0301"})).unwrap();
    f.h.run("partline", json!({"component": f.k[0].hex(), "kit": "Starter set", "add": {"part_number": "SUP-1", "description": "Suppressor"}}))
        .unwrap();
    let texts = |h: &mut H| -> Vec<String> {
        h.cfg(|p| {
            let o = p.outputs().clone();
            ecad_cadcraft::generated::parts_of(p.ledger.document(), page, Some(&o))
                .into_iter()
                .filter_map(|p| match p.kind {
                    cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
                    _ => None,
                })
                .collect()
        })
    };
    let t = texts(&mut f.h);
    assert!(t.contains(&"2.5 m".to_string()), "{t:?}");
    assert!(!t.contains(&"Article".to_string()) && !t.contains(&"A-0301".to_string()), "{t:?}");
    f.h.run("setdefault", json!({"key": "bom.column.article", "level": "project", "value": "on"})).unwrap();
    let t = texts(&mut f.h);
    assert!(t.contains(&"Article".to_string()) && t.contains(&"A-0301".to_string()), "{t:?}");
    f.h.run("setdefault", json!({"key": "bom.lines", "level": "project", "value": "nested"})).unwrap();
    let t = texts(&mut f.h);
    assert!(t.iter().any(|x| x.contains("\u{21b3} Suppressor (kit: Starter set)")), "{t:?}");
    same_as_built(&mut f.h);
}
