//! Files on disk (proposal §20.5): the file medium, the writer lock and the replacement steps.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use ecad_store::disk::{self, LockAttempt, WriterLock};
use ecad_store::{FileIdentity, FileMedium, Medium};

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-store-{name}-{}", disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn acquired(project: &Path, who: &str) -> (WriterLock, Option<String>) {
    match WriterLock::try_acquire(project, who).unwrap() {
        LockAttempt::Acquired { lock, leftover } => (lock, leftover),
        LockAttempt::HeldByOther { holder } => panic!("lock held by {holder:?}"),
    }
}

#[test]
fn the_file_medium_appends_flushes_truncates_and_reads() {
    let d = TempDir::new("medium");
    let p = d.path("a.ecad");
    let mut m = FileMedium::create_new(&p).unwrap();
    assert!(FileMedium::create_new(&p).is_err(), "never overwrites");
    m.append(b"hello ").unwrap();
    m.append(b"world").unwrap();
    m.flush().unwrap();
    assert_eq!(m.len(), 11);
    assert_eq!(m.read_range(6, 5).unwrap(), b"world");
    assert_eq!(m.read_range(9, 50).unwrap(), b"ld", "short at the end");
    m.truncate(5).unwrap();
    assert_eq!(m.read_all().unwrap(), b"hello");
    // Injected faults: a torn append, a failed flush, a failed truncate.
    m.faults.partial_append = Some(2);
    assert!(m.append(b"XYZ").is_err());
    assert_eq!(m.read_all().unwrap(), b"helloXY");
    m.faults.fail_next_flush = true;
    assert!(m.flush().is_err());
    m.faults.fail_next_truncate = true;
    assert!(m.truncate(0).is_err());
    assert_eq!(m.len(), 7, "a failed truncate changes nothing");
    // Read-only handles refuse writes.
    drop(m);
    let mut r = FileMedium::open(&p, false).unwrap();
    assert!(r.append(b"!").is_err());
    assert!(r.truncate(0).is_err());
    assert_eq!(r.read_all().unwrap(), b"helloXY");
}

#[test]
fn an_active_lock_refuses_other_writers_and_a_crash_never_locks_out() {
    let d = TempDir::new("lock");
    let p = d.path("p.ecad");
    let (lock, leftover) = acquired(&p, "pid 1 since t0");
    assert_eq!(leftover, None, "a new lock file has no leftover record");
    // A second writer (another handle, even in this process) is refused. Who holds it: see
    // `a_refused_writer_is_told_who_holds_the_lock`.
    assert!(matches!(WriterLock::try_acquire(&p, "pid 2").unwrap(), LockAttempt::HeldByOther { .. }), "two writers");
    // A clean release clears the record: the next writer sees no leftover.
    drop(lock);
    let (lock, leftover) = acquired(&p, "pid 3");
    assert_eq!(leftover, None);
    let _still_running = lock; // the handle stays open, like a process that is still running
    // A session that ended without releasing (a crash): the OS releases its lock, the record
    // stays. Simulated by writing a record into an unlocked lock file.
    let q = d.path("q.ecad");
    std::fs::write(disk::lock_path(&q), "pid 4 since t1 (crashed)").unwrap();
    let (_lock, leftover) = acquired(&q, "pid 5");
    assert_eq!(leftover.as_deref(), Some("pid 4 since t1 (crashed)"), "reclaimed and reported");
    assert!(disk::lock_path(&q).exists(), "the lock file is never deleted");
    // `_still_running` is released here, before `d` removes the folder (Windows keeps open files).
}

/// What a refused writer is told about the holder: its record, except on Windows, where the
/// mandatory lock keeps the record unreadable and nothing is shown (owner decision, brief 002).
fn expected_holder(record: &str) -> Option<&str> {
    assert_eq!(disk::HOLDER_SHOWN, !cfg!(windows));
    disk::HOLDER_SHOWN.then_some(record)
}

