//! Project files on disk (proposal §20.5, stage 2): save, quit and reopen; every step of Save and
//! Save As failing; a crash mid-batch; the writer lock; external changes; damaged files.
#![cfg(not(target_arch = "wasm32"))]

use crate::common;

use std::path::{Path, PathBuf};

use common::*;
use ecad_file::{DiskError, DiskProject, Durability, Outcome, ProjectFile, ReadOnly, StepFaults};
use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-file-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    /// File names in the folder, sorted.
    fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn edit(ledger: &mut Ledger, title: &str) {
    let id = PageId(0x2000 + ledger.head() as u128);
    let index = ledger.document().page_order.len();
    ledger.commit(Author::User, title, vec![Op::AddPage { id, page: Page::new(title), index }]).unwrap();
}

fn create(
    path: &Path,
    ledger: &mut Ledger,
    faults: StepFaults,
) -> std::result::Result<(DiskProject, Durability), DiskError> {
    DiskProject::create(path, PROJECT, ledger, None, WRITER, faults)
}

/// The file reopened by another session: clean, and holding exactly `ledger`'s history.
fn assert_reopens_as(path: &Path, ledger: &Ledger) {
    let o = DiskProject::open(path, SEED, "checker").unwrap();
    assert_eq!(o.report.outcome, Outcome::Clean, "{}", o.report.describe());
    assert_eq!(o.ledger.log(), ledger.log());
    assert_eq!(o.ledger.document(), ledger.document());
    assert_eq!(o.ledger.persisted(), o.ledger.head());
}

#[test]
fn save_quit_and_reopen_keep_document_ids_log_and_head() {
    let d = TempDir::new("roundtrip");
    let p = d.path("demo.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, durability) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    assert_eq!(durability, Durability::Confirmed);
    assert!(!ledger.is_unsaved());
    edit(&mut ledger, "2");
    edit(&mut ledger, "3");
    assert!(ledger.is_unsaved());
    project.save(&mut ledger).unwrap();
    assert_eq!(ledger.persisted(), ledger.head());
    drop(project); // quit
    let o = DiskProject::open(&p, SEED, WRITER).unwrap();
    assert_eq!(o.report.outcome, Outcome::Clean);
    assert_eq!(o.ledger.document(), ledger.document(), "same objects with the same IDs");
    assert_eq!(o.ledger.log(), ledger.log(), "same history");
    assert_eq!((o.ledger.head(), o.ledger.persisted()), (ledger.head(), ledger.head()));
    assert_eq!(o.project.project_id(), Some(PROJECT));
    assert_eq!(o.reclaimed_lock, None, "a clean quit leaves no leftover lock");
    // Saving in two steps on disk gives the same bytes as one write in memory.
    let mut again = ledger.clone();
    let mem = ProjectFile::create(MemoryMedium::default(), PROJECT, &mut again, None, WRITER).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), mem.medium().read_all().unwrap());
    assert_eq!(d.names(), ["demo.ecad", "demo.ecad.lock"], "no temporary files left");
}

#[test]
fn a_save_as_failing_before_replacement_leaves_the_target_unchanged() {
    type Inject = fn(&mut StepFaults);
    let cases: &[(&str, &str, Inject)] = &[
        ("temporary file creation", "creating the temporary file", |f| f.fail_temp_create = true),
        ("torn write", "writing the temporary file", |f| f.temp_partial_append = Some(37)),
        ("flush", "flushing the temporary file", |f| f.fail_temp_flush = true),
        ("rename", "replacing the file", |f| f.fail_rename = true),
    ];
    for (name, step, inject) in cases {
        for existing in [false, true] {
            let d = TempDir::new("before");
            let p = d.path("p.ecad");
            if existing {
                let mut old = Ledger::replay(fixture().ledger.log()[..3].to_vec(), SEED).unwrap();
                drop(create(&p, &mut old, StepFaults::default()).unwrap());
            }
            let before = std::fs::read(&p).ok();
            let mut ledger = fixture().ledger;
            let mut faults = StepFaults::default();
            inject(&mut faults);
            let e = create(&p, &mut ledger, faults).unwrap_err();
            let text = e.to_string();
            assert!(matches!(e, DiskError::BeforeReplacement { .. }), "{name}: {text}");
            assert!(text.contains(step) && text.contains("the existing file is unchanged"), "{name}: {text}");
            assert_eq!(std::fs::read(&p).ok(), before, "{name} (existing {existing}): target untouched");
            assert!(d.names().iter().all(|n| !n.contains(".tmp-")), "{name}: temporary file removed: {:?}", d.names());
            assert_eq!(ledger.persisted(), 0, "{name}: nothing acknowledged");
            // The target's lock was released: the save can be retried at once.
            create(&p, &mut ledger, StepFaults::default()).unwrap();
            assert_reopens_as(&p, &ledger);
        }
    }
}

