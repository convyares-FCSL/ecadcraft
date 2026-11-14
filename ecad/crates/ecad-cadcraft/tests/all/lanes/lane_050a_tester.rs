//! Tester 050a (the style model: standards and style catalogues): acceptance checks from outside,
//! written from brief 050a and design 050 (§2–§4, §6, §10, §12 checks 1–9, §13 and the overseer's
//! FP rulings) only, before the implementation existed.
//!
//! The checks drive the JSON commands the design names (`standard`, `style`, `deletestyle`,
//! `promote`, `styles`, `setdefault`, `textstyle`, `checks`) and read the document, so they depend
//! on as little of the new code's Rust API as possible. The Rust names used are the ones the
//! design fixes: `Document.standard` / `.styles` (FP1, FP2), `Op::SetStyle` / `Op::SetStandard`
//! (by `Op::name`) and `ObjectRef::Standard` (FP3), `Kind::Style`, `Level::Standard`,
//! `text_style` / `text_style_at` (§2.4). Result field names of the new commands are read
//! tolerantly where the design gives none.
//!
//! Each test name starts with the §12 check (`a01`…`a09`) or the focus item it checks (`fp1`…,
//! `res` for resolution order, `ids` for stable IDs, `map` for the update map).

use cadcraft_engine::doc::color::{Color, Rgb};
use cadcraft_engine::doc::{EntityKind, Lineweight};
use ecad_cadcraft::defaults::{self, Kind, Level, Scope, Source};
use ecad_cadcraft::text_style::{self, TextKind};
use ecad_cadcraft::{Persistence, ProjectHandle, printbook};
use ecad_model::{Author, ComponentId, ObjectRef, Op, PageId, Transaction};
use ecad_tokens::sheet::PAPER;
use serde_json::{Value, json};

use crate::common::{H, id, pts};

// ------------------------------------------------------------------------------- helpers

pub(crate) fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn num(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|x| x.trim().trim_end_matches("mm").trim().parse().ok()))
}

/// `styles {catalogue}` rows (design §6.7: a list; tolerated inside `{styles: [...]}`).
pub(crate) fn styles(h: &mut H, catalogue: &str) -> Vec<Value> {
    rows_of(h.run("styles", json!({"catalogue": catalogue})).unwrap_or_else(|e| panic!("styles {catalogue}: {e}")))
}

pub(crate) fn rows_of(v: Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a,
        other => other["styles"].as_array().cloned().unwrap_or_else(|| panic!("no style rows in {other}")),
    }
}

/// One row of `styles` by ID or by name.
pub(crate) fn style_row(h: &mut H, catalogue: &str, which: &str) -> Value {
    let rows = styles(h, catalogue);
    rows.iter().find(|r| s(&r["id"]) == which || s(&r["name"]) == which).cloned().unwrap_or_else(|| panic!("{catalogue} {which} not in {rows:?}"))
}

/// A field's resolved value and source in a `styles` row (§6.7: `fields: {key: {value, from}}`).
pub(crate) fn field(row: &Value, key: &str) -> (Value, String) {
    let f = &row["fields"][key];
    assert!(!f.is_null(), "no field {key} in {row}");
    (f["value"].clone(), s(&f["from"]).to_lowercase())
}

pub(crate) fn field_num(row: &Value, key: &str) -> f64 {
    let (v, _) = field(row, key);
    num(&v).unwrap_or_else(|| panic!("{key} is not a number: {v}"))
}

pub(crate) fn field_text(row: &Value, key: &str) -> String {
    s(&field(row, key).0)
}

/// The ID a `style {new}` result gives (§2.2: `#` + 32 hex digits).
pub(crate) fn own_id(v: &Value) -> String {
    let i = ["id", "style"].iter().map(|k| s(&v[*k])).find(|x| !x.is_empty()).unwrap_or_else(|| panic!("no style ID in {v}"));
    assert!(i.len() == 33 && i.starts_with('#') && i[1..].chars().all(|c| c.is_ascii_hexdigit()), "an own style ID is # + 32 hex: {i}");
    i
}

pub(crate) fn last_tx(h: &mut H) -> Transaction {
    h.p(|p| p.ledger.log().last().cloned()).expect("a transaction")
}

pub(crate) fn op_names(tx: &Transaction) -> Vec<&'static str> {
    tx.ops.iter().map(Op::name).collect()
}

/// Run a writing command and check it appended exactly one transaction.
pub(crate) fn one_tx(h: &mut H, cmd: &str, params: Value) -> (Value, Transaction) {
    let head = h.head();
    let v = h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    assert_eq!(h.head(), head + 1, "{cmd} {params}: one transaction");
    (v, last_tx(h))
}

/// Run a command that must be refused, recording nothing; its message.
pub(crate) fn refused(h: &mut H, cmd: &str, params: Value) -> String {
    let head = h.head();
    let d = h.doc();
    let m = h.run(cmd, params.clone()).err().unwrap_or_else(|| panic!("{cmd} {params} should be refused"));
    assert_eq!(h.head(), head, "{cmd} {params}: a refusal records nothing ({m})");
    assert!(h.doc() == d, "{cmd} {params}: a refusal changes nothing");
    assert!(!m.is_empty());
    m
}

pub(crate) fn undo(h: &mut H) {
    h.cmdline("u");
}

pub(crate) fn redo(h: &mut H) {
    h.cmdline("redo");
}

pub(crate) fn checks_all(h: &mut H) -> Vec<Value> {
    h.run("checks", json!({"limit": 2000})).unwrap()["checks"].as_array().cloned().unwrap_or_default()
}

pub(crate) fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind], "limit": 2000})).unwrap_or_else(|e| panic!("checks {kind}: {e}"))["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Commit `ops` straight to the ledger (a stored value no command writes, as a hostile or older
/// file holds), then re-project the active page.
pub(crate) fn inject(h: &mut H, ops: Vec<Op>) {
    h.cfg(|p| p.ledger.commit(Author::User, "tester 050 stored value", ops)).unwrap();
    reproject(h);
}

pub(crate) fn reproject(h: &mut H) {
    let page = h.first_page();
    let other = match h.doc().page_order.get(1).copied() {
        Some(p) => p,
        None => id(&h.run("newpage", json!({"title": "Other"})).unwrap(), "page"),
    };
    h.activate(other);
    h.activate(page);
    h.check();
}

pub(crate) fn rgb(c: ecad_tokens::Color) -> Rgb {
    Rgb(c.r, c.g, c.b)
}

/// The paper colour an IEC code draws in (040's table).
pub(crate) fn code_colour(code: &str) -> Color {
    Color::True(rgb(ecad_cadcraft::look::wire_colours(code, &PAPER).unwrap_or_else(|| panic!("{code} is a colour")).solid))
}

pub(crate) const INK: Color = Color::Index(7);

/// The text entities on `page` whose value is `text`.
pub(crate) fn texts_on(h: &mut H, page: PageId, text: &str) -> Vec<std::sync::Arc<cadcraft_engine::doc::Entity>> {
    h.pv(|p, pg| pg.entities_of(p, page)).into_iter().filter(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == text)).collect()
}

pub(crate) fn tag(h: &mut H, c: ComponentId) -> String {
    ecad_cadcraft::devices::tag_words(&h.doc(), c)
}

pub(crate) fn coil(h: &mut H, x: f64) -> ComponentId {
    id(&h.run("place", json!({"symbol": "coil", "at": [x, 100]})).unwrap(), "component")
}

pub(crate) fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t050 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn close_all(h: &mut H) {
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
}

pub(crate) fn reopen(path: &str, seed: u64) -> H {
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, path, seed).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    h.trail.push(d);
    h
}

/// Save `h` at `dir/name` and give the path.
pub(crate) fn save_as(h: &mut H, dir: &std::path::Path, name: &str) -> String {
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join(name).display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    path
}

pub(crate) fn save_and_reopen(mut h: H, dir: &std::path::Path, name: &str, seed: u64) -> H {
    let path = save_as(&mut h, dir, name);
    close_all(&mut h);
    drop(h);
    reopen(&path, seed)
}

/// Every transaction in the log goes through the file's record form and back unchanged (FP3: the
/// `record.rs` / `v1.rs` arms).
pub(crate) fn records_round_trip(h: &mut H) {
    let log = h.p(|p| p.ledger.full_log().unwrap());
    for tx in &log {
        let v = ecad_file::record::tx_value(tx, &[0; 32]).unwrap_or_else(|e| panic!("{}: {e}", tx.label));
        let back = ecad_file::record::decode_tx(&v).unwrap_or_else(|e| panic!("{}: {e:?}", tx.label));
        assert_eq!(&back, tx, "{}: the record reads back as written", tx.label);
    }
}

