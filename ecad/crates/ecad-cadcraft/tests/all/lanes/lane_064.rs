//! Lane 064 (design 064 §10): one update pipeline for derived views.
//!
//! # Frozen pre-R9 behaviour, test oracle only
//! Every `old_*` function here is a frozen copy of code R9 replaced, kept only as a test oracle.
//! It is not live legacy code and nothing outside this module calls it. Delete it when R9's
//! step 10 matrix covers it; step 10 decides whether it stays.

use std::collections::BTreeSet;

use ecad_cadcraft::{connectors, defaults};
use ecad_model::Word;
use ecad_model::keys::{self, Area};
use ecad_model::view::PageSet;
use ecad_model::{DeviceId, Document, ObjectRef, Op, PageId, Representation, Subject};

/// C3: every fixed key and every defaults entry classifies to an area other than `Free`, and the
/// registry's area words, in order, are the 17 written out before design 064.
#[test]
fn areas_cover_the_keys() {
    for k in keys::KEYS {
        assert_ne!(keys::area_of(k.key.as_str()), Area::Free, "{}", k.key);
        assert_eq!(keys::area_of(k.key.as_str()), k.area, "{}", k.key);
    }
    for e in defaults::all() {
        assert_ne!(keys::area_of(e.key), Area::Free, "{}", e.key);
    }
    // The registry's words as they were written out before design 064 (`defaults::AREAS` is now
    // derived from `Area::REGISTRY`, so compare both with the old list).
    let old = [
        "text",
        "wiring",
        "wire",
        "tags",
        "revisions",
        "print",
        "symbol",
        "terminal",
        "connector",
        "strippage",
        "bom",
        "location",
        "continuation",
        "plc",
        "page",
        "front",
        "rating",
    ];
    let words: Vec<&str> = Area::REGISTRY.iter().map(|a| a.word()).collect();
    assert_eq!(words, old, "Area::REGISTRY");
    assert_eq!(defaults::AREAS, old, "defaults::AREAS");
}

/// Step 3, `TreeFeed`'s update map (design 064 §4.1): which ops make the Pages tree read again,
/// through `Views::begin` and `finish` (both phases).
#[test]
fn the_tree_feed_follows_its_update_map() {
    use ecad_electrical::derived::{EngineInputs, Views};
    use ecad_model::{Document, Folder, FolderId, ObjectRef, Op, Page, PageId, PageNo, Properties};
    let page = Page::titled("1").unwrap();
    let retitled = Page { title: ecad_model::page::PageTitle::new("2").unwrap(), ..page.clone() };
    let renumbered = Page { number: PageNo::parse("9"), ..page.clone() };
    let refiled = Page { folder: Some(FolderId(4)), ..page.clone() };
    let mut display = Properties::new();
    display.insert("text.tag.height".into(), "3".into());
    let restyled = Page { display, ..page.clone() };
    let rekinded = Page { kind: ecad_model::page::PageKind::TerminalStrip { strip: ecad_model::DeviceId(9) }, ..page.clone() };
    let resheeted = Page { sheet: ecad_model::SheetSpec::parse("A3").ok(), ..page.clone() };
    let (p, f) = (PageId(1), FolderId(4));
    let mut retitled_project = ecad_model::project::Project::default();
    retitled_project.info.title = ecad_model::project::InfoText::new("Plant").ok();
    let folder = Folder { name: "A".into(), parent: None, start: None };
    let cases: Vec<(&str, Vec<Op>, bool)> = vec![
        ("page added", vec![Op::AddPage { id: p, page: page.clone(), index: 0 }], true),
        ("page removed", vec![Op::RemovePage { id: p, page: page.clone(), index: 0 }], true),
        ("page moved", vec![Op::RemovePage { id: p, page: page.clone(), index: 0 }, Op::AddPage { id: p, page: page.clone(), index: 1 }], true),
        ("page retitled", vec![Op::ModifyPage { id: p, before: page.clone(), after: retitled }], true),
        ("page renumbered", vec![Op::ModifyPage { id: p, before: page.clone(), after: renumbered }], true),
        ("page refiled", vec![Op::ModifyPage { id: p, before: page.clone(), after: refiled }], true),
        ("page display choice", vec![Op::ModifyPage { id: p, before: page.clone(), after: restyled }], false),
        ("folder added", vec![Op::AddFolder { id: f, folder: folder.clone(), index: 0 }], true),
        (
            "folder renamed",
            vec![Op::ModifyFolder { id: f, before: folder.clone(), after: Folder { name: "B".into(), parent: None, start: None } }],
            true,
        ),
        ("folder removed", vec![Op::RemoveFolder { id: f, folder: folder.clone(), index: 0 }], true),
        (
            "folder moved",
            vec![Op::RemoveFolder { id: f, folder: folder.clone(), index: 0 }, Op::AddFolder { id: f, folder: folder.clone(), index: 2 }],
            true,
        ),
        ("page kind", vec![Op::ModifyPage { id: p, before: page.clone(), after: rekinded }], false),
        ("page sheet", vec![Op::ModifyPage { id: p, before: page.clone(), after: resheeted }], false),
        ("a page property", vec![Op::SetProperty { object: ObjectRef::Page(p), key: "note".into(), before: None, after: Some("x".into()) }], false),
        ("project info", vec![Op::ModifyProject { before: Default::default(), after: retitled_project.clone() }], false),
        ("a version named", vec![Op::NameVersion { name: "v".into(), at: 1, description: None }], false),
        ("nothing", vec![], false),
    ];
    let doc = Document::default();
    for (what, ops, want) in cases {
        let mut views = Views::new(EngineInputs { metrics: std::sync::Arc::new(ecad_electrical::text_style::fonts::FixedWidth) });
        let pending = views.begin(&doc, &ops);
        let changes = views.finish(&doc, &ops, pending);
        assert_eq!(changes.list, want, "{what}");
    }
}

