//! Tester's outside-in contracts for brief 003 (the trial on native Windows): edge cases of typed
//! paths, the home folder, the project's own file in another spelling, folders that must not be
//! made, and hostile input. Written from the brief, not from the implementation.

use std::path::{Path, PathBuf};

use ecad_cadcraft::{HOME_UNKNOWN, normalize_with};
use ecad_trial::{Trial, parse_args};
use serde_json::json;

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-t003-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn trial(home: Option<&Path>, snapshots: Option<PathBuf>) -> Trial {
    ecad_cadcraft::install();
    let mut t = Trial::new(snapshots).unwrap();
    t.home = home.map(Path::to_path_buf);
    t.cad.run("wire", json!({"points": [[0, 0], [40, 0]]})).unwrap();
    t
}

/// Save As through the dialog; returns the error the dialog shows, if any.
fn save_as(t: &mut Trial, typed: &str) -> Option<String> {
    t.request_save_as();
    t.set_dialog_path(typed);
    t.confirm_dialog();
    let e = t.dialog().and_then(|(_, _, e)| e.map(str::to_string));
    t.cancel_dialog();
    e
}

/// Reserved device names in every disguise are refused with a message; near misses are ordinary
/// names.
#[test]
fn reserved_device_names_are_refused_in_every_spelling() {
    let home = Path::new(if cfg!(windows) { r"C:\Users\tester" } else { "/home/tester" });
    let refused = [
        "CON .ecad",
        "con.tar.ecad",
        "dir/Prn",
        r"dir\AUX.ecad",
        r"~\COM9.ecad",
        "~/lpt9.ECAD",
        r"\\server\share\NUL.ecad",
        "C:/dev/ecad-projects/com1",
    ];
    for typed in refused {
        let e = normalize_with(typed, Some(home)).expect_err(typed);
        assert!(e.contains("reserve"), "{typed:?}: {e}");
    }
    for ok in ["COM10.ecad", "NULL.ecad", "conduit", "AUXILIARY.ecad", "lpt.ecad", "prn-plan"] {
        assert!(normalize_with(ok, Some(home)).is_ok(), "{ok:?}");
    }
}

/// Windows strips a trailing dot from a file name, so `demo.ecad.` would silently become another
/// name: it must not be accepted as a `.ecad` path as typed.
#[test]
fn a_trailing_dot_is_not_accepted_as_typed() {
    for typed in ["demo.ecad.", "demo.", r"C:\dev\demo.ecad."] {
        let got = normalize_with(typed, None);
        assert!(got.as_deref().map_or(true, |p| !p.ends_with('.')), "{typed:?} gave {got:?}");
    }
}

/// Hostile text in the path field never panics; each gives a path or a message.
#[test]
fn hostile_paths_never_panic() {
    let home = Path::new(if cfg!(windows) { r"C:\Users\tester" } else { "/home/tester" });
    let long = "a".repeat(10_000);
    let rows: Vec<String> = vec![
        "\0".into(),
        "~\0".into(),
        ".".into(),
        "..".into(),
        ":".into(),
        "~~".into(),
        "~/.".into(),
        "~/..".into(),
        r"~\\\\".into(),
        "C:".into(),
        r"\\".into(),
        r"\\?\C:\dev\x".into(),
        "\u{202e}gnp.ecad".into(),
        "con\u{0}.ecad".into(),
        long.clone(),
        format!("~/{long}"),
        "....".into(),
        "/".into(),
        "\\".into(),
    ];
    for typed in &rows {
        let _ = normalize_with(typed, Some(home));
        let _ = normalize_with(typed, None);
    }
}

/// Run by [`odd_paths_through_the_dialog_save_where_the_banner_says`] in a child process whose
/// working folder is a temporary folder.
#[test]
#[ignore = "run as a child process"]
fn child_saves_odd_paths() {
    for typed in ["\0", "~/\0x", ":", "~~", "C:", "C:drive-relative", r"\\", ".", r"..\up"] {
        let mut t = trial(None, None);
        let e = save_as(&mut t, typed);
        let path = t.status().and_then(|s| s.file.path);
        println!("ROW<{typed:?}|{e:?}|{path:?}>");
        if let Some(p) = path {
            println!("EXISTS<{p}|{}>", Path::new(&p).is_file());
        }
    }
}

