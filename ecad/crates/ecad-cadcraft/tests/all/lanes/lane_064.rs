//! Lane 064 (design 064 §10): one update pipeline for derived views.

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
                out.insert(terminal.component);
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
    let mut named = 0;
    for tx in &log {
        let pending = views.begin(&doc, &tx.ops);
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        let pages = views.finish(&doc, &tx.ops, pending).pages;
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
    assert!(pages.as_ref().is_none_or(|p| p.contains(&page)), "{pages:?}");
    let inh = h.p(|p| p.connectors().inheriting(ecad_cadcraft::defaults::connectors::SIDE_SOCKET.key));
    assert!(inh.contains(&a));
}