/// Every op of the new kinds applies and its inverse restores the document exactly (FP3).
pub(crate) fn inverses_restore(h: &mut H) -> usize {
    let log = h.p(|p| p.ledger.full_log().unwrap());
    let mut doc = ecad_model::Ledger::new(0).document().clone();
    let mut n = 0;
    for tx in &log {
        for op in &tx.ops {
            let before = doc.clone();
            op.apply(&mut doc).unwrap_or_else(|e| panic!("{}: {} applies: {e:?}", tx.label, op.name()));
            if is_new_op(op) {
                let mut back = doc.clone();
                op.inverse().apply(&mut back).unwrap_or_else(|e| panic!("{}: inverse of {}: {e:?}", tx.label, op.name()));
                assert!(back == before, "{}: the inverse of {} restores the document", tx.label, op.name());
                assert_eq!(op.inverse().inverse(), *op, "{}: inverse twice is the op", op.name());
                n += 1;
            }
        }
    }
    n
}

pub(crate) fn is_new_op(op: &Op) -> bool {
    matches!(op.name(), "SetStyle" | "SetStandard") || matches!(op, Op::SetProperty { object: ObjectRef::Standard, .. })
}

/// The record JSON of the last transaction, for building stored values no command writes.
pub(crate) fn last_tx_json(h: &mut H) -> Value {
    let tx = last_tx(h);
    ecad_file::record::tx_value(&tx, &[0; 32]).unwrap()
}

/// Decode a record JSON back into its ops (as an older or hostile file is read).
pub(crate) fn decoded_ops(v: &Value) -> Vec<Op> {
    ecad_file::record::decode_tx(v).unwrap_or_else(|e| panic!("a hostile but well-formed record still reads: {e:?}")).ops
}

/// The first JSON object under `v` that has key `k`.
pub(crate) fn find_obj_mut<'a>(v: &'a mut Value, k: &str) -> Option<&'a mut serde_json::Map<String, Value>> {
    if v.as_object().is_some_and(|m| m.contains_key(k)) {
        return v.as_object_mut();
    }
    match v {
        Value::Object(m) => m.values_mut().find_map(|x| find_obj_mut(x, k)),
        Value::Array(a) => a.iter_mut().find_map(|x| find_obj_mut(x, k)),
        _ => None,
    }
}

/// The 031/040 built-in text sizes (cap height, weight) of every kind, from `dev` before 050a
/// (`defaults/text.rs`); the built-in text styles must carry them exactly (design §4.1).
pub(crate) const BUILTIN_TEXT: [(&str, f64, &str); 21] = [
    ("tag", 2.5, "bold"),
    ("point_label", 1.5, "regular"),
    ("description", 1.6, "regular"),
    ("part_number", 1.6, "regular"),
    ("continuation_name", 2.0, "regular"),
    ("reference", 2.0, "regular"),
    // Brief 049's continuation caption (merged after 050a).
    ("continuation_caption", 2.5, "regular"),
    ("wire_label", 1.6, "regular"),
    ("cable_text", 1.6, "regular"),
    ("location", 1.6, "regular"),
    ("title_value", 2.5, "regular"),
    ("title_caption", 1.8, "regular"),
    ("title_main", 3.5, "bold"),
    ("title_page", 7.0, "regular"),
    ("title_rev", 5.0, "regular"),
    ("grid", 3.0, "regular"),
    ("table", 2.5, "regular"),
    ("table_header", 2.5, "bold"),
    ("table_heading", 3.5, "regular"),
    ("cover", 10.0, "regular"),
    ("free", 2.5, "regular"),
];

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// A project with a standard named "House".
fn with_standard(seed: u64) -> H {
    let mut h = H::new(seed);
    one_tx(&mut h, "standard", json!({"new": "House"}));
    h
}

// ------------------------------------------------------------- a01: IEC and the built-ins

/// §12.1, AC 4: a new project reads IEC (built in), and `styles` lists the 20 built-in text styles
/// with 031/040's heights and weights; `text_style(doc, kind)` keeps its signature and gives them.
#[test]
fn a01_new_project_is_iec_with_twenty_builtin_text_styles_at_031_sizes() {
    let mut h = H::new(0x50a_0101);
    let head = h.head();
    let v = h.run("standard", json!({})).unwrap();
    assert_eq!((s(&v["name"]), v["id"].clone()), ("IEC (built in)".to_string(), Value::Null), "{v}");
    let d = h.doc();
    assert!(d.standard.is_none() && d.styles.is_empty(), "FP1/FP2: nothing stored for a new project");
    let rows = styles(&mut h, "text");
    assert_eq!(rows.len(), 21, "{rows:?}");
    for (word, height, weight) in BUILTIN_TEXT {
        let r = rows.iter().find(|r| s(&r["id"]) == word).unwrap_or_else(|| panic!("built-in text style {word}"));
        assert_eq!(s(&r["layer"]), "builtin", "{word}");
        assert!(!s(&r["name"]).is_empty(), "{word} has a name");
        assert!(near(field_num(r, "height"), height), "{word} height: {r}");
        assert_eq!(field_text(r, "weight"), weight, "{word}");
        assert_eq!(field_text(r, "colour"), "none", "{word}: colour is new, built-in none (ink)");
        assert_eq!(field_text(r, "font"), "project", "{word}");
        let kind = TextKind::ALL.into_iter().find(|k| k.word() == word).unwrap();
        // 031's drift test: the shared contract's signature, unchanged.
        let st: text_style::TextStyle = text_style::text_style(&d, kind);
        assert!(near(st.height, height), "{word}: text_style height {}", st.height);
        assert_eq!(st.weight == text_style::Weight::Bold, weight == "bold", "{word}: text_style weight");
    }
    assert_eq!(h.head(), head, "reads record nothing");
}

/// §4.2, §4.3: the location catalogue ships `box` (today's look) and the frame catalogue `sheet`.
#[test]
fn a01_location_and_frame_catalogues_ship_box_and_sheet() {
    let mut h = H::new(0x50a_0102);
    let rows = styles(&mut h, "location");
    assert_eq!(rows.iter().map(|r| s(&r["id"])).collect::<Vec<_>>(), ["box"], "{rows:?}");
    let b = &rows[0];
    assert_eq!(field_text(b, "line_type"), "dashed");
    assert!(near(field_num(b, "line_weight"), 0.25));
    assert_eq!(field_text(b, "line_colour"), "none");
    assert_eq!(field_text(b, "label_style"), "location");
    assert_eq!(field_text(b, "fill"), "none");
    let rows = styles(&mut h, "frame");
    assert_eq!(rows.iter().map(|r| s(&r["id"])).collect::<Vec<_>>(), ["sheet"], "{rows:?}");
    let f = &rows[0];
    assert!(near(field_num(f, "border_weight"), 0.5));
    assert!(near(field_num(f, "inner_weight"), 0.25));
    assert_eq!(field_text(f, "line_colour"), "none");
    assert_eq!(field_text(f, "grid_labels"), "on");
    assert_eq!(field_text(f, "title_block"), "standard");
    // Every catalogue at once: text + location + frame (050b adds wire).
    let all = rows_of(h.run("styles", json!({})).unwrap());
    for cat in ["text", "location", "frame"] {
        assert!(all.iter().any(|r| s(&r["catalogue"]) == cat), "{cat} listed without a filter");
    }
}

