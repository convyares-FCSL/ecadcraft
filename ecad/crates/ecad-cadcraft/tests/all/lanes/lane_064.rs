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
    let (p, f) = (PageId(1), FolderId(4));
    let folder = Folder { name: "A".into(), parent: None };
    let cases: Vec<(&str, Vec<Op>, bool)> = vec![
        ("page added", vec![Op::AddPage { id: p, page: page.clone(), index: 0 }], true),
        ("page removed", vec![Op::RemovePage { id: p, page: page.clone(), index: 0 }], true),
        ("page moved", vec![Op::RemovePage { id: p, page: page.clone(), index: 0 }, Op::AddPage { id: p, page: page.clone(), index: 1 }], true),
        ("page retitled", vec![Op::ModifyPage { id: p, before: page.clone(), after: retitled }], true),
        ("page renumbered", vec![Op::ModifyPage { id: p, before: page.clone(), after: renumbered }], true),
        ("page refiled", vec![Op::ModifyPage { id: p, before: page.clone(), after: refiled }], true),
        ("page display choice", vec![Op::ModifyPage { id: p, before: page.clone(), after: restyled }], false),
        ("folder added", vec![Op::AddFolder { id: f, folder: folder.clone(), index: 0 }], true),
        ("folder renamed", vec![Op::ModifyFolder { id: f, before: folder.clone(), after: Folder { name: "B".into(), parent: None } }], true),
        ("a page property", vec![Op::SetProperty { object: ObjectRef::Page(p), key: "note".into(), before: None, after: Some("x".into()) }], false),
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
