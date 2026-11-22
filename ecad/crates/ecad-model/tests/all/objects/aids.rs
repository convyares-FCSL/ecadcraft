//! Lane 072 F (design 072 §6.2; acceptance A2, A4, A9): drawing aids are their own objects.
//! `CreateAid`, `ModifyAid` and `DeleteAid` change them, undo inverts them, a representation may
//! draw one, and a stored aid in any other form is a read error.

use ecad_model::objects::aid::{Aid, Caption, ContinuationName, LocationBox};
use ecad_model::geometry::units::Celsius;
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
    assert_eq!(ecad_model::document::diff::ops_between(&before, &after, &keep), vec![Op::CreateAid { id: a, aid: arrow("L1") }]);
    assert_eq!(ecad_model::document::diff::ops_between(&after, &before, &keep), vec![Op::DeleteAid { id: a, aid: arrow("L1") }]);
}

// ---------------------------------------------------------------- F2a: loose parts (§6.4)

fn rail(qty: &str) -> ecad_model::objects::loose_part::LoosePart {
    use ecad_model::objects::loose_part::LooseDescription as Description;
    use ecad_model::objects::loose_part::{LoosePart, Qty};
    LoosePart::new(Description::new("DIN rail 35 mm").unwrap(), Qty::parse(qty).unwrap())
}

#[test]
fn loose_parts_are_added_modified_removed_and_undone() {
    use ecad_model::objects::loose_part::{ItemUnit, Qty};
    use ecad_model::{LocationId, LoosePartId, Touched};
    let mut ids = IdAllocator::seeded(0x72fa);
    let mut l = Ledger::new(0x72fa);
    let (a, b): (LoosePartId, LoosePartId) = (ids.fresh(), ids.fresh());
    let ops = vec![Op::CreateLoosePart { id: a, part: rail("2") }, Op::CreateLoosePart { id: b, part: rail("1") }];
    // A loose part touches itself and no page, device or project data.
    let t = Touched::of(&ops);
    assert_eq!(t.loose_parts.iter().copied().collect::<Vec<_>>(), {
        let mut v = vec![a, b];
        v.sort();
        v
    });
    assert!(t.pages.is_empty() && t.devices.is_empty() && !t.project && !t.page_order);
    l.commit(Author::User, "add", ops).unwrap();
    assert_eq!(l.document().loose_parts.len(), 2);
    assert!(l.document().devices.is_empty(), "a loose part is not a device");

    let mut cut = rail("3.6");
    cut.unit = ItemUnit::Metres;
    cut.location = Some(LocationId(0xc1));
    l.commit(Author::User, "edit", vec![Op::ModifyLoosePart { id: a, before: rail("2"), after: cut.clone() }]).unwrap();
    assert_eq!(l.document().loose_parts.get(&a), Some(&cut));
    // A stale `before` is refused.
    assert!(l.commit(Author::User, "stale", vec![Op::ModifyLoosePart { id: a, before: rail("2"), after: rail("5") }]).is_err());
    // A twice-made one is refused.
    assert!(l.commit(Author::User, "again", vec![Op::CreateLoosePart { id: b, part: rail("1") }]).is_err());
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().loose_parts.get(&a), Some(&rail("2")));

    l.commit(Author::User, "erase", vec![Op::DeleteLoosePart { id: a, part: rail("2") }]).unwrap();
    assert_eq!(l.document().loose_parts.keys().copied().collect::<Vec<_>>(), vec![b]);
    l.undo(Author::User).unwrap();
    assert!(l.document().loose_parts.contains_key(&a));
    l.redo(Author::User).unwrap();
    assert!(!l.document().loose_parts.contains_key(&a));
    l.document().check_index().unwrap();

    // The diff between two documents records loose part ops.
    let before = l.document().clone();
    l.commit(Author::User, "more", vec![Op::CreateLoosePart { id: a, part: rail("4") }]).unwrap();
    let keep = |_: &str| true;
    assert_eq!(ecad_model::document::diff::ops_between(&before, l.document(), &keep), vec![Op::CreateLoosePart { id: a, part: rail("4") }]);
    assert_eq!(Op::CreateLoosePart { id: a, part: rail("4") }.inverse(), Op::DeleteLoosePart { id: a, part: rail("4") });
    let q = Qty::parse("4").unwrap();
    assert_eq!(q, Qty(4000));
}

#[test]
fn a_loose_part_is_well_formed_and_read_strictly() {
    use ecad_model::objects::loose_part::{ItemUnit, LoosePart, Qty};
    use ecad_model::{LocationId, LoosePartId};
    let mut ids = IdAllocator::seeded(0x72fb);
    let mut l = Ledger::new(0x72fb);
    let a: LoosePartId = ids.fresh();
    // Pieces are whole; a quantity is more than 0 and at most 9999.
    for bad in [LoosePart { qty: Qty(3600), ..rail("1") }, LoosePart { qty: Qty(0), ..rail("1") }, LoosePart { qty: Qty(10_000_000), ..rail("1") }] {
        assert!(l.commit(Author::User, "bad", vec![Op::CreateLoosePart { id: a, part: bad.clone() }]).is_err(), "{bad:?} is refused");
    }
    let metres = LoosePart { qty: Qty(3600), unit: ItemUnit::Metres, ..rail("1") };
    l.commit(Author::User, "ok", vec![Op::CreateLoosePart { id: a, part: metres.clone() }]).unwrap();
    assert!(
        l.commit(
            Author::User,
            "bad",
            vec![Op::ModifyLoosePart { id: a, before: metres.clone(), after: LoosePart { unit: ItemUnit::Pcs, ..metres } }]
        )
        .is_err(),
        "3.6 pieces"
    );

    let located = LoosePart { location: Some(LocationId(0xc1)), unit: ItemUnit::Metres, qty: Qty(750), ..rail("1") };
    let loc = format!("{:032x}", 0xc1);
    let located_text = format!(r#"{{"description":"DIN rail 35 mm","qty":"0.75","unit":"m","location":"{loc}"}}"#);
    let cases: Vec<(String, LoosePart)> = vec![(r#"{"description":"DIN rail 35 mm","qty":"2"}"#.to_string(), rail("2")), (located_text, located)];
    for (text, part) in &cases {
        assert_eq!(&serde_json::from_str::<LoosePart>(text).unwrap(), part, "{text}");
        assert_eq!(&serde_json::to_string(part).unwrap(), text);
    }
    for not_stored in [
        r#"{"qty":"2"}"#,
        r#"{"description":"DIN rail 35 mm"}"#,
        r#"{"description":"","qty":"2"}"#,
        r#"{"description":" DIN rail","qty":"2"}"#,
        r#"{"description":"DIN rail","qty":2}"#,
        r#"{"description":"DIN rail","qty":"0"}"#,
        r#"{"description":"DIN rail","qty":"-1"}"#,
        r#"{"description":"DIN rail","qty":"2.50"}"#,
        r#"{"description":"DIN rail","qty":" 2"}"#,
        r#"{"description":"DIN rail","qty":"1e3"}"#,
        r#"{"description":"DIN rail","qty":"10000"}"#,
        r#"{"description":"DIN rail","qty":"2","unit":"metres"}"#,
        r#"{"description":"DIN rail","qty":"2","kind":"bom_item"}"#,
        r#"{"description":"DIN rail","qty":"2","location":"+CP1"}"#,
    ] {
        assert!(serde_json::from_str::<LoosePart>(not_stored).is_err(), "{not_stored} is a read error");
    }
}
