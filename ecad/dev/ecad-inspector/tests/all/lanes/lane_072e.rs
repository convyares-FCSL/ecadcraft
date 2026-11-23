//! The inspector shows the project's location and
//! function lists, a page's location, and the `ModifyProject` op in its Readable and Raw views.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::{Author, IdAllocator, ListCode, ListDescription, Location, LocationId, Op, Page, PageId, Project};

use crate::common::*;

#[test]
fn the_project_s_lists_and_modify_project_are_shown_raw_and_readable() {
    let dir = TempDir::new("072e-project");
    let mut l = history();
    let mut ids = IdAllocator::seeded(72);
    let loc: LocationId = ids.fresh();
    let mut entry = Location::new(loc, ListCode::new("+CP1").unwrap());
    entry.descriptions.push(ListDescription::new("Main panel").unwrap());
    let project = Project { locations: vec![entry], ..Project::default() };
    l.commit(Author::User, "list", vec![Op::ModifyProject { before: Project::default(), after: project }]).unwrap();
    let page: PageId = ids.fresh();
    let mut p = Page::titled("Supply").unwrap();
    p.location = Some(loc);
    let index = l.document().page_order().len();
    l.commit(Author::User, "page", vec![Op::AddPage { id: page, page: p, index }]).unwrap();
    let path = written(&dir, &l);

    // Readable: the CLI log names the op and the entry.
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "2"]).unwrap();
    for want in ["ModifyProject", "+CP1", "Main panel"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);

    // The model views: the list and the page's location by its code.
    let pr = ask(s, serde_json::json!({"op": "project"}));
    let locations = pr["ok"]["locations"].to_string();
    assert!(locations.contains("+CP1") && locations.contains("Main panel"), "{pr}");
    let pages = ask(s, serde_json::json!({"op": "pages"}));
    assert!(pages["ok"].to_string().contains("\"location\":\"+CP1\""), "{pages}");

    // Readable and Raw rows of the `ModifyProject` transaction.
    let all = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = all["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let mut found = None;
    for r in &rows {
        let key = r["key"].as_str().unwrap().to_string();
        let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
        if readable["ok"]["decoded"].to_string().contains("ModifyProject") {
            found = Some(key);
        }
    }
    let key = found.expect("a transaction with ModifyProject");
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    let r = f.store().unwrap().reader().unwrap();
    let mut bytes = None;
    r.raw_rows(|t, k, v| {
        if t == "transactions" && ecad_inspector::text::hex(k) == key {
            bytes = Some(v.to_vec());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(raw["ok"]["hex"], ecad_inspector::text::hex(&bytes.unwrap()));
}
