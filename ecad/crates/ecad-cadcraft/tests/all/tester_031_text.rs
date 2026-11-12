//! Tester 031 (text styles, show and hide, placement; v0.15 lane T): acceptance checks from
//! outside, written from brief 031 and design 031 (§13 T1–T16) only, before the implementation
//! existed. They drive JSON commands (`textstyle`, `showtext`, `movetext`, `pagetexts`, `defaults`,
//! and the commands 030 already had), the interactive tools by their inputs, and the page drawing.
//!
//! un-ignored once the worker's branch is merged. A test that needs a stored value no command may
//! write (a hostile `text.*` value, `text.run.gap`) commits the `SetProperty` straight to the
//! ledger and then re-projects the page by switching pages, as a person would see it.
//!
//! Reading `pagetexts {page}` rows (design §10.4): `{representation, component, kind, terminal?,
//! value, at, justify, height, weight, shown, moved, overlaps, box: [[x,y],[x,y]]}`.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::doc::{Common, EntityKind, HAlign, Lineweight, Text, VAlign};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{Author, ComponentId, Document, ItemId, ObjectRef, Op, PageId, RepresentationId, Subject, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

// ---------------------------------------------------------------------------------------------
// Helpers

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap()
}

fn place_on(h: &mut H, symbol: &str, at: (f64, f64), device: ComponentId) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1], "device": device.hex()})).unwrap()
}

/// The representation that holds `item`.
fn rep_of_item(h: &mut H, item: ItemId) -> RepresentationId {
    let doc = h.doc();
    doc.representations.iter().find(|(_, r)| r.items.contains(&item)).map(|(id, _)| *id).expect("item in a representation")
}

fn rep_of(h: &mut H, placed: &Value) -> RepresentationId {
    let item: ItemId = id(placed, "item");
    rep_of_item(h, item)
}

fn texts(h: &mut H, page: PageId) -> Vec<Value> {
    let v = h.run("pagetexts", json!({"page": page.hex()})).unwrap_or_else(|e| panic!("pagetexts: {e}"));
    v["texts"].as_array().cloned().unwrap_or_default()
}

/// `pagetexts` without the harness checks (after a direct ledger commit, before re-projection).
fn texts_raw(h: &mut H, page: PageId) -> Vec<Value> {
    let v = h.s.execute("pagetexts", &json!({"page": page.hex()})).unwrap();
    v["texts"].as_array().cloned().unwrap_or_default()
}

fn of_kind<'a>(rows: &'a [Value], kind: &str) -> Vec<&'a Value> {
    rows.iter().filter(|r| r["kind"] == kind).collect()
}

fn row_of<'a>(rows: &'a [Value], rep: RepresentationId, kind: &str) -> &'a Value {
    rows.iter().find(|r| r["kind"] == kind && r["representation"] == rep.hex()).unwrap_or_else(|| panic!("no {kind} row for {}: {rows:?}", rep.hex()))
}

fn shown(r: &Value) -> bool {
    r["shown"] == json!(true)
}

/// The shown tag rows with value `tag`.
fn shown_tags<'a>(rows: &'a [Value], tag: &str) -> Vec<&'a Value> {
    rows.iter().filter(|r| r["kind"] == "tag" && r["value"] == tag && shown(r)).collect()
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

fn at(r: &Value) -> (f64, f64) {
    (num(&r["at"][0]), num(&r["at"][1]))
}

type Bx = ((f64, f64), (f64, f64));

fn bx(r: &Value) -> Bx {
    let b = &r["box"];
    let (x0, y0, x1, y1) = (num(&b[0][0]), num(&b[0][1]), num(&b[1][0]), num(&b[1][1]));
    ((x0.min(x1), y0.min(y1)), (x0.max(x1), y0.max(y1)))
}

