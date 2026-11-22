#![allow(clippy::unwrap_used)]

use crate::Document;
use crate::document::op::Op;
use crate::error::ModelError;

use std::panic::{AssertUnwindSafe, catch_unwind};

use super::journal::hooks;
use super::*;
use crate::naming::id::{CableId, ConductorId, ConnectionPointId, DeviceId, LayerId, NetId, PageId, WireId};
use crate::{Cable, ConnectionPoint, Core, Device, End, Layer, Page, Wire};

fn json(doc: &Document) -> String {
    serde_json::to_string(doc).unwrap()
}

/// A ledger with a component, three terminals, a wire from the first to the third and a cable
/// holding it.
fn ledger() -> Ledger {
    let mut l = Ledger::new(3);
    let (c, w) = (DeviceId(3), WireId(6));
    l.commit(
        Author::User,
        "setup",
        vec![
            Op::AddPage { id: PageId(1), page: Page::titled("1").unwrap(), index: 0 },
            Op::AddLayer { id: LayerId(2), layer: Layer { name: "0".into() } },
            Op::CreateDevice { id: c, device: Device::default() },
            Op::AddConnectionPoint {
                id: ConnectionPointId(4),
                connection_point: ConnectionPoint { owner: c.into(), label: None, key: None, accepts: Default::default() },
            },
            Op::AddConnectionPoint {
                id: ConnectionPointId(5),
                connection_point: ConnectionPoint { owner: c.into(), label: None, key: None, accepts: Default::default() },
            },
            Op::AddConnectionPoint {
                id: ConnectionPointId(10),
                connection_point: ConnectionPoint { owner: c.into(), label: None, key: None, accepts: Default::default() },
            },
            Op::CreateWire { id: w, wire: Wire::new(NetId(14), ConductorId(15), ConnectionPointId(4), ConnectionPointId(10)) },
            Op::CreateCable { id: CableId(7), cable: Cable { cores: vec![Core::on_wire(w, None)], ..Cable::default() } },
        ],
    )
    .unwrap();
    l
}

/// A valid three-op transaction touching indexes.
fn three_ops() -> Vec<Op> {
    vec![
        Op::CreateWire { id: WireId(11), wire: Wire::new(NetId(12), ConductorId(13), ConnectionPointId(4), ConnectionPointId(5)) },
        Op::ModifyCable {
            id: CableId(7),
            before: Cable { cores: vec![Core::on_wire(WireId(6), None)], ..Cable::default() },
            after: Cable::default(),
        },
        Op::AddPage { id: PageId(8), page: Page::titled("2").unwrap(), index: 1 },
    ]
}

fn assert_unchanged(l: &Ledger, doc: &str, head: u64) {
    assert_eq!(json(l.document()), doc);
    l.document().check_index().unwrap();
    assert_eq!(l.head(), head);
    assert_eq!(l.state_at(head).unwrap(), *l.document());
    assert_eq!(l.document().ends_at(ConnectionPointId(4)), vec![(WireId(6), End::A)]);
    assert!(l.document().ends_at(ConnectionPointId(5)).is_empty());
    assert_eq!(l.document().cables_of(WireId(6)), vec![CableId(7)]);
}

#[test]
fn a_panic_in_the_middle_of_a_transaction_leaves_document_indexes_and_log_as_they_were() {
    // Panic before op 1, between ops, and after the last op (in the end check's place).
    for at in 0..=3 {
        let mut l = ledger();
        let (doc, head) = (json(l.document()), l.head());
        hooks::PANIC_AT.with(|c| c.set(Some(at)));
        let r = catch_unwind(AssertUnwindSafe(|| l.commit(Author::User, "boom", three_ops())));
        hooks::PANIC_AT.with(|c| c.set(None));
        assert!(r.is_err(), "panicked at {at}");
        assert_unchanged(&l, &doc, head);
        assert!(l.diagnostics().is_empty());
        // The ledger carries on: the same transaction now commits, replays and undoes.
        l.commit(Author::User, "again", three_ops()).unwrap();
        assert_eq!(Ledger::replay(l.log().to_vec(), 0).unwrap().document(), l.document());
        l.undo(Author::User).unwrap();
        assert_unchanged(&l, &doc, head + 2);
    }
}

