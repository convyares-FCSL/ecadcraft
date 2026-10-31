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

#[test]
#[cfg_attr(windows, ignore = "R-M4: the lock is mandatory on Windows, so the holder record cannot be read")]
fn a_refused_writer_is_told_who_holds_the_lock() {
    let d = TempDir::new("holder");
    let p = d.path("p.ecad");
    let (_lock, _) = acquired(&p, "pid 1 since t0");
    match WriterLock::try_acquire(&p, "pid 2").unwrap() {
        LockAttempt::HeldByOther { holder } => assert_eq!(holder.as_deref(), Some("pid 1 since t0")),
        LockAttempt::Acquired { .. } => panic!("two writers"),
    }
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
#[cfg_attr(windows, ignore = "R-S1: no file identity on Windows (FileIdentity::of returns None)")]
fn identity_changes_when_another_file_replaces_the_path() {
    let d = TempDir::new("identity");
    let target = d.path("p.ecad");
    std::fs::write(&target, b"old").unwrap();
    let before = FileIdentity::at(&target).unwrap();
    let temp = disk::temp_path(&target);
    disk::write_new_file(&temp, b"new").unwrap();
    disk::replace(&temp, &target).unwrap();
    assert_ne!(before, FileIdentity::at(&target).unwrap());
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
    assert!(
        matches!(WriterLock::try_acquire(&p, "parent").unwrap(), LockAttempt::HeldByOther { .. }),
        "the child's lock was not respected"
    );
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
#[cfg_attr(windows, ignore = "R-M4: the lock is mandatory on Windows, so the holder record cannot be read")]
fn another_process_is_told_who_holds_the_lock() {
    let d = TempDir::new("holder-process");
    let p = d.path("p.ecad");
    let (mut child, child_record) = child_holding_lock(&p);
    let attempt = WriterLock::try_acquire(&p, "parent").unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    match attempt {
        LockAttempt::HeldByOther { holder } => assert_eq!(holder.as_deref(), Some(child_record.as_str())),
        LockAttempt::Acquired { .. } => panic!("the child's lock was not respected"),
    }
}