#[test]
fn a_failed_folder_flush_after_replacement_is_unconfirmed_until_a_save_confirms_it() {
    let d = TempDir::new("dirsync");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let faults = StepFaults { fail_dir_sync: true, ..StepFaults::default() };
    let (mut project, durability) = create(&p, &mut ledger, faults).unwrap();
    assert!(matches!(durability, Durability::Unconfirmed(_)));
    assert!(project.unconfirmed());
    assert!(ledger.is_unsaved(), "nothing acknowledged while durability is unconfirmed");
    assert_reopens_as(&p, &ledger); // the new file is in place
    // Confirming fails again: still unconfirmed, still unsaved, and the plain Save says that
    // nothing new was written (not "written").
    edit(&mut ledger, "2");
    let before = std::fs::read(&p).unwrap();
    project.faults.fail_dir_sync = true;
    let e = project.save(&mut ledger).unwrap_err();
    assert!(matches!(e, DiskError::NotConfirmed(_)), "{e}");
    let text = e.to_string();
    assert!(text.starts_with("not saved:") && text.contains("new changes were not written"), "{text}");
    assert!(text.contains("Save As to a folder that can be flushed"), "{text}");
    assert_eq!(std::fs::read(&p).unwrap(), before, "nothing appended");
    assert_eq!(ledger.persisted(), 0);
    // The next Save confirms the replacement, then appends.
    project.save(&mut ledger).unwrap();
    assert!(!project.unconfirmed());
    assert_eq!(ledger.persisted(), ledger.head());
    drop(project);
    assert_reopens_as(&p, &ledger);
}

#[test]
fn failed_appends_and_flushes_are_retried_without_duplicates() {
    let d = TempDir::new("retry");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    edit(&mut ledger, "2");
    project.file_mut().medium_mut().faults.partial_append = Some(10);
    assert!(project.save(&mut ledger).is_err());
    assert!(ledger.is_unsaved());
    edit(&mut ledger, "3");
    project.file_mut().medium_mut().faults.fail_next_flush = true;
    assert!(project.save(&mut ledger).is_err());
    assert!(ledger.is_unsaved());
    project.save(&mut ledger).unwrap();
    drop(project);
    assert_reopens_as(&p, &ledger);
}

#[test]
fn a_crash_mid_batch_reopens_at_a_valid_prefix_and_the_tail_is_preserved() {
    let d = TempDir::new("crash");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    let saved_head = ledger.head();
    edit(&mut ledger, "2");
    project.file_mut().medium_mut().faults.partial_append = Some(40);
    assert!(project.save(&mut ledger).is_err());
    drop(project); // the process ends here: 40 bytes of a record are left at the end
    let torn = std::fs::read(&p).unwrap();
    let o = DiskProject::open(&p, SEED, WRITER).unwrap();
    let Outcome::IncompleteTail { len, .. } = o.report.outcome else { panic!("{:?}", o.report.outcome) };
    assert_eq!(len, 40);
    assert_eq!(o.ledger.head(), saved_head, "the valid prefix");
    assert!(o.project.read_only().is_none(), "an interrupted write opens writable");
    let (mut project, mut reopened) = (o.project, o.ledger);
    edit(&mut reopened, "2 again");
    // Preserving the tail fails: the file is not touched and nothing is saved.
    project.faults.fail_sidecar = true;
    let e = project.save(&mut reopened).unwrap_err();
    assert!(matches!(e, DiskError::PreserveFailed { .. }), "{e}");
    assert!(e.to_string().contains("the file was not changed"), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), torn);
    assert!(reopened.is_unsaved());
    // Then it succeeds: the 40 bytes are in a uniquely named sidecar, and the file is clean.
    project.save(&mut reopened).unwrap();
    let sidecar = project.preserved_fragment().unwrap().to_path_buf();
    assert!(sidecar.file_name().unwrap().to_string_lossy().starts_with("p.ecad.tail-"));
    assert_eq!(std::fs::read(&sidecar).unwrap(), torn[torn.len() - 40..]);
    drop(project);
    assert_reopens_as(&p, &reopened);
}

