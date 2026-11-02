//! Tester edge cases for brief 002 (Windows persistence): replacement by other programs, a
//! file renamed away and back, Save As onto a read-only target, a folder this user may not add
//! files to, and Windows path spellings.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use common::*;
use ecad_file::{DiskError, DiskProject, Durability, ReadOnly, StepFaults};
use ecad_model::*;

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-edge {name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
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
    let id = PageId(0x3000 + ledger.head() as u128);
    let index = ledger.document().page_order.len();
    ledger.commit(Author::User, title, vec![Op::AddPage { id, page: Page { title: title.into() }, index }]).unwrap();
}

fn create(path: &Path, ledger: &mut Ledger) -> (DiskProject, Durability) {
    DiskProject::create(path, PROJECT, ledger, None, WRITER, StepFaults::default()).unwrap()
}

/// Run a shell command line (cmd on Windows, sh elsewhere) and require success.
fn shell(line: &str) {
    // cmd parses its own command line, so it is passed unescaped.
    #[cfg(windows)]
    let out = {
        use std::os::windows::process::CommandExt;
        Command::new("cmd").raw_arg(format!("/C \"{line}\"")).output().unwrap()
    };
    #[cfg(not(windows))]
    let out = Command::new("sh").args(["-c", line]).output().unwrap();
    assert!(out.status.success(), "{line}: {}", String::from_utf8_lossy(&out.stderr));
}

fn q(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

#[test]
fn another_program_moving_a_file_over_the_project_is_refused() {
    let d = TempDir::new("move");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger);
    edit(&mut ledger, "2");
    let other = d.path("other.ecad");
    std::fs::copy(&p, &other).unwrap();
    // `move /Y` (Windows) or `mv -f` (Unix), run by another process.
    if cfg!(windows) {
        // cmd's `move` replaces without POSIX semantics, so Windows refuses it while the project
        // holds the file open: the project is protected rather than replaced, and still saves.
        let line = format!("/C \"move /Y {} {}\"", q(&other), q(&p));
        #[cfg(windows)]
        let out = {
            use std::os::windows::process::CommandExt;
            Command::new("cmd").raw_arg(line).output().unwrap()
        };
        #[cfg(not(windows))]
        let out = Command::new("true").arg(line).output().unwrap();
        if !out.status.success() {
            assert!(other.exists(), "the move did nothing");
            project.save(&mut ledger).unwrap();
            assert!(!ledger.is_unsaved());
            return;
        }
    } else {
        shell(&format!("mv -f {} {}", q(&other), q(&p)));
    }
    let bytes = std::fs::read(&p).unwrap();
    let e = project.save(&mut ledger).unwrap_err();
    assert!(matches!(e, DiskError::Replaced(_)), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), bytes, "left alone");
    assert!(ledger.is_unsaved());
}

#[test]
fn another_program_deleting_and_copying_back_the_project_is_refused() {
    let d = TempDir::new("delcopy");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger);
    edit(&mut ledger, "2");
    let backup = d.path("backup.ecad");
    std::fs::copy(&p, &backup).unwrap();
    if cfg!(windows) {
        shell(&format!("del /F /Q {} && copy /Y {} {}", q(&p), q(&backup), q(&p)));
    } else {
        shell(&format!("rm -f {} && cp {} {}", q(&p), q(&backup), q(&p)));
    }
    let e = project.save(&mut ledger).unwrap_err();
    assert!(matches!(e, DiskError::Replaced(_)), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), std::fs::read(&backup).unwrap());
}

#[test]
fn a_project_renamed_away_and_back_is_still_the_same_file_and_saves() {
    let d = TempDir::new("away");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger);
    let away = d.path("away.ecad");
    std::fs::rename(&p, &away).unwrap();
    edit(&mut ledger, "2");
    assert!(matches!(project.save(&mut ledger).unwrap_err(), DiskError::Replaced(_)), "nothing at the path");
    std::fs::rename(&away, &p).unwrap();
    project.save(&mut ledger).unwrap();
    assert!(!ledger.is_unsaved());
    drop(project);
    let o = DiskProject::open(&p, SEED, "checker").unwrap();
    assert_eq!(o.ledger.log(), ledger.log());
}