/// Step 3: a page renamed through the command path still moves the Pages list's version, now
/// through the tree feed; an edit that leaves the tree alone does not.
#[test]
fn the_tree_feed_reaches_the_change_log() {
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(64).page().coil("K1", (20.0, 20.0)).build();
    let page = b.added_pages[0];
    let k1 = b.device("K1");
    let mut h = crate::common::H::from_built(b);
    let v0 = h.p(|p| p.page_list_version());
    h.run("renamepage", serde_json::json!({ "page": page, "title": "Pumps" })).unwrap();
    let v1 = h.p(|p| p.page_list_version());
    assert!(v1 > v0, "a rename moves the list version");
    h.run("setprop", serde_json::json!({ "component": k1, "key": "note", "value": "x" })).unwrap();
    assert_eq!(h.p(|p| p.page_list_version()), v1, "a device note leaves the list");
}

/// Step 3, `DevicesFeed`'s update map (design 064 §4.1): which ops reach the Devices list,
/// through `Views::begin` and `finish` on a project with K1 on its second page.
#[test]
fn the_devices_feed_follows_its_update_map() {
    use crate::common::{applied, devices_touched};
    use ecad_model::{ObjectRef, Op, Page, PageId, PageNo};
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(641).page().page().coil("K1", (20.0, 20.0)).build();
    let (first, second) = (b.added_pages[0], b.added_pages[1]);
    let k1 = b.device("K1");
    let mut h = crate::common::H::from_built(b);
    let doc = h.doc();
    let on = doc.representations_of(ecad_model::Subject::Device(k1)).map(|(_, r)| r.page).next().unwrap();
    assert_eq!(on, second, "K1 is on the second page");
    let index = |p: PageId| doc.page_order.iter().position(|x| *x == p).unwrap();
    let page = |p: PageId| doc.pages[&p].clone();
    let renumbered = Page { number: PageNo::parse("9"), ..page(second) };
    let retitled = Page { title: ecad_model::page::PageTitle::new("Other").unwrap(), ..page(first) };
    let settings = doc.settings.clone();
    let mut info = doc.project.clone();
    info.info.title = ecad_model::project::InfoText::new("Plant").ok();
    let note = |object| Op::SetProperty { object, key: "note".into(), before: None, after: Some("x".into()) };
    let new_page = PageId(9_999);
    // (what, ops, the devices it names, `pages`, `settings`)
    let cases: Vec<(&str, Vec<Op>, bool, bool, bool)> = vec![
        ("a device property", vec![note(ObjectRef::Device(k1))], true, false, false),
        ("its page renumbered", vec![Op::ModifyPage { id: second, before: page(second), after: renumbered }], true, false, false),
        ("another page retitled", vec![Op::ModifyPage { id: first, before: page(first), after: retitled }], false, false, false),
        (
            "the page before it moved after it",
            vec![
                Op::RemovePage { id: first, page: page(first), index: index(first) },
                Op::AddPage { id: first, page: page(first), index: index(second) },
            ],
            true,
            false,
            false,
        ),
        ("a page appended", vec![Op::AddPage { id: new_page, page: Page::titled("New").unwrap(), index: doc.page_order.len() }], false, false, false),
        ("a page property", vec![note(ObjectRef::Page(first))], false, false, false),
        ("the project settings", vec![Op::SetProjectSettings { before: settings.clone(), after: settings }], false, false, true),
        ("the project's own data", vec![Op::ModifyProject { before: doc.project.clone(), after: info }], false, false, false),
        ("nothing", vec![], false, false, false),
    ];
    for (what, ops, named, pages, settings) in cases {
        let t = devices_touched(&doc, &applied(&doc, &ops), &ops);
        assert_eq!(t.devices.contains(&k1), named, "{what}: {t:?}");
        assert_eq!(t.pages, pages, "{what}: pages");
        assert_eq!(t.settings, settings, "{what}: settings");
        if !named {
            assert!(t.devices.is_empty(), "{what}: {t:?}");
        }
    }
}