/// §12.1 / AC 2 by entities: writing a project record equal to each built-in value moves nothing
/// on the page (the built-ins carry today's values; the screenshot compare against `dev` runs in
/// phase 2 with `cadcraft-cli`). The frame's weights and the box's linetype are 019/024's.
#[test]
fn a01_built_in_styles_draw_todays_page() {
    let mut h = H::new(0x50a_0103);
    let page = h.first_page();
    coil(&mut h, 100.0);
    h.run("wire", json!({"points": pts(&[(0.0, 50.0), (60.0, 50.0)])})).unwrap();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[80, 80], [140, 140]]})).unwrap();
    // The second page `reproject` switches through exists before the first snapshot (page totals).
    reproject(&mut h);
    let ents = h.pv(|p, pg| pg.entities_of(p, page));
    let frame_ents = h.pv(|p, pg| pg.frame_entities_of(p, page));
    let frame: Vec<_> = frame_ents.iter().filter(|e| e.common.layer == ecad_cadcraft::frame::LAYER).collect();
    assert!(!frame.is_empty(), "the frame is drawn");
    let weights: std::collections::BTreeSet<_> = frame.iter().map(|e| format!("{:?}", e.common.lineweight)).collect();
    for w in [Lineweight::Mm100(50), Lineweight::Mm100(25)] {
        assert!(weights.contains(&format!("{w:?}")), "frame weight {w:?} in {weights:?}");
    }
    let boxes: Vec<_> = ents.iter().filter(|e| e.common.layer == ecad_cadcraft::locations::LAYER).collect();
    assert!(boxes.iter().any(|e| e.common.linetype.eq_ignore_ascii_case("DASHED")), "the box is dashed");
    let before: Vec<String> = ents.iter().chain(frame_ents.iter()).map(|e| format!("{:?}{:?}", e.common, e.kind)).collect();
    // The same values written as project records: nothing on the page moves.
    for (word, height, weight) in BUILTIN_TEXT {
        h.run("setstyle", json!({"catalogue": "text", "id": word, "fields": {"height": height, "weight": weight}})).unwrap();
    }
    h.run("setstyle", json!({"catalogue": "location", "id": "box", "fields": {"line_type": "dashed", "line_weight": 0.25}})).unwrap();
    h.run("setstyle", json!({"catalogue": "frame", "id": "sheet", "fields": {"border_weight": 0.5, "inner_weight": 0.25}})).unwrap();
    reproject(&mut h);
    let after: Vec<String> = h
        .pv(|p, pg| pg.entities_of(p, page).into_iter().chain(pg.frame_entities_of(p, page)).collect::<Vec<_>>())
        .iter()
        .map(|e| format!("{:?}{:?}", e.common, e.kind))
        .collect();
    let (mut before, mut after) = (before, after);
    before.sort();
    after.sort();
    let gone: Vec<&String> = before.iter().filter(|x| !after.contains(x)).collect();
    let new: Vec<&String> = after.iter().filter(|x| !before.contains(x)).collect();
    assert!(
        gone.is_empty() && new.is_empty(),
        "built-in values written as records draw the same page:
before only {gone:#?}
after only {new:#?}"
    );
}

// ---------------------------------------------------------------- a02: registry and levels

/// AC 3, §4, question 26: every choice entry is `Kind::Style` of its catalogue, its built-in names
/// a built-in style, and it allows exactly the levels the design gives (text S P O, location
/// S P O, frame S P G). The old height/weight/font entries leave; `text.font` and `.show` stay.
#[test]
fn a02_registry_choice_entries_and_their_levels() {
    let mut h = H::new(0x50a_0201);
    assert!(defaults::AREAS.contains(&"location"), "FP5: the new area");
    let builtin_ids = |h: &mut H, cat: &str| styles(h, cat).iter().filter(|r| s(&r["layer"]) == "builtin").map(|r| s(&r["id"])).collect::<Vec<_>>();
    let text_ids = builtin_ids(&mut h, "text");
    let mut checked = 0;
    for (key, cat, ids, allowed, not) in [
        ("location.style", "location", builtin_ids(&mut h, "location"), vec![Level::Standard, Level::Project, Level::Object], vec![Level::Page]),
        ("print.frame.style", "frame", builtin_ids(&mut h, "frame"), vec![Level::Standard, Level::Project, Level::Page], vec![Level::Object]),
    ]
    .into_iter()
    .chain(TextKind::ALL.iter().map(|k| {
        let key: &'static str = Box::leak(format!("text.{}.style", k.word()).into_boxed_str());
        (key, "text", text_ids.clone(), vec![Level::Standard, Level::Project, Level::Object], vec![Level::Page])
    })) {
        let e = defaults::find(key).unwrap_or_else(|| panic!("choice entry {key}"));
        assert!(matches!(&e.kind, Kind::Style(c) if *c == cat), "{key} is Kind::Style({cat})");
        let b = defaults::Resolved { value: e.builtin, from: Source::Builtin }.text();
        assert!(ids.contains(&b), "{key}: built-in {b} names a built-in style of {cat} ({ids:?})");
        for l in allowed {
            assert!(e.allows(l), "{key} allows {l:?}");
        }
        for l in not {
            assert!(!e.allows(l), "{key} does not allow {l:?}");
        }
        checked += 1;
    }
    assert_eq!(checked, 22);
    for k in TextKind::ALL {
        for old in ["height", "weight", "font"] {
            assert!(defaults::find(&format!("text.{}.{old}", k.word())).is_none(), "FP4: text.{}.{old} left the registry", k.word());
        }
    }
    for k in ["tag", "point_label", "description", "part_number", "continuation_name", "reference", "location", "wire_label"] {
        assert!(defaults::find(&format!("text.{k}.show")).is_some(), "the .show entries stay: {k}");
    }
    assert!(defaults::find("text.font").is_some(), "the project font stays");
}

/// §12.2: a standard holds registry values at the Standard level; the chain shows them; a project
/// value wins over it; undo twice is IEC again. (The design's example has the coil prefix built-in
/// `Q`; the registry's built-in is `K`, so the standard writes `Q`.)
#[test]
fn a02_standard_level_values_resolve_between_built_in_and_project() {
    let mut h = H::new(0x50a_0202);
    let base = h.doc();
    let (v, tx) = one_tx(&mut h, "standard", json!({"new": "House"}));
    assert_eq!(op_names(&tx), ["SetStandard"], "{v}");
    let r = h.run("standard", json!({})).unwrap();
    assert_eq!(s(&r["name"]), "House");
    assert_eq!(s(&r["based_on"]).to_lowercase(), "iec");
    assert!(r["origin"].is_null(), "FP1: origin is reserved, null in 050");
    assert!(!s(&r["id"]).is_empty(), "a fresh StandardId");
    let e = defaults::find("symbol.coil.prefix").unwrap();
    let (_, tx) = one_tx(&mut h, "setdefault", json!({"key": e.key, "level": "standard", "value": "Q"}));
    assert!(matches!(tx.ops.as_slice(), [Op::SetProperty { object: ObjectRef::Standard, .. }]), "{:?}", tx.ops);
    let d = h.doc();
    assert_eq!(defaults::resolve(&d, e, Scope::project()).from, Source::Standard);
    let chain: Vec<(Source, Option<String>)> =
        defaults::chain(&d, e, Scope::project()).iter().map(|(s, v)| (*s, v.map(|v| defaults::Resolved { value: v, from: *s }.text()))).collect();
    assert_eq!(chain[..2], [(Source::Builtin, Some("K".into())), (Source::Standard, Some("Q".into()))], "{chain:?}");
    let k = coil(&mut h, 100.0);
    assert!(tag(&mut h, k).starts_with("-Q"), "a new coil takes the standard's prefix: {}", tag(&mut h, k));
    // The project's own value wins over the standard's.
    h.run("setdefault", json!({"key": e.key, "level": "project", "value": "R"})).unwrap();
    assert_eq!(defaults::resolve(&h.doc(), e, Scope::project()).from, Source::Project);
    undo(&mut h);
    assert_eq!(defaults::resolve(&h.doc(), e, Scope::project()).from, Source::Standard, "the standard value is back");
    for _ in 0..3 {
        undo(&mut h);
    }
    assert!(h.doc() == base, "undo twice more is IEC (built in)");
    assert_eq!(s(&h.run("standard", json!({})).unwrap()["name"]), "IEC (built in)");
}

/// §2.4, question 26: a field resolves object (wire only, 050b) → project → standard → built-in
/// style → field built-in; a choice resolves object → page (frame only) → project → standard →
/// built-in. Each level is shown by `styles`' `from`.
#[test]
fn res_field_and_choice_resolution_order() {
    let mut h = with_standard(0x50a_0203);
    let tag_h = |h: &mut H| field_num(&style_row(h, "text", "tag"), "height");
    let tag_from = |h: &mut H| field(&style_row(h, "text", "tag"), "height").1;
    assert_eq!((tag_h(&mut h), tag_from(&mut h)), (2.5, "builtin".into()));
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": 4}})).unwrap();
    assert_eq!((tag_h(&mut h), tag_from(&mut h)), (4.0, "standard".into()));
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 3}})).unwrap();
    assert_eq!((tag_h(&mut h), tag_from(&mut h)), (3.0, "project".into()));
    assert!(near(text_style::text_style(&h.doc(), TextKind::Tag).height, 3.0));
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": null}})).unwrap();
    assert_eq!((tag_h(&mut h), tag_from(&mut h)), (4.0, "standard".into()), "clearing the project field falls to the standard");
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": null}})).unwrap();
    assert_eq!((tag_h(&mut h), tag_from(&mut h)), (2.5, "builtin".into()));

    // The choice: standard → project → object; there is no page level for text.
    let page = h.first_page();
    let (k1, k2) = (coil(&mut h, 100.0), coil(&mut h, 200.0));
    let big = own_id(
        &h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Big", "copy_of": "tag", "layer": "standard", "fields": {"height": 6}}))
            .unwrap(),
    );
    let mid =
        own_id(&h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Mid", "copy_of": "tag", "fields": {"height": 4.5}})).unwrap());
    let small =
        own_id(&h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Small", "copy_of": "tag", "fields": {"height": 1.5}})).unwrap());
    let at = |c: ComponentId| Scope { page: Some(page), component: Some(c), ..Scope::project() };
    let height = |h: &mut H, c: ComponentId| text_style::text_style_at(&h.doc(), TextKind::Tag, at(c)).height;
    let e = defaults::find("text.tag.style").unwrap();
    h.run("setdefault", json!({"key": e.key, "level": "standard", "value": big})).unwrap();
    assert_eq!((height(&mut h, k1), defaults::resolve(&h.doc(), e, at(k1)).from), (6.0, Source::Standard));
    h.run("setdefault", json!({"key": e.key, "level": "project", "value": "Mid"})).unwrap();
    assert_eq!((height(&mut h, k1), defaults::resolve(&h.doc(), e, at(k1)).from), (4.5, Source::Project), "a name is accepted ({mid})");
    h.run("setdefault", json!({"key": e.key, "level": "object", "component": k1.hex(), "value": small})).unwrap();
    assert_eq!((height(&mut h, k1), defaults::resolve(&h.doc(), e, at(k1)).from), (1.5, Source::Object));
    assert!(near(height(&mut h, k2), 4.5), "only K1 chose its own");
    let m = refused(&mut h, "setdefault", json!({"key": e.key, "level": "page", "page": page.hex(), "value": "tag"}));
    assert!(m.contains("cannot be set at page level"), "{m}");
    // Frames: page level allowed, object not.
    let f = defaults::find("print.frame.style").unwrap();
    h.run("setdefault", json!({"key": f.key, "level": "page", "page": page.hex(), "value": "sheet"})).unwrap();
    assert_eq!(defaults::resolve(&h.doc(), f, Scope::page(page)).from, Source::Page);
}

/// AC 3 (031's hostile table, one row more): hostile values stored at the Standard level and in
/// style records resolve as absent, falling through to the built-in; nothing panics.
#[test]
fn a02_hostile_standard_level_values_resolve_to_the_built_in() {
    let mut h = with_standard(0x50a_0204);
    let huge = "9".repeat(10_000);
    for (key, value) in [
        ("symbol.coil.prefix", "k1!".to_string()),
        ("symbol.coil.prefix", huge.clone()),
        ("text.tag.style", "#zz".to_string()),
        ("text.tag.style", format!("#{}", "f".repeat(32))),
        ("text.tag.style", "box".to_string()),
        ("text.tag.style", String::new()),
        ("print.frame.style", "\u{0}".to_string()),
        ("text.tag.show", "maybe".to_string()),
        ("print.colour", "sepia".to_string()),
    ] {
        let e = defaults::find(key).unwrap();
        let before = h.doc().standard.as_ref().and_then(|s| s.values.get(key).cloned());
        inject(&mut h, vec![Op::SetProperty { object: ObjectRef::Standard, key: key.into(), before, after: Some(value.clone()) }]);
        let d = h.doc();
        let r = defaults::resolve(&d, e, Scope::project());
        assert_eq!(r.from, Source::Builtin, "{key}={value:.40}: a hostile standard value counts as absent");
    }
    // Hostile record fields: the style resolves to its built-in values. The record is built from a
    // real `SetStyle` (as a file holds it), injected, read, then taken back by its inverse.
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 3}})).unwrap();
    let template = last_tx_json(&mut h);
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": null}})).unwrap();
    for (k, v) in [("height", "NaN"), ("height", "11"), ("height", "-1"), ("height", "1e308"), ("weight", "heavy"), ("colour", "PURPLISH\u{7}")] {
        let mut tx = template.clone();
        find_obj_mut(&mut tx, "fields").expect("a record with fields").insert("fields".into(), json!({k: v}));
        let ops = decoded_ops(&tx);
        inject(&mut h, ops.clone());
        let st = text_style::text_style(&h.doc(), TextKind::Tag);
        assert!(near(st.height, 2.5) && st.weight == text_style::Weight::Bold, "{k}={v:?}: the built-in tag style");
        let _ = styles(&mut h, "text");
        inject(&mut h, ops.iter().rev().map(Op::inverse).collect());
    }
}