#[test]
fn save_as_onto_a_read_only_target_fails_before_replacement_and_cleans_up() {
    let d = TempDir::new("rotarget");
    let target = d.path("target.ecad");
    let mut first = fixture().ledger;
    drop(create(&target, &mut first));
    let before = std::fs::read(&target).unwrap();
    let mut perms = std::fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&target, perms).unwrap();

    let mut ledger = fixture().ledger;
    edit(&mut ledger, "2");
    let (mut project, _) = create(&d.path("src.ecad"), &mut ledger);
    let start = Instant::now();
    let r = project.save_as(&target, &mut ledger, None, WRITER);
    let took = start.elapsed();
    if cfg!(windows) {
        // Windows refuses to replace a read-only file: the retry gives up after a bounded wait.
        let e = r.unwrap_err();
        assert!(matches!(e, DiskError::BeforeReplacement { .. }), "{e}");
        assert_eq!(std::fs::read(&target).unwrap(), before, "target unchanged");
        assert!(took < Duration::from_secs(3), "bounded: {took:?}");
        assert!(!d.names().iter().any(|n| n.contains(".tmp-")), "temporary file removed: {:?}", d.names());
        assert_eq!(project.path(), d.path("src.ecad"), "still bound to its own file");
    } else {
        // Unix replaces the directory entry; the folder is writable, so it succeeds.
        assert_eq!(r.unwrap(), Durability::Confirmed);
    }
    let mut perms = std::fs::metadata(&target).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&target, perms).unwrap();
}

/// A folder this user may not add files to: on Windows an ACL denying "add file", on Unix mode
/// 555. Restored when dropped.
struct NoAddFolder<'a>(&'a Path);

impl<'a> NoAddFolder<'a> {
    /// `None` if the restriction does not take effect (for example running as root).
    fn deny(dir: &'a Path) -> Option<Self> {
        if cfg!(windows) {
            let who = format!("{}\\{}", std::env::var("USERDOMAIN").unwrap(), std::env::var("USERNAME").unwrap());
            let out = Command::new("icacls").arg(dir).args(["/deny", &format!("{who}:(WD,AD)")]).output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
        } else {
            shell(&format!("chmod 555 {}", q(dir)));
        }
        let me = NoAddFolder(dir);
        let probe = dir.join("probe");
        if std::fs::write(&probe, b"x").is_ok() {
            let _ = std::fs::remove_file(&probe);
            return None;
        }
        Some(me)
    }
}

impl Drop for NoAddFolder<'_> {
    fn drop(&mut self) {
        if cfg!(windows) {
            let who = format!(
                "{}\\{}",
                std::env::var("USERDOMAIN").unwrap_or_default(),
                std::env::var("USERNAME").unwrap_or_default()
            );
            let _ = Command::new("icacls").arg(self.0).args(["/remove:d", &who]).output();
        } else {
            let _ = Command::new("chmod").arg("755").arg(self.0).output();
        }
    }
}

#[test]
fn a_folder_that_cannot_take_a_lock_file_opens_no_lock() {
    let d = TempDir::new("noadd");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    drop(create(&p, &mut ledger));
    std::fs::remove_file(ecad_store::disk::lock_path(&p)).unwrap();
    let Some(guard) = NoAddFolder::deny(&d.0) else {
        eprintln!("skipped: the folder restriction did not take effect (root?)");
        return;
    };
    let o = DiskProject::open(&p, SEED, WRITER).unwrap();
    let ro = o.project.read_only().cloned();
    assert!(matches!(ro, Some(ReadOnly::NoLock(_))), "{ro:?}");
    let text = ro.unwrap().to_string();
    assert!(!text.contains("another session"), "{text}");
    assert_eq!(o.ledger.log(), ledger.log());
    // Save As into the same folder fails before replacement (no temp file can be made there).
    let (mut project, mut copy) = (o.project, o.ledger);
    edit(&mut copy, "2");
    assert!(project.save_as(&d.path("q.ecad"), &mut copy, None, WRITER).is_err());
    drop(guard);
    // Save As elsewhere rescues it.
    let e = TempDir::new("noadd-rescue");
    assert_eq!(project.save_as(&e.path("r.ecad"), &mut copy, None, WRITER).unwrap(), Durability::Confirmed);
}

