//! Tester 052 (brief 052a–c on `feature/device-first`): old projects, the format round trip,
//! atomic refusals and exact inverses, update maps, rename and merge, bounded input, and the
//! BOM-by-location `?` heading. The harness checks after every command that replay equals the
//! document and the shown page equals a fresh projection.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::common::{H, id};

use crate::common::close_all;
use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use ecad_engine::checks::{self, Kind};
use ecad_engine::designations::{self, Aspect};
use ecad_engine::outputs::{Group, kept::ReportModels};
use ecad_engine::{Persistence, ProjectHandle, generated, locations};
use ecad_model::{AidId, Author, DeviceId, Document, Op, PageId};
use serde_json::{Value, json};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-tester-052-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Save to `dir`/`name` and reopen in a fresh session.
fn save_and_reopen(h: &mut H, dir: &TempDir, name: &str, seed: u64) -> H {
    h.cfg(|p| p.persistence = Persistence::File);
    h.s.execute("saveas", &json!({"path": dir.path(name)})).unwrap();
    let path = dir.path(&format!("{name}.ecad"));
    close_all(&mut h.s);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, seed).unwrap();
    H { s, project, trail: Vec::new() }
}

fn page(h: &mut H, number: &str) -> PageId {
    id(&h.run("newpage", json!({"title": number, "number": number})).unwrap(), "page")
}

/// Place with proposals off (`keep`, brief 052d): these fixtures set every value themselves.
fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> DeviceId {
    id(&h.run("place", json!({"symbol": symbol, "at": [at.0, at.1], "location": "keep", "function": "keep"})).unwrap(), "component")
}

fn code(d: &Document, c: DeviceId) -> Option<String> {
    locations::resolve(d, c).value
}

fn key(d: &Document, a: Aspect, code: &str) -> String {
    designations::find(d, a, code).unwrap_or_else(|| panic!("{code} in the list")).key
}

fn changed_pages(h: &mut H, n: u64) -> BTreeSet<PageId> {
    h.p(|p| p.changes_since(n)).expect("the pages are known")
}

fn change_n(h: &mut H) -> u64 {
    h.p(|p| p.change_count())
}

/// The kept report models equal a fresh build (the update maps left nothing behind).
fn same_as_built(h: &mut H, what: &str) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, ReportModels::build_with(p.ledger.document(), &ecad_engine::text_style::EngineMetrics))
    });
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location), "{what}: BOM by location");
    assert_eq!(kept.bom_table(Group::Part), built.bom_table(Group::Part), "{what}: BOM by part");
    assert_eq!(kept.wire_table(), built.wire_table(), "{what}: wires");
    let mut a: Vec<String> = kept.checks().iter().map(|c| format!("{c:?}")).collect();
    let mut b: Vec<String> = built.checks().iter().map(|c| format!("{c:?}")).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "{what}: checks");
}

/// The `designations` answer with the kept use index equals the answer after a rebuild (a
/// reopen).
fn designations_now(h: &mut H) -> Value {
    h.run("designations", json!({})).unwrap()
}

// ------------------------------------------------------------------------------------ focus 1

/// A project as v0.2.0 wrote it: a coil with `location +CP1` and `function =F1` stored as codes, a
/// contact, a connector with the retired `half.s.location` and a box `+FIELD`
/// (a page's and the project's location are ids since design 072 §5.3). Built with raw ops, saved
/// and reopened.
/// List ids an old project's devices hold with no entry in the list.
const OLD_CP1: u128 = 0xc1;
const OLD_F1: u128 = 0xf1;
const OLD_FIELD: u128 = 0xfe;

