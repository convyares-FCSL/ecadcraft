//! Lane 072 F (design 072 §6.2; acceptance A2, A4, A9): drawing aids are their own objects.
//! `CreateAid`, `ModifyAid` and `DeleteAid` change them, undo inverts them, a representation may
//! draw one, and a stored aid in any other form is a read error.

use ecad_model::aid::{Aid, Caption, ContinuationName, LocationBox};
use ecad_model::units::Celsius;
use ecad_model::{AidId, Author, Form, IdAllocator, Ledger, Op, Page, PageId, Representation, RepresentationId, Role, Subject};

fn arrow(name: &str) -> Aid {
    Aid::Continuation { name: Some(ContinuationName::new(name).unwrap()), caption: None }
}

#[test]
fn aids_are_added_modified_removed_and_undone() {
    let mut ids = IdAllocator::seeded(0x72f1);
    let mut l = Ledger::new(0x72f1);
    let (p, a, b, r): (PageId, AidId, AidId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let boxed = Aid::Box(LocationBox { panel_temperature: Some(Celsius(35.0)), ..LocationBox::default() });
    let rep = Representation {
        subject: Subject::Aid(a),
        page: p,
        form: Form::Drawn,
        role: Role::Drawn,
        items: Vec::new(),
        connection_points: Default::default(),
    };
    l.commit(
        Author::User,
        "draw",
        vec![
            Op::AddPage { id: p, page: Page::titled("1").unwrap(), index: 0 },
            Op::CreateAid { id: a, aid: arrow("24V") },
            Op::CreateAid { id: b, aid: boxed.clone() },
            Op::LinkRepresentation { id: r, rep: rep.clone() },
        ],
    )
    .unwrap();
    assert_eq!(l.document().aids.len(), 2);
    assert_eq!(l.document().representations_of(Subject::Aid(a)).count(), 1);

    let captioned = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: Some(Caption::new("To the PSU").unwrap()) };
    l.commit(Author::User, "caption", vec![Op::ModifyAid { id: a, before: arrow("24V"), after: captioned.clone() }]).unwrap();
    assert_eq!(l.document().aids.get(&a), Some(&captioned));
    // A stale `before` is refused.
    assert!(l.commit(Author::User, "stale", vec![Op::ModifyAid { id: a, before: arrow("24V"), after: arrow("0V") }]).is_err());
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().aids.get(&a), Some(&arrow("24V")));

    // An aid a representation still draws cannot go on its own.
    let e = l.commit(Author::User, "erase", vec![Op::DeleteAid { id: a, aid: arrow("24V") }]).unwrap_err();
    assert!(e.to_string().contains("missing subject"), "{e}");
    l.commit(Author::User, "erase", vec![Op::UnlinkRepresentation { id: r, rep }, Op::DeleteAid { id: a, aid: arrow("24V") }]).unwrap();
    assert_eq!(l.document().aids.keys().copied().collect::<Vec<_>>(), vec![b]);
    l.undo(Author::User).unwrap();
    assert!(l.document().aids.contains_key(&a));
    l.document().check_index().unwrap();
}

#[test]
fn an_aid_is_well_formed_and_read_strictly() {
    let mut ids = IdAllocator::seeded(0x72f2);
    let mut l = Ledger::new(0x72f2);
    let a: AidId = ids.fresh();
    let bad = Aid::Box(LocationBox { panel_temperature: Some(Celsius(f64::NAN)), ..LocationBox::default() });
    assert!(l.commit(Author::User, "bad", vec![Op::CreateAid { id: a, aid: bad }]).is_err(), "a panel temperature must be a number");

    let cases: &[(&str, Aid)] = &[
        (r#"{"continuation":{}}"#, Aid::Continuation { name: None, caption: None }),
        (r#"{"continuation":{"name":"24V"}}"#, arrow("24V")),
        (r#"{"box":{}}"#, Aid::Box(LocationBox::default())),
        (r#"{"box":{"panel_temperature":40.0}}"#, Aid::Box(LocationBox { panel_temperature: Some(Celsius(40.0)), ..LocationBox::default() })),
    ];
    for (text, aid) in cases {
        assert_eq!(&serde_json::from_str::<Aid>(text).unwrap(), aid, "{text}");
        assert_eq!(serde_json::to_string(aid).unwrap(), *text);
    }
    for not_stored in [
        r#"{"arrow":{}}"#,
        r#"{"continuation":{"name":"24V","tag":"-X1"}}"#,
        r#"{"continuation":{"name":" 24V"}}"#,
        r#"{"continuation":{"name":"a name far longer than sixteen"}}"#,
        r#"{"box":{"location":"A1 panel"}}"#,
    ] {
        assert!(serde_json::from_str::<Aid>(not_stored).is_err(), "{not_stored} is a read error");
    }
}

#[test]
fn the_diff_between_two_documents_records_aid_ops() {
    let mut ids = IdAllocator::seeded(0x72f3);
    let mut l = Ledger::new(0x72f3);
    let a: AidId = ids.fresh();
    let before = l.document().clone();
    l.commit(Author::User, "add", vec![Op::CreateAid { id: a, aid: arrow("L1") }]).unwrap();
    let after = l.document().clone();
    let keep = |_: &str| true;
    assert_eq!(ecad_model::diff::ops_between(&before, &after, &keep), vec![Op::CreateAid { id: a, aid: arrow("L1") }]);
    assert_eq!(ecad_model::diff::ops_between(&after, &before, &keep), vec![Op::DeleteAid { id: a, aid: arrow("L1") }]);
}
