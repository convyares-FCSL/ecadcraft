//! Save and retry (proposal §20.5), in memory: our own failed bytes are told apart from external
//! changes, a failed flush is never retried on the same bytes, and no transaction is ever
//! duplicated or lost.

mod common;

use common::*;
use ecad_file::{AttemptStage, Outcome, ProjectFile, SaveError, SaveOutcome, open_project};
use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};

/// A file holding the first `n` transactions of the fixture, and the full ledger with the rest
/// pending.
fn half_saved(n: usize) -> (ProjectFile<MemoryMedium>, Ledger) {
    let full = fixture().ledger.log().to_vec();
    let mut prefix = Ledger::replay(full[..n].to_vec(), SEED).unwrap();
    let file = ProjectFile::create(MemoryMedium::default(), PROJECT, &mut prefix, None, WRITER).unwrap();
    let mut ledger = Ledger::replay(full, SEED).unwrap();
    ledger.mark_persisted(n as u64).unwrap();
    (file, ledger)
}

fn edit(ledger: &mut Ledger, title: &str) {
    let id = PageId(0x1000 + ledger.head() as u128);
    let index = ledger.document().page_order.len();
    ledger.commit(Author::User, title, vec![Op::AddPage { id, page: Page { title: title.into() }, index }]).unwrap();
}

/// The file reopened: every in-memory transaction exactly once, nothing else.
fn assert_file_holds(file: &ProjectFile<MemoryMedium>, ledger: &Ledger) {
    let o = open_project(&file.medium().read_all().unwrap(), SEED).unwrap();
    assert_eq!(o.report.outcome, Outcome::Clean, "{}", o.report.describe());
    assert_eq!(o.ledger.log(), ledger.log());
    assert_eq!(ledger.persisted(), ledger.head());
    assert_eq!(file.state().persisted, ledger.head());
    assert!(file.state().attempt.is_none());
}

#[test]
fn a_plain_save_appends_and_acknowledges() {
    let (mut file, mut ledger) = half_saved(5);
    let r = file.save(&mut ledger).unwrap();
    assert!(matches!(r, SaveOutcome::Saved { first_seq: 6, .. }));
    assert_file_holds(&file, &ledger);
    assert_eq!(file.save(&mut ledger).unwrap(), SaveOutcome::NothingToSave);
}

#[test]
fn a_partial_append_is_retried_without_duplicates() {
    let (file0, ledger0) = half_saved(5);
    let batch_len = {
        let (mut f, mut l) = half_saved(5);
        let before = f.medium().len();
        f.save(&mut l).unwrap();
        (f.medium().len() - before) as usize
    };
    // Cut anywhere: nothing written, mid-record, exactly at a record boundary, all but one byte.
    for cut in [0, 1, 37, batch_len / 2, batch_len - 1] {
        let mut file = ProjectFile::open(file0.medium().clone(), SEED).unwrap().0;
        let mut ledger = ledger0.clone();
        file.medium_mut().partial_append = Some(cut);
        assert!(matches!(file.save(&mut ledger), Err(SaveError::Append(_))), "cut {cut}");
        assert!(ledger.is_unsaved() && ledger.persisted() == 5, "cut {cut}: nothing acknowledged");
        let attempt = file.state().attempt.unwrap();
        assert_eq!((attempt.stage, attempt.first_seq), (AttemptStage::AppendFailed, 6));
        // A new edit between the failure and the retry is included once.
        edit(&mut ledger, &format!("after cut {cut}"));
        file.save(&mut ledger).unwrap();
        assert_file_holds(&file, &ledger);
    }
}

#[test]
fn a_failed_flush_is_retried_by_rewriting_never_by_flushing_the_same_bytes() {
    let (mut file, mut ledger) = half_saved(5);
    let len_before = file.medium().len();
    file.medium_mut().fail_next_flush = true;
    assert!(matches!(file.save(&mut ledger), Err(SaveError::Flush(_))));
    assert_eq!(ledger.persisted(), 5);
    let attempt = file.state().attempt.unwrap();
    assert_eq!(attempt.stage, AttemptStage::FlushFailed);
    assert_eq!(file.medium().len(), len_before + attempt.bytes.len() as u64, "the whole batch reached the medium");
    // The retry truncates back to the verified prefix and writes the batch again, then flushes.
    let flushes = file.medium().flushes;
    file.save(&mut ledger).unwrap();
    assert_eq!(file.medium().flushes, flushes + 1);
    assert_file_holds(&file, &ledger);
}