fn old_project(dir: &TempDir) -> (H, DeviceId, DeviceId, AidId, PageId, Document) {
    let mut h = H::new(0x52_7001);
    let p1 = h.first_page();
    h.run("pageprops", json!({"page": p1.hex(), "number": "100"})).unwrap();
    let k = place(&mut h, "coil", (75.0, 180.0));
    let p2 = page(&mut h, "105");
    h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k.hex(), "location": "keep", "function": "keep"})).unwrap();
    let x: DeviceId =
        id(&h.run("connector", json!({"halves": "both", "family": "plain", "pins": 2, "layout": "row", "at": [200, 50]})).unwrap(), "component");
    let bx: AidId = id(
        &h.run("locationbox", json!({"location": "+TMP", "page": p2.hex(), "corners": [[60, 110], [140, 190]], "proposal": "keep"})).unwrap(),
        "aid",
    );
    h.cfg(|p| {
        let d = p.ledger.document();
        // Design 072 §6.3: a device stores list ids; these name no entry (a removed one).
        let gone = |d: &ecad_model::Device, l: u128, f: Option<u128>| ecad_model::Device {
            location: Some(ecad_model::LocationId(l)),
            function: f.map(ecad_model::FunctionId).or(d.function),
            ..d.clone()
        };
        let ops = vec![
            Op::ModifyDevice { id: k, before: d.devices[&k].clone(), after: gone(&d.devices[&k], OLD_CP1, Some(OLD_F1)) },
            Op::ModifyAid {
                id: bx,
                before: d.aids[&bx].clone(),
                after: ecad_model::Aid::Box(ecad_model::LocationBox {
                    location: Some(ecad_model::LocationId(OLD_FIELD)),
                    ..d.aids[&bx].as_box().cloned().unwrap()
                }),
            },
        ];
        p.ledger.commit(Author::User, "as v0.2.0 wrote it", ops).unwrap();
    });
    let doc = h.doc();
    let h2 = save_and_reopen(&mut h, dir, "old", 0x52_7002);
    (h2, k, x, bx, p1, doc)
}