/// Step 3: a device edit through the command path still reaches the Devices list's change log,
/// now through the Devices feed; a page retitled does not.
#[test]
fn the_devices_feed_reaches_the_change_log() {
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(642).page().coil("K1", (20.0, 20.0)).build();
    let page = b.added_pages[0];
    let k1 = b.device("K1");
    let mut h = crate::common::H::from_built(b);
    let n = h.p(|p| p.change_count());
    h.run("setprop", serde_json::json!({ "component": k1, "key": "note", "value": "x" })).unwrap();
    let t = h.p(|p| p.device_changes_since(n)).unwrap();
    assert!(t.devices.contains(&k1), "{t:?}");
    let n = h.p(|p| p.change_count());
    h.run("renamepage", serde_json::json!({ "page": page, "title": "Pumps" })).unwrap();
    let t = h.p(|p| p.device_changes_since(n)).unwrap();
    assert!(t.is_empty(), "{t:?}");
}

// ---------------------------------------------------------------- the Properties feed

/// The Properties feed's update map: what each effect names, `None` where any object may change.
#[test]
fn the_props_feed_follows_its_update_map() {
    use crate::common::{applied, props_touched};
    use ecad_model::Page;
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(643).page().coil("K1", (20.0, 20.0)).build();
    let page = b.added_pages[0];
    let k1 = b.device("K1");
    let mut h = crate::common::H::from_built(b);
    let doc = h.doc();
    let retitled = Page { title: ecad_model::page::PageTitle::new("Other").unwrap(), ..doc.pages[&page].clone() };
    let settings = doc.settings.clone();
    let mut info = doc.project.clone();
    info.info.title = ecad_model::project::InfoText::new("Plant").ok();
    let note = |object| Op::SetProperty { object, key: "note".into(), before: None, after: Some("x".into()) };
    let set = |o: &[ObjectRef]| Some(o.iter().copied().collect::<BTreeSet<_>>());
    let cases: Vec<(&str, Vec<Op>, Option<BTreeSet<ObjectRef>>)> = vec![
        ("a device property", vec![note(ObjectRef::Device(k1))], set(&[ObjectRef::Device(k1)])),
        ("a page property", vec![note(ObjectRef::Page(page))], set(&[ObjectRef::Page(page)])),
        ("a page retitled", vec![Op::ModifyPage { id: page, before: doc.pages[&page].clone(), after: retitled }], set(&[ObjectRef::Page(page)])),
        ("the project settings", vec![Op::SetProjectSettings { before: settings.clone(), after: settings }], None),
        ("the project's own data", vec![Op::ModifyProject { before: doc.project.clone(), after: info }], None),
        ("nothing", vec![], set(&[])),
    ];
    for (what, ops, want) in cases {
        assert_eq!(props_touched(&doc, &applied(&doc, &ops), &ops, &ops), want, "{what}");
    }
}

/// The Properties feed's After phase: the transaction's extension (the wire rewrite and the
/// numbering pass) names the wires it changes and the devices at their ends, and the properties it
/// writes; with the ops alone it names neither.
#[test]
fn the_props_feed_reads_the_extension() {
    use crate::common::{applied, props_touched};
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(644).page().coil("K1", (20.0, 20.0)).coil("K2", (80.0, 20.0)).wire("K1", "K2").build();
    let page = b.added_pages[0];
    let (k1, k2) = (b.device("K1"), b.device("K2"));
    let mut h = crate::common::H::from_built(b);
    let doc = h.doc();
    let (&w, wire) = doc.wires.iter().next().expect("the builder drew a wire");
    let given = vec![Op::SetProperty { object: ObjectRef::Page(page), key: "note".into(), before: None, after: Some("x".into()) }];
    let extension = vec![
        Op::ModifyWire { id: w, before: wire.clone(), after: ecad_model::Wire { order_fixed: !wire.order_fixed, ..wire.clone() } },
        Op::SetProperty { object: ObjectRef::Device(k2), key: "note".into(), before: None, after: Some("y".into()) },
    ];
    let tx: Vec<Op> = given.iter().chain(&extension).cloned().collect();
    let after = applied(&doc, &tx);
    let got = props_touched(&doc, &after, &given, &tx).unwrap();
    let want: BTreeSet<ObjectRef> = [ObjectRef::Page(page), ObjectRef::Wire(w), ObjectRef::Device(k1), ObjectRef::Device(k2)].into_iter().collect();
    assert_eq!(got, want);
    let alone = props_touched(&doc, &applied(&doc, &given), &given, &given).unwrap();
    assert_eq!(alone, [ObjectRef::Page(page)].into_iter().collect::<BTreeSet<_>>());
}