#[test]
fn a_failed_truncate_keeps_the_attempt_for_the_next_retry() {
    let (mut file, mut ledger) = half_saved(5);
    file.medium_mut().partial_append = Some(20);
    assert!(file.save(&mut ledger).is_err());
    file.medium_mut().fail_next_truncate = true;
    assert!(matches!(file.save(&mut ledger), Err(SaveError::Truncate(_))));
    assert!(file.state().attempt.is_some(), "still identifiable as ours");
    file.save(&mut ledger).unwrap();
    assert_file_holds(&file, &ledger);
}

#[test]
fn external_changes_are_refused_and_the_file_is_left_alone() {
    type Change = fn(&mut Vec<u8>, usize);
    let changes: &[(&str, bool, Change)] = &[
        ("appended after a clean save", false, |b, _| b.extend_from_slice(b"{\"x\":1}\n")),
        ("appended after a failed save", true, |b, _| b.extend_from_slice(b"ZZZZ")),
        ("shortened", false, |b, _| {
            let n = b.len() - 10;
            b.truncate(n);
        }),
        ("last saved record edited", false, |b, verified| b[verified - 5] ^= 1),
        // Same length, last line intact: only a check of the whole prefix sees these.
        ("an earlier record edited", false, |b, verified| b[verified / 2] ^= 1),
        ("the header edited", false, |b, _| b[30] ^= 1),
        ("an earlier record edited after a failed save", true, |b, verified| b[verified / 2] ^= 1),
    ];
    for (name, after_failure, change) in changes {
        let (mut file, mut ledger) = half_saved(5);
        if *after_failure {
            file.medium_mut().partial_append = Some(3);
            assert!(file.save(&mut ledger).is_err());
        }
        let verified = file.state().verified_len as usize;
        change(file.medium_mut().external(), verified);
        let before = file.medium().read_all().unwrap();
        let r = file.save(&mut ledger);
        assert!(matches!(r, Err(SaveError::ExternalChange(_))), "{name}: {r:?}");
        assert_eq!(file.medium().read_all().unwrap(), before, "{name}: untouched");
        assert!(ledger.is_unsaved(), "{name}: still unsaved");
    }
}

#[test]
fn an_incomplete_tail_found_on_open_is_replaced_by_the_next_save() {
    let (mut file, mut ledger) = half_saved(5);
    file.medium_mut().partial_append = Some(50);
    assert!(file.save(&mut ledger).is_err());
    // The process "crashes": reopen the medium as it is.
    let (mut reopened, mut recovered, report) = ProjectFile::open(file.medium().clone(), SEED).unwrap();
    assert!(matches!(report.outcome, Outcome::IncompleteTail { len: 50, .. }));
    assert_eq!(reopened.state().pending_fragment, Some(50));
    edit(&mut recovered, "after recovery");
    reopened.save(&mut recovered).unwrap();
    assert_file_holds(&reopened, &recovered);
}

#[test]
fn a_different_history_is_not_appended() {
    let (mut file, _) = half_saved(5);
    let mut other = Ledger::new(99);
    edit(&mut other, "unrelated");
    for _ in 0..6 {
        edit(&mut other, "more");
    }
    assert!(matches!(file.save(&mut other), Err(SaveError::NotThisHistory(_))));
}

#[test]
fn a_new_file_needs_an_empty_target() {
    let mut f = fixture();
    let target = MemoryMedium::from_bytes(b"something".to_vec());
    assert!(matches!(ProjectFile::create(target, PROJECT, &mut f.ledger, None, WRITER), Err(SaveError::NotEmpty)));
    // A failed first write leaves the in-memory project unsaved.
    let mut m = MemoryMedium::default();
    m.fail_next_flush = true;
    let mut ledger = Ledger::replay(f.ledger.log().to_vec(), SEED).unwrap();
    ledger.mark_persisted(0).unwrap();
    assert!(matches!(ProjectFile::create(m, PROJECT, &mut ledger, None, WRITER), Err(SaveError::Flush(_))));
    assert!(ledger.is_unsaved());
}

#[test]
fn a_reopened_file_checks_its_whole_prefix_too() {
    let (file, mut ledger) = half_saved(5);
    let (mut reopened, _, _) = ProjectFile::open(file.into_medium(), SEED).unwrap();
    let verified = reopened.state().verified_len as usize;
    reopened.medium_mut().external()[verified / 2] ^= 1;
    let before = reopened.medium().read_all().unwrap();
    let r = reopened.save(&mut ledger);
    assert!(matches!(r, Err(SaveError::ExternalChange(_))), "{r:?}");
    assert_eq!(reopened.medium().read_all().unwrap(), before, "untouched");
    // Undo the change: the prefix is ours again and the save goes through.
    reopened.medium_mut().external()[verified / 2] ^= 1;
    reopened.save(&mut ledger).unwrap();
    assert_file_holds(&reopened, &ledger);
}
