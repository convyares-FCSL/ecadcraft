//! Lane 064 (design 064 §10): one update pipeline for derived views.

use ecad_cadcraft::defaults;
use ecad_model::Word;
use ecad_model::keys::{self, Area};

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