#[test]
fn a_second_opener_is_read_only_and_can_save_a_copy() {
    let d = TempDir::new("lock");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (first, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    let o = DiskProject::open(&p, SEED, "second session").unwrap();
    // Who holds it: see `a_second_opener_is_told_who_is_editing`.
    assert!(matches!(o.project.read_only(), Some(ReadOnly::Locked { .. })), "not read-only");
    let (mut second, mut copy) = (o.project, o.ledger);
    edit(&mut copy, "2");
    let before = std::fs::read(&p).unwrap();
    let e = second.save(&mut copy).unwrap_err();
    assert!(e.to_string().contains("another session is editing it"), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), before);
    // Save As to another file works and keeps the project ID (E9).
    let q = d.path("copy.ecad");
    assert_eq!(second.save_as(&q, &mut copy, None, "second session").unwrap(), Durability::Confirmed);
    assert_eq!(second.project_id(), Some(PROJECT));
    assert!(second.read_only().is_none());
    assert!(!copy.is_unsaved());
    // Once the first session ends, the file can be written again, with no leftover reported.
    drop(first);
    let o = DiskProject::open(&p, SEED, "third").unwrap();
    assert!(o.project.read_only().is_none());
    assert_eq!(o.reclaimed_lock, None);
}

#[test]
fn a_second_opener_is_told_who_is_editing_where_the_system_shows_it() {
    let d = TempDir::new("holder");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (_first, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    let o = DiskProject::open(&p, SEED, "second session").unwrap();
    let Some(ReadOnly::Locked { holder }) = o.project.read_only().cloned() else { panic!("not read-only") };
    let text = o.project.read_only().unwrap().to_string();
    if cfg!(windows) {
        // The lock is mandatory on Windows, so the holder's record cannot be read; the message
        // says so rather than leaving it out (owner decision, brief 002).
        assert_eq!(holder, None);
        assert!(text.contains("Windows does not say which session"), "{text}");
    } else {
        assert!(holder.as_deref().is_some_and(|h| h.contains(WRITER)), "{holder:?}");
        assert!(text.contains(WRITER), "{text}");
    }
    // A session that has just ended may not have released its lock yet (review R-M3).
    assert!(text.contains("another session is editing it") && text.contains("may take a moment"), "{text}");
    // Save As onto that file is refused with the same explanation.
    let mut other = fixture().ledger;
    let q = d.path("q.ecad");
    let (mut project, _) = create(&q, &mut other, StepFaults::default()).unwrap();
    let e = project.save_as(&p, &mut other, None, WRITER).unwrap_err().to_string();
    assert!(e.contains("being edited in another session") && e.contains("may take a moment"), "{e}");
    assert_eq!(e.contains("Windows does not say which session"), cfg!(windows), "{e}");
}

#[test]
fn a_lock_that_cannot_be_taken_is_no_lock_not_another_session() {
    let d = TempDir::new("nolock");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    drop(create(&p, &mut ledger, StepFaults::default()).unwrap());
    // Nothing can be created or opened where the lock file belongs (as in a folder that cannot
    // be written): no lock can be taken, which says nothing about other sessions.
    let lock = ecad_store::disk::lock_path(&p);
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    let o = DiskProject::open(&p, SEED, WRITER).unwrap();
    assert!(matches!(o.project.read_only(), Some(ReadOnly::NoLock(_))), "{:?}", o.project.read_only());
    assert_eq!(o.ledger.log(), ledger.log(), "it can still be read");
    let (mut project, mut copy) = (o.project, o.ledger);
    edit(&mut copy, "2");
    let e = project.save(&mut copy).unwrap_err().to_string();
    assert!(e.contains("no writer lock could be taken") && !e.contains("another session"), "{e}");
    // Save As to a file whose lock can be taken rescues the work.
    let q = d.path("q.ecad");
    assert_eq!(project.save_as(&q, &mut copy, None, WRITER).unwrap(), Durability::Confirmed);
    drop(project);
    assert_reopens_as(&q, &copy);
}

/// Make `path` read-only (the read-only attribute on Windows, no write permission on Unix), or
/// writable again.
fn set_read_only(path: &Path, read_only: bool) {
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)] // a test file in a private temporary folder
    perms.set_readonly(read_only);
    std::fs::set_permissions(path, perms).unwrap();
}

