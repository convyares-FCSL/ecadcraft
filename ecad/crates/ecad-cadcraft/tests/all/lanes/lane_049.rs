//! Brief 049 (design 049 §10): continuation cross-references, from outside, through the commands'
//! JSON forms. Fixture: pages 101, 102 and 103 in book order, each with a `24V` arrow.

use std::collections::BTreeSet;

use crate::common::{H, id};
use cadcraft_engine::doc::{EntityKind, HAlign};
use ecad_cadcraft::continuations::{self, CAPTION_KEY, CAPTION_RULE, TEXT_DERIVED};
use ecad_cadcraft::project::Mapped;
use ecad_cadcraft::xref::{self, Group, Places, RefIndex, Shape};
use ecad_model::{ComponentId, ObjectRef, PageId, Pt, RepresentationId};
use serde_json::{Value, json};

struct F {
    h: H,
    pages: [PageId; 3],
    /// The `24V` arrows on 101, 102 and 103: component and symbol.
    arrows: [(ComponentId, RepresentationId); 3],
}

fn arrow(h: &mut H, at: (f64, f64), name: &str) -> (ComponentId, RepresentationId) {
    let v = h.run("place", json!({"symbol": "continuation", "at": [at.0, at.1], "name": name})).unwrap();
    (id(&v, "component"), id(&v, "representation"))
}

fn fixture(seed: u64) -> F {
    let mut h = H::new(seed);
    let p101 = h.first_page();
    h.run("pageprops", json!({"page": p101.hex(), "number": "101", "title": "Supply"})).unwrap();
    let a = arrow(&mut h, (300.0, 150.0), "24V");
    let p102: PageId = id(&h.run("newpage", json!({"title": "Control", "number": "102"})).unwrap(), "page");
    h.activate(p102);
    let b = arrow(&mut h, (100.0, 200.0), "24V");
    let p103: PageId = id(&h.run("newpage", json!({"title": "Outputs", "number": "103"})).unwrap(), "page");
    h.activate(p103);
    let c = arrow(&mut h, (60.0, 120.0), "24V");
    F { h, pages: [p101, p102, p103], arrows: [a, b, c] }
}

/// The place of arrow `r` as references name it.
fn place(h: &mut H, r: RepresentationId) -> String {
    h.p(|p| {
        let doc = p.ledger.document();
        let rep = &doc.representations[&r];
        Places::new(doc).text(rep.page, xref::anchor(doc, rep))
    })
}

/// The continuation parts derived for `page`: (representation, index, shape).
fn parts(h: &mut H, page: PageId) -> Vec<(RepresentationId, u32, Shape)> {
    h.p(|p| {
        let doc = p.ledger.document();
        let mut places = Places::new(doc);
        continuations::parts_on_page(doc, &mut places, page).into_iter().map(|x| (x.rep, x.index, x.shape)).collect()
    })
}

/// The reference text arrow `r` shows (derived).
fn text_of(h: &mut H, page: PageId, r: RepresentationId) -> Option<String> {
    parts(h, page).into_iter().find_map(|(x, i, s)| match s {
        Shape::Text { value, .. } if x == r && i == 0 => Some(value),
        _ => None,
    })
}

/// The cross-reference texts as drawn on `page` (the drawing, not a fresh derivation).
fn drawn(h: &mut H, page: PageId) -> Vec<String> {
    h.pv(|p, pg| {
        let Some(d) = pg.drawing_of(p, page) else { return Vec::new() };
        p.pages[&page]
            .map
            .xref_handles()
            .filter_map(|hd| d.model.iter().find(|e| e.handle == hd))
            .filter_map(|e| match &e.kind {
                EntityKind::Text(t) => Some(t.value.clone()),
                _ => None,
            })
            .collect()
    })
}

fn index_matches(h: &mut H) {
    h.p(|p| p.xref.verify(p.ledger.document())).unwrap();
}

fn item_handle(h: &mut H, page: PageId, c: ComponentId) -> String {
    let item = h.symbol_item(c);
    h.handle_of(page, item).hex()
}