/// §7, §10.5, §10.9: opening writes nothing; every old code shows `?` (never a wrong value) and is
/// listed; tags and the head are kept.
#[test]
fn an_old_project_opens_unchanged_and_flags_its_old_codes() {
    let dir = TempDir::new("old");
    let (mut h, k, x, bx, _p1, doc) = old_project(&dir);
    let d = h.doc();
    assert_eq!(d, doc, "the same document: nothing converted, nothing dropped");
    assert!(!h.p(|p| p.unsaved()), "opening writes nothing");
    // Devices: `?`, flagged as missing.
    assert_eq!(code(&d, k), None);
    assert!(locations::resolve(&d, k).missing());
    assert!(locations::resolve_function(&d, k).missing());
    // The retired half key is not read (FP4, Q9), and the property is still in the file.
    let rows = ecad_engine::connectors::half_rows(&d, x, &mut locations::BoxCache::default());
    assert!(rows.iter().all(|r| r.location.value.is_none()), "half.s.location is not read");
    assert!(d.devices[&x].properties.keys().all(|k| !k.starts_with("half.")), "a half's keys are never in the bag");
    // Checks: the device's two values and the box.
    let rows = checks::all(&d, 1e-6);
    let missing: Vec<&str> = rows.iter().filter(|r| r.kind == Kind::DesignationMissing).map(|r| r.which.as_str()).collect();
    assert!(missing.contains(&"-K01's location is no longer in the list"), "{missing:?}");
    assert!(missing.contains(&"-K01's function is no longer in the list"), "{missing:?}");
    assert!(missing.contains(&"A box's location is no longer in the list"), "{missing:?}");
    // The list's `missing` names every old value.
    let v = designations_now(&mut h);
    let lm: Vec<&str> = v["locations"]["missing"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    let hex = |n: u128| format!("{n:032x}");
    assert!(lm.contains(&hex(OLD_CP1).as_str()) && lm.contains(&hex(OLD_FIELD).as_str()), "{v}");
    assert_eq!(v["functions"]["missing"], json!([hex(OLD_F1)]), "{v}");
    // The tag and the head are kept: the contact is a contact, the coil the head.
    let tags = ecad_engine::devices::tag_words(&d, k);
    assert_eq!(tags, "-K01");
    let heads = ecad_engine::reps::heads(&d, k);
    assert_eq!(heads.len(), 1);
    assert_eq!(d.representations[&heads[0]].role, ecad_model::Role::Part(ecad_model::PartRole::Head));
    // Setting a location through the list is the way out, and it is one transaction.
    let head = h.head();
    h.run("setlocation", json!({"component": k.hex(), "value": "+CP1"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(code(&h.doc(), k).as_deref(), Some("+CP1"));
    let _ = bx;
}

// ------------------------------------------------------------------------------------ focus 2, 4

/// A project using one entry everywhere: a device, a connector half override, a box, a page and
/// the project default, all `+CP01`; and an entry `+CP1` with a description.
struct Users {
    h: H,
    k: DeviceId,
    x: DeviceId,
    bx: AidId,
    p1: PageId,
    p2: PageId,
    p3: PageId,
}

fn users(seed: u64) -> Users {
    let mut h = H::new(seed);
    let p1 = h.first_page();
    h.run("pageprops", json!({"page": p1.hex(), "number": "100"})).unwrap();
    let k = place(&mut h, "coil", (75.0, 180.0));
    let p2 = page(&mut h, "105");
    h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k.hex(), "location": "keep", "function": "keep"})).unwrap();
    let x: DeviceId =
        id(&h.run("connector", json!({"halves": "both", "family": "plain", "pins": 2, "layout": "row", "at": [250, 50]})).unwrap(), "component");
    let p3 = page(&mut h, "110");
    place(&mut h, "coil", (75.0, 180.0));
    h.run("setlocation", json!({"component": k.hex(), "value": "+CP01"})).unwrap();
    h.run("setlocation", json!({"component": x.hex(), "type": "plug", "value": "+CP01"})).unwrap();
    h.run("setlocation", json!({"component": x.hex(), "value": "+CP2"})).unwrap();
    let bx: AidId = id(
        &h.run("locationbox", json!({"location": "+CP01", "page": p1.hex(), "corners": [[300, 100], [380, 180]], "proposal": "keep"})).unwrap(),
        "aid",
    );
    h.run("pageprops", json!({"page": p1.hex(), "location": "+CP01", "proposal": "keep"})).unwrap();
    h.run("projectprops", json!({"location": "+CP01", "proposal": "keep"})).unwrap();
    h.run("designationadd", json!({"aspect": "location", "code": "+CP1", "description": "Main panel"})).unwrap();
    Users { h, k, x, bx, p1, p2, p3 }
}

/// §10.14, Q11: a merge moves every kind of user in one transaction, the BOM has one group, undo
/// gives back exactly the document before, redo exactly the one after; the shown page (with the
/// box, the page value and the device) is redrawn.
#[test]
fn a_merge_moves_every_user_and_undo_redo_are_exact() {
    let mut u = users(0x52_7101);
    let h = &mut u.h;
    h.activate(u.p1);
    same_as_built(h, "before");
    let before = h.doc();
    let head = h.head();
    let v = h.run("designationmerge", json!({"aspect": "location", "from": "+CP01", "into": "+CP1"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction: {v}");
    let after = h.doc();
    let k1 = key(&after, Aspect::Location, "+CP1");
    assert!(designations::find(&after, Aspect::Location, "+CP01").is_none());
    assert_eq!(after.devices[&u.k].value("location"), Some(k1.clone()));
    assert_eq!(after.devices[&u.x].properties.get("location.plug"), Some(&k1), "the override follows");
    assert_eq!(designations::box_key(after.aids[&u.bx].as_box().unwrap(), Aspect::Location), Some(k1.clone()), "the box follows");
    assert_eq!(designations::page_key(&after.pages[&u.p1], Aspect::Location), Some(k1.clone()), "the page follows");
    assert_eq!(designations::default_key(&after.project, Aspect::Location), Some(k1.clone()), "the project default follows");
    // No stored value anywhere still names the removed key.
    let old = key(&before, Aspect::Location, "+CP01");
    let dangling = after.devices.values().flat_map(|c| c.properties.values()).chain(after.pages.values().flat_map(|p| p.display.values()));
    assert!(dangling.chain(after.display.values()).all(|v| *v != old), "a value still names +CP01's key");
    same_as_built(h, "after merge");
    let locs: Vec<Option<String>> = h.cfg(|p| p.outputs().bom(Group::Location).into_iter().map(|r| r.location).collect());
    assert!(!locs.contains(&Some("+CP01".into())), "{locs:?}");
    let v = designations_now(h);
    let rows = v["locations"]["entries"].as_array().unwrap();
    let cp1 = rows.iter().find(|r| r["code"] == "+CP1").unwrap();
    assert_eq!((cp1["devices"].as_u64(), cp1["boxes"].as_u64(), cp1["pages"].as_u64()), (Some(1), Some(1), Some(1)), "{v}");
    assert_eq!(cp1["overrides"], 1, "{v}");
    assert_eq!(cp1["default"], true, "{v}");
    assert_eq!(cp1["description"], "Main panel", "into keeps its description: {v}");
    h.cmdline("u");
    assert_eq!(h.doc(), before, "undo is the exact inverse");
    same_as_built(h, "undo");
    h.cmdline("redo");
    assert_eq!(h.doc(), after, "redo is the exact inverse of the undo");
    same_as_built(h, "redo");
}

/// Merge takes `from`'s description when `into` has none; the use index follows undo, redo and a
/// second merge, and equals a rebuild after save and reopen.
#[test]
fn the_use_index_follows_undo_redo_and_equals_a_reopen() {
    let dir = TempDir::new("uses");
    let mut u = users(0x52_7102);
    let h = &mut u.h;
    h.run("designationedit", json!({"aspect": "location", "code": "+CP01", "description": "Old panel"})).unwrap();
    h.run("designationadd", json!({"aspect": "location", "code": "+CP9"})).unwrap();
    h.run("designationmerge", json!({"aspect": "location", "from": "+CP01", "into": "+CP9"})).unwrap();
    let d = h.doc();
    assert_eq!(designations::find(&d, Aspect::Location, "+CP9").and_then(|e| e.description).as_deref(), Some("Old panel"));
    h.cmdline("u");
    h.cmdline("redo");
    h.cmdline("u");
    // The index is right after the undo: +CP01 is used again, so it cannot be removed.
    let e = h.run("designationremove", json!({"aspect": "location", "code": "+CP01"})).unwrap_err();
    assert!(e.contains("is used by"), "{e}");
    // A second merge after the undo moves every user again.
    h.run("designationmerge", json!({"aspect": "location", "from": "+CP01", "into": "+CP1"})).unwrap();
    let d = h.doc();
    let k1 = key(&d, Aspect::Location, "+CP1");
    assert_eq!(d.devices[&u.x].properties.get("location.plug"), Some(&k1));
    assert_eq!(designations::default_key(&d.project, Aspect::Location), Some(k1.clone()));
    // A new device on +CP1, then undo both steps (a SetProperty and a CreateDevice undone).
    let q = place(h, "coil", (150.0, 150.0));
    h.run("setlocation", json!({"component": q.hex(), "value": "+CP1"})).unwrap();
    h.cmdline("u");
    h.cmdline("u");
    // Not the project default here: renaming that is its own test.
    h.run("projectprops", json!({"location": "+CP2", "proposal": "keep"})).unwrap();
    h.run("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP10"})).unwrap();
    let kept = designations_now(h);
    same_as_built(h, "after the sequence");
    let doc = h.doc();
    let mut h2 = save_and_reopen(h, &dir, "uses", 0x52_7103);
    assert_eq!(h2.doc(), doc, "save and reopen keeps the document");
    assert_eq!(designations_now(&mut h2), kept, "the kept use index equals a rebuild");
    assert_eq!(code(&h2.doc(), u.k).as_deref(), Some("+CP10"));
}

/// §10.12, §8.2a: a rename with the BOM-by-location page shown redraws it (the harness compares
/// the shown page with a fresh projection); the rename names the pages of its users and no other.
#[test]
fn a_rename_redraws_its_users_pages_and_no_other() {
    let mut u = users(0x52_7103);
    let h = &mut u.h;
    // An unused page with a device of its own location.
    let other = page(h, "120");
    let q = place(h, "coil", (75.0, 180.0));
    h.run("setlocation", json!({"component": q.hex(), "value": "+CP2"})).unwrap();
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    h.activate(bom);
    // Not the project default: that would touch every page by design.
    h.run("projectprops", json!({"location": "+CP2", "proposal": "keep"})).unwrap();
    let n = change_n(h);
    h.run("designationedit", json!({"aspect": "location", "code": "+CP01", "to": "+CP10"})).unwrap();
    let pages = changed_pages(h, n);
    for p in [u.p1, u.p2] {
        assert!(pages.contains(&p), "{p:?} has a user: {pages:?}");
    }
    assert!(!pages.contains(&u.p3), "page 110 has no user: {pages:?}");
    assert!(!pages.contains(&other), "page 120 has no user: {pages:?}");
    same_as_built(h, "rename");
    let texts: Vec<String> = h.cfg(|p| {
        let o = p.outputs().clone();
        generated::parts_of(p.ledger.document(), bom, Some(&o))
            .into_iter()
            .filter_map(|x| match x.kind {
                EntityKind::Text(t) => Some(t.value),
                _ => None,
            })
            .collect()
    });
    assert!(texts.iter().any(|t| t == "+CP10"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "+CP01"), "{texts:?}");
    // A description edit is drawn on no ordinary page.
    let n = change_n(h);
    h.run("designationedit", json!({"aspect": "location", "code": "+CP10", "description": "Renamed"})).unwrap();
    let pages = changed_pages(h, n);
    assert!(pages.iter().all(|p| *p == bom), "a description touches no ordinary page: {pages:?}");
    h.cmdline("u");
    h.cmdline("u");
    assert!(designations::find(&h.doc(), Aspect::Location, "+CP01").is_some());
}

/// §8.2a: renaming the entry that is the project's default, with a page shown, redraws that page
/// (the harness compares the shown page with a fresh projection after the command).
#[test]
fn renaming_the_project_default_redraws_the_shown_page() {
    let mut h = H::new(0x52_7104);
    let p1 = h.first_page();
    place(&mut h, "coil", (75.0, 180.0));
    h.run("projectprops", json!({"location": "+CP1", "proposal": "keep"})).unwrap();
    h.activate(p1);
    h.run("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP10"})).unwrap();
    h.cmdline("u");
}