/// A device edit through the command path reaches the Properties palette's change log through
/// the feed.
#[test]
fn the_props_feed_reaches_the_change_log() {
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(645).page().coil("K1", (20.0, 20.0)).build();
    let k1 = b.device("K1");
    let mut h = crate::common::H::from_built(b);
    let n = h.p(|p| p.change_count());
    h.run("setprop", serde_json::json!({ "component": k1, "key": "note", "value": "x" })).unwrap();
    let t = h.p(|p| p.props_changes_since(n)).unwrap();
    assert!(t.contains(&ObjectRef::Device(k1)), "{t:?}");
}

// ---------------------------------------------------------------- step 5: the connector index

/// `connectors::Index::touched` as it was before design 064 step 5 (`doc` is after `ops`).
fn old_connectors_touched(doc: &Document, ops: &[Op]) -> BTreeSet<DeviceId> {
    let mut out = BTreeSet::new();
    let subject = |r: &Representation, out: &mut BTreeSet<DeviceId>| {
        if let Subject::Device(c) = r.subject {
            out.insert(c);
        }
    };
    for op in ops {
        match op {
            Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                out.extend(terminal.device());
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => subject(rep, &mut out),
            Op::ModifyRepresentation { before, after, .. } => {
                subject(before, &mut out);
                subject(after, &mut out);
            }
            Op::AddItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    subject(r, &mut out);
                }
            }
            Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                out.insert(*id);
            }
            Op::SetProperty { object: ObjectRef::Device(c), .. } => {
                out.insert(*c);
            }
            Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => out.extend(doc.components_using(*id)),
            _ => {}
        }
    }
    out
}

/// `connectors::Index::pages_for` as it was before design 064 step 5.
fn old_pages_for(index: &connectors::Index, ops: &[Op]) -> BTreeSet<PageId> {
    let mut out = BTreeSet::new();
    for op in ops {
        if let Op::SetProperty { object: ObjectRef::Project, key, .. } = op
            && key.starts_with("connector.")
            && (ecad_cadcraft::defaults::connectors::is_naming_key(key) || key.ends_with(".description"))
        {
            out.extend(index.pages_inheriting(key));
        }
    }
    out
}

/// Step 5, `ConnectorsView` (design 064 §4.1): over every transaction of a project built with
/// connector commands (two connectors, a connector-level and a project naming key, a description
/// default, a half's maker, then undos past the second connector and redos), the view keeps the same index as the old
/// `Index::touched` and `update`, equal to a fresh build, and names the same pages as the old
/// `pages_for`.
#[test]
fn the_connectors_view_matches_its_old_functions() {
    use serde_json::json;
    let mut h = crate::common::H::new(6451);
    let row = |h: &mut crate::common::H, at: [i32; 2]| -> DeviceId {
        let v = h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": at})).unwrap();
        crate::common::id(&v, "component")
    };
    let a = row(&mut h, [50, 50]);
    let b = row(&mut h, [50, 150]);
    h.run("setprop", json!({"component": b.hex(), "key": "connector.side.socket", "value": "F"})).unwrap();
    let side = ecad_cadcraft::defaults::connectors::SIDE_SOCKET.key;
    h.run("setdefault", json!({"key": side, "value": "B"})).unwrap();
    h.run("setdefault", json!({"key": "connector.description", "value": "Plug"})).unwrap();
    h.run("setprop", json!({"component": a.hex(), "key": "half.s.maker", "value": "ACME"})).unwrap();
    h.run("wire", json!({"points": crate::common::pts(&[(300.0, 300.0), (350.0, 300.0)])})).unwrap();
    // Six undos take B away again (its deletion), two redos bring it and its side name back.
    for _ in 0..6 {
        h.cmdline("u");
    }
    h.cmdline("r");
    h.cmdline("r");
    let log = h.p(|p| p.ledger.log().to_vec());
    let metrics = std::sync::Arc::new(ecad_electrical::text_style::fonts::FixedWidth);
    let mut views = ecad_electrical::derived::Views::new(ecad_electrical::derived::EngineInputs { metrics });
    let mut doc = Document::default();
    views.rebuild(&doc);
    let mut old = connectors::Index::build(&doc);
    // The view on its own, for its pages: the pipeline's `Changes.pages` holds every view's.
    let mut alone = ecad_electrical::derived::ConnectorsView::build(&doc);
    let mut named = 0;
    for tx in &log {
        use ecad_electrical::derived::{Inputs, note_view};
        use ecad_model::view::{DerivedView, NoteCx, Phase};
        let effects = ecad_model::effect::Effects::of(&tx.ops);
        let mut scope = Default::default();
        note_view::<_, Inputs<'_>>(&alone, &NoteCx { doc: &doc, phase: Phase::Before, effects: &effects }, &mut scope);
        let pending = views.begin(&doc, &tx.ops);
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        views.finish(&doc, &tx.ops, pending);
        note_view::<_, Inputs<'_>>(&alone, &NoteCx { doc: &doc, phase: Phase::After, effects: &effects }, &mut scope);
        let pages = alone.update(&doc, scope, &Inputs { engine: views.inputs(), frame_page: None }).pages;
        old.update(&doc, &old_connectors_touched(&doc, &tx.ops));
        let what = format!("transaction {} {:?}", tx.seq, tx.label);
        assert_eq!(views.connectors(), &old, "{what}: the index");
        assert_eq!(views.connectors(), &connectors::Index::build(&doc), "{what}: a fresh build");
        let old_pages = old_pages_for(&old, &tx.ops);
        named += old_pages.len();
        assert_eq!(pages, PageSet::Only(old_pages), "{what}: pages");
    }
    assert!(!views.connectors().is_empty(), "a connector is indexed at the end");
    assert_eq!(views.connectors(), &h.p(|p| p.connectors().clone()), "the replay ends where the project's index is");
    assert!(named > 0, "a project naming default named pages");
}