/// §2.2, §5.7: a dangling style reference (a hostile file, or a merge) falls through to the next
/// level and draws; Checks reports `style-unknown` (Info) for text, location and frame.
#[test]
fn ids_a_dangling_reference_falls_through_and_is_reported() {
    let mut h = H::new(0x50a_0205);
    let page = h.first_page();
    let k = coil(&mut h, 100.0);
    let b: ComponentId = id(&h.run("locationbox", json!({"location": "+CP1", "corners": [[80, 80], [140, 140]]})).unwrap(), "component");
    let ghost = format!("#{}", "a".repeat(32));
    inject(
        &mut h,
        vec![
            Op::SetProperty { object: ObjectRef::Component(k), key: "text.tag.style".into(), before: None, after: Some(ghost.clone()) },
            Op::SetProperty { object: ObjectRef::Component(b), key: "location.style".into(), before: None, after: Some(ghost.clone()) },
            Op::SetProperty { object: ObjectRef::Page(page), key: "print.frame.style".into(), before: None, after: Some(ghost.clone()) },
        ],
    );
    let at = Scope { page: Some(page), component: Some(k), ..Scope::project() };
    assert!(near(text_style::text_style_at(&h.doc(), TextKind::Tag, at).height, 2.5), "drawn as tag");
    let t = tag(&mut h, k);
    assert!(!texts_on(&mut h, page, &t).is_empty(), "the tag still draws");
    let rows = checks_of(&mut h, "style-unknown");
    assert_eq!(rows.len(), 3, "one Info row per dangling reference: {rows:?}");
    assert!(rows.iter().all(|r| r.to_string().to_lowercase().contains("info")), "{rows:?}");
    assert!(h.doc().check().is_ok(), "§3.3: a dangling reference is not an invariant breach");
}

// ------------------------------------------------------------------ a03: a style changes

/// §12.3: one `style` call sets the tag colour and height; tags are red on screen and in colour
/// print, ink in mono print, at 3 mm; one transaction of one `SetStyle`, one undo.
#[test]
fn a03_tag_colour_and_height_in_one_transaction() {
    let mut h = H::new(0x50a_0301);
    let page = h.first_page();
    let k = coil(&mut h, 100.0);
    let t = tag(&mut h, k);
    let before = h.doc();
    let (_, tx) = one_tx(&mut h, "setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD", "height": 3}}));
    assert_eq!(op_names(&tx), ["SetStyle"], "one record write");
    assert!(near(text_style::text_style(&h.doc(), TextKind::Tag).height, 3.0));
    let red = code_colour("RD");
    let shown = texts_on(&mut h, page, &t);
    assert!(!shown.is_empty() && shown.iter().all(|e| e.common.color == red), "the tag is red on screen");
    let d = h.doc();
    let fan = Default::default();
    let (colour, _) = printbook::build_in(&d, &[page], &fan, 0.01, None, false, true).unwrap();
    let tags = |m: &cadcraft_engine::doc::Drawing| {
        m.model.iter().filter(|e| matches!(&e.kind, EntityKind::Text(x) if x.value == t)).map(|e| e.common.color).collect::<Vec<_>>()
    };
    assert!(tags(&colour).iter().all(|c| *c == red) && !tags(&colour).is_empty(), "red in colour print");
    let (mono, _) = printbook::build_pending(&d, &[page], &fan, 0.01, None, false).unwrap();
    assert!(tags(&mono).iter().all(|c| *c == INK) && !tags(&mono).is_empty(), "ink in mono print (question 10)");
    undo(&mut h);
    assert!(h.doc() == before, "one undo");
    assert!(texts_on(&mut h, page, &t).iter().all(|e| e.common.color != red));
}

/// §6.2: refusals name the field and its allowed values; nothing is recorded.
#[test]
fn a03_style_refusals_say_why() {
    let mut h = H::new(0x50a_0302);
    let many: serde_json::Map<String, Value> = (0..65).map(|i| (format!("f{i}"), json!("x"))).collect();
    for (params, words) in [
        (json!({"catalogue": "text", "id": "tag", "fields": {"height": 11}}), "height is 0.5 to 10 mm"),
        (json!({"catalogue": "location", "id": "box", "fields": {"line_type": "wavy"}}), "line_type is one of dashed, continuous, dashdot"),
        (json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": 3}}), "Make a standard first"),
        (json!({"catalogue": "text", "id": "nope", "fields": {"height": 3}}), ""),
        (json!({"catalogue": "Bad Word", "id": "tag", "fields": {"height": 3}}), ""),
        (json!({"catalogue": "text", "new": true, "name": "", "copy_of": "tag"}), ""),
        (json!({"catalogue": "text", "new": true, "name": "x".repeat(65), "copy_of": "tag"}), ""),
        (json!({"catalogue": "text", "new": true, "name": "Bell\u{7}", "copy_of": "tag"}), ""),
        (json!({"catalogue": "text", "id": "tag", "fields": {"colour": "x".repeat(257)}}), ""),
        (json!({"catalogue": "text", "id": "tag", "fields": many}), ""),
        (json!({"catalogue": "text", "id": "tag", "fields": {"height": "NaN"}}), ""),
        (json!({"catalogue": "text", "id": "tag", "layer": "sideways", "fields": {"height": 3}}), ""),
    ] {
        let m = refused(&mut h, "setstyle", params.clone());
        assert!(m.contains(words), "{params}: {m}");
    }
    // A name must be unique in its catalogue.
    h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag"})).unwrap();
    refused(&mut h, "setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag"}));
}