// ------------------------------------------------------------------------------------ focus 3

/// §10.7, §8.1–§8.2: a device's location names its symbols' pages and no other; a box or a page
/// value names its own page only; adding an unused entry names none.
#[test]
fn device_box_and_page_values_name_only_their_pages() {
    let mut h = H::new(0x52_7201);
    let p105 = h.first_page();
    let k = place(&mut h, "coil", (75.0, 180.0));
    let p110 = page(&mut h, "110");
    h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k.hex(), "location": "keep", "function": "keep"})).unwrap();
    let p120 = page(&mut h, "120");
    place(&mut h, "coil", (75.0, 180.0));
    // The first box adds the box layer, which may touch any page; draw one before measuring.
    h.run("locationbox", json!({"location": "+X", "page": p120.hex(), "corners": [[300, 100], [380, 180]], "proposal": "keep"})).unwrap();
    h.activate(p120);
    let n = change_n(&mut h);
    h.run("setlocation", json!({"component": k.hex(), "value": "+CP2"})).unwrap();
    assert_eq!(changed_pages(&mut h, n), BTreeSet::from([p105, p110]), "the device's pages only");
    let n = change_n(&mut h);
    h.run("locationbox", json!({"location": "+FIELD", "page": p110.hex(), "corners": [[60, 110], [140, 190]], "proposal": "keep"})).unwrap();
    assert_eq!(changed_pages(&mut h, n), BTreeSet::from([p110]), "a box: its page only");
    assert_eq!(code(&h.doc(), k).as_deref(), Some("+CP2"), "the box changed no device");
    let n = change_n(&mut h);
    h.run("pageprops", json!({"page": p120.hex(), "location": "+CP3", "proposal": "keep"})).unwrap();
    assert_eq!(changed_pages(&mut h, n), BTreeSet::from([p120]), "a page value: that page only");
    let n = change_n(&mut h);
    h.run("designationadd", json!({"aspect": "function", "code": "=SPARE"})).unwrap();
    assert_eq!(changed_pages(&mut h, n), BTreeSet::new(), "an unused entry is drawn nowhere");
    same_as_built(&mut h, "after");
}