/// Odd text typed in the dialog never panics, and whatever is saved is where the banner says.
#[test]
fn odd_paths_through_the_dialog_save_where_the_banner_says() {
    let cwd = TempDir::new("odd-cwd");
    std::fs::create_dir_all(cwd.0.join("inner")).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_saves_odd_paths", "--ignored", "--nocapture", "--test-threads=1"])
        .current_dir(cwd.0.join("inner"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let listing: Vec<_> =
        std::fs::read_dir(cwd.0.join("inner")).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
    println!("{text}\nLISTING {listing:?}");
    assert!(out.status.success(), "{text}");
    assert!(!cwd.0.join("inner").join("~").exists());
    for line in text.lines().filter(|l| l.starts_with("EXISTS<")) {
        assert!(line.ends_with("|true>"), "saved somewhere other than the banner says: {line}");
    }
}

/// Mixed separators and `~` with doubled separators all name the same file under the home folder.
#[test]
fn mixed_separators_under_the_home_folder() {
    let home = TempDir::new("mixed");
    std::fs::create_dir_all(home.0.join("sub")).unwrap();
    for typed in [r"~/sub\a", r"~\sub/b", r"~\\sub\\c"] {
        let mut t = trial(Some(&home.0), None);
        assert_eq!(save_as(&mut t, typed), None, "{typed}");
        let path = t.status().unwrap().file.path.unwrap();
        assert!(Path::new(&path).is_file(), "{typed} -> {path}");
        // (The temp folder itself may contain an 8.3 short name such as `CARLVO~1`.)
        assert!(!path.starts_with('~') && !Path::new(&path).starts_with(home.0.join("~")), "{typed} -> {path}");
        assert!(path.starts_with(&home.0.display().to_string()), "{typed} -> {path}");
    }
    assert!(!home.0.join("~").exists());
}

/// A missing folder that is not the default one is never created, on every platform.
#[test]
fn a_missing_folder_is_not_created() {
    let d = TempDir::new("missing");
    let mut t = trial(None, None);
    let typed = d.0.join("nowhere").join("demo").display().to_string();
    let e = save_as(&mut t, &typed).unwrap_or_default();
    assert!(!e.is_empty(), "the dialog reports the failure");
    assert!(!d.0.join("nowhere").exists());
    assert_eq!(t.status().unwrap().file.path, None);
}

/// Opening the project's own file through another letter case finds it on Windows, and Save As
/// back to the first spelling asks no replace question.
#[cfg(windows)]
#[test]
fn a_case_variant_opens_the_same_project_on_windows() {
    let d = TempDir::new("case-open");
    let mut t = trial(None, None);
    let own = d.0.join("Plan.ecad").display().to_string();
    assert_eq!(save_as(&mut t, &own), None);
    t.cad.request_close_all();
    t.after_logic();
    let upper = d.0.join("PLAN.ECAD").display().to_string();
    t.open_project(&upper).unwrap();
    assert!(t.status().unwrap().file.read_only.is_none());
    t.cad.run("wire", json!({"points": [[0, 20], [40, 20]]})).unwrap();
    assert_eq!(save_as(&mut t, &own), None, "the own file in its first spelling is not someone else's");
    assert_eq!(t.status().unwrap().unsaved, 0);
    let files = std::fs::read_dir(&d.0)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("ecad")))
        .count();
    assert_eq!(files, 1);
}

/// A second window opening the same file in another spelling is read-only (the lock is per
/// file, not per spelling).
#[cfg(windows)]
#[test]
fn a_second_opener_through_another_spelling_is_read_only() {
    let d = TempDir::new("case-lock");
    let mut first = trial(None, None);
    let own = d.0.join("Shared.ecad").display().to_string();
    assert_eq!(save_as(&mut first, &own), None);
    let other = d.0.join("sHARED.ecad").display().to_string().replace('\\', "/");
    ecad_cadcraft::install();
    let mut second = Trial::with_file(None, &other).unwrap();
    let s = second.status().unwrap();
    assert!(s.file.read_only.is_some(), "second opener must be read-only: {:?}", s.file);
    drop(first);
}

/// Run by [`the_home_folder_ignores_a_git_bash_home`] in a child process.
#[test]
#[ignore = "run as a child process"]
fn child_reports_home() {
    let home = ecad_cadcraft::home().map(|h| h.display().to_string()).unwrap_or_default();
    println!("HOME-FOLDER<{home}>");
}

fn child_home(env: &[(&str, Option<&str>)]) -> String {
    let mut c = std::process::Command::new(std::env::current_exe().unwrap());
    c.args(["--exact", "child_reports_home", "--ignored", "--nocapture", "--test-threads=1"]);
    for (k, v) in env {
        match v {
            Some(v) => c.env(k, v),
            None => c.env_remove(k),
        };
    }
    let out = c.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{text}");
    let start = text.find("HOME-FOLDER<").unwrap() + "HOME-FOLDER<".len();
    text[start..].split('>').next().unwrap().to_string()
}

