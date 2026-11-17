//! Brief 072 lane E (design 072 §5.1): one answer for a page's type. The registry's page-level
//! read of `page.type` and `ecad_model::sheet::page_type_of` read the same field with the same
//! rule, for a built-in type and for a project-defined one.

use ecad_cadcraft::defaults::{self, Scope, Source};
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
        assert_eq!(ecad_model::sheet::page_type_of(&doc, &doc.pages[&page]), ty, "page_type_of agrees");
        let chain = defaults::chain(&doc, &defaults::pages::PAGE_TYPE, Scope::page(page));
        assert!(chain.iter().any(|(s, v)| *s == Source::Page && v.and_then(|v| v.style()) == Some(ty.clone())), "{chain:?}");
    }
    h.check();
}