/// §6.6: `textstyle` keeps its JSON form and now writes the kind's style record at project level;
/// the wire label height still writes `settings.wiring`.
#[test]
fn a03_textstyle_writes_the_kinds_style_record() {
    let mut h = H::new(0x50a_0303);
    let (v, tx) = one_tx(&mut h, "textstyle", json!({"kind": "tag", "height": 3}));
    let row = v["kinds"].as_array().and_then(|a| a.iter().find(|k| k["kind"] == json!("tag")).cloned()).unwrap_or_default();
    assert_eq!(s(&row["style"]), "tag", "{v}");
    assert!(op_names(&tx).contains(&"SetStyle"), "{:?}", op_names(&tx));
    assert!(!h.doc().properties.contains_key("text.tag.height"), "FP4: no old property written");
    let r = style_row(&mut h, "text", "tag");
    assert!(near(field_num(&r, "height"), 3.0) && field(&r, "height").1 == "project", "the project record: {r}");
    let (_, tx) = one_tx(&mut h, "textstyle", json!({"kind": "wire_label", "height": 2}));
    assert!(op_names(&tx).contains(&"SetProjectSettings"), "the wire label height stays in settings.wiring: {:?}", op_names(&tx));
    assert!(near(h.doc().settings.wiring.label_height, 2.0));
    let r = style_row(&mut h, "text", "wire_label");
    assert!(near(field_num(&r, "height"), 2.0) && field(&r, "height").1 == "project", "read as the project field: {r}");
}

// --------------------------------------------- a04: own styles, object choices, delete

/// §12.4, §6.3, question 28: an own style with a stable ID; an object chooses it; deleting it is
/// refused without `replace_with` (with the user count) and with one rewrites the user in the
/// same transaction; built-ins cannot be deleted.
#[test]
fn a04_own_style_object_choice_and_delete_with_a_replacement() {
    let mut h = H::new(0x50a_0401);
    let page = h.first_page();
    let (k1, k2) = (coil(&mut h, 100.0), coil(&mut h, 200.0));
    let (v, tx) =
        one_tx(&mut h, "setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}}));
    assert_eq!(op_names(&tx), ["SetStyle"]);
    let large = own_id(&v);
    let r = style_row(&mut h, "text", &large);
    assert_eq!((s(&r["name"]), s(&r["layer"])), ("Tag large".into(), "project".into()));
    assert_eq!(field_text(&r, "weight"), "bold", "copy_of copies every field of tag");
    let (_, tx) = one_tx(&mut h, "setdefault", json!({"key": "text.tag.style", "level": "object", "component": k1.hex(), "value": "Tag large"}));
    assert!(
        matches!(tx.ops.as_slice(), [Op::SetProperty { object: ObjectRef::Component(c), key, after: Some(a), .. }] if *c == k1 && key == "text.tag.style" && *a == large),
        "{:?}",
        tx.ops
    );
    let at = |c| Scope { page: Some(page), component: Some(c), ..Scope::project() };
    assert!(near(text_style::text_style_at(&h.doc(), TextKind::Tag, at(k1)).height, 5.0), "K1's tag is large");
    assert!(near(text_style::text_style_at(&h.doc(), TextKind::Tag, at(k2)).height, 2.5), "K2's is not");
    let (t1, t2) = (tag(&mut h, k1), tag(&mut h, k2));
    let hgt =
        |h: &mut H, t: &str| texts_on(h, page, t).first().map(|e| if let EntityKind::Text(x) = &e.kind { x.height } else { 0.0 }).unwrap_or(0.0);
    let (a, b) = (hgt(&mut h, &t1), hgt(&mut h, &t2));
    assert!(near(a / b, 2.0), "drawn 5 mm against 2.5 mm: {a} {b}");
    // Users.
    let u = rows_of(h.run("styles", json!({"catalogue": "text", "users": true})).unwrap());
    let ur = u.iter().find(|r| s(&r["id"]) == large).unwrap();
    assert_eq!(ur["users"]["explicit"], json!(1), "{ur}");
    // Delete: built-in refused; with users refused naming the count; with replace_with, one tx.
    let m = refused(&mut h, "deletestyle", json!({"catalogue": "text", "id": "tag"}));
    assert!(m.contains("Built-in styles cannot be deleted"), "{m}");
    let m = refused(&mut h, "deletestyle", json!({"catalogue": "text", "id": large}));
    assert!(m.contains('1') && m.contains("replace_with"), "{m}");
    refused(&mut h, "deletestyle", json!({"catalogue": "text", "id": large, "replace_with": "nope"}));
    refused(&mut h, "deletestyle", json!({"catalogue": "text", "id": large, "replace_with": "box"}));
    let before = h.doc();
    let (_, tx) = one_tx(&mut h, "deletestyle", json!({"catalogue": "text", "name": "Tag large", "replace_with": "tag"}));
    let names = op_names(&tx);
    assert!(names.contains(&"SetStyle") && names.contains(&"SetProperty"), "{names:?}");
    let d = h.doc();
    assert_eq!(d.components[&k1].properties.get("text.tag.style").map(String::as_str), Some("tag"), "the user moved to the replacement");
    assert!(near(text_style::text_style_at(&d, TextKind::Tag, at(k1)).height, 2.5), "K1 is back to normal");
    assert!(styles(&mut h, "text").iter().all(|r| s(&r["id"]) != large));
    undo(&mut h);
    assert!(h.doc() == before, "one undo restores the style and K1's choice");
}

/// Question 28: renaming changes only the name; the ID, and every user, stay.
#[test]
fn ids_rename_keeps_the_id_and_the_users() {
    let mut h = H::new(0x50a_0402);
    let k = coil(&mut h, 100.0);
    let large = own_id(
        &h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}})).unwrap(),
    );
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k.hex(), "value": large})).unwrap();
    let (_, tx) = one_tx(&mut h, "setstyle", json!({"catalogue": "text", "id": large, "rename": "Tag huge"}));
    assert_eq!(op_names(&tx), ["SetStyle"], "a rename touches no user");
    let r = style_row(&mut h, "text", &large);
    assert_eq!(s(&r["name"]), "Tag huge");
    assert_eq!(h.doc().components[&k].properties.get("text.tag.style"), Some(&large));
    // Built-in IDs are words; an own style's ID is the same after save and reopen.
    let dir = tmp_dir("ids");
    let mut h = save_and_reopen(h, &dir, "ids.ecad", 0x50a_0403);
    assert_eq!(s(&style_row(&mut h, "text", &large)["name"]), "Tag huge");
    let _ = std::fs::remove_dir_all(dir);
}

// --------------------------------------------------------------------- a05: promote

/// §12.5, §6.4, question 29: a project-only style moves to the standard with the same ID and no
/// reference changes; a project value moves to the standard; the refusals.
#[test]
fn a05_promote_a_style_and_a_value() {
    let mut h = H::new(0x50a_0501);
    let k = coil(&mut h, 100.0);
    let large = own_id(
        &h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}})).unwrap(),
    );
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k.hex(), "value": large})).unwrap();
    refused(&mut h, "promote", json!({"catalogue": "text", "id": large}));
    h.run("standard", json!({"new": "House"})).unwrap();
    let props = h.doc().components[&k].properties.clone();
    let (_, tx) = one_tx(&mut h, "promote", json!({"catalogue": "text", "id": large}));
    assert_eq!(op_names(&tx), ["SetStyle", "SetStyle"], "project Some→None, then standard None→Some");
    assert_eq!(h.doc().components[&k].properties, props, "no reference changes");
    let r = style_row(&mut h, "text", &large);
    assert_eq!(s(&r["layer"]), "standard");
    assert!(near(field_num(&r, "height"), 5.0) && field(&r, "height").1 == "standard");
    let page = h.first_page();
    assert!(near(text_style::text_style_at(&h.doc(), TextKind::Tag, Scope { page: Some(page), component: Some(k), ..Scope::project() }).height, 5.0));
    // A built-in or a standard style cannot be promoted.
    refused(&mut h, "promote", json!({"catalogue": "text", "id": large}));
    refused(&mut h, "promote", json!({"catalogue": "text", "id": "tag"}));
    // A value: project → standard, the project value cleared.
    let e = defaults::find("symbol.coil.prefix").unwrap();
    h.run("setdefault", json!({"key": e.key, "level": "project", "value": "Q"})).unwrap();
    let (_, tx) = one_tx(&mut h, "promote", json!({"key": e.key}));
    assert!(tx.ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Standard, .. })), "{:?}", tx.ops);
    let d = h.doc();
    let r = defaults::resolve(&d, e, Scope::project());
    assert_eq!((r.from, r.text()), (Source::Standard, "Q".to_string()));
    // Refused when the standard already holds a different value.
    h.run("setdefault", json!({"key": e.key, "level": "project", "value": "R"})).unwrap();
    let m = refused(&mut h, "promote", json!({"key": e.key}));
    assert!(m.contains("use setdefault level standard"), "{m}");
}