// ------------------------------------------------------------------------------------ focus 2, 5

/// New ops are atomic on refusal: a list with an unknown device, a strip override in a list, the
/// same entry twice, an entry of the other aspect, codes too long or with control characters;
/// nothing is written and no entry is added.
#[test]
fn refusals_write_nothing_and_add_no_entry() {
    let mut h = H::new(0x52_7301);
    let k = place(&mut h, "coil", (75.0, 180.0));
    let strip = place(&mut h, "terminal", (150.0, 150.0));
    h.run("setlocation", json!({"component": k.hex(), "value": "+CP1"})).unwrap();
    h.run("setfunction", json!({"component": k.hex(), "value": "=F1"})).unwrap();
    let long = format!("+{}", "A".repeat(ecad_engine::devices::MAX_PROPERTY_CHARS + 1));
    let cases: Vec<(&str, Value)> = vec![
        ("setlocation", json!({"devices": [k.hex(), "fffffffffffffffffffffff"], "value": "+NEW1"})),
        ("setlocation", json!({"devices": [k.hex(), strip.hex()], "type": "terminal", "value": "+NEW2"})),
        ("setlocation", json!({"component": k.hex(), "value": long})),
        ("setlocation", json!({"component": k.hex(), "value": "+CP\u{7}9"})),
        ("setlocation", json!({"component": k.hex(), "value": 12})),
        ("setlocation", json!({"component": k.hex()})),
        ("setlocation", json!({"component": k.hex(), "aspect": "colour", "value": "+X"})),
        ("setlocation", json!({"component": k.hex(), "type": "head", "value": "+NEW3"})),
        ("setlocation", json!({"component": k.hex(), "type": "bogus", "value": "+NEW4"})),
        ("setfunction", json!({"component": k.hex(), "type": "terminal", "value": "=NEW5"})),
        ("designationadd", json!({"aspect": "location", "code": "+cp1"})),
        ("designationadd", json!({"aspect": "location", "code": long})),
        ("designationadd", json!({"aspect": "location", "code": ""})),
        ("designationadd", json!({"aspect": "nope", "code": "+X"})),
        ("designationadd", json!({"aspect": "location", "code": "+X", "description": 5})),
        ("designationedit", json!({"aspect": "location", "code": "+CP1", "to": long})),
        ("designationedit", json!({"aspect": "location", "code": "+CP1", "description": 5})),
        ("designationedit", json!({"aspect": "function", "code": "+CP1", "to": "=F9"})),
        ("designationmerge", json!({"aspect": "location", "from": "+CP1", "into": "+cp1"})),
        ("designationmerge", json!({"aspect": "function", "from": "=F1", "into": "+CP1"})),
        ("designationmerge", json!({"aspect": "location", "from": "+CP1"})),
        ("designationremove", json!({"aspect": "location", "code": "+CP1"})),
        ("designationremove", json!({"aspect": "location", "code": "+NOPE"})),
        ("designationremove", json!({"aspect": "location", "key": "../../etc"})),
    ];
    for (cmd, params) in cases {
        let before = h.doc();
        let head = h.head();
        let e = h.run(cmd, params.clone()).expect_err(&format!("{cmd} {params} is refused"));
        assert!(!e.to_lowercase().contains("internal error") && !e.contains("panicked"), "{cmd} {params}: {e}");
        assert_eq!(h.head(), head, "{cmd} {params}: nothing written");
        assert_eq!(h.doc(), before, "{cmd} {params}: the document is unchanged");
    }
    let d = h.doc();
    for c in ["+NEW1", "+NEW2", "+NEW3", "+NEW4"] {
        assert!(designations::find(&d, Aspect::Location, c).is_none(), "{c} was not added");
    }
}