#[test]
fn a_panic_during_undo_also_rolls_back() {
    let mut l = ledger();
    l.commit(Author::User, "edit", three_ops()).unwrap();
    let (doc, head) = (json(l.document()), l.head());
    hooks::PANIC_AT.with(|c| c.set(Some(2)));
    let r = catch_unwind(AssertUnwindSafe(|| l.undo(Author::User)));
    hooks::PANIC_AT.with(|c| c.set(None));
    assert!(r.is_err());
    assert_eq!(json(l.document()), doc);
    assert_eq!(l.head(), head);
    l.document().check_index().unwrap();
    assert_eq!(l.undo_label(), Some("edit"));
}

#[test]
fn a_failed_revert_rebuilds_the_document_from_the_log() {
    let mut l = ledger();
    let (doc, head) = (json(l.document()), l.head());
    // Ops that apply, then an end state that fails: a wire end on a missing terminal.
    let ops = vec![
        Op::AddPage { id: PageId(8), page: Page::titled("2").unwrap(), index: 1 },
        Op::CreateWire { id: WireId(11), wire: Wire::new(NetId(12), ConductorId(13), ConnectionPointId(4), ConnectionPointId(99)) },
    ];
    hooks::FAIL_REVERT.with(|c| c.set(true));
    // The debug assertion fires only after the document has been rebuilt.
    let r = catch_unwind(AssertUnwindSafe(|| l.commit(Author::User, "bad", ops)));
    hooks::FAIL_REVERT.with(|c| c.set(false));
    match r {
        Ok(r) => assert!(matches!(r, Err(ModelError::Invariant(_))), "{r:?}"),
        Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
    }
    assert_unchanged(&l, &doc, head);
    assert_eq!(l.diagnostics().len(), 1, "{:?}", l.diagnostics());
    assert!(l.diagnostics().iter().all(|d| d.contains("rebuilt from the log")), "{:?}", l.diagnostics());
    l.commit(Author::User, "fine", three_ops()).unwrap();
}

#[test]
fn a_failed_revert_on_a_damaged_file_opens_at_the_last_good_transaction() {
    let mut l = ledger();
    l.commit(Author::User, "edit", three_ops()).unwrap();
    let good = l.document().clone();
    let mut log = l.log().to_vec();
    let mut bad = log.get(1).cloned().unwrap();
    bad.seq = 3;
    bad.ops = vec![
        Op::AddLayer { id: LayerId(50), layer: Layer { name: "x".into() } },
        Op::RemoveConnectionPoint {
            id: ConnectionPointId(5),
            connection_point: ConnectionPoint { owner: DeviceId(3).into(), label: None, key: None, accepts: Default::default() },
        },
    ];
    log.push(bad);
    hooks::FAIL_REVERT.with(|c| c.set(true));
    let r = catch_unwind(AssertUnwindSafe(|| Ledger::replay_prefix(log.clone(), 0)));
    hooks::FAIL_REVERT.with(|c| c.set(false));
    match r {
        Ok((opened, failure)) => {
            assert_eq!(opened.document(), &good);
            opened.document().check_index().unwrap();
            assert!(matches!(failure, Some((2, ModelError::Invariant(_)))), "{failure:?}");
            assert_eq!(opened.diagnostics().len(), 1);
        }
        Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
    }
}

#[test]
fn only_the_value_1_turns_the_switch_on() {
    use std::ffi::OsStr;
    assert!(crate::dev::switch_on(Some(OsStr::new("1"))));
    for v in ["0", "", "true", "yes", " 1"] {
        assert!(!crate::dev::switch_on(Some(OsStr::new(v))), "{v:?}");
    }
    assert!(!crate::dev::switch_on(None));
}

#[test]
fn the_test_build_verifies() {
    assert!(crate::dev::verifying(false), "every cargo test runs in verify mode");
}

#[test]
fn with_applied_shows_the_changed_document_and_reverts_it() {
    let mut l = ledger();
    let (doc, head) = (json(l.document()), l.head());
    let seen = l.with_applied(&three_ops(), |d| (d.ends_at(ConnectionPointId(5)), d.cables_of(WireId(6)), d.pages.len(), d.check().is_ok())).unwrap();
    assert_eq!(seen, (vec![(WireId(11), End::B)], vec![], 2, true));
    assert_unchanged(&l, &doc, head);
    // The plain document form does the same.
    let mut d = l.document().clone();
    assert_eq!(d.with_applied(&three_ops(), |d| d.pages.len()).unwrap(), 2);
    assert_eq!(json(&d), doc);
    d.check_index().unwrap();
}