#[test]
fn a_read_only_file_opens_read_only_and_says_why() {
    let d = TempDir::new("readonly");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    drop(create(&p, &mut ledger, StepFaults::default()).unwrap());
    set_read_only(&p, true);
    let o = DiskProject::open(&p, SEED, WRITER).unwrap();
    assert!(matches!(o.project.read_only(), Some(ReadOnly::NotWritable(_))), "{:?}", o.project.read_only());
    assert_eq!(o.report.outcome, Outcome::Clean);
    assert_eq!(o.ledger.log(), ledger.log());
    assert_eq!(o.project.lock_file(), None, "the lock is left to a session that can write");
    let (mut project, mut copy) = (o.project, o.ledger);
    edit(&mut copy, "2");
    let before = std::fs::read(&p).unwrap();
    let e = project.save(&mut copy).unwrap_err().to_string();
    assert!(e.contains("the file cannot be written") && e.contains("Save As"), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), before);
    // A second session is told the same, not that another session is editing.
    let second = DiskProject::open(&p, SEED, "second").unwrap();
    assert!(matches!(second.project.read_only(), Some(ReadOnly::NotWritable(_))));
    drop(second);
    // Save As to another file rescues the work.
    let q = d.path("q.ecad");
    assert_eq!(project.save_as(&q, &mut copy, None, WRITER).unwrap(), Durability::Confirmed);
    drop(project);
    assert_reopens_as(&q, &copy);
    // Writable again: it opens writable.
    set_read_only(&p, false);
    assert!(DiskProject::open(&p, SEED, WRITER).unwrap().project.read_only().is_none());
}

#[test]
fn a_lock_left_by_a_crash_is_reclaimed_and_reported() {
    let d = TempDir::new("leftover");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    drop(create(&p, &mut ledger, StepFaults::default()).unwrap());
    // What a session that crashed leaves: a record in a lock file nobody holds.
    std::fs::write(ecad_store::disk::lock_path(&p), "ecad trial; process 4242; since 1").unwrap();
    let o = DiskProject::open(&p, SEED, WRITER).unwrap();
    assert!(o.project.read_only().is_none(), "no lockout");
    assert_eq!(o.reclaimed_lock.as_deref(), Some("ecad trial; process 4242; since 1"));
}

#[test]
fn save_as_refuses_a_target_another_session_is_editing() {
    let d = TempDir::new("target");
    let p = d.path("p.ecad");
    let q = d.path("q.ecad");
    let mut other = Ledger::replay(fixture().ledger.log()[..2].to_vec(), SEED).unwrap();
    let (_editing_q, _) = create(&q, &mut other, StepFaults::default()).unwrap();
    let before = std::fs::read(&q).unwrap();
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    edit(&mut ledger, "2");
    let e = project.save_as(&q, &mut ledger, None, WRITER).unwrap_err();
    assert!(matches!(e, DiskError::TargetLocked { .. }), "{e}");
    assert_eq!(std::fs::read(&q).unwrap(), before, "nothing was written");
    assert_eq!(project.path(), p, "still bound to its own file");
    assert!(ledger.is_unsaved());
}