// ------------------------------------------------------------- a06: standards by copy

/// §12.6, §6.1, questions 6 and 25: a second project copies the first's standard (same ID,
/// values, styles); the first file is unchanged on disk; refused when it has none.
#[test]
fn a06_a_standard_is_taken_by_copy_and_keeps_its_id() {
    let dir = tmp_dir("from");
    let mut a = with_standard(0x50a_0601);
    a.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"})).unwrap();
    let own = own_id(
        &a.run(
            "setstyle",
            json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "layer": "standard", "fields": {"height": 5}}),
        )
        .unwrap(),
    );
    a.run("setstyle", json!({"catalogue": "location", "id": "box", "layer": "standard", "fields": {"line_type": "continuous"}})).unwrap();
    let a_std = a.run("standard", json!({})).unwrap();
    let a_doc_std = a.doc().standard.clone();
    let path_a = save_as(&mut a, &dir, "Panel Std (A).ecad");
    close_all(&mut a);
    drop(a);
    let hash = |p: &str| std::fs::read(p).unwrap();
    let before = hash(&path_a);

    let mut b = H::new(0x50a_0602);
    // An IEC project has nothing to copy.
    let iec = save_as(&mut H::new(0x50a_0603), &dir, "iec.ecad");
    let m = refused(&mut b, "standard", json!({"from": iec}));
    assert!(m.contains("IEC (built in)"), "{m}");
    refused(&mut b, "standard", json!({"from": dir.join("missing.ecad").display().to_string()}));
    let (_, _tx) = one_tx(&mut b, "standard", json!({"from": path_a}));
    let r = b.run("standard", json!({})).unwrap();
    assert_eq!((r["id"].clone(), r["name"].clone()), (a_std["id"].clone(), a_std["name"].clone()), "same StandardId and name");
    assert!(b.doc().standard == a_doc_std, "header, values and styles copied");
    assert_eq!(s(&style_row(&mut b, "text", &own)["layer"]), "standard");
    assert_eq!(hash(&path_a), before, "the first file is unchanged on disk");
    // Replacing an existing standard; the project's own values stay.
    let rule = defaults::find("wiring.label_rule").unwrap();
    b.run("setdefault", json!({"key": rule.key, "level": "project", "value": "each"})).unwrap();
    one_tx(&mut b, "standard", json!({"from": path_a}));
    assert_eq!(defaults::resolve(&b.doc(), rule, Scope::project()).from, Source::Project, "project values stay");
    undo(&mut b);
    let _ = std::fs::remove_dir_all(dir);
}

/// §6.1 `remove`: back to IEC in one transaction whose value and record ops come before
/// `SetStandard { after: None }`; undo restores everything; explicit users of a standard-only
/// own style need `replace_with`.
#[test]
fn a06_back_to_iec_is_one_transaction_and_undo_restores_it() {
    let mut h = with_standard(0x50a_0604);
    h.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"})).unwrap();
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": 4}})).unwrap();
    let own =
        own_id(&h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Std big", "copy_of": "tag", "layer": "standard"})).unwrap());
    let k = coil(&mut h, 100.0);
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k.hex(), "value": own})).unwrap();
    let m = refused(&mut h, "standard", json!({"remove": true}));
    assert!(m.contains("replace_with"), "{m}");
    let before = h.doc();
    let (v, tx) = one_tx(&mut h, "standard", json!({"remove": true, "replace_with": {"text": "tag"}}));
    let names = op_names(&tx);
    assert_eq!(names.last(), Some(&"SetStandard"), "SetStandard comes last: {names:?}");
    assert!(
        names.iter().filter(|n| **n == "SetStyle").count() >= 2
            && tx.ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Standard, .. })),
        "{names:?}"
    );
    assert!(v.to_string().contains('1') && v.to_string().contains('2'), "the result says how many values and styles went: {v}");
    let d = h.doc();
    assert!(d.standard.is_none());
    assert_eq!(d.components[&k].properties.get("text.tag.style").map(String::as_str), Some("tag"));
    assert_eq!(s(&h.run("standard", json!({})).unwrap()["name"]), "IEC (built in)");
    undo(&mut h);
    assert!(h.doc() == before, "undo restores the standard, its values, its records and the user");
    redo(&mut h);
    assert!(h.doc().standard.is_none());
}

/// §6.1: `new` is refused when a standard exists; `name` renames it; names are 1–64 characters.
#[test]
fn a06_standard_new_and_rename_refusals() {
    let mut h = with_standard(0x50a_0605);
    refused(&mut h, "standard", json!({"new": "Second"}));
    let (_, tx) = one_tx(&mut h, "standard", json!({"name": "House 2026"}));
    assert_eq!(op_names(&tx), ["SetStandard"]);
    assert_eq!(s(&h.run("standard", json!({})).unwrap()["name"]), "House 2026");
    for bad in [String::new(), "x".repeat(65), "a\u{7}b".into()] {
        refused(&mut h, "standard", json!({"name": bad}));
    }
    let mut g = H::new(0x50a_0606);
    refused(&mut g, "standard", json!({"name": "No standard yet"}));
    refused(&mut g, "standard", json!({"remove": true}));
    refused(&mut g, "standard", json!({"new": ""}));
    refused(&mut g, "setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"}));
}

// ------------------------------------------------------- a07: location boxes and frames

/// §12.7: the `box` style changes every location box; a frame style chosen at page level changes
/// only that page.
#[test]
fn a07_box_style_changes_every_box_and_a_page_frame_style_one_page() {
    let mut h = H::new(0x50a_0701);
    let p1 = h.first_page();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 20], [80, 80]]})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Second"})).unwrap(), "page");
    h.activate(p2);
    h.run("locationbox", json!({"location": "+CP2", "corners": [[20, 20], [80, 80]]})).unwrap();
    let (_, tx) =
        one_tx(&mut h, "setstyle", json!({"catalogue": "location", "id": "box", "fields": {"line_type": "continuous", "line_weight": 0.5}}));
    assert_eq!(op_names(&tx), ["SetStyle"]);
    for p in [p1, p2] {
        h.activate(p);
        let boxes: Vec<_> = h.pv(|pr, pg| pg.entities_of(pr, p)).into_iter().filter(|e| e.common.layer == ecad_cadcraft::locations::LAYER).collect();
        assert!(!boxes.is_empty() && boxes.iter().all(|e| !e.common.linetype.eq_ignore_ascii_case("DASHED")), "page {p}: continuous");
        assert!(boxes.iter().any(|e| e.common.lineweight == Lineweight::Mm100(50)), "page {p}: 0.5 mm");
    }
    // Frames: an own frame style chosen on page 2 only.
    let heavy = own_id(
        &h.run("setstyle", json!({"catalogue": "frame", "new": true, "name": "Heavy", "copy_of": "sheet", "fields": {"border_weight": 1.0}}))
            .unwrap(),
    );
    h.run("setdefault", json!({"key": "print.frame.style", "level": "page", "page": p2.hex(), "value": heavy})).unwrap();
    let frame_weights = |h: &mut H, p: PageId| {
        h.activate(p);
        h.pv(|pr, pg| pg.frame_entities_of(pr, p))
            .into_iter()
            .filter(|e| e.common.layer == ecad_cadcraft::frame::LAYER)
            .map(|e| e.common.lineweight)
            .collect::<Vec<_>>()
    };
    let w2 = frame_weights(&mut h, p2);
    let w1 = frame_weights(&mut h, p1);
    assert!(w2.contains(&Lineweight::Mm100(100)) && !w2.contains(&Lineweight::Mm100(50)), "page 2's border is 1 mm: {w2:?}");
    assert!(w1.contains(&Lineweight::Mm100(50)) && !w1.contains(&Lineweight::Mm100(100)), "page 1 keeps the sheet style: {w1:?}");
    // grid_labels off removes the grid labels on that page only.
    h.run("setstyle", json!({"catalogue": "frame", "id": heavy, "fields": {"grid_labels": "off"}})).unwrap();
    h.check();
}