#[test]
fn a_refused_writer_is_told_who_holds_the_lock_where_the_system_shows_it() {
    let d = TempDir::new("holder");
    let p = d.path("p.ecad");
    let (_lock, _) = acquired(&p, "pid 1 since t0");
    match WriterLock::try_acquire(&p, "pid 2").unwrap() {
        LockAttempt::HeldByOther { holder } => assert_eq!(holder.as_deref(), expected_holder("pid 1 since t0")),
        LockAttempt::Acquired { .. } => panic!("two writers"),
    }
}

#[test]
fn a_lock_file_that_cannot_be_opened_is_an_error_not_another_writer() {
    let d = TempDir::new("nolock");
    let p = d.path("p.ecad");
    // Something that is not a file where the lock file belongs: no lock can be taken.
    std::fs::create_dir(disk::lock_path(&p)).unwrap();
    assert!(WriterLock::try_acquire(&p, "pid 1").is_err());
}

#[test]
fn replacement_steps_are_atomic_and_identity_tracks_the_file() {
    let d = TempDir::new("replace");
    let target = d.path("p.ecad");
    std::fs::write(&target, b"old").unwrap();
    let temp = disk::temp_path(&target);
    assert_eq!(temp.parent(), target.parent(), "same folder, so the rename is atomic");
    assert_ne!(disk::temp_path(&target), temp, "unique names");
    disk::write_new_file(&temp, b"new").unwrap();
    assert!(disk::write_new_file(&temp, b"x").is_err(), "never overwrites");
    let handle = FileMedium::open(&temp, false).unwrap();
    disk::replace(&temp, &target).unwrap();
    disk::sync_dir(&disk::parent_dir(&target)).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert!(!temp.exists());
    // The open handle follows the renamed file. That the path now names a different file than
    // before: see `identity_changes_when_another_file_replaces_the_path`.
    assert_eq!(handle.identity().unwrap(), FileIdentity::at(&target).unwrap());
    assert_eq!(disk::parent_dir(Path::new("x.ecad")), PathBuf::from("."));
    assert_eq!(disk::sibling(&target, ".lock"), d.path("p.ecad.lock"));
}

#[test]
fn identity_changes_when_another_file_replaces_the_path() {
    let d = TempDir::new("identity");
    let target = d.path("p.ecad");
    std::fs::write(&target, b"old").unwrap();
    // Renamed over.
    let before = FileIdentity::at(&target).unwrap();
    assert_eq!(before, FileIdentity::at(&target).unwrap(), "the same file, looked at twice");
    let temp = disk::temp_path(&target);
    disk::write_new_file(&temp, b"new").unwrap();
    disk::replace(&temp, &target).unwrap();
    assert_ne!(before, FileIdentity::at(&target).unwrap());
    // Deleted and recreated, while a handle to the old file is still open (as the writer's is).
    let open = FileMedium::open(&target, true).unwrap();
    let before = open.identity().unwrap();
    disk::remove_file(&target).unwrap();
    std::fs::write(&target, b"recreated").unwrap();
    assert_ne!(before, FileIdentity::at(&target).unwrap());
    assert!(FileIdentity::at(&d.path("missing")).is_err(), "nothing there");
}

#[test]
fn a_folder_flush_really_runs() {
    let d = TempDir::new("syncdir");
    disk::sync_dir(&d.0).unwrap();
    // It is not a no-op: a folder that is not there cannot be flushed.
    assert!(disk::sync_dir(&d.path("missing")).is_err());
}

#[test]
fn transient_failures_are_retried_on_windows_only_and_a_bounded_number_of_times() {
    for (code, transient) in [(32, true), (33, true), (5, true), (2, false)] {
        let mut tries = 0;
        let start = std::time::Instant::now();
        let r: std::io::Result<()> = disk::retry_transient(|| {
            tries += 1;
            Err(std::io::Error::from_raw_os_error(code))
        });
        assert!(r.is_err());
        let expected = if cfg!(windows) && transient { disk::TRANSIENT_ATTEMPTS } else { 1 };
        assert_eq!(tries, expected, "os error {code}");
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "bounded");
    }
    // A failure that clears up is retried until it succeeds (on Windows).
    let mut tries = 0;
    let r = disk::retry_transient(|| {
        tries += 1;
        if tries < 3 { Err(std::io::Error::from_raw_os_error(32)) } else { Ok(tries) }
    });
    if cfg!(windows) {
        assert_eq!(r.unwrap(), 3);
    } else {
        assert!(r.is_err());
    }
}