#[test]
fn save_as_keeps_the_project_id_and_releases_the_previous_lock_only_after_success() {
    let d = TempDir::new("saveas");
    let (p, q) = (d.path("p.ecad"), d.path("q.ecad"));
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    edit(&mut ledger, "2");
    // A failed Save As keeps the old binding and its lock.
    project.faults.fail_rename = true;
    assert!(project.save_as(&q, &mut ledger, None, WRITER).is_err());
    assert_eq!(project.path(), p);
    assert!(matches!(DiskProject::open(&p, SEED, "x").unwrap().project.read_only(), Some(ReadOnly::Locked { .. })));
    // A successful one binds to the new file and releases the old lock.
    project.save_as(&q, &mut ledger, None, WRITER).unwrap();
    assert_eq!(project.path(), q);
    assert_eq!(project.project_id(), Some(PROJECT));
    assert!(DiskProject::open(&p, SEED, "x").unwrap().project.read_only().is_none());
    assert!(matches!(DiskProject::open(&q, SEED, "x").unwrap().project.read_only(), Some(ReadOnly::Locked { .. })));
    // Save As onto its own file is a plain save.
    edit(&mut ledger, "3");
    assert_eq!(project.save_as(&q, &mut ledger, None, WRITER).unwrap(), Durability::Confirmed);
    drop(project);
    assert_reopens_as(&q, &ledger);
}

#[test]
fn external_changes_are_refused_and_the_file_left_alone() {
    let d = TempDir::new("external");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    edit(&mut ledger, "2");
    // Appended by something else.
    let mut bytes = std::fs::read(&p).unwrap();
    bytes.extend_from_slice(b"{\"x\":1}\n");
    std::fs::write(&p, &bytes).unwrap();
    let e = project.save(&mut ledger).unwrap_err();
    assert!(e.to_string().contains("changed by something else"), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    assert!(ledger.is_unsaved());
}

#[test]
fn a_file_renamed_over_the_project_is_refused_and_left_alone() {
    let d = TempDir::new("replaced");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    edit(&mut ledger, "2");
    let other = d.path("other");
    std::fs::write(&other, b"replacement").unwrap();
    std::fs::rename(&other, &p).unwrap();
    let e = project.save(&mut ledger).unwrap_err();
    assert!(matches!(e, DiskError::Replaced(_)), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), b"replacement");
    assert!(ledger.is_unsaved());
}

#[test]
fn a_project_deleted_and_recreated_is_refused_and_left_alone() {
    let d = TempDir::new("recreated");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    edit(&mut ledger, "2");
    // Deleted and written again at the same path (as some editors and sync clients save), with
    // the same bytes, so only the file's identity tells them apart.
    let bytes = std::fs::read(&p).unwrap();
    std::fs::remove_file(&p).unwrap();
    std::fs::write(&p, &bytes).unwrap();
    let e = project.save(&mut ledger).unwrap_err();
    assert!(matches!(e, DiskError::Replaced(_)), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    assert!(ledger.is_unsaved());
    // Deleted and not recreated: refused too.
    std::fs::remove_file(&p).unwrap();
    assert!(matches!(project.save(&mut ledger).unwrap_err(), DiskError::Replaced(_)));
    assert!(!p.exists());
}

/// Windows only: a file at the project's path that another program holds open without sharing
/// is still identified (since brief 021 the identity is read through a handle opened for its
/// attributes only, which sharing does not block), so a replacement is reported as one at once,
/// and nothing is written. (The project's own file cannot be held this way while the project has
/// it open, so a file moved over it stands in for it.)
#[cfg(windows)]
#[test]
fn a_file_another_program_holds_exclusively_is_still_identified() {
    use std::os::windows::fs::OpenOptionsExt;
    let d = TempDir::new("exclusive");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger, StepFaults::default()).unwrap();
    edit(&mut ledger, "2");
    let other = d.path("other");
    std::fs::write(&other, b"replacement").unwrap();
    std::fs::rename(&other, &p).unwrap();
    let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&p).unwrap();
    let e = project.save(&mut ledger).unwrap_err();
    assert!(matches!(e, DiskError::Replaced(_)), "{e}");
    assert!(ledger.is_unsaved());
    drop(held);
    assert_eq!(std::fs::read(&p).unwrap(), b"replacement");
}