/// One device named twice in `devices` is set once (one op, one device in the answer), not
/// refused or double-counted.
#[test]
fn a_device_named_twice_is_set_once() {
    let mut h = H::new(0x52_7302);
    let k = place(&mut h, "coil", (75.0, 180.0));
    let head = h.head();
    let v = h.run("setlocation", json!({"devices": [k.hex(), k.hex()], "value": "+CP1"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(v["devices"], 1, "{v}");
    assert_eq!(code(&h.doc(), k).as_deref(), Some("+CP1"));
}

/// §5.9: "Code rules are 024's `location_value`: trimmed, `+` or `=` added". A code typed without
/// its sign names the entry in the list's own commands as it does in `setlocation`.
#[test]
fn a_code_without_its_sign_names_the_entry_in_the_list_commands() {
    let mut h = H::new(0x52_7303);
    let k = place(&mut h, "coil", (75.0, 180.0));
    h.run("setlocation", json!({"component": k.hex(), "value": "CP1"})).unwrap();
    assert_eq!(code(&h.doc(), k).as_deref(), Some("+CP1"), "setlocation adds the sign");
    h.run("designationadd", json!({"aspect": "location", "code": "CP2"})).unwrap();
    assert!(designations::find(&h.doc(), Aspect::Location, "+CP2").is_some(), "designationadd adds the sign");
    let r = h.run("designationedit", json!({"aspect": "location", "code": "CP1", "to": "CP10"}));
    assert!(r.is_ok(), "designationedit names +CP1 by CP1: {r:?}");
    let r = h.run("designationremove", json!({"aspect": "location", "code": "CP2"}));
    assert!(r.is_ok(), "designationremove names +CP2 by CP2: {r:?}");
}

/// Bounded input: a `devices` list over the handle limit is refused before any work; a huge
/// description is refused; the list commands survive hostile keys.
#[test]
fn oversized_input_is_refused() {
    let mut h = H::new(0x52_7304);
    let k = place(&mut h, "coil", (75.0, 180.0));
    let many: Vec<String> = (0..=10_000usize).map(|_| k.hex()).collect();
    let head = h.head();
    let e = h.run("setlocation", json!({"devices": many, "value": "+CP1"})).unwrap_err();
    assert!(!e.to_lowercase().contains("internal error"), "{e}");
    assert_eq!(h.head(), head);
    let big = "x".repeat(100_000);
    let e = h.run("designationadd", json!({"aspect": "location", "code": "+CP1", "description": big})).unwrap_err();
    assert!(e.contains("at most"), "{e}");
    assert_eq!(h.head(), head);
    for k in ["", " ", "+", "=", "\u{202e}+CP1", "designation.l.x.code"] {
        let _ = h.run("designationedit", json!({"aspect": "location", "key": k, "code": "+Z"}));
        let _ = h.run("designationremove", json!({"aspect": "location", "key": k}));
        let _ = h.run("designationmerge", json!({"aspect": "location", "from": k, "into": k}));
    }
    assert_eq!(h.head(), head, "nothing written");
}

// ------------------------------------------------------------------------------------ format keys

/// The location style (`location.style`, brief 050a) is a style ID, not a list entry: it raises no
/// `designation-missing` row and is not listed as a missing location. A box's own style is its
/// aid's field (design 072 §6.2).
#[test]
fn a_box_location_style_is_not_a_missing_location() {
    let mut h = H::new(0x52_7401);
    let bx: AidId = id(&h.run("locationbox", json!({"location": "+CP1", "corners": [[60, 110], [140, 190]], "proposal": "keep"})).unwrap(), "aid");
    let style = h.run("styles", json!({"catalogue": "location"})).unwrap();
    let ids: Vec<String> = style["styles"]
        .as_array()
        .or_else(|| style.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|r| r["id"].as_str().map(str::to_string))
        .collect();
    let pick = ids.last().cloned().unwrap_or_else(|| "box".into());
    h.run("setprop", json!({"aids": [bx.hex()], "key": "style", "value": pick})).unwrap();
    h.run("setdefault", json!({"key": "location.style", "level": "project", "value": pick})).unwrap();
    let d = h.doc();
    assert!(d.aids[&bx].as_box().is_some_and(|b| b.style.is_some()), "the box stores its style");
    let rows = checks::all(&d, 1e-6);
    let missing: Vec<&str> = rows.iter().filter(|r| r.kind == Kind::DesignationMissing).map(|r| r.which.as_str()).collect();
    assert!(missing.is_empty(), "a style is not a location: {missing:?}");
    let v = designations_now(&mut h);
    assert_eq!(v["locations"]["missing"], json!([]), "{v}");
}

// ------------------------------------------------------------------------------------ focus 6

/// Known open item: on the BOM-by-location page, the group of devices with no location is headed
/// `?`, and that heading is flagged (ACI 2 with its box) like every other `?` on the page.
#[test]
fn the_bom_page_flags_its_question_mark_heading() {
    let mut h = H::new(0x52_7601);
    place(&mut h, "coil", (100.0, 150.0)); // no location
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    let doc = h.doc();
    let parts = generated::parts_of(&doc, bom, Some(&ReportModels::build_with(&doc, &ecad_engine::text_style::EngineMetrics)));
    let qs: Vec<_> = parts.iter().filter(|p| matches!(&p.kind, EntityKind::Text(t) if t.value == generated::MISSING)).collect();
    // The heading `?`, the maker `?` and the part number `?`.
    assert!(qs.len() >= 3, "{} `?` texts", qs.len());
    let plain: Vec<_> = qs.iter().filter(|p| p.aci != Some(generated::FLAGGED_ACI)).collect();
    assert!(plain.is_empty(), "{} `?` drawn unflagged (the `?` location heading)", plain.len());
    let boxes = parts.iter().filter(|p| p.aci == Some(generated::FLAGGED_ACI) && !matches!(p.kind, EntityKind::Text(_))).count();
    assert!(boxes >= qs.len(), "each `?` has its box ({boxes} boxes for {} marks)", qs.len());
}
