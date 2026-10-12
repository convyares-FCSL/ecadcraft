//! Contract tests for the test-only store: SAVE-01..SAVE-04, FILE-02..FILE-04.

use ecad_model::{Author, Component, IdAllocator, Layer, Ledger, Op, Page};
use ecad_store::{Medium, MemoryMedium, TestStore};

fn ledger_with_edits(n: usize) -> (Ledger, IdAllocator) {
    let mut ids = IdAllocator::seeded(1);
    let mut l = Ledger::new(2);
    l.commit(
        Author::User,
        "New project",
        vec![
            Op::AddPage { id: ids.fresh(), page: Page { title: "1".into() }, index: 0 },
            Op::AddLayer { id: ids.fresh(), layer: Layer { name: "0".into() } },
        ],
    )
    .unwrap();
    for _ in 0..n {
        l.commit(
            Author::User,
            "component",
            vec![Op::CreateComponent { id: ids.fresh(), component: Component::default() }],
        )
        .unwrap();
    }
    (l, ids)
}

#[test]
fn save_then_open_gives_the_same_state() {
    let (mut l, _) = ledger_with_edits(3);
    let mut store = TestStore::new(MemoryMedium::default());
    store.save(&mut l).unwrap();
    assert!(!l.is_unsaved());
    let opened = store.open(9).unwrap();
    assert_eq!(opened.document(), l.document());
    assert_eq!(opened.log(), l.log());
    assert!(!opened.is_unsaved());
}

#[test]
fn a_failed_save_after_a_new_edit_keeps_it_unsaved_and_the_file_intact() {
    for fail_flush in [false, true] {
        let (mut l, mut ids) = ledger_with_edits(1);
        let mut store = TestStore::new(MemoryMedium::default());
        store.save(&mut l).unwrap();
        let saved_bytes = store.medium.read_all().unwrap();
        let saved_head = l.head();

        // A new, non-graphical edit after the successful save.
        l.commit(
            Author::User,
            "new edit",
            vec![Op::CreateComponent { id: ids.fresh(), component: Component::default() }],
        )
        .unwrap();
        assert!(l.is_unsaved());

        if fail_flush {
            store.medium.fail_next_flush = true;
        } else {
            store.medium.fail_next_append = true;
        }
        assert!(store.save(&mut l).is_err());
        assert!(l.is_unsaved(), "still unsaved after the failure");
        assert_eq!(l.persisted(), saved_head);
        assert_eq!(store.medium.read_all().unwrap(), saved_bytes, "the file holds exactly the first save");
        assert_eq!(store.open(3).unwrap().head(), saved_head);

        store.save(&mut l).unwrap();
        assert!(!l.is_unsaved());
        assert_eq!(store.open(3).unwrap().document(), l.document());
    }
}

#[test]
fn a_torn_trailing_line_is_ignored_on_open() {
    let (mut l, _) = ledger_with_edits(2);
    let mut store = TestStore::new(MemoryMedium::default());
    store.save(&mut l).unwrap();
    let head = l.head();
    store.medium.append(b"{\"id\":").unwrap();
    let opened = store.open(1).unwrap();
    assert_eq!(opened.head(), head);
}

#[test]
fn checkpoints_are_disposable_caches() {
    let (mut l, mut ids) = ledger_with_edits(2);
    let mut store = TestStore::new(MemoryMedium::default());
    store.save(&mut l).unwrap();
    store.write_checkpoint(&l).unwrap();
    l.commit(
        Author::User,
        "after checkpoint",
        vec![Op::CreateComponent { id: ids.fresh(), component: Component::default() }],
    )
    .unwrap();
    l.undo(Author::User).unwrap();
    store.save(&mut l).unwrap();
    let with = store.open_with_checkpoint(1).unwrap();
    store.drop_checkpoint();
    let without = store.open_with_checkpoint(1).unwrap();
    assert_eq!(with.document(), l.document());
    assert_eq!(without.document(), l.document());
    assert_eq!(with.log(), without.log());
}
