//! Brief 072 lane E (design 072 §5.1): one answer for a page's type. The registry's page-level
//! read of `page.type` and `ecad_model::document::sheet::page_type_of` read the same field with the same
//! rule, for a built-in type and for a project-defined one.

use ecad_electrical::defaults::{self, Scope, Source};
use ecad_model::StyleRef;
use serde_json::json;

use crate::common::H;

#[test]
fn a_pages_type_is_the_same_through_the_registry_and_page_type_of() {
    let mut h = H::new(0x72e1);
    let page = h.first_page();
    let v = h.run("setstyle", json!({"catalogue": "page", "new": true, "name": "Wide", "copy_of": "schematic", "fields": {"sheet": "A1"}})).unwrap();
    let wide = StyleRef::parse(v["id"].as_str().unwrap()).unwrap();
    assert!(matches!(wide, StyleRef::Own(_)), "a project-defined type: {wide:?}");
    for ty in [wide.clone(), StyleRef::Builtin("network".into())] {
        h.run("setdefault", json!({"key": "page.type", "level": "page", "page": page.hex(), "value": ty.text()})).unwrap();
        let doc = h.doc();
        assert_eq!(doc.pages[&page].page_type.as_ref(), Some(&ty));
        let r = defaults::resolve(&doc, &defaults::pages::PAGE_TYPE, Scope::page(page));
        assert_eq!(r.from, Source::Page, "{ty:?}");
        assert_eq!(r.value.style(), Some(ty.clone()), "the registry reads the page's own type");
        assert_eq!(ecad_model::document::sheet::page_type_of(&doc, &doc.pages[&page]), ty, "page_type_of agrees");
        let chain = defaults::chain(&doc, &defaults::pages::PAGE_TYPE, Scope::page(page));
        assert!(chain.iter().any(|(s, v)| *s == Source::Page && v.and_then(|v| v.style()) == Some(ty.clone())), "{chain:?}");
    }
    h.check();
}

/// Design 072 §5.3: a page's location id and the project's function id reach its title block as
/// codes; renaming the entry renames what the block shows.
#[test]
fn a_pages_location_and_the_projects_function_show_in_the_title_block() {
    use ecad_electrical::pages::frames::{FrameInput, FrameText};
    let mut h = H::new(0x72e2);
    let page = h.first_page();
    h.run("pageprops", json!({"page": page.hex(), "location": "+CP7", "proposal": "keep"})).unwrap();
    h.run("projectprops", json!({"function": "=F1", "proposal": "keep"})).unwrap();
    let frame = |h: &mut H| {
        let doc = h.doc();
        let book = ecad_model::document::book::Book::of(&doc);
        let inp = FrameInput::with(&doc, page, &book, FrameText::sizes(&doc));
        (inp.location, inp.function)
    };
    assert_eq!(frame(&mut h), ("+CP7".to_string(), "=F1".to_string()));
    h.run("designationedit", json!({"aspect": "location", "code": "+CP7", "to": "+CP8"})).unwrap();
    assert_eq!(frame(&mut h).0, "+CP8");
}

/// R9's probe (2026-11-18): a revision records only `ModifyProject`, and a Revisions page that is
/// not in the front matter is in the change log after it.
#[test]
fn a_revisions_page_follows_a_revision() {
    let mut h = H::new(6431);
    let r: ecad_model::PageId = crate::common::id(&h.run("newpage", json!({"type": "front", "generated": "revisions"})).unwrap(), "page");
    let s: ecad_model::PageId = crate::common::id(&h.run("newpage", json!({"type": "schematic"})).unwrap(), "page");
    h.activate(s);
    let n = h.p(|p| p.change_count());
    h.run("revision", json!({"description": "First issue", "drawn": "CVA"})).unwrap();
    let pages = h.p(|p| p.changes_since(n));
    assert!(pages.as_ref().is_none_or(|p| p.contains(&r)), "{pages:?}");
}

/// Design 072 §5.3: the project's title is drawn in every page's frame, so changing it marks every
/// page (as the project property it was).
#[test]
fn a_project_title_marks_every_page() {
    let mut h = H::new(6432);
    let first = h.first_page();
    let n = h.p(|p| p.change_count());
    h.run("projectprops", json!({"title": "Plant"})).unwrap();
    let pages = h.p(|p| p.changes_since(n));
    assert!(pages.as_ref().is_none_or(|p| p.contains(&first)), "{pages:?}");
}