/// Windows only: another program holding the target open without letting it be deleted (as a
/// virus scanner or indexer may, briefly) delays the replacement instead of failing it. Elsewhere
/// an open file never stops a rename.
#[cfg(windows)]
#[test]
fn a_replacement_waits_for_a_program_that_briefly_holds_the_target() {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    let d = TempDir::new("sharing");
    let target = d.path("p.ecad");
    std::fs::write(&target, b"old").unwrap();
    let hold = || std::fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&target).unwrap();
    // Released after 150 ms: the replacement succeeds.
    let held = hold();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(held);
    });
    let temp = disk::temp_path(&target);
    disk::write_new_file(&temp, b"new").unwrap();
    disk::replace(&temp, &target).unwrap();
    release.join().unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    // Held throughout: it fails after a bounded wait, and nothing changes.
    let held = hold();
    let temp = disk::temp_path(&target);
    disk::write_new_file(&temp, b"newer").unwrap();
    let start = std::time::Instant::now();
    assert!(disk::replace(&temp, &target).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(2), "bounded");
    assert!(disk::remove_file(&target).is_err(), "removal is refused too");
    drop(held);
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    disk::remove_file(&temp).unwrap();
}

/// Run by the killed-writer tests in a child process: take the lock named by
/// `ECAD_LOCK_CHILD`, say so, and wait to be killed.
#[test]
fn child_holds_lock_until_killed() {
    let Ok(project) = std::env::var("ECAD_LOCK_CHILD") else { return };
    let (_lock, _) = acquired(Path::new(&project), &format!("pid {} (child)", std::process::id()));
    println!("LOCKED");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// Start a child process holding the lock on `project`; returns it once it has the lock, with the
/// record it wrote.
fn child_holding_lock(project: &Path) -> (std::process::Child, String) {
    use std::io::BufRead;
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_holds_lock_until_killed", "--nocapture", "--test-threads=1"])
        .env("ECAD_LOCK_CHILD", project)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    while !line.contains("LOCKED") {
        line.clear();
        assert!(out.read_line(&mut line).unwrap() > 0, "the child ended before locking");
    }
    let record = format!("pid {} (child)", child.id());
    (child, record)
}

#[test]
fn a_killed_writer_releases_its_lock() {
    let d = TempDir::new("crash");
    let p = d.path("p.ecad");
    let (mut child, child_record) = child_holding_lock(&p);
    assert!(matches!(WriterLock::try_acquire(&p, "parent").unwrap(), LockAttempt::HeldByOther { .. }), "the child's lock was not respected");
    // A crash: the process is killed without releasing anything.
    child.kill().unwrap();
    child.wait().unwrap();
    // Windows releases a killed process's locks a little later (review R-M3), so poll briefly.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let leftover = loop {
        match WriterLock::try_acquire(&p, "parent").unwrap() {
            LockAttempt::Acquired { leftover, .. } => break leftover,
            LockAttempt::HeldByOther { .. } if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            LockAttempt::HeldByOther { holder } => panic!("still locked 2 s after the kill: {holder:?}"),
        }
    };
    assert_eq!(leftover.as_deref(), Some(child_record.as_str()), "no lockout; the leftover is reported");
}

#[test]
fn another_process_is_told_who_holds_the_lock_where_the_system_shows_it() {
    let d = TempDir::new("holder-process");
    let p = d.path("p.ecad");
    let (mut child, child_record) = child_holding_lock(&p);
    let attempt = WriterLock::try_acquire(&p, "parent").unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    match attempt {
        LockAttempt::HeldByOther { holder } => assert_eq!(holder.as_deref(), expected_holder(&child_record)),
        LockAttempt::Acquired { .. } => panic!("the child's lock was not respected"),
    }
}