/// Open boxes overlap (touching edges do not count).
fn overlap(a: Bx, b: Bx) -> bool {
    a.0.0 < b.1.0 - 1e-6 && b.0.0 < a.1.0 - 1e-6 && a.0.1 < b.1.1 - 1e-6 && b.0.1 < a.1.1 - 1e-6
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

/// Every text entity of `page`'s drawing as it is on screen.
fn drawn(h: &mut H, page: PageId) -> Vec<(Text, Common)> {
    h.pv(|p, pg| {
        pg.drawing_of(p, page)
            .map(|d| {
                d.model
                    .iter()
                    .filter_map(|e| match &e.kind {
                        EntityKind::Text(t) => Some((t.clone(), e.common.clone())),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn drawn_value(h: &mut H, page: PageId, value: &str) -> Vec<(Text, Common)> {
    drawn(h, page).into_iter().filter(|(t, _)| t.value == value).collect()
}

/// The font of a text style in `page`'s drawing.
fn style_font(h: &mut H, page: PageId, style: &str) -> Option<String> {
    h.pv(|p, pg| pg.drawing_of(p, page).and_then(|d| d.text_styles.iter().find(|s| s.name.eq_ignore_ascii_case(style)).map(|s| s.font.clone())))
}

/// One write: exactly one transaction.
fn one_tx(h: &mut H, cmd: &str, params: Value) -> Value {
    let head = h.head();
    let v = h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    assert_eq!(h.head(), head + 1, "{cmd} {params}: one transaction ({v})");
    v
}

/// A refused write: the error contains `sentence` and nothing is recorded.
fn refused(h: &mut H, cmd: &str, params: Value, sentence: &str) {
    let head = h.head();
    let e = h.run(cmd, params.clone()).expect_err(&format!("{cmd} {params} should be refused"));
    assert!(e.contains(sentence), "{cmd} {params}: {e:?} should say {sentence:?}");
    assert_eq!(h.head(), head, "{cmd} {params}: refused writes nothing");
}

fn last_ops(h: &mut H) -> Vec<Op> {
    h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default())
}

fn last_label(h: &mut H) -> String {
    h.p(|p| p.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default())
}

fn set_project_prop(key: &str, value: &str, before: Option<String>) -> Op {
    Op::SetProperty { object: ObjectRef::Project, key: key.into(), before, after: Some(value.into()) }
}

/// Commit `ops` straight to the ledger (a stored value no command writes), then re-project
/// `page` by making `other` and then `page` the active page.
fn inject(h: &mut H, ops: Vec<Op>, page: PageId, other: PageId) {
    h.cfg(|p| p.ledger.commit(Author::User, "tester 031 stored value", ops)).unwrap();
    reproject(h, page, other);
}

fn reproject(h: &mut H, page: PageId, other: PageId) {
    h.activate(other);
    h.activate(page);
    h.check();
}

/// A second page to switch to; the first page stays active.
fn second_page(h: &mut H) -> PageId {
    let first = h.first_page();
    let p: PageId = id(&h.run("newpage", json!({"title": "Other"})).unwrap(), "page");
    h.activate(first);
    p
}

fn project_prop(h: &mut H, key: &str) -> Option<String> {
    h.doc().properties.get(key).cloned()
}

fn comp_props(h: &mut H, c: ComponentId, prefix: &str) -> BTreeMap<String, String> {
    h.doc().components[&c].properties.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.components, &d.terminals, &d.conductors, &d.cables, &d.pages, &d.page_order, &d.representations, &d.items, &d.properties, &d.settings)
}

fn textstyle_kind(v: &Value, kind: &str) -> Value {
    v["kinds"].as_array().and_then(|k| k.iter().find(|r| r["kind"] == kind).cloned()).unwrap_or_else(|| panic!("no kind {kind}: {v}"))
}

/// A strip of terminals on vertical wires at `xs` crossing the line y = `y`; returns the strip
/// and its terminals (added to `strip` when given).
fn strip_at(h: &mut H, xs: &[f64], y: f64, strip: Option<ComponentId>) -> (ComponentId, Vec<TerminalId>) {
    for x in xs {
        h.run("wire", json!({"points": pts(&[(*x, y + 30.0), (*x, y - 30.0)])})).unwrap();
    }
    let lo = xs.iter().copied().fold(f64::INFINITY, f64::min) - 4.0;
    let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 4.0;
    let mut p = json!({"from": [lo, y], "to": [hi, y]});
    if let Some(s) = strip {
        p["strip"] = json!(s.hex());
    }
    let v = h.run("terminals", p).unwrap();
    let t: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    assert_eq!(t.len(), xs.len(), "{v}");
    (id(&v, "strip"), t)
}

/// The symbol representation of strip terminal `t` on `page`.
fn rep_of_terminal(h: &mut H, page: PageId, t: TerminalId) -> RepresentationId {
    let doc = h.doc();
    ecad_model::derive::terminal_positions(&doc, page).into_iter().find(|(x, _, _)| *x == t).map(|(_, r, _)| r).expect("terminal drawn")
}

fn click(h: &mut H, at: (f64, f64)) -> cadcraft_engine::Result<()> {
    let p = Vec2::new(at.0, at.1);
    h.s.cursor = p;
    h.s.input(Input::Point(p))
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn echo(h: &H) -> String {
    h.s.log.iter().rev().take(4).cloned().collect::<Vec<_>>().join(" | ")
}

fn centre(b: Bx) -> (f64, f64) {
    ((b.0.0 + b.1.0) / 2.0, (b.0.1 + b.1.1) / 2.0)
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t031 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
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

fn save_and_reopen(h: H, dir: &std::path::Path, name: &str, seed: u64) -> H {
    let mut h = h;
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join(name).display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    close_all(&mut h);
    drop(h);
    reopen(&path, seed)
}

/// Fixture C: coil `-K01` at (100, 200) with contacts `-K01` at (150, 200) and (200, 200), and
/// coil `-K02` at (250, 200); a second page; the first page active.
struct C {
    h: H,
    page: PageId,
    other: PageId,
    k1: ComponentId,
    k2: ComponentId,
    /// K1's coil, first contact, second contact.
    r1: [RepresentationId; 3],
    r2: RepresentationId,
}

fn fixture_c(seed: u64) -> C {
    let mut h = H::new(seed);
    let page = h.first_page();
    let c = place(&mut h, "coil", (100.0, 200.0));
    let k1: ComponentId = id(&c, "component");
    let rc = rep_of(&mut h, &c);
    let n1 = place_on(&mut h, "no", (150.0, 200.0), k1);
    let rn1 = rep_of(&mut h, &n1);
    let n2 = place_on(&mut h, "no", (200.0, 200.0), k1);
    let rn2 = rep_of(&mut h, &n2);
    let c2 = place(&mut h, "coil", (250.0, 200.0));
    let k2: ComponentId = id(&c2, "component");
    let r2 = rep_of(&mut h, &c2);
    let other = second_page(&mut h);
    C { h, page, other, k1, k2, r1: [rc, rn1, rn2], r2 }
}

// ---------------------------------------------------------------------------------------------
// Runs now: the pre-031 state the tests build on.

/// The fixtures build on today's commands, and today every derived tag is drawn (no `text.*` key
/// stored by building a page).
#[test]
fn tester_031_fixtures_build_and_store_no_text_keys() {
    let mut f = fixture_c(0x7031_0001);
    let (_, t) = strip_at(&mut f.h, &[100.0, 110.0, 120.0], 100.0, None);
    assert_eq!(t.len(), 3);
    let d = f.h.doc();
    assert!(d.properties.keys().all(|k| !k.starts_with("text.")));
    assert!(d.components.values().all(|c| c.properties.keys().all(|k| !k.starts_with("text."))));
    assert!(d.pages.values().all(|p| p.properties.keys().all(|k| !k.starts_with("text."))));
    f.h.check();
}

// ---------------------------------------------------------------------------------------------
// Q1: text styles (AC 1; design T1–T4, T12)

/// T1 (AC 1): a project with no `text.*` keys: tags 2.5 mm bold, connection point labels 1.5 mm,
/// location labels 1.6 mm, in `ECAD-BOLD` / `Standard`; the stroke-font fallback draws bold with
/// lineweight 0.35 mm; ten adjacent strip terminals show their tag once, on the leftmost.
#[test]
fn t1_defaults_sizes_weights_and_one_tag_per_strip_run() {
    let mut h = H::new(0x7031_0101);
    let page = h.first_page();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 20], [380, 280]]})).unwrap();
    let c = place(&mut h, "coil", (60.0, 200.0));
    let rc = rep_of(&mut h, &c);
    let n = place(&mut h, "no", (110.0, 200.0));
    let rn = rep_of(&mut h, &n);
    let xs: Vec<f64> = (0..10).map(|i| 100.0 + 10.0 * f64::from(i)).collect();
    let (_, terms) = strip_at(&mut h, &xs, 80.0, None);
    let rows = texts(&mut h, page);

    for rep in [rc, rn] {
        let t = row_of(&rows, rep, "tag");
        assert_eq!(num(&t["height"]), 2.5, "{t}");
        assert_eq!(t["weight"], "bold", "{t}");
        assert!(shown(t), "{t}");
    }
    for l in of_kind(&rows, "point_label") {
        assert_eq!(num(&l["height"]), 1.5, "{l}");
        assert_eq!(l["weight"], "regular", "{l}");
    }
    // Ten -X01 tag rows, one shown: the leftmost terminal's.
    let x01: Vec<&Value> = rows.iter().filter(|r| r["kind"] == "tag" && r["value"] == "-X01").collect();
    assert_eq!(x01.len(), 10, "hidden tags are still rows: {rows:?}");
    let shown_x01 = shown_tags(&rows, "-X01");
    assert_eq!(shown_x01.len(), 1, "{x01:?}");
    let first = rep_of_terminal(&mut h, page, terms[0]);
    assert_eq!(shown_x01[0]["representation"], first.hex(), "the leftmost carries the tag");

    // The drawing: tags in ECAD-BOLD at 2.5, labels in Standard at 1.5, location 1.6.
    let ts = h.run("textstyle", json!({})).unwrap();
    let stroke = ts["drawn_with"] == "stroke";
    if option_env!("CRAFT_FONTS_DIR").is_none() {
        assert!(stroke, "a build without craft-fonts draws with the stroke font: {ts}");
    } else {
        assert_eq!(ts["drawn_with"], "BIZ UDPGothic", "{ts}");
    }
    for (t, common) in drawn_value(&mut h, page, "-K01") {
        assert_eq!(t.height, 2.5);
        assert!(t.style.eq_ignore_ascii_case("ECAD-BOLD"), "{t:?}");
        if stroke {
            assert_eq!(common.lineweight, Lineweight::Mm100(35), "bold in the stroke font: lineweight 0.35");
        }
    }
    assert_eq!(drawn_value(&mut h, page, "-X01").len(), 1, "one -X01 tag drawn");
    let a1 = drawn_value(&mut h, page, "A1");
    assert!(!a1.is_empty());
    for (t, common) in a1 {
        assert_eq!(t.height, 1.5);
        assert!(t.style.eq_ignore_ascii_case("Standard"), "{t:?}");
        assert_ne!(common.lineweight, Lineweight::Mm100(35), "regular text is not drawn heavy");
    }
    let loc = drawn_value(&mut h, page, "+CP1");
    assert!(!loc.is_empty(), "location label drawn");
    assert!(loc.iter().all(|(t, _)| t.height == 1.6), "{loc:?}");
    // The drawing has both styles; Standard names the project family (or the stroke font).
    assert!(style_font(&mut h, page, "Standard").is_some());
    assert!(style_font(&mut h, page, "ECAD-BOLD").is_some());
    h.check();
}

/// The §3.2 defaults table: (kind, height, weight, shown).
const DEFAULTS: &[(&str, f64, &str, &str)] = &[
    ("tag", 2.5, "bold", "on"),
    ("point_label", 1.5, "regular", "on"),
    ("description", 1.6, "regular", "off"),
    ("part_number", 1.6, "regular", "off"),
    ("continuation_name", 2.0, "regular", "off"),
    ("reference", 2.0, "regular", "on"),
    ("wire_label", 1.6, "regular", "on"),
    ("cable_text", 1.6, "regular", "on"),
    ("location", 1.6, "regular", "on"),
    ("title_value", 2.5, "regular", "on"),
    ("title_caption", 1.8, "regular", "on"),
    ("title_main", 3.5, "bold", "on"),
    ("grid", 3.0, "regular", "on"),
    ("table", 2.5, "regular", "on"),
    ("table_header", 2.5, "bold", "on"),
];

/// T2 (AC 1): `textstyle {}` reads with no write and gives the §3.2 defaults for every kind; the
/// wire label default is 1.6 (FP6).
#[test]
fn t2_textstyle_reads_the_defaults_of_every_kind() {
    let mut h = H::new(0x7031_0201);
    let head = h.head();
    let v = h.run("textstyle", json!({})).unwrap();
    assert_eq!(h.head(), head, "reading records nothing");
    assert_eq!(v["changed"], json!(false), "{v}");
    for (kind, height, weight, show) in DEFAULTS {
        let k = textstyle_kind(&v, kind);
        assert_eq!(num(&k["height"]), *height, "{kind}: {k}");
        assert_eq!(k["weight"], *weight, "{kind}: {k}");
        // Tags are `on` in general; the strip and PLC run rules are their own entries.
        // §5.1: only some kinds have visibility in this brief; the others report no `show`.
        let no_visibility = ["cable_text", "title_value", "title_caption", "title_main", "grid", "table", "table_header"];
        let ok = k["show"] == *show || (*kind == "tag" && k["show"] == "first") || (no_visibility.contains(kind) && k["show"].is_null());
        assert!(ok, "{kind}: {k}");
        assert_eq!(k["default"], json!(true), "{kind}: {k}");
    }
    assert_eq!(ecad_model::wiring::DEFAULT_LABEL_HEIGHT, 1.6, "FP6: wire label default 1.8 → 1.6");
    let s = serde_json::to_value(&h.doc().settings).unwrap();
    assert!(s.pointer("/wiring/label_height").is_none_or(|x| x.as_f64() == Some(1.6)), "a new project stores no 1.8: {s}");
}

/// T2 (AC 1): hostile stored values fall back to the default; nothing panics; the page draws.
#[test]
fn t2_hostile_stored_values_give_the_default() {
    let mut f = fixture_c(0x7031_0202);
    let long_font = "F".repeat(65);
    let huge = "9".repeat(10_000);
    let cases: Vec<(&str, String)> = vec![
        ("text.tag.height", "NaN".into()),
        ("text.tag.height", "abc".into()),
        ("text.tag.height", "0.1".into()),
        ("text.tag.height", "11".into()),
        ("text.tag.height", "1e308".into()),
        ("text.tag.height", "-2.5".into()),
        ("text.tag.height", "inf".into()),
        ("text.tag.height", huge.clone()),
        ("text.tag.weight", "heavy".into()),
        ("text.tag.weight", String::new()),
        ("text.tag.show", "maybe".into()),
        ("text.point_label.show", "first".into()),
        ("text.font", long_font),
        ("text.font", String::new()),
        ("text.font", "Bad\u{1}Font".into()),
        ("text.run.gap", "-5".into()),
        ("text.run.gap", "1e999".into()),
        ("text.tag.gap", "99".into()),
        ("text.line_pitch", "0".into()),
        ("text.bold_stroke_weight", "5".into()),
    ];
    let clean = f.h.run("textstyle", json!({})).unwrap();
    let clean_rows = texts(&mut f.h, f.page);
    for (key, value) in cases {
        let before = project_prop(&mut f.h, key);
        inject(&mut f.h, vec![set_project_prop(key, &value, before)], f.page, f.other);
        let v = f.h.run("textstyle", json!({})).unwrap();
        let tag = textstyle_kind(&v, "tag");
        assert_eq!(num(&tag["height"]), 2.5, "{key}={value:.40}: {tag}");
        assert_eq!(tag["weight"], "bold", "{key}={value:.40}: {tag}");
        assert_eq!(v["drawn_with"], clean["drawn_with"], "{key}={value:.40}: a hostile font falls back");
        let rows = texts(&mut f.h, f.page);
        assert_eq!(rows, clean_rows, "{key}={value:.40}: the page draws as with no stored value");
        let d = f.h.run("defaults", json!({"key": key})).unwrap();
        if let Some(e) = d["defaults"].as_array().and_then(|a| a.first()) {
            assert_eq!(e["from"], "builtin", "{key}={value:.40}: {e}");
        }
        // Back to absent for the next case.
        let now = project_prop(&mut f.h, key);
        f.h.cfg(|p| {
            p.ledger.commit(Author::User, "clear", vec![Op::SetProperty { object: ObjectRef::Project, key: key.into(), before: now, after: None }])
        })
        .unwrap();
        reproject(&mut f.h, f.page, f.other);
    }
}

/// T3 (AC 1): font fallback. Without craft-fonts in the build, `drawn_with` is the stroke font;
/// with it, BIZ UDPGothic. A missing family is drawn with the fallback, never refused, and the
/// stored name is kept.
#[test]
fn t3_font_fallback_never_refuses_and_keeps_the_stored_name() {
    let mut f = fixture_c(0x7031_0301);
    let v = f.h.run("textstyle", json!({})).unwrap();
    let built_in = if option_env!("CRAFT_FONTS_DIR").is_some() { "BIZ UDPGothic" } else { "stroke" };
    assert_eq!(v["drawn_with"], built_in, "{v}");
    assert_eq!(v["font"], "BIZ UDPGothic", "absent means the built-in family: {v}");
    let w = one_tx(&mut f.h, "textstyle", json!({"font": "NoSuchFont"}));
    assert_eq!(w["changed"], json!(true), "{w}");
    assert_eq!(w["font"], "NoSuchFont", "{w}");
    assert_eq!(w["drawn_with"], built_in, "a missing family falls back: {w}");
    assert_eq!(project_prop(&mut f.h, "text.font").as_deref(), Some("NoSuchFont"), "the stored value is kept");
    assert_eq!(last_label(&mut f.h), "Text styles");
    // The page still draws its tags.
    assert!(!drawn_value(&mut f.h, f.page, "-K01").is_empty());
    // `null` removes it.
    one_tx(&mut f.h, "textstyle", json!({"font": null}));
    assert_eq!(project_prop(&mut f.h, "text.font"), None);
    f.h.check();
}

/// T4 (AC 1): `textstyle {kind: tag, height: 3, weight: regular}` is one transaction; the open
/// page redraws (tags 3 mm in `Standard`); `u` restores both; the same values again write nothing.
#[test]
fn t4_one_transaction_per_change_and_the_page_redraws() {
    let mut f = fixture_c(0x7031_0401);
    let before = f.h.doc();
    let v = one_tx(&mut f.h, "textstyle", json!({"kind": "tag", "height": 3, "weight": "regular"}));
    assert_eq!(v["changed"], json!(true));
    assert_eq!(last_label(&mut f.h), "Text styles");
    let ops = last_ops(&mut f.h);
    assert!(ops.iter().all(|o| matches!(o, Op::SetProperty { object: ObjectRef::Project, key, .. } if key.starts_with("text.tag."))), "{ops:?}");
    assert_eq!(project_prop(&mut f.h, "text.tag.height").map(|s| s.parse::<f64>().ok()), Some(Some(3.0)));
    assert_eq!(project_prop(&mut f.h, "text.tag.weight").as_deref(), Some("regular"));
    let tag = textstyle_kind(&v, "tag");
    assert_eq!((num(&tag["height"]), tag["weight"].clone(), tag["default"].clone()), (3.0, json!("regular"), json!(false)));
    for (t, _) in drawn_value(&mut f.h, f.page, "-K01") {
        assert_eq!(t.height, 3.0, "the open page redraws");
        assert!(t.style.eq_ignore_ascii_case("Standard"), "{t:?}");
    }
    // The same values: nothing written.
    let head = f.h.head();
    let again = f.h.run("textstyle", json!({"kind": "tag", "height": 3, "weight": "regular"})).unwrap();
    assert_eq!((f.h.head(), again["changed"].clone()), (head, json!(false)));
    // Undo restores both.
    f.h.cmdline("u");
    assert_eq!(model_parts(&f.h.doc()), model_parts(&before));
    for (t, _) in drawn_value(&mut f.h, f.page, "-K01") {
        assert_eq!(t.height, 2.5);
        assert!(t.style.eq_ignore_ascii_case("ECAD-BOLD"));
    }
    f.h.run("redo", Value::Null).unwrap();
    assert_eq!(project_prop(&mut f.h, "text.tag.weight").as_deref(), Some("regular"));
    // `null` removes a key (back to default).
    one_tx(&mut f.h, "textstyle", json!({"kind": "tag", "height": null}));
    assert_eq!(project_prop(&mut f.h, "text.tag.height"), None);
    f.h.check();
}

/// AC 1: the wire label height has one home, `settings.wiring.label_height`; `textstyle` writes
/// it with a `SetProjectSettings`, never a `text.wire_label.height` property.
#[test]
fn t4_wire_label_height_is_written_to_the_wiring_settings() {
    let mut h = H::new(0x7031_0402);
    one_tx(&mut h, "textstyle", json!({"kind": "wire_label", "height": 2}));
    let ops = last_ops(&mut h);
    assert!(ops.iter().any(|o| matches!(o, Op::SetProjectSettings { .. })), "{ops:?}");
    assert!(!ops.iter().any(|o| matches!(o, Op::SetProperty { key, .. } if key.starts_with("text.wire_label"))), "{ops:?}");
    assert_eq!(h.doc().settings.wiring.label_height(), 2.0);
    assert_eq!(project_prop(&mut h, "text.wire_label.height"), None);
    let v = h.run("textstyle", json!({})).unwrap();
    assert_eq!(num(&textstyle_kind(&v, "wire_label")["height"]), 2.0);
    let d = h.run("defaults", json!({"key": "text.wire_label.height"})).unwrap();
    assert_eq!(d["defaults"][0]["from"], "project", "{d}");
    h.check();
}

/// AC 1, design §10.1: every refusal sentence, nothing written.
#[test]
fn t4_textstyle_refusals_write_nothing() {
    let mut h = H::new(0x7031_0403);
    let height = "A text height is 0.5 to 10 mm.";
    for bad in [json!(0.4), json!(10.5), json!(0), json!(-1), json!("abc"), json!(1e300)] {
        refused(&mut h, "textstyle", json!({"kind": "tag", "height": bad}), height);
    }
    refused(&mut h, "textstyle", json!({"kind": "x", "height": 3}), "Unknown text kind \"x\". Kinds: tag, point_label");
    refused(&mut h, "textstyle", json!({"kind": "tag", "weight": "heavy"}), "Weight is regular or bold.");
    refused(&mut h, "textstyle", json!({"kind": "point_label", "show": "first"}), "Shown is on, off or first (first: tag only).");
    refused(&mut h, "textstyle", json!({"kind": "tag", "show": "maybe"}), "Shown is on, off or first (first: tag only).");
    let font = "A font name is 1 to 64 characters with no control characters.";
    refused(&mut h, "textstyle", json!({"font": ""}), font);
    refused(&mut h, "textstyle", json!({"font": "F".repeat(65)}), font);
    refused(&mut h, "textstyle", json!({"font": "Bad\u{7}Font"}), font);
    refused(&mut h, "textstyle", json!({"font": "Bad\nFont"}), font);
    refused(&mut h, "textstyle", json!({"height": 3}), "height, weight and show need a kind.");
    refused(&mut h, "textstyle", json!({"weight": "bold"}), "height, weight and show need a kind.");
    // A refusal among good fields writes none of them.
    refused(&mut h, "textstyle", json!({"font": "Arial", "kind": "tag", "height": 11}), height);
    assert_eq!(project_prop(&mut h, "text.font"), None);
    // 64 characters is allowed.
    one_tx(&mut h, "textstyle", json!({"font": "F".repeat(64)}));
}

/// T12: a font change re-projects the page and is not taken for a user edit of the text style
/// table: an ordinary `line` afterwards is accepted.
#[test]
fn t12_a_font_change_is_not_a_user_edit() {
    let mut f = fixture_c(0x7031_1201);
    one_tx(&mut f.h, "textstyle", json!({"font": "NoSuchFont"}));
    let head = f.h.head();
    f.h.cmdline("line 10,10 40,10 ");
    assert_eq!(f.h.head(), head + 1, "the line is recorded: {}", echo(&f.h));
    one_tx(&mut f.h, "textstyle", json!({"kind": "tag", "weight": "regular"}));
    let head = f.h.head();
    f.h.cmdline("line 10,20 40,20 ");
    assert_eq!(f.h.head(), head + 1, "{}", echo(&f.h));
    f.h.check();
}

// ---------------------------------------------------------------------------------------------
// Q3: show and hide (AC 2; design T5–T8)

/// T5 (AC 2): hide on the project, show on the page, hide on the device, show on one symbol:
/// the narrowest level wins at every step.
#[test]
fn t5_levels_symbol_device_page_project_narrowest_wins() {
    let mut f = fixture_c(0x7031_0501);
    let shown_reps = |h: &mut H, page: PageId, tag: &str| -> BTreeSet<String> {
        let rows = texts(h, page);
        shown_tags(&rows, tag).iter().map(|r| r["representation"].as_str().unwrap_or("").to_string()).collect()
    };
    let all_k1: BTreeSet<String> = f.r1.iter().map(|r| r.hex()).collect();
    assert_eq!(shown_reps(&mut f.h, f.page, "-K01"), all_k1);

    let v = one_tx(&mut f.h, "showtext", json!({"kind": "tag", "show": "off"}));
    assert_eq!(v["level"], "project", "{v}");
    assert!(shown_reps(&mut f.h, f.page, "-K01").is_empty());
    assert!(shown_reps(&mut f.h, f.page, "-K02").is_empty());

    let v = one_tx(&mut f.h, "showtext", json!({"kind": "tag", "show": "on", "page": f.page.hex()}));
    assert_eq!(v["level"], "page", "{v}");
    assert_eq!(shown_reps(&mut f.h, f.page, "-K01"), all_k1);
    assert_eq!(shown_reps(&mut f.h, f.page, "-K02").len(), 1);

    let v = one_tx(&mut f.h, "showtext", json!({"kind": "tag", "show": "off", "component": f.k1.hex()}));
    assert_eq!(v["level"], "device", "{v}");
    assert!(shown_reps(&mut f.h, f.page, "-K01").is_empty());
    assert_eq!(shown_reps(&mut f.h, f.page, "-K02").len(), 1, "another device keeps the page's value");

    let v = one_tx(&mut f.h, "showtext", json!({"kind": "tag", "show": "on", "representation": f.r1[1].hex()}));
    assert_eq!(v["level"], "symbol", "{v}");
    assert_eq!(shown_reps(&mut f.h, f.page, "-K01"), BTreeSet::from([f.r1[1].hex()]), "exactly that symbol's tag");

    // Where the keys live (FP1–FP3).
    let d = f.h.doc();
    assert_eq!(d.properties.get("text.tag.show").map(String::as_str), Some("off"));
    assert_eq!(d.pages[&f.page].properties.get("text.tag.show").map(String::as_str), Some("on"));
    assert_eq!(d.components[&f.k1].properties.get("text.tag.show").map(String::as_str), Some("off"));
    let sym = format!("text.{}.tag.show", f.r1[1].hex());
    assert_eq!(d.components[&f.k1].properties.get(&sym).map(String::as_str), Some("on"), "{sym}");
    // `null` removes the symbol's value: back to the device's off.
    one_tx(&mut f.h, "showtext", json!({"kind": "tag", "show": null, "representation": f.r1[1].hex()}));
    assert!(shown_reps(&mut f.h, f.page, "-K01").is_empty());
    // The same value again writes nothing.
    let head = f.h.head();
    let v = f.h.run("showtext", json!({"kind": "tag", "show": "off", "component": f.k1.hex()})).unwrap();
    assert_eq!((f.h.head(), v["changed"].clone()), (head, json!(false)));
    f.h.check();
}

/// AC 2: description and part number are hidden by default and shown under the tag when switched
/// on; continuation names are hidden by default.
#[test]
fn t5_description_and_part_number_hidden_by_default_and_stack_under_the_tag() {
    let mut f = fixture_c(0x7031_0502);
    f.h.run("setprop", json!({"component": f.k1.hex(), "key": "description", "value": "Pump contactor"})).unwrap();
    f.h.run("setprop", json!({"component": f.k1.hex(), "key": "part_number", "value": "LC1D09"})).unwrap();
    let rows = texts(&mut f.h, f.page);
    assert!(rows.iter().filter(|r| r["kind"] == "description" || r["kind"] == "part_number").all(|r| !shown(r)), "{rows:?}");
    assert!(drawn_value(&mut f.h, f.page, "Pump contactor").is_empty());
    one_tx(&mut f.h, "showtext", json!({"kind": "description", "show": "on", "component": f.k1.hex()}));
    one_tx(&mut f.h, "showtext", json!({"kind": "part_number", "show": "on", "component": f.k1.hex()}));
    let rows = texts(&mut f.h, f.page);
    let tag = row_of(&rows, f.r1[0], "tag");
    let desc = row_of(&rows, f.r1[0], "description");
    let part = row_of(&rows, f.r1[0], "part_number");
    assert!(shown(desc) && shown(part), "{desc} {part}");
    assert_eq!(desc["value"], "Pump contactor");
    assert_eq!(part["value"], "LC1D09");
    assert_eq!(num(&desc["height"]), 1.6);
    // Stacked under the tag: each line lower than the one before, the same justification.
    assert!(at(desc).1 < at(tag).1 && at(part).1 < at(desc).1, "{tag} {desc} {part}");
    assert_eq!(desc["justify"], tag["justify"]);
    assert_eq!(part["justify"], tag["justify"]);
    assert_eq!(drawn_value(&mut f.h, f.page, "Pump contactor").len(), 3, "under each of -K01's three symbols");
    assert!(drawn_value(&mut f.h, f.page, "LC1D09").iter().all(|(t, _)| t.height == 1.6));
    // No overlap between the lines of one block.
    assert!(!overlap(bx(tag), bx(desc)) && !overlap(bx(desc), bx(part)), "{tag} {desc} {part}");
    f.h.check();
}

/// AC 2, design §10.2: `showtext` refusals, nothing written.
#[test]
fn t5_showtext_refusals() {
    let mut f = fixture_c(0x7031_0503);
    refused(&mut f.h, "showtext", json!({"kind": "x", "show": "off"}), "tag");
    refused(&mut f.h, "showtext", json!({"kind": "point_label", "show": "first"}), "first");
    refused(&mut f.h, "showtext", json!({"kind": "tag", "show": "maybe"}), "Shown is on, off or first");
    // `reference` and `location` are page and project kinds only (§5.1).
    refused(&mut f.h, "showtext", json!({"kind": "reference", "show": "off", "component": f.k1.hex()}), "tag");
    refused(&mut f.h, "showtext", json!({"kind": "location", "show": "off", "representation": f.r1[0].hex()}), "tag");
    let ghost = "0123456789abcdef0123456789abcdef";
    refused(&mut f.h, "showtext", json!({"kind": "tag", "show": "off", "representation": ghost}), &format!("No representation {ghost}."));
    refused(&mut f.h, "showtext", json!({"kind": "tag", "show": "off", "component": ghost}), "No component");
    refused(&mut f.h, "showtext", json!({"kind": "tag", "show": "off", "page": ghost}), "No page");
    refused(&mut f.h, "showtext", json!({"kind": "tag", "show": "off", "page": "zz"}), "");
    // A wire's representation is not a symbol.
    let w: ecad_model::ConductorId = id(&f.h.run("wire", json!({"points": [[10, 10], [40, 10]]})).unwrap(), "conductor");
    let wr = f.h.doc().representations_of(Subject::Conductor(w)).map(|(id, _)| id).next().unwrap();
    refused(&mut f.h, "showtext", json!({"kind": "tag", "show": "off", "representation": wr.hex()}), "That representation is not a symbol.");
    // Two targets at once.
    refused(&mut f.h, "showtext", json!({"kind": "tag", "show": "off", "page": f.page.hex(), "component": f.k1.hex()}), "");
    // A wire label key on a conductor is stored for lane W.
    let v = f.h.run("showtext", json!({"kind": "wire_label", "show": "off", "page": f.page.hex()})).unwrap();
    assert_eq!(v["level"], "page", "{v}");
}

/// T6 (AC 2): runs. Two groups of -X01 on one line 40 mm apart: the tag twice; the 5th terminal
/// set `on`: three times; a -X02 terminal between two -X01 terminals splits their run; a PLC
/// device's channels in a row: its tag once.
#[test]
fn t6_runs_once_per_run_symbol_override_and_another_device_splits() {
    let mut h = H::new(0x7031_0601);
    let page = h.first_page();
    // Group A x 100…140, group B x 180…220 (40 mm between 140 and 180), one strip.
    let (x1, a) = strip_at(&mut h, &[100.0, 110.0, 120.0, 130.0, 140.0], 150.0, None);
    let (_, b) = strip_at(&mut h, &[180.0, 190.0, 200.0, 210.0, 220.0], 150.0, Some(x1));
    let rows = texts(&mut h, page);
    let s = shown_tags(&rows, "-X01");
    assert_eq!(s.len(), 2, "{s:?}");
    let firsts: BTreeSet<String> = [a[0], b[0]].iter().map(|t| rep_of_terminal(&mut h, page, *t).hex()).collect();
    assert_eq!(s.iter().map(|r| r["representation"].as_str().unwrap().to_string()).collect::<BTreeSet<_>>(), firsts);
    // The fifth terminal (last of group A) set on: three.
    let fifth = rep_of_terminal(&mut h, page, a[4]);
    one_tx(&mut h, "showtext", json!({"kind": "tag", "show": "on", "representation": fifth.hex()}));
    assert_eq!(shown_tags(&texts(&mut h, page), "-X01").len(), 3);
    // A symbol-level off on the first of group B hides that run's tag.
    let b0 = rep_of_terminal(&mut h, page, b[0]);
    one_tx(&mut h, "showtext", json!({"kind": "tag", "show": "off", "representation": b0.hex()}));
    assert_eq!(shown_tags(&texts(&mut h, page), "-X01").len(), 2);
    // Device on: every terminal of -X01 shows its tag except b0 (symbol off wins).
    one_tx(&mut h, "showtext", json!({"kind": "tag", "show": "on", "component": x1.hex()}));
    assert_eq!(shown_tags(&texts(&mut h, page), "-X01").len(), 9);
    h.check();

    // Another device between: -X01 at 100, -X02 at 110, -X01 at 120 on another line.
    let (s1, _) = strip_at(&mut h, &[100.0], 60.0, None);
    let (s2, _) = strip_at(&mut h, &[110.0], 60.0, None);
    strip_at(&mut h, &[120.0], 60.0, Some(s1));
    let tag1 = h.doc().components[&s1].properties.get("tag").cloned().unwrap();
    let tag2 = h.doc().components[&s2].properties.get("tag").cloned().unwrap();
    let rows = texts(&mut h, page);
    let on_line = |t: &str| rows.iter().filter(|r| r["kind"] == "tag" && r["value"] == t && shown(r) && (at(r).1 - 60.0).abs() < 15.0).count();
    assert_eq!(on_line(&tag1), 2, "{tag1} split by {tag2}: {rows:?}");
    assert_eq!(on_line(&tag2), 1);
    h.check();
}

/// T6 (AC 2): the gap rule (15 mm between boxes) and the alignment rule (1 mm across the line).
#[test]
fn t6_run_gap_and_alignment_bounds() {
    let mut h = H::new(0x7031_0602);
    let page = h.first_page();
    // 12 mm apart: one run. 30 mm apart: two runs.
    let (_, _) = strip_at(&mut h, &[100.0, 112.0, 124.0], 200.0, None);
    let (_, _) = strip_at(&mut h, &[100.0, 130.0, 160.0], 120.0, None);
    let rows = texts(&mut h, page);
    let tags: BTreeMap<String, usize> = rows.iter().filter(|r| r["kind"] == "tag" && shown(r)).fold(BTreeMap::new(), |mut m, r| {
        *m.entry(r["value"].as_str().unwrap().to_string()).or_default() += 1;
        m
    });
    let counts: Vec<usize> = tags.values().copied().collect();
    assert_eq!(counts, vec![1, 3], "12 mm apart is one run, 30 mm apart three runs: {tags:?}");
    h.check();
}

/// T6 (AC 2): a PLC device's channels in a row show its tag once.
#[test]
fn t6_plc_channels_in_a_row_show_the_tag_once() {
    let mut h = H::new(0x7031_0603);
    let page = h.first_page();
    let first = place(&mut h, "di", (100.0, 150.0));
    let a: ComponentId = id(&first, "component");
    place_on(&mut h, "di", (120.0, 150.0), a);
    place_on(&mut h, "di", (140.0, 150.0), a);
    let tag = h.doc().components[&a].properties.get("tag").cloned().unwrap();
    let rows = texts(&mut h, page);
    let s = shown_tags(&rows, &tag);
    assert_eq!(s.len(), 1, "{rows:?}");
    assert_eq!(s[0]["representation"], rep_of(&mut h, &first).hex(), "the leftmost");
    // A coil keeps its tag on every symbol (tag `on`).
    let k = place(&mut h, "coil", (100.0, 60.0));
    let k1: ComponentId = id(&k, "component");
    place_on(&mut h, "no", (115.0, 60.0), k1);
    let kt = h.doc().components[&k1].properties.get("tag").cloned().unwrap();
    assert_eq!(shown_tags(&texts(&mut h, page), &kt).len(), 2);
    h.check();
}

/// T7 (AC 2): hidden text is still data: `devices`, the BOM, the strip plan and Checks give the
/// same answers with every tag hidden.
#[test]
fn t7_hidden_text_is_still_data() {
    let mut f = fixture_c(0x7031_0701);
    let (x1, _) = strip_at(&mut f.h, &[100.0, 110.0, 120.0], 100.0, None);
    f.h.run("setprop", json!({"component": f.k1.hex(), "key": "part_number", "value": "LC1D09"})).unwrap();
    f.h.run("setprop", json!({"component": x1.hex(), "key": "part_number", "value": "UK5N"})).unwrap();
    let reads = |h: &mut H| {
        json!({
            "devices": h.run("devices", json!({})).unwrap(),
            "x01": h.run("devices", json!({"tag": "X01"})).unwrap(),
            "bom": h.run("bom", json!({})).unwrap(),
            "bom_part": h.run("bom", json!({"group": "part"})).unwrap(),
            "stripplan": h.run("stripplan", json!({})).unwrap(),
            "checks": h.run("checks", json!({})).unwrap(),
        })
    };
    let before = reads(&mut f.h);
    for kind in ["tag", "point_label", "description", "part_number"] {
        one_tx(&mut f.h, "showtext", json!({"kind": kind, "show": "off"}));
    }
    assert!(texts(&mut f.h, f.page).iter().all(|r| !shown(r)));
    assert!(drawn_value(&mut f.h, f.page, "-K01").is_empty(), "nothing drawn");
    assert_eq!(reads(&mut f.h), before, "lists, reports and checks unchanged");
    let x = &before["x01"]["devices"];
    assert!(x.as_array().is_some_and(|a| a.iter().any(|d| d["tag"] == "-X01")), "{x}");
    f.h.check();
}

/// T8 (AC 2): a printed book has none of the hidden texts: ten -X01 terminals in a row print the
/// tag once.
#[test]
fn t8_print_has_no_hidden_text() {
    let mut h = H::new(0x7031_0801);
    let page = h.first_page();
    let xs: Vec<f64> = (0..10).map(|i| 100.0 + 10.0 * f64::from(i)).collect();
    strip_at(&mut h, &xs, 150.0, None);
    let k = place(&mut h, "coil", (60.0, 200.0));
    let k1: ComponentId = id(&k, "component");
    one_tx(&mut h, "showtext", json!({"kind": "tag", "show": "off", "component": k1.hex()}));
    let doc = h.doc();
    let (d, _) = ecad_cadcraft::printbook::build(&doc, &[page], &BTreeSet::new(), 1e-6, None).unwrap();
    let values: Vec<String> = d
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(values.iter().filter(|v| *v == "-X01").count(), 1, "{values:?}");
    assert_eq!(values.iter().filter(|v| *v == "-K01").count(), 0, "{values:?}");
    assert!(d.model.iter().all(|e| e.common.layer != "ECAD-HIDDEN-TEXT"), "no hidden-text layer in print");
    // The PDF itself builds.
    ecad_cadcraft::printbook::install_io();
    let v = h.run("printbook", json!({})).unwrap();
    assert!(v["files"][0]["data"].as_str().is_some_and(|s| s.starts_with("JVBER")), "{}", v["message"]);
}

/// T8 (AC 2): Show Hidden Text draws hidden tags in ACI 8 on `ECAD-HIDDEN-TEXT`; toggling it
/// records nothing; print still has none of them. The toggle is a session view state (design
/// §5.7); its API is bound when the build lands (UI: `ecad.view.hiddentext`).
#[test]
fn t8_show_hidden_text_view_draws_dimmed_and_records_nothing() {
    let mut h = H::new(0x7031_0802);
    let page = h.first_page();
    let xs: Vec<f64> = (0..4).map(|i| 100.0 + 10.0 * f64::from(i)).collect();
    strip_at(&mut h, &xs, 150.0, None);
    let head = h.head();
    show_hidden(&mut h, true);
    assert_eq!(h.head(), head, "a view state records nothing");
    let x01 = drawn_value(&mut h, page, "-X01");
    assert_eq!(x01.len(), 4, "{x01:?}");
    let dim: Vec<_> = x01.iter().filter(|(_, c)| c.layer == "ECAD-HIDDEN-TEXT").collect();
    assert_eq!(dim.len(), 3);
    for (_, c) in &dim {
        assert!(matches!(c.color, cadcraft_engine::doc::color::Color::Index(8) | cadcraft_engine::doc::color::Color::ByLayer), "{c:?}");
    }
    h.check();
    show_hidden(&mut h, false);
    assert_eq!(drawn_value(&mut h, page, "-X01").len(), 1);
    assert_eq!(h.head(), head);
}

/// Bound at phase 2 to the build's Show Hidden Text toggle.
fn show_hidden(h: &mut H, on: bool) {
    let v = h.s.execute("hiddentext", &json!({"on": on})).expect("hiddentext");
    assert_eq!(v["on"], json!(on), "{v}");
}

// ---------------------------------------------------------------------------------------------
// Q2: placement (AC 3; design T9–T11)

/// T9 (AC 3): a vertical contact's tag takes candidate 1 (left, top); with a wire through that
/// box it moves to the next free candidate, clear of the wire.
#[test]
fn t9_a_tag_on_a_wire_moves_to_the_next_free_candidate() {
    let mut h = H::new(0x7031_0901);
    let page = h.first_page();
    let n = place(&mut h, "no", (100.0, 150.0));
    let rn = rep_of(&mut h, &n);
    let rows = texts(&mut h, page);
    let t = row_of(&rows, rn, "tag");
    assert_eq!(t["justify"], "RT", "candidate 1 of a vertical symbol: left, top, right-justified: {t}");
    assert_eq!(t["overlaps"], json!(false));
    let b1 = bx(t);
    assert!(b1.1.0 < 100.0, "left of the symbol: {t}");
    // A free wire through the middle of that box, ending short of the symbol.
    let y = (b1.0.1 + b1.1.1) / 2.0;
    h.run("wire", json!({"points": pts(&[(b1.0.0 - 10.0, y), (b1.1.0 + 0.4, y)])})).unwrap();
    let rows = texts(&mut h, page);
    let t2 = row_of(&rows, rn, "tag");
    let b2 = bx(t2);
    assert_ne!(b2, b1, "moved off the wire: {t2}");
    assert_eq!(t2["overlaps"], json!(false), "{t2}");
    let wire_box = ((b1.0.0 - 10.0, y - 0.3), (b1.1.0 + 0.4, y + 0.3));
    assert!(!overlap(b2, wire_box), "clear of the wire and its 0.3 mm: {t2}");
    assert_eq!(t2["moved"], json!(false));
    h.check();
}

/// T9 (AC 3): neighbouring tags never overlap when a free candidate exists: contacts 10 mm apart
/// in a row, and a horizontal row of coils.
#[test]
fn t9_neighbouring_tags_do_not_overlap() {
    let mut h = H::new(0x7031_0902);
    let page = h.first_page();
    for i in 0..6 {
        place(&mut h, "no", (100.0 + 10.0 * f64::from(i), 150.0));
    }
    for i in 0..4 {
        h.run("place", json!({"symbol": "coil", "at": [100.0 + 30.0 * f64::from(i), 60.0], "rotation": 90})).unwrap();
    }
    let rows = texts(&mut h, page);
    let tags: Vec<&Value> = rows.iter().filter(|r| r["kind"] == "tag" && shown(r)).collect();
    assert_eq!(tags.len(), 10);
    for (i, a) in tags.iter().enumerate() {
        assert_eq!(a["overlaps"], json!(false), "{a}");
        for b in &tags[i + 1..] {
            assert!(!overlap(bx(a), bx(b)), "{a} overlaps {b}");
        }
        // Nor any connection point label.
        for l in of_kind(&rows, "point_label").into_iter().filter(|l| shown(l)) {
            assert!(!overlap(bx(a), bx(l)), "{a} overlaps label {l}");
        }
    }
    // Horizontal symbols: candidate 1 above the box, centred. (A tag block is drawn as
    // top-justified lines, §6.1, so the row reports `CT`; the place is checked on the box.)
    let coil_tags: Vec<&&Value> = tags.iter().filter(|r| at(r).1 > 55.0 && at(r).1 < 90.0).collect();
    assert_eq!(coil_tags.len(), 4, "{coil_tags:?}");
    for t in coil_tags {
        let (lo, hi) = bx(t);
        let cx = (lo.0 + hi.0) / 2.0;
        assert!((0..4).any(|i| (cx - (100.0 + 30.0 * f64::from(i))).abs() < 1e-6), "centred on its coil: {t}");
        assert!(lo.1 > 60.0, "above the coil: {t}");
        assert!(t["justify"].as_str().is_some_and(|j| j.starts_with('C')), "{t}");
    }
    h.check();
}

/// T9: placement does not depend on the order the symbols were made (reading order rule).
#[test]
fn t9_placement_does_not_depend_on_op_order() {
    let build = |order: &[usize], seed: u64| {
        let mut h = H::new(seed);
        let page = h.first_page();
        let spots = [(100.0, 150.0), (108.0, 150.0), (100.0, 128.0), (116.0, 140.0)];
        for i in order {
            place(&mut h, "no", spots[*i]);
        }
        // Tags differ by order, so compare the set of boxes.
        let rows = texts(&mut h, page);
        let mut boxes: Vec<String> = rows.iter().filter(|r| r["kind"] == "tag").map(|r| format!("{:.3?}", bx(r))).collect();
        boxes.sort();
        boxes
    };
    assert_eq!(build(&[0, 1, 2, 3], 0x7031_0903), build(&[3, 2, 1, 0], 0x7031_0904));
}

/// T10 (AC 3): `movetext` stores the place in the symbol's frame; the tag keeps its place relative
/// to the symbol after moving it 20 mm, rotating it 90° (still upright), renumbering, a style
/// change, save and reopen; `reset` puts it back.
#[test]
fn t10_a_moved_tag_keeps_its_place_through_move_rotate_renumber_save() {
    let mut f = fixture_c(0x7031_1001);
    let rep = f.r1[1];
    let v = one_tx(&mut f.h, "movetext", json!({"representation": rep.hex(), "kind": "tag", "to": [165, 215], "justify": "LB"}));
    assert_eq!(v["changed"], json!(true), "{v}");
    assert!(last_label(&mut f.h).starts_with("Move tag"), "{}", last_label(&mut f.h));
    let key = format!("text.{}.tag.at", rep.hex());
    let stored = comp_props(&mut f.h, f.k1, &key);
    let s = stored.get(&key).unwrap_or_else(|| panic!("{key} stored: {stored:?}"));
    assert!(s.ends_with(",LB"), "{s}");
    let (sx, sy) = {
        let p: Vec<f64> = s.split(',').take(2).map(|x| x.trim().parse().unwrap()).collect();
        (p[0], p[1])
    };
    assert!(near((sx, sy), (15.0, 15.0)), "stored in the symbol's frame (placed at 150,200): {s}");
    // A tag block is drawn as top-justified lines (§6.1); `LB` puts the block's bottom-left
    // corner at the point, so the place is checked on the box.
    let row = row_of(&texts(&mut f.h, f.page), rep, "tag").clone();
    assert!(near(bx(&row).0, (165.0, 215.0)), "{row}");
    assert_eq!(row["moved"], json!(true), "{row}");
    assert!(row["justify"].as_str().is_some_and(|j| j.starts_with('L')), "{row}");
    let te = drawn_value(&mut f.h, f.page, "-K01");
    assert!(te.iter().any(|(t, _)| t.halign == HAlign::Left), "{te:?}");

    // Move the symbol 20 mm right.
    let handles = sym_handles(&mut f.h, f.page, rep);
    one_tx(&mut f.h, "move", json!({"handles": handles, "delta": [20, 0]}));
    assert!(near(bx(row_of(&texts(&mut f.h, f.page), rep, "tag")).0, (185.0, 215.0)));
    // Rotate it 90° about its insertion point (170, 200): (15, 15) → (-15, 15).
    let handles = sym_handles(&mut f.h, f.page, rep);
    one_tx(&mut f.h, "rotate", json!({"handles": handles, "base": [170, 200], "angle": 90}));
    let r = row_of(&texts(&mut f.h, f.page), rep, "tag").clone();
    assert!(near(bx(&r).0, (155.0, 215.0)), "{r}");
    assert!(drawn_value(&mut f.h, f.page, "-K01").iter().all(|(t, _)| t.rotation == 0.0), "always upright");
    // Renumber the devices: the key names the representation, not the tag.
    f.h.run("devicedelete", json!({"component": f.k2.hex()})).unwrap();
    place(&mut f.h, "coil", (20.0, 200.0));
    f.h.run("renumber", json!({"kind": "devices", "scope": "all"})).unwrap();
    let r2 = row_of(&texts(&mut f.h, f.page), rep, "tag").clone();
    assert!(near(bx(&r2).0, (155.0, 215.0)), "{r2}");
    assert_eq!(r2["moved"], json!(true));
    // A style change re-places other tags; the moved one stays.
    one_tx(&mut f.h, "textstyle", json!({"kind": "tag", "height": 4}));
    assert!(near(bx(row_of(&texts(&mut f.h, f.page), rep, "tag")).0, (155.0, 215.0)));
    // Save and reopen.
    let dir = tmp_dir("moved");
    let mut h2 = save_and_reopen(f.h, &dir, "Moved tag (A).ecad", 0x7031_1002);
    let page = h2.first_page();
    h2.activate(page);
    let r3 = row_of(&texts(&mut h2, page), rep, "tag").clone();
    assert!(near(bx(&r3).0, (155.0, 215.0)), "{r3}");
    h2.check();
    // Reset puts it back to an automatic place.
    let v = one_tx(&mut h2, "movetext", json!({"representation": rep.hex(), "kind": "tag", "reset": true}));
    assert_eq!(v["at"], Value::Null, "{v}");
    let r4 = row_of(&texts(&mut h2, page), rep, "tag").clone();
    assert_eq!(r4["moved"], json!(false));
    assert!(!near(bx(&r4).0, (155.0, 215.0)));
    assert!(h2.doc().components.values().all(|c| !c.properties.contains_key(&key)));
    h2.check();
    close_all(&mut h2);
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every handle of one symbol representation on `page`.
fn sym_handles(h: &mut H, page: PageId, rep: RepresentationId) -> Vec<String> {
    let doc = h.doc();
    let r = doc.representations[&rep].clone();
    let map = h.p(|p| p.pages[&page].map.clone());
    let mut out: Vec<String> = r.items.iter().filter_map(|i| map.item(*i)).map(|x| x.hex()).collect();
    out.extend(r.connection_points.keys().filter_map(|t| map.marker(rep, *t)).map(|x| x.hex()));
    out
}

/// T10 (AC 3): `movetext` of a connection point label, undo and redo exact; refusals.
#[test]
fn t10_movetext_label_undo_redo_and_refusals() {
    let mut f = fixture_c(0x7031_1003);
    let rep = f.r1[0];
    let before = f.h.doc();
    // `terminal` is the connection point as `pagetexts` reports it; the stored key uses its label.
    let a1 =
        texts(&mut f.h, f.page).into_iter().find(|r| r["kind"] == "point_label" && r["representation"] == rep.hex() && r["value"] == "A1").unwrap();
    let a1 = a1["terminal"].as_str().unwrap().to_string();
    one_tx(&mut f.h, "movetext", json!({"representation": rep.hex(), "kind": "point_label", "terminal": a1, "to": [90, 215]}));
    let after = f.h.doc();
    let key = format!("text.{}.point_label.A1.at", rep.hex());
    assert!(comp_props(&mut f.h, f.k1, "text.").contains_key(&key), "{:?}", comp_props(&mut f.h, f.k1, "text."));
    let l =
        texts(&mut f.h, f.page).into_iter().find(|r| r["kind"] == "point_label" && r["representation"] == rep.hex() && r["value"] == "A1").unwrap();
    assert!(near(at(&l), (90.0, 215.0)), "{l}");
    f.h.cmdline("u");
    assert_eq!(model_parts(&f.h.doc()), model_parts(&before));
    f.h.run("redo", Value::Null).unwrap();
    assert_eq!(model_parts(&f.h.doc()), model_parts(&after));
    // Refusals.
    let r = rep.hex();
    refused(&mut f.h, "movetext", json!({"representation": r, "kind": "tag", "to": ["a", 0]}), "A text position must be a finite point.");
    refused(&mut f.h, "movetext", json!({"representation": r, "kind": "tag", "to": [1]}), "A text position must be a finite point.");
    refused(
        &mut f.h,
        "movetext",
        json!({"representation": r, "kind": "tag", "to": [1, 2], "justify": "ZZ"}),
        "Justify is two letters: L, C or R, then T, M or B.",
    );
    refused(&mut f.h, "movetext", json!({"representation": r, "kind": "tag", "to": [1, 2], "justify": "TL"}), "Justify is two letters");
    refused(&mut f.h, "movetext", json!({"representation": r, "kind": "point_label", "to": [1, 2]}), "point_label needs the connection point's key.");
    refused(&mut f.h, "movetext", json!({"representation": "0123456789abcdef0123456789abcdef", "kind": "tag", "to": [1, 2]}), "No representation");
    refused(&mut f.h, "movetext", json!({"representation": r, "kind": "description", "to": [1, 2]}), "");
    f.h.check();
}

/// T10 (FP5): `text.<rep>.*` keys follow their symbol: erased with the symbol and with the device
/// in the same transaction, moved to the survivor of a device merge.
#[test]
fn t10_symbol_keys_follow_delete_erase_and_merge() {
    let mut f = fixture_c(0x7031_1004);
    for r in f.r1 {
        f.h.run("movetext", json!({"representation": r.hex(), "kind": "tag", "to": [10, 10]})).unwrap();
        f.h.run("showtext", json!({"kind": "point_label", "show": "off", "representation": r.hex()})).unwrap();
    }
    f.h.run("movetext", json!({"representation": f.r2.hex(), "kind": "tag", "to": [20, 20]})).unwrap();
    // Erase one contact: its keys go in the same transaction.
    let gone = f.r1[2].hex();
    let handles = sym_handles(&mut f.h, f.page, f.r1[2]);
    one_tx(&mut f.h, "erase", json!({"handles": handles}));
    assert!(comp_props(&mut f.h, f.k1, "text.").keys().all(|k| !k.contains(&gone)), "{:?}", comp_props(&mut f.h, f.k1, "text."));
    // Merge a lone contact device into -K01 (rename its tag): its symbol keys move to the
    // survivor. (-K02 has a coil, as -K01 does, so it cannot merge.)
    let k3 = place(&mut f.h, "no", (200.0, 150.0));
    let k3_id: ComponentId = id(&k3, "component");
    let r3 = rep_of(&mut f.h, &k3);
    f.h.run("movetext", json!({"representation": r3.hex(), "kind": "tag", "to": [30, 30]})).unwrap();
    let k3_at = comp_props(&mut f.h, k3_id, &format!("text.{}.tag.at", r3.hex()));
    assert_eq!(k3_at.len(), 1, "{k3_at:?}");
    let moved_before = comp_props(&mut f.h, f.k1, "text.").keys().filter(|k| k.ends_with(".tag.at")).count();
    f.h.run("setprop", json!({"component": k3_id.hex(), "key": "tag", "value": "-K01", "conflict": "merge"})).unwrap();
    let doc = f.h.doc();
    assert!(!doc.components.contains_key(&k3_id), "merged away");
    // The symbol may get a new representation ID on merge; every `text.<rep>.` key lives on the
    // component that owns <rep>, and -K01 gained the moved tag.
    for (cid, c) in &doc.components {
        for k in c.properties.keys().filter(|k| k.starts_with("text.") && k.split('.').count() > 3) {
            let rep_hex = k.split('.').nth(1).unwrap_or_default();
            let owner = doc.representations.iter().find(|(id, _)| id.hex() == rep_hex).map(|(_, r)| r.subject);
            assert_eq!(owner, Some(Subject::Component(*cid)), "{k} on {}", cid.hex());
        }
    }
    let moved_after = comp_props(&mut f.h, f.k1, "text.").keys().filter(|k| k.ends_with(".tag.at")).count();
    assert_eq!(moved_after, moved_before + 1, "{:?}", comp_props(&mut f.h, f.k1, "text."));
    let owner = Subject::Component(f.k1);
    // Delete the device: every text.<rep>.* key goes.
    let Subject::Component(c) = owner else { panic!("a symbol of a component") };
    one_tx(&mut f.h, "devicedelete", json!({"component": c.hex()}));
    let doc = f.h.doc();
    assert!(!doc.components.contains_key(&f.k1));
    // No orphan: every `text.<rep>.` key left names a symbol its component owns (-K02's stays).
    for (cid, c) in &doc.components {
        for k in c.properties.keys().filter(|k| k.starts_with("text.") && k.split('.').count() > 3) {
            let rep_hex = k.split('.').nth(1).unwrap_or_default();
            let owner = doc.representations.iter().find(|(id, _)| id.hex() == rep_hex).map(|(_, r)| r.subject);
            assert_eq!(owner, Some(Subject::Component(*cid)), "{k} on {}", cid.hex());
        }
    }
    assert!(doc.components[&f.k2].properties.contains_key(&format!("text.{}.tag.at", f.r2.hex())), "-K02's moved tag stays");
    f.h.check();
}

/// The Move Text tool: pick, place (one transaction), Reset, Esc writes nothing; MOVE on a tag is
/// refused with the new sentence.
#[test]
fn t10_move_text_tool_and_tag_derived_sentence() {
    let mut f = fixture_c(0x7031_1005);
    let rows = texts(&mut f.h, f.page);
    let t = row_of(&rows, f.r1[0], "tag").clone();
    let head = f.h.head();
    f.h.s.start("movetext").unwrap();
    assert!(prompt(&f.h).starts_with("Pick a tag or label to move"), "{}", prompt(&f.h));
    click(&mut f.h, centre(bx(&t))).unwrap();
    assert!(prompt(&f.h).starts_with("Place the text"), "{}", prompt(&f.h));
    f.h.s.input(Input::Cancel).unwrap();
    assert_eq!(f.h.head(), head, "Esc writes nothing");
    f.h.s.start("movetext").unwrap();
    click(&mut f.h, centre(bx(&t))).unwrap();
    click(&mut f.h, (60.0, 230.0)).unwrap();
    assert_eq!(f.h.head(), head + 1, "{}", echo(&f.h));
    assert!(echo(&f.h).contains("Tag -K01 moved. Reset puts it back."), "{}", echo(&f.h));
    while f.h.s.running.is_some() {
        f.h.s.input(Input::Cancel).unwrap();
    }
    let moved = row_of(&texts(&mut f.h, f.page), f.r1[0], "tag").clone();
    assert_eq!(moved["moved"], json!(true));
    // Reset by the tool.
    f.h.s.start("movetext").unwrap();
    f.h.s.input(Input::Keyword("Reset".into())).unwrap();
    click(&mut f.h, centre(bx(&moved))).unwrap();
    assert_eq!(f.h.head(), head + 2, "{}", echo(&f.h));
    while f.h.s.running.is_some() {
        f.h.s.input(Input::Cancel).unwrap();
    }
    assert_eq!(row_of(&texts(&mut f.h, f.page), f.r1[0], "tag")["moved"], json!(false));
    // CADCraft's MOVE on the tag entity: refused with the new sentence.
    let tag_handle = f.h.pv(|p, pg| {
        let d = pg.drawing_of(p, f.page).unwrap();
        d.model.iter().find(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "-K01")).map(|e| e.handle.hex())
    });
    let e = f.h.run("move", json!({"handles": [tag_handle.unwrap()], "delta": [5, 0]})).unwrap_err();
    assert!(e.contains("The tag text is derived: change the tag with Device Properties, or move it with Move Text."), "{e}");
    f.h.check();
}

/// The Show/Hide Text tool hides a picked tag on the symbol (one transaction, words on the
/// command line) and repeats until Esc.
#[test]
fn t5_show_hide_text_tool() {
    let mut f = fixture_c(0x7031_0504);
    let t = row_of(&texts(&mut f.h, f.page), f.r1[1], "tag").clone();
    let head = f.h.head();
    f.h.s.start("showtext").unwrap();
    assert!(prompt(&f.h).starts_with("Pick a text to hide, or a symbol to show its hidden texts"), "{}", prompt(&f.h));
    click(&mut f.h, centre(bx(&t))).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert!(echo(&f.h).contains("Tag -K01 hidden on this symbol."), "{}", echo(&f.h));
    assert!(f.h.s.running.is_some(), "the tool repeats");
    // A click on nothing derived.
    click(&mut f.h, (5.0, 5.0)).unwrap();
    assert!(echo(&f.h).contains("Pick a tag, a label or a symbol."), "{}", echo(&f.h));
    f.h.s.input(Input::Cancel).unwrap();
    assert!(f.h.s.running.is_none());
    assert!(!shown(row_of(&texts(&mut f.h, f.page), f.r1[1], "tag")));
    f.h.check();
}

/// T11 (AC 3): `text` with `justify: "MC"` is recorded, draws centred and reopens centred; a text
/// without a justification stores no alignment fields (old files read and save the same).
#[test]
fn t11_justified_text_is_recorded_and_reopens_centred() {
    let mut h = H::new(0x7031_1101);
    let page = h.first_page();
    one_tx(&mut h, "text", json!({"at": [100, 100], "text": "MIDDLE", "height": 2.5, "justify": "MC"}));
    one_tx(&mut h, "text", json!({"at": [100, 50], "text": "PLAIN", "height": 2.5}));
    let check_drawn = |h: &mut H| {
        let m = drawn_value(h, page, "MIDDLE");
        assert_eq!(m.len(), 1);
        let (t, _) = &m[0];
        assert_eq!((t.halign, t.valign), (HAlign::Center, VAlign::Middle), "{t:?}");
        let ap = t.align_pt.expect("an alignment point");
        assert!((ap.x - 100.0).abs() < 1e-6 && (ap.y - 100.0).abs() < 1e-6, "{t:?}");
        assert!(t.style.eq_ignore_ascii_case("Standard"), "free text takes Standard: {t:?}");
        let p = drawn_value(h, page, "PLAIN");
        assert_eq!((p[0].0.halign, p[0].0.valign), (HAlign::Left, VAlign::Baseline));
    };
    check_drawn(&mut h);
    let doc = h.doc();
    let json_of = |value: &str| doc.items.values().map(|i| serde_json::to_string(i).unwrap()).find(|s| s.contains(value)).unwrap();
    let mid = json_of("MIDDLE");
    assert!(mid.contains("halign") && mid.contains("valign"), "{mid}");
    let plain = json_of("PLAIN");
    assert!(!plain.contains("halign") && !plain.contains("valign"), "FP4: left/baseline left out: {plain}");
    h.check();
    let dir = tmp_dir("justify");
    let mut h2 = save_and_reopen(h, &dir, "justify.ecad", 0x7031_1102);
    h2.activate(page);
    check_drawn(&mut h2);
    h2.check();
    close_all(&mut h2);
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T11 (AC 3): the golden v1 project from before 031 converts and opens with no transaction, its
/// items serialise without alignment fields, and a save and reopen gives the same document.
#[test]
fn t11_an_old_file_opens_and_saves_unchanged() {
    let golden = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ecad-file/tests/golden/project-v1.ecad");
    let dir = tmp_dir("golden");
    let path = dir.join("old.ecad");
    std::fs::copy(&golden, &path).unwrap();
    let (_, v2) = ecad_cadcraft::file::convert(&path.display().to_string(), None, "tester-031").unwrap();
    let mut h = reopen(&v2, 0x7031_1103);
    let head = h.head();
    let doc = h.doc();
    for i in doc.items.values() {
        let s = serde_json::to_string(i).unwrap();
        assert!(!s.contains("halign") && !s.contains("valign"), "{s}");
    }
    assert!(doc.properties.keys().all(|k| !k.starts_with("text.")));
    let page = h.first_page();
    h.activate(page);
    let _ = texts(&mut h, page);
    assert_eq!(h.head(), head, "opening and reading record nothing");
    h.check();
    let mut h2 = save_and_reopen(h, &dir, "old resaved.ecad", 0x7031_1104);
    assert_eq!(model_parts(&h2.doc()), model_parts(&doc));
    close_all(&mut h2);
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// Update maps (AC 5; design §11, T14)

/// T14 (AC 5): a `text.*` change on a device marks only the pages of its symbols; a page key only
/// that page; a project key every page; an unrelated edit on another page does not re-place the
/// texts of this one.
#[test]
fn t14_update_maps_name_only_the_pages_that_change() {
    let mut h = H::new(0x7031_1401);
    let p1 = h.first_page();
    let c = place(&mut h, "coil", (100.0, 200.0));
    let k1: ComponentId = id(&c, "component");
    let p2: PageId = id(&h.run("newpage", json!({"title": "P2"})).unwrap(), "page");
    place_on(&mut h, "no", (100.0, 200.0), k1);
    let p3: PageId = id(&h.run("newpage", json!({"title": "P3"})).unwrap(), "page");
    let c2 = place(&mut h, "coil", (100.0, 200.0));
    let k2: ComponentId = id(&c2, "component");
    h.activate(p1);
    let changed = |h: &mut H, cmd: &str, params: Value| {
        let n = h.p(|p| p.change_count());
        one_tx(h, cmd, params);
        h.p(|p| p.changes_since(n))
    };
    let s = changed(&mut h, "showtext", json!({"kind": "tag", "show": "off", "component": k1.hex()})).expect("pages known");
    assert!(s.contains(&p1) && s.contains(&p2) && !s.contains(&p3), "{s:?}");
    let s = changed(&mut h, "showtext", json!({"kind": "tag", "show": "off", "page": p3.hex()})).expect("pages known");
    assert!(s.contains(&p3) && !s.contains(&p1) && !s.contains(&p2), "{s:?}");
    let s = changed(&mut h, "setprop", json!({"component": k1.hex(), "key": "description", "value": "Pump"})).expect("pages known");
    assert!(s.contains(&p1) && s.contains(&p2) && !s.contains(&p3), "{s:?}");
    let all = changed(&mut h, "textstyle", json!({"kind": "tag", "height": 3}));
    assert!(all.as_ref().is_none_or(|s| [p1, p2, p3].iter().all(|p| s.contains(p))), "{all:?}");
    // An unrelated edit (a device on page 3) leaves page 1's texts where they were.
    let before = texts(&mut h, p1);
    let s = changed(&mut h, "setprop", json!({"component": k2.hex(), "key": "part_number", "value": "X1"})).expect("pages known");
    assert!(!s.contains(&p1), "{s:?}");
    assert_eq!(texts(&mut h, p1), before);
    h.check();
}

// ---------------------------------------------------------------------------------------------
// Hostile input (T15) and the defaults registry (AC 7, T16)

/// T15: hostile moved-text values and hostile keys draw the automatic place and never panic.
#[test]
fn t15_hostile_text_keys_draw_the_automatic_place() {
    let mut f = fixture_c(0x7031_1501);
    let rep = f.r1[0].hex();
    let auto = row_of(&texts(&mut f.h, f.page), f.r1[0], "tag").clone();
    let key = format!("text.{rep}.tag.at");
    let long = "7".repeat(10_000);
    for value in ["a,b,ZZ", "1e999,0,LT", "NaN,NaN,LT", "1,2", "1,2,3,4", "", ",,", "1,2,QQ", long.as_str()] {
        let before = f.h.doc().components[&f.k1].properties.get(&key).cloned();
        inject(
            &mut f.h,
            vec![Op::SetProperty { object: ObjectRef::Component(f.k1), key: key.clone(), before, after: Some(value.to_string()) }],
            f.page,
            f.other,
        );
        let r = row_of(&texts_raw(&mut f.h, f.page), f.r1[0], "tag").clone();
        assert!(near(at(&r), at(&auto)), "{value:.40}: automatic place: {r}");
        assert_eq!(r["moved"], json!(false), "{value:.40}");
    }
    // Hostile keys: an orphan representation, a show value, unknown kinds, a key of 10,000 chars.
    let orphan = "fedcba9876543210fedcba9876543210";
    let odd = [
        (format!("text.{orphan}.tag.at"), "1,1,LT".to_string()),
        (format!("text.{orphan}.tag.show"), "off".to_string()),
        (format!("text.{rep}.tag.show"), "sometimes".to_string()),
        (format!("text.{rep}.nonsense.show"), "on".to_string()),
        (format!("text.{rep}.point_label.ZZZ.at"), "1,1,LT".to_string()),
        (format!("text.{}", "x".repeat(10_000)), "on".to_string()),
        ("text.".to_string(), "on".to_string()),
        ("text.tag.show".to_string(), "\u{0}".to_string()),
    ];
    let ops: Vec<Op> = odd
        .iter()
        .map(|(k, v)| Op::SetProperty { object: ObjectRef::Component(f.k1), key: k.clone(), before: None, after: Some(v.clone()) })
        .collect();
    inject(&mut f.h, ops, f.page, f.other);
    let rows = texts_raw(&mut f.h, f.page);
    assert!(shown(row_of(&rows, f.r1[0], "tag")), "hostile values are absent: {rows:?}");
    // Hostile command input.
    assert!(f.h.run("pagetexts", json!({"page": "zz"})).is_err());
    let _ = f.h.run("pagetexts", json!({}));
    assert!(f.h.run("defaults", json!({"key": "nope"})).unwrap_err().contains("No default \"nope\"."));
    assert!(f.h.run("defaults", json!({"key": "x".repeat(10_000)})).is_err());
    let _ = f.h.run("movetext", json!({"representation": rep, "kind": "tag", "to": [1e308, 1e308]}));
    f.h.check();
}

/// Lane T's entries of design §3.5 (keys).
fn lane_t_keys() -> Vec<String> {
    let mut keys: Vec<String> = vec!["text.font".into()];
    for (kind, ..) in DEFAULTS {
        keys.push(format!("text.{kind}.height"));
        keys.push(format!("text.{kind}.weight"));
    }
    for kind in ["tag", "point_label", "description", "part_number", "continuation_name"] {
        keys.push(format!("text.{kind}.show"));
    }
    for k in [
        "text.tag.show.strip",
        "text.tag.show.plc",
        "text.run.align",
        "text.run.gap",
        "text.tag.gap",
        "text.line_pitch",
        "text.wire_clearance",
        "text.bold_stroke_weight",
    ] {
        keys.push(k.into());
    }
    keys
}

/// T16 / AC 7: `defaults {}` lists every entry with unique keys that follow the key rule; every
/// built-in is valid for its kind; lane T's entries are all there with `from: builtin` on a new
/// project; after `textstyle {kind: tag, height: 3}` the tag height says `value: 3, from: project`.
#[test]
fn t16_defaults_registry_lists_every_entry_and_where_it_comes_from() {
    let mut h = H::new(0x7031_1601);
    let head = h.head();
    let all = h.run("defaults", json!({})).unwrap();
    assert_eq!(h.head(), head, "reading records nothing");
    let rows = all["defaults"].as_array().unwrap().clone();
    let mut seen = BTreeSet::new();
    for r in &rows {
        let key = r["key"].as_str().unwrap();
        assert!(seen.insert(key.to_string()), "duplicate key {key}");
        let parts: Vec<&str> = key.split('.').collect();
        assert!(
            parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')),
            "key rule: {key}"
        );
        assert!(ecad_cadcraft::defaults::AREAS.contains(&parts[0]), "area first: {key}");
        for f in ["label", "group", "brief", "kind", "builtin", "levels", "value", "from"] {
            assert!(r.get(f).is_some(), "{key} has {f}: {r}");
        }
        assert!(r["levels"].to_string().to_lowercase().contains("project"), "every entry allows Project: {r}");
        assert_eq!(r["value"], r["builtin"], "a new project resolves to the built-in: {r}");
        // The wire label height's project level is the wiring settings, which a new project has.
        if key.starts_with("text.") && key != "text.wire_label.height" {
            assert_eq!(r["from"], "builtin", "{r}");
        }
    }
    for k in lane_t_keys() {
        assert!(seen.contains(&k), "missing registry entry {k}");
    }
    let text = h.run("defaults", json!({"group": "Text"})).unwrap();
    assert!(text["defaults"].as_array().unwrap().iter().all(|r| r["group"] == "Text"));
    // Built-in values of §3.5's single entries.
    let one = |h: &mut H, key: &str| h.run("defaults", json!({"key": key})).unwrap()["defaults"][0].clone();
    for (key, val) in [
        ("text.run.align", 1.0),
        ("text.run.gap", 15.0),
        ("text.tag.gap", 0.6),
        ("text.line_pitch", 1.25),
        ("text.wire_clearance", 0.3),
        ("text.bold_stroke_weight", 0.35),
    ] {
        let r = one(&mut h, key);
        let b = r["builtin"].as_f64().or_else(|| r["builtin"].as_str().and_then(|s| s.parse().ok()));
        assert_eq!(b, Some(val), "{key}: {r}");
    }
    assert_eq!(one(&mut h, "text.font")["builtin"], "BIZ UDPGothic");
    assert_eq!(one(&mut h, "text.tag.show.strip")["builtin"], "first");
    // A project value.
    h.run("textstyle", json!({"kind": "tag", "height": 3})).unwrap();
    let r = one(&mut h, "text.tag.height");
    let value = r["value"].as_f64().or_else(|| r["value"].as_str().and_then(|s| s.parse().ok()));
    assert_eq!((value, r["from"].clone()), (Some(3.0), json!("project")), "{r}");
    // Page and object scope.
    let page = h.first_page();
    h.run("showtext", json!({"kind": "tag", "show": "off", "page": page.hex()})).unwrap();
    let r = h.run("defaults", json!({"key": "text.tag.show", "page": page.hex()})).unwrap()["defaults"][0].clone();
    assert_eq!((r["value"].clone(), r["from"].clone()), (json!("off"), json!("page")), "{r}");
    let ghost = "0123456789abcdef0123456789abcdef";
    assert!(h.run("defaults", json!({"page": ghost})).is_err());
    assert!(h.run("defaults", json!({"component": ghost})).is_err());
}

/// T16: `text.run.gap` 50 on the project joins two groups 40 mm apart into one run.
#[test]
fn t16_run_gap_from_the_project_joins_two_groups() {
    let mut h = H::new(0x7031_1602);
    let page = h.first_page();
    let other = second_page(&mut h);
    let (x1, _) = strip_at(&mut h, &[100.0, 110.0, 120.0, 130.0, 140.0], 150.0, None);
    strip_at(&mut h, &[180.0, 190.0, 200.0, 210.0, 220.0], 150.0, Some(x1));
    assert_eq!(shown_tags(&texts(&mut h, page), "-X01").len(), 2);
    inject(&mut h, vec![set_project_prop("text.run.gap", "50", None)], page, other);
    assert_eq!(shown_tags(&texts_raw(&mut h, page), "-X01").len(), 1);
    let r = h.s.execute("defaults", &json!({"key": "text.run.gap"})).unwrap()["defaults"][0].clone();
    assert_eq!(r["from"], "project", "{r}");
}

// ---------------------------------------------------------------------------------------------
// `projectprops`, `pageprops`, `setprop` reserve the `text.` prefix (design §10.6)

#[test]
fn text_keys_are_refused_by_the_property_commands() {
    let mut f = fixture_c(0x7031_1701);
    let s = "is set with Text styles (textstyle, showtext, movetext).";
    refused(&mut f.h, "projectprops", json!({"props": {"text.font": "Arial"}}), s);
    refused(&mut f.h, "projectprops", json!({"props": {"text.run.gap": "50"}}), s);
    refused(&mut f.h, "pageprops", json!({"page": f.page.hex(), "props": {"text.tag.show": "off"}}), s);
    refused(&mut f.h, "setprop", json!({"component": f.k1.hex(), "key": "text.tag.show", "value": "off"}), s);
    refused(&mut f.h, "setprop", json!({"component": f.k1.hex(), "key": format!("text.{}.tag.at", f.r1[0].hex()), "value": "1,1,LT"}), s);
    refused(&mut f.h, "setprop", json!({"components": [f.k1.hex(), f.k2.hex()], "key": "text.tag.show", "value": "off"}), s);
    // Keys that only start alike are not reserved by this rule.
    f.h.run("setprop", json!({"component": f.k1.hex(), "key": "textile", "value": "cotton"})).unwrap();
}
