//! Brief 072 lane E (design 072 §5.1, §14): the typed page. `ModifyPage` (A9: inverse, applied
//! and checked), `PageKind` and `GenKind` words (A2), the page's old keys reserved (A3), and the
//! strict page record (A4).

use ecad_model::book::{Book, PageNo};
use ecad_model::keys;
use ecad_model::{Author, GenKind, IdAllocator, Ledger, ModelError, ObjectRef, Op, Page, PageId, PageKind, StyleRef, Word};

fn one_page() -> (Ledger, PageId, IdAllocator) {
    let mut ids = IdAllocator::seeded(72);
    let mut ledger = Ledger::new(72);
    let page: PageId = ids.fresh();
    ledger.commit(Author::User, "page", vec![Op::AddPage { id: page, page: Page::titled("Supply").unwrap(), index: 0 }]).unwrap();
    (ledger, page, ids)
}

/// A9: `ModifyPage` applies, inverts, undoes and replays; a page that is not as recorded, or a
/// changed kind, is refused and leaves the document as it was.
#[test]
fn modify_page_applies_inverts_and_refuses() {
    let (mut ledger, page, _) = one_page();
    let before = ledger.document().pages[&page].clone();
    let mut after = before.clone();
    after.title = ecad_model::PageTitle::new("Main supply").unwrap();
    after.number = PageNo::parse("105a");
    after.page_type = StyleRef::builtin("network");
    after.sheet = ecad_model::SheetSpec::parse("A2").ok();
    after.display.insert("text.tag.show".into(), "yes".into());
    let op = Op::ModifyPage { id: page, before: before.clone(), after: after.clone() };
    assert_eq!(op.inverse(), Op::ModifyPage { id: page, before: after.clone(), after: before.clone() });
    ledger.commit(Author::User, "modify", vec![op.clone()]).unwrap();
    let doc = ledger.document();
    assert_eq!(doc.pages[&page], after);
    assert_eq!(Book::of(doc).number(doc, page), "105a");
    assert_eq!(doc.page_order, [page], "the page keeps its place");
    ledger.undo(Author::User).unwrap();
    assert_eq!(ledger.document().pages[&page], before);
    assert!(ledger.replay_matches().unwrap());

    let snapshot = ledger.document().clone();
    let cases: Vec<(&str, Op)> = vec![
        ("before differs", Op::ModifyPage { id: page, before: after.clone(), after: before.clone() }),
        (
            "kind changes",
            Op::ModifyPage { id: page, before: before.clone(), after: Page { kind: PageKind::generated(GenKind::Cover), ..before.clone() } },
        ),
        ("a missing folder", {
            let mut a = before.clone();
            a.folder = Some(ecad_model::FolderId(9));
            Op::ModifyPage { id: page, before: before.clone(), after: a }
        }),
    ];
    for (what, op) in cases {
        let e = ledger.commit(Author::User, what, vec![op]).unwrap_err();
        assert!(matches!(e, ModelError::Precondition(_) | ModelError::Invariant(_)), "{what}: {e:?}");
        assert_eq!(ledger.document(), &snapshot, "{what}");
    }
}

/// A3: the page's old keys are reserved; writing one to any bag is refused.
#[test]
fn the_pages_old_keys_are_reserved() {
    let (mut ledger, page, _) = one_page();
    for key in ["number", "folder", "sheet", "title_block", "generated", "strip", "page.type"] {
        assert!(keys::is_reserved(key), "{key}");
        assert!(keys::spec(key).is_none(), "{key} is no longer a free key");
        for object in [ObjectRef::Page(page), ObjectRef::Project] {
            let op = Op::SetProperty { object, key: key.into(), before: None, after: Some("x".into()) };
            let e = ledger.commit(Author::User, key, vec![op]).unwrap_err();
            assert!(matches!(e, ModelError::Precondition(_)), "{key}: {e:?}");
        }
    }
}