// ---------------------------------------------------------------- a08: the file format

/// A session exercising every new op: standard new/rename, Standard-level values, standard and
/// project records (built-in overrides and own styles), object/page choices, delete, promote.
fn exercised(seed: u64) -> (H, ComponentId) {
    let mut h = with_standard(seed);
    let page = h.first_page();
    let k = coil(&mut h, 100.0);
    h.run("standard", json!({"name": "House 2026"})).unwrap();
    h.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"})).unwrap();
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD", "height": 3}})).unwrap();
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": 4}})).unwrap();
    let a = own_id(&h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "A", "copy_of": "tag"})).unwrap());
    let b = own_id(&h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "B", "copy_of": "tag"})).unwrap());
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k.hex(), "value": a})).unwrap();
    h.run("deletestyle", json!({"catalogue": "text", "id": a, "replace_with": b})).unwrap();
    h.run("promote", json!({"catalogue": "text", "id": b})).unwrap();
    let f = own_id(
        &h.run("setstyle", json!({"catalogue": "frame", "new": true, "name": "Heavy", "copy_of": "sheet", "fields": {"border_weight": 1}})).unwrap(),
    );
    h.run("setdefault", json!({"key": "print.frame.style", "level": "page", "page": page.hex(), "value": f})).unwrap();
    h.run("setstyle", json!({"catalogue": "location", "id": "box", "fields": {"fill": "YE"}})).unwrap();
    (h, k)
}

/// FP1–FP3 (overseer ruling: full arms): every new op applies, its inverse restores the document,
/// and every transaction's record reads back as written. (Replay equals the live document after
/// every step: the harness checks it.)
#[test]
fn fp3_new_ops_apply_invert_and_round_trip_through_records() {
    let (mut h, _) = exercised(0x50a_0801);
    let names: std::collections::BTreeSet<&str> = h.p(|p| p.ledger.log().iter().flat_map(|t| t.ops.iter().map(Op::name)).collect());
    assert!(names.contains("SetStyle") && names.contains("SetStandard"), "{names:?}");
    let n = inverses_restore(&mut h);
    assert!(n >= 12, "only {n} new ops checked");
    records_round_trip(&mut h);
    // Replay of the whole log gives the live document.
    let log = h.p(|p| p.ledger.full_log().unwrap());
    let replayed = ecad_model::Ledger::replay(log, 0x50a_0801).unwrap();
    assert!(*replayed.document() == h.doc(), "replay from the log");
}

/// §3.1: a Standard-layer `SetStyle` while there is no standard is refused by `apply`, never by a
/// panic; `SetStandard { after: None }` removes header, values and records.
#[test]
fn fp3_a_standard_layer_record_without_a_standard_is_refused_by_apply() {
    let mut h = with_standard(0x50a_0802);
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": 4}})).unwrap();
    let std_op = last_tx(&mut h).ops[0].clone();
    assert_eq!(std_op.name(), "SetStyle");
    let empty = H::new(0x50a_0803).doc();
    let mut d = empty.clone();
    assert!(std_op.apply(&mut d).is_err(), "no standard: refused");
    // Directly removing the standard while records exist: the invariant `no Standard layer records
    // without a standard` holds, whichever way apply resolves it (refuse, or remove them too).
    let mut d = h.doc();
    let header = d.standard.as_ref().map(|s| s.header.clone());
    let first = h.p(|p| p.ledger.log().iter().find(|t| t.ops.iter().any(|o| o.name() == "SetStandard")).cloned()).unwrap();
    let remove = first.ops.iter().find(|o| o.name() == "SetStandard").unwrap().inverse();
    match remove.apply(&mut d) {
        Ok(()) => assert!(d.check().is_ok() && d.standard.is_none(), "removed with everything it held"),
        Err(_) => assert!(header.is_some(), "or refused while it holds records"),
    }
}

/// §12.8: save, close and reopen keep the standard, the records and every choice; undo and redo
/// step exactly across `SetStyle`, `SetStandard` and `ObjectRef::Standard`.
#[test]
fn a08_save_reopen_and_undo_redo_across_the_new_ops() {
    let (mut h, _) = exercised(0x50a_0804);
    let trail = h.trail.clone();
    let n = trail.len();
    for i in (0..n - 1).rev() {
        undo(&mut h);
        assert!(h.doc() == trail[i], "undo step {i}");
    }
    for (i, want) in trail.iter().enumerate().skip(1) {
        redo(&mut h);
        assert!(h.doc() == *want, "redo step {i}");
    }
    let want = h.doc();
    let styles_before = rows_of(h.run("styles", json!({})).unwrap());
    let dir = tmp_dir("reopen");
    let mut h = save_and_reopen(h, &dir, "Haus Standard (1).ecad", 0x50a_0805);
    assert!(h.doc() == want, "reopen gives the same document");
    assert_eq!(rows_of(h.run("styles", json!({})).unwrap()), styles_before);
    h.check();
    let _ = std::fs::remove_dir_all(dir);
}

/// §3.5: the diff names the style and the fields that changed; history rows use these words.
#[test]
fn a08_diff_and_history_name_the_style_and_fields() {
    let mut h = H::new(0x50a_0806);
    let from = h.doc();
    h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}})).unwrap();
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD"}})).unwrap();
    let to = h.doc();
    let ops = ecad_model::diff::ops_between(&from, &to, &|_| true);
    assert!(ops.iter().filter(|o| o.name() == "SetStyle").count() == 2, "{ops:?}");
    let steps = h.run("steps", json!({"limit": 10})).unwrap().to_string();
    let info = h.run("pointinfo", json!({"seq": 3})).map(|v| v.to_string()).unwrap_or_default();
    let hist = format!("{steps} {info}");
    assert!(hist.contains("Tag large"), "history names the style: {hist:.600}");
    assert!(hist.to_lowercase().contains("colour"), "history names the field: {hist:.600}");
}

/// §12.8 (hostile file): 2,000 styles, 100 fields per style, a 10,000-character name, a dangling
/// reference and an unknown catalogue: the file reads, the page draws, `styles` lists (bounded),
/// Checks reports rows, and nothing panics. A regression test per bound (AC 5).
#[test]
fn a08_a_hostile_style_file_opens_draws_and_reports() {
    let mut h = H::new(0x50a_0807);
    let k = coil(&mut h, 100.0);
    h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Seed", "copy_of": "tag"})).unwrap();
    let template = last_tx_json(&mut h);
    let mut ops = Vec::new();
    let make = |id: String, cat: &str, name: String, fields: Value| {
        let mut v = template.clone();
        let rec = find_obj_mut(&mut v, "catalogue").expect("SetStyle has a catalogue");
        rec.insert("catalogue".into(), json!(cat));
        rec.insert("id".into(), json!(id));
        rec.insert("before".into(), Value::Null);
        let after = find_obj_mut(&mut v, "fields").expect("a record");
        after.insert("name".into(), json!(name));
        after.insert("fields".into(), fields);
        decoded_ops(&v)
    };
    for i in 0..2000u32 {
        ops.extend(make(format!("#{:032x}", 0xbad0_0000u64 + u64::from(i)), "text", format!("S{i}"), json!({"height": "3"})));
    }
    let many: serde_json::Map<String, Value> = (0..100).map(|i| (format!("f{i}"), json!("x"))).collect();
    ops.extend(make(format!("#{:032x}", 0xfeed_0001u64), "text", "Many".into(), Value::Object(many)));
    ops.extend(make(format!("#{:032x}", 0xfeed_0002u64), "text", "N".repeat(10_000), json!({"height": "1e308"})));
    ops.extend(make(format!("#{:032x}", 0xfeed_0003u64), "zz_future", "Future".into(), json!({"shade": "x"})));
    ops.extend(make(format!("#{:032x}", 0xfeed_0004u64), "text", "Unknown field".into(), json!({"height": "3", "wobble": "on"})));
    ops.push(Op::SetProperty {
        object: ObjectRef::Component(k),
        key: "text.tag.style".into(),
        before: None,
        after: Some(format!("#{}", "c".repeat(32))),
    });
    inject(&mut h, ops);
    let rows = styles(&mut h, "text");
    assert!(rows.len() <= 20 + 1000 + 5, "MAX_STYLES_PER_CATALOGUE bounds the listing: {}", rows.len());
    let unk = rows.iter().find(|r| s(&r["name"]) == "Unknown field");
    if let Some(r) = unk {
        assert!(r["unknown_fields"].to_string().contains("wobble"), "unknown fields are listed: {r}");
    }
    assert!(rows.iter().all(|r| s(&r["name"]).chars().count() <= 10_000));
    let all = checks_all(&mut h);
    assert!(!all.is_empty(), "Checks reports the breaches");
    assert!(!checks_of(&mut h, "style-unknown").is_empty(), "the dangling reference is reported");
    let page = h.first_page();
    let t = tag(&mut h, k);
    assert!(!texts_on(&mut h, page, &t).is_empty(), "the page draws");
    let d = h.doc();
    let fan = Default::default();
    assert!(printbook::build_pending(&d, &[page], &fan, 0.01, None, false).is_ok(), "it prints");
    // Save and reopen: still reads.
    let dir = tmp_dir("hostile");
    let mut h = save_and_reopen(h, &dir, "hostile.ecad", 0x50a_0808);
    let all = h.run("styles", json!({})).unwrap();
    assert!(all["unknown_catalogues"].to_string().contains("zz_future"), "the unknown catalogue is kept and named: {}", all["unknown_catalogues"]);
    let _ = rows_of(h.run("styles", json!({"users": true})).unwrap());
    let _ = std::fs::remove_dir_all(dir);
}