#[test]
fn with_applied_refuses_as_commit_does_without_running_the_closure() {
    let mut l = ledger();
    let (doc, head) = (json(l.document()), l.head());
    let bad_end = vec![
        Op::AddPage { id: PageId(8), page: Page::titled("2").unwrap(), index: 1 },
        Op::CreateWire { id: WireId(11), wire: Wire::new(NetId(12), ConductorId(13), ConnectionPointId(4), ConnectionPointId(99)) },
    ];
    // A recorded wire that differs from the one in the document.
    let bad_before = vec![Op::DeleteWire { id: WireId(6), wire: Wire::new(NetId(14), ConductorId(15), ConnectionPointId(5), ConnectionPointId(10)) }];
    for ops in [bad_end, bad_before] {
        let mut ran = false;
        let previewed = l.with_applied(&ops, |_| ran = true).map_err(|e| e.to_string());
        assert!(!ran, "the closure runs only on a valid end state");
        let committed = l.clone().commit(Author::User, "x", ops).map(|_| ()).map_err(|e| e.to_string());
        assert!(previewed.is_err());
        assert_eq!(previewed, committed, "the same error as a commit");
        assert_unchanged(&l, &doc, head);
    }
}

#[test]
fn a_panic_inside_the_with_applied_closure_leaves_document_indexes_and_log_as_they_were() {
    let mut l = ledger();
    let (doc, head) = (json(l.document()), l.head());
    let r = catch_unwind(AssertUnwindSafe(|| {
        l.with_applied(&three_ops(), |d| {
            assert_eq!(d.pages.len(), 2, "the closure sees the change");
            std::panic::resume_unwind(Box::new("panic in the closure"));
        })
    }));
    assert!(r.is_err());
    assert_unchanged(&l, &doc, head);
    assert!(l.diagnostics().is_empty());
    // A panic while the ops are being applied is reverted too.
    for at in 0..=3 {
        hooks::PANIC_AT.with(|c| c.set(Some(at)));
        let r = catch_unwind(AssertUnwindSafe(|| l.with_applied(&three_ops(), |_| ())));
        hooks::PANIC_AT.with(|c| c.set(None));
        assert!(r.is_err(), "panicked at {at}");
        assert_unchanged(&l, &doc, head);
    }
    // And the plain document form.
    let mut d = l.document().clone();
    let r = catch_unwind(AssertUnwindSafe(|| d.with_applied(&three_ops(), |_| std::panic::resume_unwind(Box::new("boom")))));
    assert!(r.is_err());
    assert_eq!(json(&d), doc);
    d.check_index().unwrap();
    // The ledger carries on.
    l.commit(Author::User, "edit", three_ops()).unwrap();
}

#[test]
fn with_applied_reports_a_rollback_it_could_not_repair_at_once() {
    // A log that no longer replays, so a failed revert cannot be repaired from it.
    let mut l = ledger();
    let mut unreplayable = l.log.first().cloned().unwrap();
    unreplayable.seq = 2;
    unreplayable.ops = vec![Op::RemoveLayer { id: LayerId(99), layer: Layer { name: "gone".into() } }];
    l.log.push(unreplayable);
    hooks::FAIL_REVERT.with(|c| c.set(true));
    let r = catch_unwind(AssertUnwindSafe(|| l.with_applied(&three_ops(), |_| ())));
    hooks::FAIL_REVERT.with(|c| c.set(false));
    match r {
        Ok(r) => assert!(matches!(r, Err(ModelError::CorruptLog(_))), "{r:?}"),
        Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
    }
    assert!(l.health.broken);
    assert!(matches!(l.with_applied(&three_ops(), |_| ()), Err(ModelError::CorruptLog(_))));
}

#[test]
fn with_applied_runs_the_full_check_in_verify_mode() {
    // A terminal written straight into the map: the full check passes, the index check does
    // not, and the scoped check of `three_ops` never looks at it. Only verify mode notices.
    let mut l = ledger();
    l.doc
        .connection_points
        .insert(ConnectionPointId(50), ConnectionPoint { owner: DeviceId(3).into(), label: None, key: None, accepts: Default::default() });
    let doc = json(l.document());
    let r = catch_unwind(AssertUnwindSafe(|| l.with_applied(&three_ops(), |_| ())));
    match r {
        Ok(r) => assert!(matches!(r, Err(ModelError::Invariant(_))), "{r:?}"),
        // The disagreement between the scoped and the full check is asserted in debug builds,
        // once the document is settled.
        Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
    }
    assert_eq!(json(l.document()), doc);
}