/// A2: every generated kind's word round-trips, and a page record round-trips with each kind.
#[test]
fn page_kinds_and_their_words_round_trip() {
    let words: Vec<&str> = GenKind::ALL.iter().map(|k| k.word()).collect();
    assert_eq!(words, ["cover", "contents", "revisions", "bom-location", "terminals", "cables", "plcrack", "iolist"]);
    for k in GenKind::ALL {
        assert_eq!(GenKind::parse(k.word()), Some(*k));
    }
    let strip = ecad_model::DeviceId(0xabc);
    for kind in [PageKind::Schematic, PageKind::generated(GenKind::Cover), PageKind::TerminalStrip { strip }] {
        let mut p = Page::titled("p").unwrap();
        p.kind = kind;
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Page>(&text).unwrap(), p, "{text}");
        assert_eq!(text.contains("kind"), kind != PageKind::Schematic, "a schematic page writes no kind: {text}");
    }
}

/// A4: a page record with an unknown field, an unknown kind word, a hidden character in its title
/// or a malformed number does not read.
#[test]
fn a_page_record_is_strict() {
    let bad = [
        r#"{"title":"p","properties":{"number":"1"}}"#,
        r#"{"title":"p","kind":{"generated":"poster"}}"#,
        r#"{"title":"p","kind":{"terminal_strip":{}}}"#,
        r#"{"title":"p","kind":{"generated":"terminals","strip":"0abc"}}"#,
        "{\"title\":\"p\u{202E}q\"}",
        r#"{"title":""}"#,
        r#"{"title":"p","number":"1.5"}"#,
    ];
    for text in bad {
        assert!(serde_json::from_str::<Page>(text).is_err(), "{text}");
    }
    let good: Page = serde_json::from_str(r#"{"title":"p","number":"7","kind":{"generated":"iolist"}}"#).unwrap();
    assert_eq!(good.kind.generated_kind(), Some(GenKind::IoList));
}

// ---------------------------------------------------------------- step 2: the project's lists

fn location(ids: &mut IdAllocator, code: &str) -> ecad_model::Location {
    ecad_model::Location::new(ids.fresh(), ecad_model::ListCode::new(code).unwrap())
}

/// A9 (design 072 §5.3): `ModifyProject` applies, inverts, undoes and replays; a project that is
/// not as recorded, or a list with a code twice, an id twice or a code without its sign, is
/// refused and leaves the document as it was.
#[test]
fn modify_project_applies_inverts_and_refuses() {
    let (mut ledger, page, mut ids) = one_page();
    let cp1 = location(&mut ids, "+CP1");
    let f1 = ecad_model::Function::new(ids.fresh(), ecad_model::ListCode::new("=F1").unwrap());
    let after = ecad_model::Project { locations: vec![cp1.clone()], functions: vec![f1], location: Some(cp1.id), ..Default::default() };
    let op = Op::ModifyProject { before: ecad_model::Project::default(), after: after.clone() };
    assert_eq!(op.inverse(), Op::ModifyProject { before: after.clone(), after: ecad_model::Project::default() });
    ledger.commit(Author::User, "lists", vec![op]).unwrap();
    assert_eq!(ledger.document().project, after);
    // A page refers to the entry by id.
    let mut p = ledger.document().pages[&page].clone();
    p.location = Some(cp1.id);
    let before_page = ledger.document().pages[&page].clone();
    ledger.commit(Author::User, "page", vec![Op::ModifyPage { id: page, before: before_page, after: p }]).unwrap();
    ledger.undo(Author::User).unwrap();
    ledger.undo(Author::User).unwrap();
    assert!(ledger.document().project.is_empty());
    ledger.redo(Author::User).unwrap();
    assert!(ledger.replay_matches().unwrap());

    let snapshot = ledger.document().clone();
    let now = snapshot.project.clone();
    let with = |l: Vec<ecad_model::Location>| ecad_model::Project { locations: l, ..now.clone() };
    let cases: Vec<(&str, Op)> = vec![
        ("before differs", Op::ModifyProject { before: ecad_model::Project::default(), after: now.clone() }),
        ("a code twice", Op::ModifyProject { before: now.clone(), after: with(vec![cp1.clone(), location(&mut ids, "+cp1")]) }),
        ("an id twice", Op::ModifyProject { before: now.clone(), after: with(vec![cp1.clone(), cp1.clone()]) }),
        ("no sign", Op::ModifyProject { before: now.clone(), after: with(vec![location(&mut ids, "CP2")]) }),
    ];
    for (what, op) in cases {
        let e = ledger.commit(Author::User, what, vec![op]).unwrap_err();
        assert!(matches!(e, ModelError::Precondition(_) | ModelError::Invariant(_)), "{what}: {e:?}");
        assert_eq!(ledger.document(), &snapshot, "{what}");
    }
}

/// A3: a page's and the project's location and function, and the old list keys, are fields now;
/// a device still holds its own until lane B moves them.
#[test]
fn the_lists_old_keys_are_reserved() {
    let (mut ledger, page, _) = one_page();
    let refused = [
        (ObjectRef::Page(page), "location"),
        (ObjectRef::Page(page), "function"),
        (ObjectRef::Project, "location"),
        (ObjectRef::Project, "function"),
        (ObjectRef::Project, "designation.l.0123.code"),
        (ObjectRef::Project, "designation.f.0123.description"),
    ];
    for (object, key) in refused {
        let op = Op::SetProperty { object, key: key.into(), before: None, after: Some("x".into()) };
        let e = ledger.commit(Author::User, key, vec![op]).unwrap_err();
        assert!(matches!(e, ModelError::Precondition(_)), "{key} on {object:?}: {e:?}");
    }
    assert!(keys::is_reserved_on(keys::Holder::Page, "location"));
    assert!(keys::is_reserved_on(keys::Holder::Device, "location"), "a device's location is a field (lane B)");
}

/// A4: a project record with an unknown field, a code over 16 characters, a hidden character or a
/// malformed id does not read; a full one round-trips.
#[test]
fn a_project_record_is_strict() {
    let id = "0000000000000000000000000000000a";
    let bad = [
        format!(r#"{{"locations":[{{"id":"{id}","code":"+CP1","colour":"red"}}]}}"#),
        format!(r#"{{"locations":[{{"id":"{id}","code":"+ABCDEFGHIJKLMNOPQ"}}]}}"#),
        format!("{{\"functions\":[{{\"id\":\"{id}\",\"code\":\"=F\u{202E}1\"}}]}}"),
        r#"{"locations":[{"id":"xyz","code":"+CP1"}]}"#.to_string(),
        r#"{"sites":[]}"#.to_string(),
    ];
    for text in &bad {
        assert!(serde_json::from_str::<ecad_model::Project>(text).is_err(), "{text}");
    }
    let text = format!(r#"{{"locations":[{{"id":"{id}","code":"+CP1","tag":"P1","descriptions":["Main","Left"]}}],"location":"{id}"}}"#);
    let p: ecad_model::Project = serde_json::from_str(&text).unwrap();
    assert_eq!(p.locations[0].descriptions.len(), 2);
    assert_eq!(serde_json::to_string(&p).unwrap(), text);
}

// ---------------------------------------------------------------- step 2b: revision rows

fn row(rev: &str, issued: bool) -> ecad_model::Revision {
    ecad_model::Revision {
        rev: ecad_model::Rev::new(rev).ok(),
        description: ecad_model::RevValue::new("Issue").ok(),
        issued: issued.then(|| ecad_model::RevValue::new(&format!("Rev {rev}")).unwrap()),
        ..Default::default()
    }
}

/// A4: a revision row with an unknown field, a bad custom field name, an over-long rev or a hidden
/// character does not read; a full one round-trips.
#[test]
fn a_revision_record_is_strict() {
    let bad = [
        r#"{"rev":"A","colour":"red"}"#,
        r#"{"rev":"A","custom":{"Bad Name":"x"}}"#,
        r#"{"rev":"A","custom":{"issued":"x"}}"#,
        r#"{"rev":"ABCDEFGHI"}"#,
        "{\"description\":\"a\u{202E}b\"}",
    ];
    for text in bad {
        assert!(serde_json::from_str::<ecad_model::Revision>(text).is_err(), "{text}");
    }
    let text = r#"{"rev":"A","date":"2026-11-17","custom":{"ecr":"ECR-1"},"issued":"Rev A","released":"release/Rev A"}"#;
    let r: ecad_model::Revision = serde_json::from_str(text).unwrap();
    assert_eq!(serde_json::to_string(&r).unwrap(), text);
}

/// I22: only the last revision row may be unissued; Restore (a filter refusing `rev.`) keeps the
/// rows as they are now, and accepting it restores them.
#[test]
fn revision_rows_are_ordered_and_kept_by_restore() {
    let (mut ledger, _, _) = one_page();
    let ok = ecad_model::Project { revisions: vec![row("A", true), row("B", false)], ..Default::default() };
    ledger.commit(Author::User, "rows", vec![Op::ModifyProject { before: Default::default(), after: ok.clone() }]).unwrap();
    let bad = ecad_model::Project { revisions: vec![row("A", false), row("B", true)], ..Default::default() };
    let e = ledger.commit(Author::User, "bad", vec![Op::ModifyProject { before: ok.clone(), after: bad }]).unwrap_err();
    assert!(matches!(e, ModelError::Invariant(_)), "{e:?}");

    let past = ecad_model::Document::default();
    let live = ledger.document().clone();
    let keep = ecad_model::diff::ops_between(&live, &past, &|k| !k.starts_with(ecad_model::project::REVISIONS_KEY));
    assert!(keep.iter().all(|o| !matches!(o, Op::ModifyProject { .. })), "rows kept: {keep:?}");
    let all = ecad_model::diff::ops_between(&live, &past, &|_| true);
    assert!(all.iter().any(|o| matches!(o, Op::ModifyProject { after, .. } if after.revisions.is_empty())), "{all:?}");
}

// ---------------------------------------------------------------- step 2c: project info

/// A3, A4: the info keys are fields of the project now; an info record with an unknown field, a
/// line break in a one-line field or a hidden character does not read; notes keep their lines.
#[test]
fn the_project_info_is_reserved_and_strict() {
    let (mut ledger, _, _) = one_page();
    for key in ["title", "drawing_number", "customer", "site", "status", "cover.notes"] {
        let op = Op::SetProperty { object: ObjectRef::Project, key: key.into(), before: None, after: Some("x".into()) };
        assert!(matches!(ledger.commit(Author::User, key, vec![op]).unwrap_err(), ModelError::Precondition(_)), "{key}");
        assert!(<ecad_model::InfoField as Word>::parse(key).is_some(), "{key} names an info field");
    }
    for text in [r#"{"title":"P","company":"X"}"#, r#"{"title":"a\nb"}"#, "{\"customer\":\"a\u{202E}b\"}"] {
        assert!(serde_json::from_str::<ecad_model::ProjectInfo>(text).is_err(), "{text}");
    }
    let text = r#"{"title":"Plant","drawing_number":"D-1","notes":"one\ntwo"}"#;
    let info: ecad_model::ProjectInfo = serde_json::from_str(text).unwrap();
    assert_eq!(info.get(ecad_model::InfoField::Notes), Some("one\ntwo"));
    assert_eq!(serde_json::to_string(&info).unwrap(), text);
}

// ---------------------------------------------------------------- step 3: Folder.start

/// Design 072 §5.2 (A3, A4, A9): a folder's start number is its own field, changed by
/// `ModifyFolder`; the old `folder.<id>.start` keys are reserved; the record is strict.
#[test]
fn a_folder_start_is_its_own_field() {
    let (mut ledger, _, mut ids) = one_page();
    let f: ecad_model::FolderId = ids.fresh();
    let folder = ecad_model::Folder::new("Schematic", None);
    ledger.commit(Author::User, "folder", vec![Op::AddFolder { id: f, folder: folder.clone(), index: 0 }]).unwrap();
    let after = ecad_model::Folder { start: PageNo::parse("100"), ..folder.clone() };
    ledger.commit(Author::User, "start", vec![Op::ModifyFolder { id: f, before: folder, after: after.clone() }]).unwrap();
    assert_eq!(ledger.document().folders[&f], after);
    let key = format!("folder.{}.start", f.hex());
    let op = Op::SetProperty { object: ObjectRef::Project, key: key.clone(), before: None, after: Some("100".into()) };
    assert!(matches!(ledger.commit(Author::User, "old key", vec![op]).unwrap_err(), ModelError::Precondition(_)));
    assert!(serde_json::from_str::<ecad_model::Folder>(r#"{"name":"A","parent":null,"begin":"1"}"#).is_err());
    let text = serde_json::to_string(&after).unwrap();
    assert!(text.contains("\"start\":\"100\""), "{text}");
    assert_eq!(serde_json::from_str::<ecad_model::Folder>(&text).unwrap(), after);
}