// --------------------------------------------- fp4: old text keys are reported, not read

/// FP4 and the overseer's ruling: 031's `text.<kind>.height` / `.weight` / `.font` project
/// properties are no longer read; Checks shows one Info row per kind of old key, which goes when
/// those keys are cleared, and updates only on a project `SetProperty` of such a key.
#[test]
fn fp4_old_text_keys_are_ignored_and_reported_once_per_kind() {
    let mut h = H::new(0x50a_0809);
    let set = |key: &str, v: Option<&str>, before: Option<String>| Op::SetProperty {
        object: ObjectRef::Project,
        key: key.into(),
        before,
        after: v.map(str::to_string),
    };
    inject(
        &mut h,
        vec![
            set("text.tag.height", Some("5"), None),
            set("text.description.height", Some("3"), None),
            set("text.tag.weight", Some("regular"), None),
            set("text.tag.font", Some("Plex"), None),
            set("text.title_main.font", Some("Plex"), None),
        ],
    );
    let st = text_style::text_style(&h.doc(), TextKind::Tag);
    assert!(near(st.height, 2.5) && st.weight == text_style::Weight::Bold, "the old keys are not read");
    let rows = checks_of(&mut h, "ignored-old-setting");
    assert_eq!(rows.len(), 3, "one row per kind (heights, weights, fonts): {rows:?}");
    let all = rows.iter().map(Value::to_string).collect::<String>();
    assert!(all.contains("Text heights set in an older build, now ignored"), "{all}");
    assert!(all.to_lowercase().contains("info"), "{all}");
    // Clearing the weight key removes its row; heights and fonts stay.
    inject(&mut h, vec![set("text.tag.weight", None, Some("regular".into()))]);
    assert_eq!(checks_of(&mut h, "ignored-old-setting").len(), 2);
    inject(&mut h, vec![set("text.tag.font", None, Some("Plex".into())), set("text.title_main.font", None, Some("Plex".into()))]);
    assert_eq!(checks_of(&mut h, "ignored-old-setting").len(), 1, "only heights are left");
    // Reopen keeps it: the old keys are still in the file.
    let dir = tmp_dir("fp4");
    let mut h = save_and_reopen(h, &dir, "old keys.ecad", 0x50a_0810);
    assert_eq!(checks_of(&mut h, "ignored-old-setting").len(), 1, "computed on open");
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------- users and the update map

/// §2.5: `styles {users}` counts explicit users (objects naming the style) and implicit ones
/// (objects using the page's, project's, standard's or built-in choice).
#[test]
fn map_users_counts_explicit_and_implicit() {
    let mut h = H::new(0x50a_0901);
    let (k1, _k2, _k3) = (coil(&mut h, 100.0), coil(&mut h, 150.0), coil(&mut h, 200.0));
    let big = own_id(&h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Big", "copy_of": "tag"})).unwrap());
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k1.hex(), "value": big})).unwrap();
    let rows = rows_of(h.run("styles", json!({"catalogue": "text", "users": true})).unwrap());
    let u = |id: &str| rows.iter().find(|r| s(&r["id"]) == id).map(|r| (r["users"]["explicit"].as_u64(), r["users"]["implicit"].as_u64())).unwrap();
    assert_eq!(u(&big), (Some(1), Some(0)));
    let (e, i) = u("tag");
    assert_eq!(e, Some(0));
    assert!(i.unwrap_or(0) >= 2, "the other coils use tag implicitly: {i:?}");
    // Without `users`, no counts are computed.
    let plain = styles(&mut h, "text");
    assert!(plain.iter().all(|r| r.get("users").is_none_or(Value::is_null)), "users only on demand");
}

/// §10: a `SetStyle` on `tag` marks the pages using it stale and redraws only the shown page; a
/// commit touching no style changes no page drawing. (The table counters are in the trial's lane
/// test; here the page drawings are compared.)
#[test]
fn map_a_style_change_reaches_the_pages_that_use_it() {
    let mut h = H::new(0x50a_0902);
    let p1 = h.first_page();
    let k = coil(&mut h, 100.0);
    let p2: PageId = id(&h.run("newpage", json!({"title": "No tags"})).unwrap(), "page");
    h.activate(p1);
    let t = tag(&mut h, k);
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD"}})).unwrap();
    assert!(texts_on(&mut h, p1, &t).iter().all(|e| e.common.color == code_colour("RD")), "the shown page redrew");
    h.activate(p2);
    h.activate(p1);
    h.check();
}

/// §3.4: "Clearing the old key (`setprop`, project, `null`) removes the row": a person (or an
/// agent) has a command to clear an old key, so the row can be made to go.
#[test]
fn fp4_a_person_can_clear_an_old_key() {
    let mut h = H::new(0x50a_0811);
    inject(&mut h, vec![Op::SetProperty { object: ObjectRef::Project, key: "text.tag.font".into(), before: None, after: Some("Plex".into()) }]);
    assert_eq!(checks_of(&mut h, "ignored-old-setting").len(), 1);
    let r = h.run("setprop", json!({"project": true, "key": "text.tag.font", "value": null}));
    assert!(r.is_ok(), "setprop on the project clears an old key (§3.4): {r:?}");
    assert!(checks_of(&mut h, "ignored-old-setting").is_empty());
}

/// §4.1, question 1: an object's own text style is the whole style: height, weight, colour and
/// the font (`font` is a text-style field).
#[test]
fn a04_an_objects_own_text_style_sets_its_font_too() {
    let mut h = H::new(0x50a_0412);
    let page = h.first_page();
    let (k1, k2) = (coil(&mut h, 100.0), coil(&mut h, 200.0));
    let mono = own_id(
        &h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag mono", "copy_of": "tag", "fields": {"font": "Plex Mono"}})).unwrap(),
    );
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k1.hex(), "value": mono})).unwrap();
    let style_of = |h: &mut H, c: ComponentId| {
        let t = tag(h, c);
        texts_on(h, page, &t).first().map(|e| if let EntityKind::Text(x) = &e.kind { x.style.clone() } else { String::new() }).unwrap_or_default()
    };
    // Control: the same font at project level does change how a tag is drawn.
    let plain = style_of(&mut h, k2);
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"font": "Plex Mono"}})).unwrap();
    let project_font = style_of(&mut h, k2);
    undo(&mut h);
    assert_ne!(plain, project_font, "control: a tag font at project level changes the drawn style");
    let (a, b) = (style_of(&mut h, k1), style_of(&mut h, k2));
    assert_ne!(a, b, "K1's tag is drawn in its own style's font, K2's in the project font ({a} / {b})");
}

/// §4.1: `text.<kind>.style` has an object level on conductors too: a conductor's own
/// `text.wire_label.style` draws its label in that style.
#[test]
fn a04_a_conductors_own_wire_label_style_draws() {
    let mut h = H::new(0x50a_0413);
    let page = h.first_page();
    let w: ecad_model::ConductorId =
        id(&h.run("wire", json!({"points": pts(&[(0.0, 50.0), (60.0, 50.0)]), "potential": "24V"})).unwrap(), "conductor");
    let big = own_id(
        &h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Label big", "copy_of": "wire_label", "fields": {"height": 3}})).unwrap(),
    );
    h.run("setdefault", json!({"key": "text.wire_label.style", "level": "object", "conductor": w.hex(), "value": big})).unwrap();
    let l = ecad_cadcraft::wires::labels_on_page(&h.doc(), page).into_iter().find(|l| l.conductor == w).expect("labelled");
    assert!(near(l.height, 3.0), "the conductor's own label style draws: {}", l.height);
}