/// Step 5: the project's connector index is the view's, and a project naming default still
/// reaches the change log with the inheriting connector's page.
#[test]
fn the_connectors_view_reaches_the_change_log() {
    use serde_json::json;
    let mut h = crate::common::H::new(6452);
    let v = h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, 50]})).unwrap();
    let a: DeviceId = crate::common::id(&v, "component");
    let page = h.first_page();
    assert!(h.p(|p| p.connectors().len()) == 1);
    let n = h.p(|p| p.change_count());
    h.run("setdefault", json!({"key": ecad_cadcraft::defaults::connectors::SIDE_SOCKET.key, "value": "B"})).unwrap();
    let pages = h.p(|p| p.changes_since(n));
    assert!(pages.as_ref().is_some_and(|p| p.contains(&page)), "the inheriting connector's page, not every page: {pages:?}");
    let inh = h.p(|p| p.connectors().inheriting(ecad_cadcraft::defaults::connectors::SIDE_SOCKET.key));
    assert!(inh.contains(&a));
}

/// Step 5 (tester note 1): in verify mode, a kept view out of step with the document (forced here
/// by emptying the views: the connector index and the reference index) is reported once each as an
/// `IndexRebuilt` event through `Changes.problems` and rebuilt in the same commit; the next commit
/// finds nothing wrong.
#[test]
fn a_view_out_of_step_is_reported_and_rebuilt() {
    use ecad_inspect::EventKind;
    use serde_json::json;
    let mut h = crate::common::H::new(6453);
    h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, 50]})).unwrap();
    assert!(ecad_model::ledger::verify_mode(), "tests run in verify mode");
    h.cfg(|p| p.views_mut().rebuild(&Document::default()));
    assert!(h.p(|p| p.connectors().is_empty()), "the kept index is out of step");
    assert!(h.p(|p| p.xref().verify(p.ledger.document()).is_err()), "the kept reference index is out of step");
    h.run("wire", json!({"points": crate::common::pts(&[(300.0, 300.0), (350.0, 300.0)])})).unwrap();
    let events: Vec<_> = h.p(|p| p.events.events().filter(|e| e.kind == EventKind::IndexRebuilt).cloned().collect());
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events[0].message.contains("reference index was out of step"), "{events:?}");
    assert!(events[1].message.contains("connector index"), "{events:?}");
    assert_eq!(h.p(|p| p.connectors().clone()), h.p(|p| connectors::Index::build(p.ledger.document())));
    h.p(|p| p.xref().verify(p.ledger.document())).unwrap();
    h.run("wire", json!({"points": crate::common::pts(&[(300.0, 320.0), (350.0, 320.0)])})).unwrap();
    assert_eq!(h.p(|p| p.events.count(EventKind::IndexRebuilt)), 2, "nothing more to repair");
}

// ---------------------------------------------------------------- step 4: the frame