#[test]
fn windows_path_spellings_reach_the_same_project() {
    let d = TempDir::new("spell");
    let p = d.path("My Project.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger);
    edit(&mut ledger, "2");
    // Save As to the same file spelled differently is a plain Save, not a second lock.
    let other = if cfg!(windows) {
        PathBuf::from(p.display().to_string().to_uppercase().replace('\\', "/"))
    } else {
        d.0.join(".").join("My Project.ecad")
    };
    assert_eq!(project.save_as(&other, &mut ledger, None, WRITER).unwrap(), Durability::Confirmed);
    assert_eq!(project.path(), p, "still bound to the same path");
    drop(project);
    // A verbatim (`\\?\`) path opens the same file, writable.
    let canon = std::fs::canonicalize(&p).unwrap();
    let o = DiskProject::open(&canon, SEED, WRITER).unwrap();
    assert!(o.project.read_only().is_none(), "{:?}", o.project.read_only());
    assert_eq!(o.ledger.log(), ledger.log());
}

/// Probe of an unverified file system: set `ECAD_TEST_OTHER_FS` to a folder on it (for example a
/// network share such as `\localhost\C$\temp`). Records what Save As and Save report there; it
/// only requires that nothing is claimed durable without a flush and that nothing panics.
#[test]
fn probe_an_unverified_file_system() {
    let Ok(dir) = std::env::var("ECAD_TEST_OTHER_FS") else { return };
    let dir = PathBuf::from(dir).join(format!("ecad-probe-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("p.ecad");
    let mut ledger = fixture().ledger;
    let flush = ecad_store::disk::sync_dir(&dir);
    eprintln!("folder flush: {flush:?}");
    match DiskProject::create(&p, PROJECT, &mut ledger, None, WRITER, StepFaults::default()) {
        Ok((mut project, durability)) => {
            eprintln!("save as: {durability:?}");
            assert_eq!(durability == Durability::Confirmed, flush.is_ok());
            edit(&mut ledger, "2");
            eprintln!("save: {:?}", project.save(&mut ledger).map(|_| ()));
            let second = DiskProject::open(&p, SEED, "second");
            eprintln!("second opener: {:?}", second.as_ref().map(|o| o.project.read_only().map(|r| r.to_string())));
        }
        Err(e) => eprintln!("save as failed: {e}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Deny this user reading `file` (Windows: an ACL denying "read data"; Unix: mode 000), or undo
/// it. Returns false if the denial does not take effect (for example running as root).
fn deny_read(file: &Path, deny: bool) -> bool {
    if cfg!(windows) {
        let who = format!("{}\\{}", std::env::var("USERDOMAIN").unwrap(), std::env::var("USERNAME").unwrap());
        let args: Vec<String> =
            if deny { vec!["/deny".into(), format!("{who}:(RD)")] } else { vec!["/remove:d".into(), who] };
        let out = Command::new("icacls").arg(file).args(&args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    } else {
        shell(&format!("chmod {} {}", if deny { "000" } else { "644" }, q(file)));
    }
    !deny || std::fs::File::open(file).is_err()
}

/// Review fix 2: a project file that cannot be looked at (not a replacement) gives "could not
/// check", writes nothing, and Save succeeds once it can be looked at again.
#[test]
fn a_project_file_that_cannot_be_looked_at_is_cannot_check_then_saves() {
    let d = TempDir::new("cannotcheck");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger);
    edit(&mut ledger, "2");
    let before = std::fs::read(&p).unwrap();
    if !deny_read(&p, true) {
        deny_read(&p, false);
        eprintln!("skipped: the read denial did not take effect (root?)");
        return;
    }
    let start = Instant::now();
    let e = project.save(&mut ledger).unwrap_err();
    let took = start.elapsed();
    deny_read(&p, false);
    assert!(matches!(e, DiskError::CannotCheck { .. }), "{e}");
    let text = e.to_string();
    assert!(text.contains("could not check") && text.contains("nothing was written"), "{text}");
    assert!(!text.contains("replaced"), "{text}");
    assert!(took < Duration::from_secs(2), "bounded: {took:?}");
    assert_eq!(std::fs::read(&p).unwrap(), before, "nothing written");
    assert!(ledger.is_unsaved());
    // It can be looked at again: Save succeeds and the work reopens.
    project.save(&mut ledger).unwrap();
    assert!(!ledger.is_unsaved());
    drop(project);
    let o = DiskProject::open(&p, SEED, "checker").unwrap();
    assert_eq!(o.ledger.log(), ledger.log());
}

/// Windows: another process cannot hold the open project's own file without sharing reads (the
/// project's handles already read it), so that form of "could not check" cannot arise for the
/// project's file; once the project is closed, it can.
#[cfg(windows)]
#[test]
fn another_process_cannot_hold_the_open_project_exclusively() {
    let d = TempDir::new("exclusive-own");
    let p = d.path("p.ecad");
    let mut ledger = fixture().ledger;
    let (mut project, _) = create(&p, &mut ledger);
    let try_exclusive = || {
        let script = format!(
            "try {{ $f = [System.IO.File]::Open('{}', 'Open', 'Read', 'None'); $f.Close(); 'opened' }} catch {{ 'refused' }}",
            p.display()
        );
        let out = Command::new("powershell").args(["-NoProfile", "-Command", &script]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(try_exclusive(), "refused", "held exclusively while the project is open");
    edit(&mut ledger, "2");
    project.save(&mut ledger).unwrap();
    drop(project);
    assert_eq!(try_exclusive(), "opened", "possible once the project is closed");
}