#[test]
fn save_as_is_confirmed_only_when_the_folder_flush_ran_and_succeeded() {
    // Ran and succeeded (the folder flush is real on Unix and on Windows).
    let d = TempDir::new("confirmed");
    let mut ledger = fixture().ledger;
    let (_project, durability) = create(&d.path("p.ecad"), &mut ledger, StepFaults::default()).unwrap();
    assert_eq!(durability, Durability::Confirmed);
    assert!(!ledger.is_unsaved());
    // Failed, or not available on this system: unconfirmed, with the reason, and nothing
    // acknowledged.
    let failed = StepFaults { fail_dir_sync: true, ..StepFaults::default() };
    let unavailable = StepFaults { dir_sync_unavailable: true, ..StepFaults::default() };
    for (name, faults, reason) in
        [("failed", failed, "injected folder flush failure"), ("unavailable", unavailable, "not available")]
    {
        let mut ledger = fixture().ledger;
        let (project, durability) = create(&d.path(&format!("{name}.ecad")), &mut ledger, faults).unwrap();
        let Durability::Unconfirmed(why) = durability else { panic!("{name}: confirmed without a flush") };
        assert!(why.contains(reason), "{name}: {why}");
        assert!(project.unconfirmed() && ledger.is_unsaved(), "{name}");
    }
}

#[test]
fn a_damaged_file_opens_read_only_and_is_rescued_by_save_as() {
    let d = TempDir::new("damaged");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    drop(create(&p, &mut ledger, StepFaults::default()).unwrap());
    let mut bytes = std::fs::read(&p).unwrap();
    let mid = bytes.len() / 2;
    let nl = mid + bytes[mid..].iter().position(|b| *b == b'\n').unwrap();
    bytes[nl - 3] ^= 1; // inside a complete record's payload
    std::fs::write(&p, &bytes).unwrap();
    let o = DiskProject::open(&p, SEED, WRITER).unwrap();
    assert!(matches!(o.report.outcome, Outcome::Corrupt(_)));
    assert_eq!(o.project.read_only(), Some(&ReadOnly::Damaged));
    let (mut project, mut prefix) = (o.project, o.ledger);
    assert!(project.save(&mut prefix).unwrap_err().to_string().contains("damaged"));
    assert_eq!(std::fs::read(&p).unwrap(), bytes, "the source is preserved");
    let q = d.path("rescued.ecad");
    project.save_as(&q, &mut prefix, None, WRITER).unwrap();
    drop(project);
    assert_reopens_as(&q, &prefix);
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
}

#[test]
fn opening_something_that_is_not_a_project_file_is_refused() {
    let d = TempDir::new("notproject");
    assert!(DiskProject::open(&d.path("missing.ecad"), SEED, WRITER).is_err());
    let p = d.path("notes.txt");
    std::fs::write(&p, b"hello\n").unwrap();
    let e = DiskProject::open(&p, SEED, WRITER).unwrap_err();
    assert!(matches!(e, DiskError::Open { .. }), "{e}");
}

#[test]
fn lock_records_carry_a_readable_utc_time() {
    use ecad_file::disk::utc;
    assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
    assert_eq!(utc(951_782_400), "2000-02-29 00:00:00 UTC");
    assert_eq!(utc(1_791_897_066), "2026-10-13 13:11:06 UTC");
    let r = ecad_file::disk::holder_record("ecad test");
    assert!(r.starts_with("ecad test, process ") && r.ends_with(" UTC"), "{r}");
}