/// Step 4, `FrameView`'s update map (design 064 §4.1): which ops make the kept frame of the first
/// of three pages stale, through `Views::begin` and `finish`; with no frame shown, none does.
#[test]
fn the_frame_view_follows_its_update_map() {
    use ecad_electrical::derived::{EngineInputs, Views};
    use ecad_electrical::frames::{FrameInput, FrameText};
    use ecad_model::{Page, PageTitle};
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(644).page().page().page().coil("K1", (20.0, 20.0)).build();
    let (first, second, third) = (b.added_pages[0], b.added_pages[1], b.added_pages[2]);
    let k1 = b.device("K1");
    let mut h = crate::common::H::from_built(b);
    let doc = h.doc();
    let metrics = ecad_electrical::text_style::fonts::FixedWidth;
    let input = FrameInput::with(&doc, first, &ecad_model::book::Book::of(&doc), FrameText::with(&doc, &metrics));
    let index = |p: PageId| doc.page_order.iter().position(|x| *x == p).unwrap();
    let page = |p: PageId| doc.pages[&p].clone();
    let retitle = |p: PageId| Op::ModifyPage { id: p, before: page(p), after: Page { title: PageTitle::new("Other").unwrap(), ..page(p) } };
    let note = |object| Op::SetProperty { object, key: "note".into(), before: None, after: Some("x".into()) };
    let settings = doc.settings.clone();
    let mut sheet = settings.clone();
    sheet.sheet = ecad_model::SheetSpec::parse("A2").ok();
    let mut info = doc.project.clone();
    info.info.title = ecad_model::project::InfoText::new("Plant").ok();
    let moved =
        |p: PageId, to: usize| vec![Op::RemovePage { id: p, page: page(p), index: index(p) }, Op::AddPage { id: p, page: page(p), index: to }];
    let folder = ecad_model::Folder { name: "A".into(), parent: None, start: None };
    let cases: Vec<(&str, Vec<Op>, bool)> = vec![
        ("its page's property", vec![note(ObjectRef::Page(first))], true),
        ("another page's property", vec![note(ObjectRef::Page(third))], false),
        ("its page retitled", vec![retitle(first)], true),
        ("another page retitled", vec![retitle(third)], false),
        ("another page removed and added in place", moved(third, index(third)), false),
        ("another page moved", moved(third, index(second)), true),
        ("a page appended", vec![Op::AddPage { id: PageId(9_999), page: Page::titled("New").unwrap(), index: doc.page_order.len() }], true),
        ("a folder added", vec![Op::AddFolder { id: ecad_model::FolderId(9), folder, index: 0 }], true),
        ("the sheet setting", vec![Op::SetProjectSettings { before: settings.clone(), after: sheet }], true),
        ("settings unchanged", vec![Op::SetProjectSettings { before: settings.clone(), after: settings }], false),
        ("the project's info", vec![Op::ModifyProject { before: doc.project.clone(), after: info }], true),
        ("a version named", vec![Op::NameVersion { name: "v".into(), at: 1, description: None }], true),
        ("a device property", vec![note(ObjectRef::Device(k1))], false),
        ("a free project property", vec![note(ObjectRef::Project)], false),
        ("nothing", vec![], false),
    ];
    let metrics = std::sync::Arc::new(ecad_electrical::text_style::fonts::FixedWidth);
    for (what, ops, want) in cases {
        let after = crate::common::applied(&doc, &ops);
        let mut views = Views::new(EngineInputs { metrics: metrics.clone() });
        let pending = views.begin(&doc, &ops);
        assert!(!views.finish(&after, &ops, pending).frame_stale, "{what}: no frame shown");
        views.frame_mut().show(first, input.clone());
        let pending = views.begin(&doc, &ops);
        assert_eq!(views.finish(&after, &ops, pending).frame_stale, want, "{what}");
    }
}

/// Whether `ops` make the frame of `page` (as `doc` shows it) stale, through `Views`.
fn frame_stale_after(doc: &Document, page: PageId, ops: &[Op]) -> bool {
    use ecad_electrical::derived::{EngineInputs, Views};
    use ecad_electrical::frames::{FrameInput, FrameText};
    let metrics = ecad_electrical::text_style::fonts::FixedWidth;
    let input = FrameInput::with(doc, page, &ecad_model::book::Book::of(doc), FrameText::with(doc, &metrics));
    let mut views = Views::new(EngineInputs { metrics: std::sync::Arc::new(metrics) });
    views.frame_mut().show(page, input);
    let after = crate::common::applied(doc, ops);
    let pending = views.begin(doc, ops);
    views.finish(&after, ops, pending).frame_stale
}

/// Step 4 (tester note 4), the rest of `FrameView`'s map: style records, the standard, the next
/// page's number, and the Contents and Revisions pages' own maps (a schematic frame ignores both).
#[test]
fn the_frame_view_follows_the_rest_of_its_map() {
    use ecad_model::{Page, PageNo, PageTitle, StandardHeader, StandardId, StyleCatalogue, StyleLayer, StyleRecord, StyleRef};
    use serde_json::json;
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(645).page().page().page().build();
    let (first, second, third) = (b.added_pages[0], b.added_pages[1], b.added_pages[2]);
    let mut h = crate::common::H::from_built(b);
    let contents: PageId = crate::common::id(&h.run("newpage", json!({"generated": "contents"})).unwrap(), "page");
    let revisions: PageId = crate::common::id(&h.run("newpage", json!({"type": "front", "generated": "revisions"})).unwrap(), "page");
    let doc = h.doc();
    let page = |p: PageId| doc.pages[&p].clone();
    let renumber = |p: PageId| Op::ModifyPage { id: p, before: page(p), after: Page { number: PageNo::parse("77"), ..page(p) } };
    let retitle = |p: PageId| Op::ModifyPage { id: p, before: page(p), after: Page { title: PageTitle::new("Other").unwrap(), ..page(p) } };
    let style = |catalogue| Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue,
        id: StyleRef::parse("mine").unwrap(),
        before: None,
        after: Some(StyleRecord::empty(catalogue)),
    };
    let current = doc.standard.as_ref().map(|s| s.header.clone());
    let other = Some(StandardHeader {
        id: StandardId(99),
        name: ecad_model::style::StyleName::new("S").unwrap(),
        based_on: ecad_model::style::BasedOn::Iec,
        origin: None,
    });
    let note = Op::SetProperty { object: ObjectRef::Project, key: "note".into(), before: None, after: Some("x".into()) };
    let cases: Vec<(&str, PageId, Vec<Op>, bool)> = vec![
        ("a frame style record", first, vec![style(StyleCatalogue::Frame)], true),
        ("a text style record", first, vec![style(StyleCatalogue::Text)], true),
        ("a wire style record", first, vec![style(StyleCatalogue::Wire)], false),
        ("the standard switched", first, vec![Op::SetStandard { before: current.clone(), after: other }], true),
        ("the standard unchanged", first, vec![Op::SetStandard { before: current.clone(), after: current.clone() }], false),
        ("the next page renumbered", first, vec![renumber(second)], true),
        ("a later page renumbered", first, vec![renumber(third)], false),
        ("Contents: another page retitled", contents, vec![retitle(third)], true),
        ("a schematic: another page retitled", first, vec![retitle(third)], false),
        ("Revisions: a project property", revisions, vec![note.clone()], true),
        ("a schematic: a project property", first, vec![note], false),
    ];
    for (what, shown, ops, want) in cases {
        assert_eq!(frame_stale_after(&doc, shown, &ops), want, "{what}");
    }
}