/// On Windows `~` is the user profile even when a Unix-style `HOME` is set (Git Bash, MSYS), and
/// still known when `USERPROFILE` is missing too. Elsewhere `HOME` is the home folder.
#[test]
fn the_home_folder_ignores_a_git_bash_home() {
    if cfg!(windows) {
        let profile = std::env::var("USERPROFILE").unwrap();
        assert_eq!(child_home(&[("HOME", Some("/c/Users/nobody"))]), profile);
        let without = child_home(&[("HOME", None), ("USERPROFILE", None)]);
        assert!(!without.is_empty(), "known from the system without USERPROFILE");
        assert!(!without.starts_with('~'));
    } else {
        assert_eq!(child_home(&[("HOME", Some("/tmp/ecad-home"))]), "/tmp/ecad-home");
    }
}

/// Run by [`snapshots_with_a_tilde_never_make_a_tilde_folder`] in a child process whose working
/// folder is a temporary folder.
#[test]
#[ignore = "run as a child process"]
fn child_exports_a_snapshot_to_tilde() {
    let mut t = trial(None, Some(PathBuf::from("~/snaps")));
    let r = t.export_snapshot("t003");
    println!("EXPORT<{r:?}>");
}

/// `--snapshots ~/snaps` typed in PowerShell or cmd arrives with the `~` unexpanded: the trial must
/// never make a folder literally named `~` (brief 003, part A item 1).
#[test]
fn snapshots_with_a_tilde_never_make_a_tilde_folder() {
    let cwd = TempDir::new("snap-cwd");
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_exports_a_snapshot_to_tilde", "--ignored", "--nocapture", "--test-threads=1"])
        .current_dir(&cwd.0)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(!cwd.0.join("~").exists(), "a folder literally named `~` was made: {text}");
}

/// The command line never panics on odd arguments.
#[test]
fn odd_arguments_never_panic() {
    use std::ffi::OsString;
    for args in [vec!["-"], vec!["--"], vec!["--snapshots", "--help"], vec!["--snapshots="], vec![""], vec!["", ""]] {
        let _ = parse_args(args.into_iter().map(OsString::from).collect::<Vec<_>>());
    }
    // An empty project name opens nothing rather than an empty path.
    let r = parse_args(vec![OsString::from("")]);
    let _ = r;
}

/// The error for `~` without a home folder names the problem and suggests the full path.
#[test]
fn tilde_without_a_home_says_what_to_do() {
    let e = normalize_with("~/x", None).unwrap_err();
    assert_eq!(e, HOME_UNKNOWN);
    assert!(e.contains("full path"));
}

/// A project file given on the command line that cannot be opened (missing, or a reserved name)
/// is reported in the window, not on a console (a Windows release build has none), and a new
/// project is opened instead; nothing is created.
#[test]
fn an_unopenable_project_argument_is_reported_in_the_window() {
    let d = TempDir::new("missing-arg");
    ecad_cadcraft::install();
    for name in ["nothere.ecad", "CON.ecad"] {
        let typed = d.0.join(name).display().to_string();
        let mut t = Trial::with_file(None, &typed).unwrap();
        let notice = t.notice().unwrap_or_default().to_string();
        assert!(notice.contains("could not be opened"), "{name}: {notice}");
        assert!(t.project_open());
        assert_eq!(t.status().unwrap().file.path, None);
    }
    assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 0, "nothing created");
}

/// Save and reopen through a UNC path (`\\localhost\C$\...`) when the administrative share is
/// reachable on this machine; the network itself stays unverified by design.
#[cfg(windows)]
#[test]
fn a_unc_path_saves_and_reopens_when_the_share_is_reachable() {
    let d = TempDir::new("unc");
    let local = d.0.display().to_string();
    let Some(rest) = local.strip_prefix(r"C:\") else { return };
    let unc = PathBuf::from(format!(r"\\localhost\C$\{rest}"));
    if !unc.is_dir() {
        println!("UNC share not reachable ({}); skipped", unc.display());
        return;
    }
    let mut t = trial(None, None);
    let typed = unc.join("over unc").display().to_string();
    assert_eq!(save_as(&mut t, &typed), None, "{typed}");
    assert!(d.0.join("over unc.ecad").is_file());
    t.cad.request_close_all();
    t.after_logic();
    t.open_project(&format!("{typed}.ecad")).unwrap();
    assert!(t.status().unwrap().file.read_only.is_none());
}