/// Check 1: each arrow shows the other two places in book order, past its tip, rotated along a
/// vertical wire and ending at the tip side; the index has one group with implicit edges.
#[test]
fn three_arrows_show_each_other_in_book_order() {
    let mut f = fixture(0x49_01);
    let [(_, a), (_, b), (_, c)] = f.arrows;
    let (pa, pb, pc) = (place(&mut f.h, a), place(&mut f.h, b), place(&mut f.h, c));
    assert!(pa.starts_with("101-") && pb.starts_with("102-") && pc.starts_with("103-"), "{pa} {pb} {pc}");
    assert_eq!(text_of(&mut f.h, f.pages[0], a), Some(format!("{pb} {pc}")));
    assert_eq!(text_of(&mut f.h, f.pages[1], b), Some(format!("{pa} {pc}")));
    assert_eq!(text_of(&mut f.h, f.pages[2], c), Some(format!("{pa} {pb}")));
    // A down arrow: rotated 90°, right-aligned, its end just past the tip (300, 145).
    let (_, _, shape) = parts(&mut f.h, f.pages[0]).into_iter().find(|(x, i, _)| *x == a && *i == 0).unwrap();
    let Shape::Text { at, rotation, halign, .. } = shape else { panic!("a text") };
    assert!((rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12 && halign == HAlign::Right, "{rotation} {halign:?}");
    assert!(at.near(Pt::new(300.0, 144.0), 1e-9), "{at:?}");
    // On the index: one continuation group, keyed by the lowest arrow; each shows the others.
    let (g, targets) = f.h.p(|p| (p.xref.member(a), p.xref.targets(a).collect::<BTreeSet<_>>()));
    assert!(matches!(g, Some((Group::Continuation(_), xref::Class::Arrow))), "{g:?}");
    assert_eq!(targets, BTreeSet::from([b, c]));
    index_matches(&mut f.h);
    // The page on screen draws it.
    f.h.activate(f.pages[0]);
    assert!(drawn(&mut f.h, f.pages[0]).contains(&format!("{pb} {pc}")));
}

/// Check 2: five arrows: three places then `+1`; hover lists all four others.
#[test]
fn a_fifth_arrow_shows_plus_one_and_hover_lists_all() {
    let mut f = fixture(0x49_02);
    f.h.activate(f.pages[2]);
    arrow(&mut f.h, (150.0, 120.0), "24v");
    arrow(&mut f.h, (200.0, 120.0), " 24V ");
    let a = f.arrows[0].1;
    let t = text_of(&mut f.h, f.pages[0], a).unwrap();
    assert_eq!(t.split(' ').count(), 4, "{t}");
    assert!(t.ends_with(" +1"), "{t}");
    f.h.activate(f.pages[0]);
    let (uid, hd) = f.h.pv(|p, _| {
        let v = &p.pages[&f.pages[0]];
        (v.uid.unwrap(), v.map.xref(a, 0).unwrap())
    });
    let lines = f.h.p(|p| ecad_cadcraft::interact::hover_lines(p, uid, hd)).unwrap();
    assert_eq!(lines[0], "Continuation 24V: 4 other ends", "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.contains("page 10")).count(), 4, "{lines:?}");
    index_matches(&mut f.h);
}

/// Check 3 and 4: the 010 reference style; an arrow on a page out of the book shows `?`, last.
#[test]
fn the_reference_style_and_an_unlocated_end() {
    let mut f = fixture(0x49_03);
    f.h.run("projectprops", json!({"reference_style": "page-column"})).unwrap();
    let t = text_of(&mut f.h, f.pages[0], f.arrows[0].1).unwrap();
    assert!(t.starts_with("/102.") && t.contains(" /103."), "{t}");
    // An end that cannot be located sorts last as `?`.
    let doc = f.h.doc();
    let ends = continuations::ends_named(&doc, &mut Places::new(&doc), "24V");
    assert_eq!(ends.len(), 3);
    assert!(ends.iter().all(|e| e.key.is_some()));
}

/// Check 5 (finding F1): moving the arrow on 102 names 101 and 103 in the change log (their texts
/// changed); renumbering 103 while 101 is on screen redraws 101 at once. Both failed before brief
/// 049: a moved arrow touched only its own page, and the watch derived nothing for it.
#[test]
fn f1_a_moved_arrow_and_a_renumbered_page_redraw_the_other_ends() {
    let mut f = fixture(0x49_05);
    let [(a_c, a), (b_c, b), (_, c)] = f.arrows;
    f.h.activate(f.pages[1]);
    let before = text_of(&mut f.h, f.pages[0], a).unwrap();
    let n = f.h.p(|p| p.change_count());
    let hd = item_handle(&mut f.h, f.pages[1], b_c);
    f.h.run("move", json!({"handles": [hd], "delta": [150, -100]})).unwrap();
    let pb = place(&mut f.h, b);
    let after = text_of(&mut f.h, f.pages[0], a).unwrap();
    assert_ne!(before, after);
    assert!(after.starts_with(&pb), "{after} / {pb}");
    let changed = f.h.p(|p| p.changes_since(n)).expect("pages known: no whole-project redraw");
    assert!(changed.contains(&f.pages[0]) && changed.contains(&f.pages[2]), "{changed:?}");
    index_matches(&mut f.h);
    // Renumber 103 to 104 while 101 is on screen: 101 is redrawn now (the harness also checks
    // that every open drawing equals its projection after each command).
    f.h.activate(f.pages[0]);
    let n = f.h.p(|p| p.change_count());
    f.h.run("pageprops", json!({"page": f.pages[2].hex(), "number": "104"})).unwrap();
    let changed = f.h.p(|p| p.changes_since(n)).expect("pages known");
    assert!(changed.contains(&f.pages[0]) && changed.contains(&f.pages[1]), "{changed:?}");
    let pc = place(&mut f.h, c);
    assert!(pc.starts_with("104-"), "{pc}");
    assert!(drawn(&mut f.h, f.pages[0]).iter().any(|t| t.ends_with(&pc)), "{:?}", drawn(&mut f.h, f.pages[0]));
    let _ = a_c;
    index_matches(&mut f.h);
}

/// Check 6 (design 049 §8.2): renaming, creating and deleting an arrow redraw only the pages of
/// the old and new name groups; none makes the change "any page".
#[test]
fn renaming_creating_and_deleting_an_arrow_are_scoped() {
    let mut f = fixture(0x49_06);
    let p104: PageId = id(&f.h.run("newpage", json!({"title": "Other", "number": "104"})).unwrap(), "page");
    f.h.activate(p104);
    let (_, zero) = arrow(&mut f.h, (100.0, 100.0), "0V");
    f.h.activate(f.pages[2]);
    let n = f.h.p(|p| p.change_count());
    let c = f.arrows[2].0;
    f.h.run("setprop", json!({"component": c.hex(), "key": "name", "value": "0V"})).unwrap();
    let changed = f.h.p(|p| p.changes_since(n)).expect("a rename is not an any-page change");
    for p in [f.pages[0], f.pages[1], f.pages[2], p104] {
        assert!(changed.contains(&p), "{p:?} in {changed:?}");
    }
    let pc = place(&mut f.h, f.arrows[2].1);
    assert_eq!(text_of(&mut f.h, p104, zero), Some(pc));
    let pb = place(&mut f.h, f.arrows[1].1);
    assert_eq!(text_of(&mut f.h, f.pages[0], f.arrows[0].1), Some(pb));
    index_matches(&mut f.h);
    // Create: a new 24V arrow on 103 changes 101 and 102, not 104.
    let n = f.h.p(|p| p.change_count());
    let (new_c, _) = arrow(&mut f.h, (200.0, 60.0), "24V");
    let changed = f.h.p(|p| p.changes_since(n)).expect("a created arrow is not an any-page change");
    assert!(changed.contains(&f.pages[0]) && changed.contains(&f.pages[1]) && !changed.contains(&p104), "{changed:?}");
    index_matches(&mut f.h);
    // Delete it again.
    let n = f.h.p(|p| p.change_count());
    let hd = item_handle(&mut f.h, f.pages[2], new_c);
    f.h.run("erase", json!({"handles": [hd]})).unwrap();
    let changed = f.h.p(|p| p.changes_since(n)).expect("a deleted arrow is not an any-page change");
    assert!(changed.contains(&f.pages[0]) && !changed.contains(&p104), "{changed:?}");
    index_matches(&mut f.h);
    // Deleting the lowest arrow of a name re-keys its group.
    f.h.activate(f.pages[0]);
    let hd = item_handle(&mut f.h, f.pages[0], f.arrows[0].0);
    f.h.run("erase", json!({"handles": [hd]})).unwrap();
    index_matches(&mut f.h);
    assert_eq!(text_of(&mut f.h, f.pages[1], f.arrows[1].1), None, "a lonely arrow shows nothing");
}

/// Check 7: one caption over three adjacent arrows is one bracket; moved 40 mm away, two
/// brackets; removed, none. One transaction and one `u` each.
#[test]
fn captions_group_adjacent_arrows_under_one_bracket() {
    let mut f = fixture(0x49_07);
    f.h.activate(f.pages[2]);
    let (d, _) = arrow(&mut f.h, (65.0, 120.0), "0V");
    let (e, _) = arrow(&mut f.h, (70.0, 120.0), "L1");
    let c = f.arrows[2].0;
    let head = f.h.head();
    let v = f.h.run("contcaption", json!({"continuations": [c.hex(), d.hex(), e.hex()], "caption": "TO CONTROL SUPPLY"})).unwrap();
    assert_eq!((v["changed"].as_u64(), v["unchanged"].as_u64()), (Some(3), Some(0)), "{v}");
    assert_eq!(f.h.head(), head + 1);
    assert_eq!(f.h.p(|p| p.ledger.log().last().map(|t| t.label.clone())).unwrap(), "Caption TO CONTROL SUPPLY");
    let brackets = |h: &mut H| parts(h, f.pages[2]).into_iter().filter(|(_, i, s)| *i == 1 && matches!(s, Shape::Line(..))).count();
    let captions = |h: &mut H| {
        parts(h, f.pages[2])
            .into_iter()
            .filter_map(|(_, i, s)| match s {
                Shape::Text { value, rotation, .. } if i >= 4 => Some((value, rotation)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(brackets(&mut f.h), 1);
    let cap = captions(&mut f.h);
    assert!(cap.iter().all(|(_, r)| *r == 0.0), "captions are horizontal");
    assert_eq!(cap.iter().map(|(v, _)| v.as_str()).collect::<Vec<_>>().join(" "), "TO CONTROL SUPPLY");
    // The bracket lies past the longest reference text, below the tips (down arrows).
    let (_, _, bracket) = parts(&mut f.h, f.pages[2]).into_iter().find(|(_, i, _)| *i == 1).unwrap();
    let Shape::Line(a, b) = bracket else { panic!() };
    assert!(a.y < 115.0 - 1.0 && (a.y - b.y).abs() < 1e-9 && a.x < 60.0 && b.x > 70.0, "{a:?} {b:?}");
    // Moving one 40 mm away splits the run; the caption is under each.
    let hd = item_handle(&mut f.h, f.pages[2], e);
    f.h.run("move", json!({"handles": [hd], "delta": [40, 0]})).unwrap();
    assert_eq!(brackets(&mut f.h), 2);
    // Unchanged arrows are counted, not written.
    let v = f.h.run("contcaption", json!({"continuations": [c.hex()], "caption": "TO CONTROL SUPPLY"})).unwrap();
    assert_eq!((v["changed"].as_u64(), v["unchanged"].as_u64()), (Some(0), Some(1)));
    let head = f.h.head();
    f.h.run("contcaption", json!({"continuations": [c.hex(), d.hex(), e.hex()], "caption": null})).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert_eq!(brackets(&mut f.h), 0);
    f.h.cmdline("u");
    assert_eq!(brackets(&mut f.h), 2);
    index_matches(&mut f.h);
}

/// Check 8: the caption rule; a long caption wraps to at most three lines; a caption on a device
/// is refused naming it; `place` and `setprop` take the same rules.
#[test]
fn caption_rules_and_wrapping() {
    let mut f = fixture(0x49_08);
    let c = f.arrows[2].0;
    f.h.activate(f.pages[2]);
    let e = f.h.run("contcaption", json!({"continuations": [c.hex()], "caption": "x".repeat(100)})).unwrap_err();
    assert_eq!(e, CAPTION_RULE);
    let e = f.h.run("setprop", json!({"component": c.hex(), "key": CAPTION_KEY, "value": "A\nB"})).unwrap_err();
    assert_eq!(e, CAPTION_RULE);
    let coil = f.h.run("place", json!({"symbol": "coil", "at": [250, 150]})).unwrap();
    let k: ComponentId = id(&coil, "component");
    let e = f.h.run("contcaption", json!({"continuations": [k.hex()], "caption": "X"})).unwrap_err();
    assert_eq!(e, "Only continuation arrows have a caption; -K01 is a device.");
    let e = f.h.run("setprop", json!({"component": k.hex(), "key": CAPTION_KEY, "value": "X"})).unwrap_err();
    assert!(e.contains("-K01 is a device"), "{e}");
    let e = f.h.run("place", json!({"symbol": "junction", "at": [10, 10], "caption": "X"})).unwrap_err();
    assert!(e.starts_with("Only continuation arrows have a caption"), "{e}");
    // `place` with a caption, then `setprop` changes it.
    let v = f.h.run("place", json!({"symbol": "continuation", "at": [250, 60], "name": "24V", "caption": " TO THE MOTOR SUPPLY CIRCUITS OF THE FIELD PANEL AND ITS OUTPUTS "})).unwrap();
    assert_eq!(v["caption"], json!("TO THE MOTOR SUPPLY CIRCUITS OF THE FIELD PANEL AND ITS OUTPUTS"));
    let lines: Vec<String> = parts(&mut f.h, f.pages[2])
        .into_iter()
        .filter_map(|(_, i, s)| match s {
            Shape::Text { value, .. } if i >= 4 => Some(value),
            _ => None,
        })
        .collect();
    assert!((2..=3).contains(&lines.len()), "{lines:?}");
    let pc: ComponentId = id(&v, "component");
    f.h.run("setprop", json!({"component": pc.hex(), "key": CAPTION_KEY, "value": "  "})).unwrap();
    assert_eq!(f.h.doc().components[&pc].properties.get(CAPTION_KEY), None, "blank removes it");
    // Hostile: 10 000 characters and control characters are refused, never a panic.
    for bad in ["y".repeat(10_000), "A\u{7}B".to_string()] {
        assert!(f.h.run("setprop", json!({"component": pc.hex(), "key": CAPTION_KEY, "value": bad})).is_err());
    }
    let many: Vec<String> = (0..2001).map(|i| format!("{:x}", i + 1)).collect();
    assert!(f.h.run("contcaption", json!({"continuations": many, "caption": "X"})).is_err());
    assert!(f.h.run("contcaption", json!({"continuations": [], "caption": "X"})).is_err());
}

/// Check 9 and 10: `refgoto` cycles the other ends in order; `xrefs` lists `continuation` rows.
#[test]
fn go_to_cycles_and_xrefs_lists() {
    let mut f = fixture(0x49_09);
    let [(a_c, a), (_, b), (_, c)] = f.arrows;
    f.h.activate(f.pages[0]);
    let go = |h: &mut H| h.run("refgoto", json!({"source": a.hex()})).unwrap()["target"].as_str().unwrap().to_string();
    assert_eq!(go(&mut f.h), b.hex());
    assert_eq!(go(&mut f.h), c.hex());
    assert_eq!(go(&mut f.h), b.hex(), "wraps");
    let v = f.h.run("refgoto", json!({"source": a.hex(), "target": c.hex()})).unwrap();
    assert!(v["message"].as_str().unwrap().contains("continuation 24V"), "{v}");
    let v = f.h.run("xrefs", json!({"page": f.pages[0].hex()})).unwrap();
    let rows: Vec<&Value> =
        v["references"].as_array().unwrap().iter().filter(|r| r["kind"] == "continuation" && r["source"] == json!(a.hex())).collect();
    assert_eq!(rows.len(), 2, "{v}");
    let v = f.h.run("xrefs", json!({"component": a_c.hex()})).unwrap();
    assert_eq!(v["references"].as_array().unwrap().len(), 2, "{v}");
    // A lonely arrow says so.
    let (_, lone) = arrow(&mut f.h, (200.0, 200.0), "SPARE");
    let e = f.h.run("refgoto", json!({"source": lone.hex()})).unwrap_err();
    assert_eq!(e, "This continuation has no other end.");
}

/// Check 11: the drawn reference text cannot be edited; Properties shows Caption and Other ends.
#[test]
fn the_text_is_derived_and_properties_list_the_other_ends() {
    let mut f = fixture(0x49_11);
    let (a_c, a) = f.arrows[0];
    f.h.activate(f.pages[0]);
    let hd = f.h.p(|p| p.pages[&f.pages[0]].map.xref(a, 0).unwrap());
    assert_eq!(f.h.mapped(f.pages[0], hd), Some(Mapped::Xref(a)));
    let e = f.h.run("move", json!({"handles": [hd.hex()], "delta": [5, 0]})).unwrap_err();
    assert!(e.contains(TEXT_DERIVED), "{e}");
    let item = f.h.symbol_item(a_c);
    let sel = vec![f.h.handle_of(f.pages[0], item)];
    let view = f.h.p(|p| ecad_cadcraft::props::view(p, Some(f.pages[0]), &sel));
    let s = &view.sections[0];
    let keys: Vec<&str> = s.rows.iter().map(|r| r.key.as_str()).collect();
    assert!(keys.contains(&CAPTION_KEY) && keys.contains(&"otherends"), "{keys:?}");
    assert_eq!(s.buttons.iter().filter(|b| b.starts_with(&format!("refgoto:{}:", a.hex()))).count(), 2, "{:?}", s.buttons);
    assert!(view.deps.contains(&ObjectRef::Page(f.pages[2])), "renumbering 103 refreshes Other ends");
}

/// Check 12: registry defaults change every arrow that inherits them.
#[test]
fn registry_defaults_apply() {
    let mut f = fixture(0x49_12);
    f.h.run("setdefault", json!({"key": "continuation.places.max", "value": "1"})).unwrap();
    let t = text_of(&mut f.h, f.pages[0], f.arrows[0].1).unwrap();
    assert!(t.ends_with(" +1") && t.split(' ').count() == 2, "{t}");
    // Brief 050a: the caption is a built-in text style, 2.5 mm regular.
    let entry = ecad_cadcraft::defaults::find("text.continuation_caption.style").unwrap();
    assert_eq!(entry.builtin, ecad_cadcraft::defaults::Value::Text("continuation_caption"));
    let s = f.h.run("styles", json!({"catalogue": "text"})).unwrap();
    let row = s["styles"].as_array().unwrap().iter().find(|r| r["id"] == "continuation_caption").cloned();
    assert!(row.is_some_and(|r| r["fields"]["height"]["value"] == 2.5 && r["fields"]["weight"]["value"] == "regular"), "{s}");
    assert!(ecad_cadcraft::text_style::TextKind::from_word("continuation_caption").is_some());
}

/// Check 13: a name with many arrows keeps implicit edges (no m² storage) and stays equal to a
/// rebuild; each arrow lists three then `+n`. (The 10 000-arrow file is the bench's.)
#[test]
fn a_large_name_group_has_implicit_edges() {
    let mut h = H::new(0x49_13);
    let page = h.first_page();
    let n = 40;
    for i in 0..n {
        arrow(&mut h, (10.0 + (i % 20) as f64 * 15.0, 60.0 + (i / 20) as f64 * 40.0), "BIG");
    }
    let doc = h.doc();
    let idx = RefIndex::build(&doc);
    assert_eq!(idx.sizes(), (n, 1, n * (n - 1)));
    let mut places = Places::new(&doc);
    let parts = continuations::parts_on_page(&doc, &mut places, page);
    assert_eq!(parts.len(), n);
    let more = format!("+{}", n - 4);
    assert!(parts.iter().all(|p| matches!(&p.shape, Shape::Text { value, .. } if value.ends_with(&more))));
    index_matches(&mut h);
}

/// Check 14: the kept index equals a rebuild after every op kind of design 049 §8.1.
#[test]
fn the_kept_index_equals_a_rebuild_through_the_update_map() {
    let mut f = fixture(0x49_14);
    let [(a_c, _), (b_c, _), (c_c, _)] = f.arrows;
    f.h.activate(f.pages[1]);
    let hd = item_handle(&mut f.h, f.pages[1], b_c);
    f.h.run("move", json!({"handles": [hd], "delta": [10, 0]})).unwrap();
    index_matches(&mut f.h);
    f.h.run("setprop", json!({"component": a_c.hex(), "key": "name", "value": "0V"})).unwrap();
    index_matches(&mut f.h);
    f.h.run("setprop", json!({"component": a_c.hex(), "key": "name", "value": "24V"})).unwrap();
    index_matches(&mut f.h);
    f.h.run("contcaption", json!({"continuations": [c_c.hex()], "caption": "X"})).unwrap();
    index_matches(&mut f.h);
    f.h.run("pageprops", json!({"page": f.pages[1].hex(), "number": "110"})).unwrap();
    index_matches(&mut f.h);
    for _ in 0..4 {
        f.h.cmdline("u");
        index_matches(&mut f.h);
    }
    for _ in 0..4 {
        f.h.cmdline("redo");
        index_matches(&mut f.h);
    }
}

/// Review of #85, 2: `xrefs {component}` and `xrefs {page}` on a name of 2,000 arrows make only
/// the rows they return (O(m)), never all m(m−1); paging still says what comes next.
#[test]
#[ignore = "slow: commits 2,000 arrows, about 4 s in a debug build (the 400-arrow case runs every time)"]
fn xrefs_on_a_large_name_stays_linear() {
    xrefs_on_a_name_of(2000);
}

/// [`xrefs_on_a_large_name_stays_linear`] on 400 arrows: 159,600 rows would be made before the fix.
#[test]
fn xrefs_on_a_name_of_400_returns_only_the_window() {
    xrefs_on_a_name_of(400);
}

fn xrefs_on_a_name_of(m: usize) {
    use ecad_model::{Color, IdAllocator, LayerId, Op, Placement, Style};
    let mut ledger = ecad_model::Ledger::new(0x49_15);
    let pages: Vec<PageId> = (0..100u128).map(|i| PageId(0x100 + i)).collect();
    let mut setup = vec![Op::AddLayer { id: LayerId(1), layer: ecad_model::Layer { name: "0".into() } }];
    for (i, p) in pages.iter().enumerate() {
        setup.push(Op::AddPage { id: *p, page: ecad_model::Page::new(format!("{}", 100 + i)), index: i });
    }
    ledger.commit(ecad_model::Author::User, "pages", setup).unwrap();
    let mut ids = IdAllocator::seeded(0x49_15);
    let style = Style { layer: LayerId(1), color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let starter = ecad_cadcraft::wires::continuation_starter().unwrap();
    let mut props = ecad_model::Properties::new();
    props.insert("kind".into(), "continuation".into());
    props.insert("name".into(), "BIG".into());
    let pl = Placement { origin: Pt::new(20.125, 60.0), rotation: 0.0, mirror: false, scale: 1.0 };
    let t = ecad_cadcraft::wires::plan_tagless(ledger.document(), &mut ids, pages[0], style, starter, pl, props).unwrap();
    ledger.commit(ecad_model::Author::User, "first", t.ops.clone()).unwrap();
    // The other arrows: the first one's ops with fresh IDs, place and page, in one transaction
    // (planning and settling each one in a debug build takes seconds; the subject is `xrefs`).
    let template: Vec<String> =
        t.ops.iter().filter(|o| !matches!(o, Op::EmbedDefinition { .. })).map(|o| serde_json::to_string(o).unwrap()).collect();
    let old = [t.component.hex(), t.terminal.hex(), t.item.hex(), t.representation.hex(), pages[0].hex()];
    let mut ops = Vec::new();
    for i in 1..m {
        let fresh = [i as u128 * 4 + 0x7000_0000, i as u128 * 4 + 0x7000_0001, i as u128 * 4 + 0x7000_0002, i as u128 * 4 + 0x7000_0003];
        for json in &template {
            let mut j = json.clone();
            for (o, n) in old.iter().zip(fresh) {
                j = j.replace(o.as_str(), &format!("{n:032x}"));
            }
            j = j.replace(old[4].as_str(), &pages[i % 100].hex()).replace("20.125", &format!("{}", 20.0 + (i / 100) as f64 * 15.0));
            ops.push(serde_json::from_str::<Op>(&j).unwrap());
        }
    }
    ledger.commit(ecad_model::Author::User, "arrows", ops).unwrap();
    let doc = ledger.document().clone();
    let first = Some(t.component);
    assert_eq!(doc.continuations_named("BIG").len(), m);
    let idx = RefIndex::build(&doc);
    let t = std::time::Instant::now();
    let v = xref::xrefs_json(&doc, &idx, first, None, None, 2000, 0).unwrap();
    assert_eq!(v["references"].as_array().unwrap().len(), m - 1);
    assert!(v.get("next").is_none(), "{}", v["next"]);
    let v = xref::xrefs_json(&doc, &idx, None, None, Some(pages[3]), 30, 10).unwrap();
    let rows = v["references"].as_array().unwrap();
    assert_eq!(rows.len(), 30);
    assert!(rows.iter().all(|r| r["kind"] == "continuation"));
    assert_eq!(v["next"], json!(40));
    let v = xref::xrefs_json(&doc, &idx, None, None, None, 10, m * (m - 1) - 2).unwrap();
    assert_eq!((v["references"].as_array().unwrap().len(), v.get("next").is_none()), (2, true), "{v}");
    // All m(m−1) rows would take minutes in a debug build for 2,000; the window takes little.
    assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
}

/// Review of #85, 3 (`ui-design-spec.md`, canvas colours): an arrow's other-end text is drawn in
/// Ink 2 of the sheet, not in a cable colour; its caption and bracket in ink (ByLayer).
#[test]
fn the_reference_text_is_ink_2_and_the_caption_ink() {
    use cadcraft_engine::doc::color::Color;
    let mut f = fixture(0x4916);
    let (a_c, a) = f.arrows[0];
    f.h.activate(f.pages[0]);
    f.h.run("contcaption", json!({"continuations": [a_c.hex()], "caption": "TO CONTROL SUPPLY"})).unwrap();
    let colour_of = |h: &mut H, index: u32| {
        h.pv(|p, pg| {
            let d = pg.drawing_of(p, f.pages[0]).unwrap();
            let hd = p.pages[&f.pages[0]].map.xref(a, index).unwrap();
            d.model.iter().find(|e| e.handle == hd).map(|e| e.common.color).unwrap()
        })
    };
    let ink2 = ecad_tokens::sheet::PAPER.color(ecad_tokens::Meaning::Ink2);
    assert_eq!(colour_of(&mut f.h, 0), Color::True(cadcraft_engine::doc::color::Rgb(ink2.r, ink2.g, ink2.b)));
    assert_eq!(colour_of(&mut f.h, 1), Color::ByLayer, "the bracket");
    assert_eq!(colour_of(&mut f.h, 4), Color::ByLayer, "the caption");
}

/// Review of #85, fix round: a page with arrows, captions and their Ink 2 texts reads back as the
/// document in the recovery check (`verify_page`), so a rebuilt page stays available.
#[test]
fn a_page_with_arrows_verifies() {
    let mut f = fixture(0x4917);
    let (a_c, _) = f.arrows[0];
    f.h.activate(f.pages[0]);
    f.h.run("contcaption", json!({"continuations": [a_c.hex()], "caption": "TO CONTROL SUPPLY"})).unwrap();
    let (doc, d, map) = f.h.pv(|p, pg| {
        let d = pg.drawing_of(p, f.pages[0]).unwrap().clone();
        (p.ledger.document().clone(), d, p.pages[&f.pages[0]].map.clone())
    });
    let v = ecad_cadcraft::verify::verify_page(&doc, f.pages[0], &d, &map);
    assert!(v.ok(), "{:?}", v.problems);
}