/// Step 4 (tester note 4): a commit refused after `begin` drops its `Pending`, which carries
/// nothing into the next commit; nothing is set before `finish`.
#[test]
fn a_dropped_pending_leaves_the_frame_alone() {
    use ecad_electrical::derived::{EngineInputs, Views};
    use ecad_electrical::frames::{FrameInput, FrameText};
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(646).page().build();
    let page = b.added_pages[0];
    let mut h = crate::common::H::from_built(b);
    let doc = h.doc();
    let metrics = ecad_electrical::text_style::fonts::FixedWidth;
    let input = FrameInput::with(&doc, page, &ecad_model::book::Book::of(&doc), FrameText::with(&doc, &metrics));
    let mut views = Views::new(EngineInputs { metrics: std::sync::Arc::new(metrics) });
    views.frame_mut().show(page, input);
    let touching = [Op::SetProperty { object: ObjectRef::Page(page), key: "note".into(), before: None, after: Some("x".into()) }];
    drop(views.begin(&doc, &touching));
    let pending = views.begin(&doc, &[]);
    assert!(!views.finish(&doc, &[], pending).frame_stale);
}

/// Step 4, D1: a drawing commit now runs the frame's map too; a drawn line is not on it, so the
/// frame is not worked out again (today's drawing commits never reach the map: items,
/// representations and the numbering pass's wire numbers only).
#[test]
fn a_drawing_commit_runs_the_frame_map_and_leaves_the_frame() {
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(647).page().build();
    let mut h = crate::common::H::from_built(b);
    let builds = h.p(|p| p.frame_builds);
    let head = h.head();
    h.cmdline("line 0,100 40,100 ");
    assert_eq!(h.head(), head + 1, "the line is a drawing commit");
    assert_eq!(h.p(|p| p.frame_builds), builds, "a line is not on the frame's map");
}

/// Step 4: a command refused by the ledger at commit (its op's `before` is stale) leaves the
/// kept frame and its flag as they were, and records nothing; the same op made valid then
/// reaches the frame's map.
#[test]
fn a_refused_commit_leaves_the_frame_and_its_flag() {
    let b = ecad_cadcraft::test_support::ProjectBuilder::new(648).page().build();
    let mut h = crate::common::H::from_built(b);
    let page = h.first_page();
    h.activate(page);
    let shown = |h: &mut crate::common::H| h.cfg(|p| p.views_mut().frame().shown().map(|(p, i)| (p, i.clone())));
    let before = shown(&mut h);
    assert!(before.as_ref().is_some_and(|(p, _)| *p == page), "the page's frame is kept");
    assert!(!h.p(|p| p.frame_is_stale()));
    let head = h.head();
    let builds = h.p(|p| p.frame_builds);
    let project = h.doc().project.clone();
    let titled = |t: &str| {
        let mut p = project.clone();
        p.info.title = ecad_model::project::InfoText::new(t).ok();
        p
    };
    // The project's title is not "Stale", so the ledger refuses the op at commit.
    let stale = ecad_model::Op::ModifyProject { before: titled("Stale"), after: titled("Panel 7") };
    let r = h.project.apply_ops(&mut h.s, "refused", vec![stale]);
    assert!(r.is_err(), "{r:?}");
    assert_eq!(h.head(), head, "nothing recorded");
    assert!(!h.p(|p| p.frame_is_stale()), "the flag is unchanged");
    assert_eq!(shown(&mut h), before, "the kept frame is unchanged");
    assert_eq!(h.p(|p| p.frame_builds), builds);
    // The same change from the real project is on the map: the frame is worked out again.
    let valid = ecad_model::Op::ModifyProject { before: project.clone(), after: titled("Panel 7") };
    h.project.apply_ops(&mut h.s, "valid", vec![valid]).unwrap();
    assert_eq!(h.p(|p| p.frame_builds), builds + 1, "a title change is on the frame's map");
}

/// Step 9 (the R9 revisions probe, 2026-11-18): a revision records only the project's data, and
/// the Revisions page behind the active page is in the change log (every page is too: the Rev box
/// is in every frame).
#[test]
fn a_revision_marks_the_revisions_page_behind() {
    use serde_json::json;
    let mut h = crate::common::H::new(6431);
    let r: PageId = crate::common::id(&h.run("newpage", json!({"type": "front", "generated": "revisions"})).unwrap(), "page");
    let s: PageId = crate::common::id(&h.run("newpage", json!({"type": "schematic"})).unwrap(), "page");
    h.activate(s);
    let n = h.p(|p| p.change_count());
    h.run("revision", json!({"description": "First issue", "drawn": "CVA"})).unwrap();
    let pages = h.p(|p| p.changes_since(n));
    assert!(pages.as_ref().is_none_or(|p| p.contains(&r)), "{pages:?}");
}

/// Step 9 (C8 for the cover and contents): each kind's trigger puts its page in the change log,
/// and an edit that leaves it does not.
#[test]
fn every_front_kind_redraws_after_its_trigger() {
    use serde_json::json;
    let mut h = crate::common::H::new(6491);
    let page = |h: &mut crate::common::H, v: serde_json::Value| -> PageId { crate::common::id(&h.run("newpage", v).unwrap(), "page") };
    let cover = page(&mut h, json!({"generated": "cover"}));
    let contents = page(&mut h, json!({"generated": "contents"}));
    let revisions = page(&mut h, json!({"type": "front", "generated": "revisions"}));
    let first = h.first_page();
    h.activate(first);
    let after = |h: &mut crate::common::H, cmd: &str, v: serde_json::Value| {
        let n = h.p(|p| p.change_count());
        h.run(cmd, v).unwrap();
        h.p(|p| p.changes_since(n)).expect("not every page")
    };
    // The first coil is new to the cover's symbol legend; the second is not.
    let pages = after(&mut h, "place", json!({"symbol": "coil", "at": [100, 100]}));
    assert!(pages.contains(&cover) && !pages.contains(&contents) && !pages.contains(&revisions), "{pages:?}");
    let pages = after(&mut h, "place", json!({"symbol": "coil", "at": [150, 100]}));
    assert!(!pages.contains(&cover) && !pages.contains(&contents) && !pages.contains(&revisions), "{pages:?}");
    // A retitled page is listed on the contents.
    let pages = after(&mut h, "renamepage", json!({"page": first.hex(), "title": "Supply"}));
    assert!(pages.contains(&contents) && !pages.contains(&cover) && !pages.contains(&revisions), "{pages:?}");
    // A revision is in every page's frame, so every page redraws: the revisions page's own map is
    // `a_revision_marks_the_revisions_page_behind`.
}

/// The page type rows of a kept Checks list, as words.
fn page_type_rows(h: &mut crate::common::H) -> Vec<String> {
    h.cfg(|p| p.checks().rows().filter(|c| c.kind == ecad_cadcraft::checks::Kind::PageTypeUnknown).map(|c| c.what()).collect())
}

/// Step 8b's fix: a page's number changed on its own works out the page type rows again (their
/// words name the page's number), as `page_types::touches_checks` says; the style rows' map used
/// to hide it.
#[test]
fn a_page_number_change_rewords_its_page_type_row() {
    let mut h = crate::common::H::new(64_081);
    let p1 = h.first_page();
    // A `page.type` naming nothing, written before the list is first shown.
    h.cfg(|p| {
        let d = p.ledger.document();
        let before = d.pages[&p1].clone();
        let after = ecad_model::Page { page_type: Some(ecad_model::StyleRef::Own(0xff)), ..before.clone() };
        p.ledger.commit(ecad_model::Author::User, "hostile", vec![Op::ModifyPage { id: p1, before, after }]).unwrap();
    });
    let shown = page_type_rows(&mut h);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].contains("Page 1:"), "{shown:?}");
    h.run("pageprops", serde_json::json!({"page": p1.hex(), "number": "7"})).unwrap();
    let kept = page_type_rows(&mut h);
    assert_eq!(kept, vec![shown[0].replacen("Page 1:", "Page 7:", 1)], "the row names the new number");
    let doc = h.doc();
    let fresh: Vec<String> = ecad_cadcraft::checks::all(&doc, h.cfg(|p| p.tolerance))
        .into_iter()
        .filter(|c| c.kind == ecad_cadcraft::checks::Kind::PageTypeUnknown)
        .map(|c| c.what())
        .collect();
    assert_eq!(kept, fresh, "the kept rows equal a rebuild");
}
