//! Contracts for the paths a person types (brief 003, review finding R-S3): `~` is the home folder
//! from `std::env::home_dir` (not `HOME`), both `~/` and `~\` are accepted, paths are built with
//! `Path::join`, a reserved device name is refused, a case-variant spelling of the project's own
//! file is that file, and no folder literally named `~` is ever made.

use std::path::{Path, PathBuf};

use ecad_cadcraft::{HOME_UNKNOWN, normalize_with};
use ecad_trial::{DialogKind, Trial, status_line};
use serde_json::{Value, json};

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-paths-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What normalising a typed path gives: the full path, or an error containing the text.
enum Expect {
    Is(String),
    Refused(&'static str),
}

fn ecad(p: &Path) -> String {
    format!("{}.ecad", p.display())
}

#[test]
fn typed_paths_are_expanded_completed_or_refused() {
    let profile = Path::new(if cfg!(windows) { r"C:\Users\Zoë Ünal" } else { "/home/zoë ünal" });
    use Expect::{Is, Refused};
    // (typed, home folder known?, expected)
    let rows: Vec<(&str, bool, Expect)> = vec![
        // `~` from the profile, whether or not `HOME` is set: both separators.
        ("~/demo", true, Is(ecad(&profile.join("demo")))),
        (r"~\demo", true, Is(ecad(&profile.join("demo")))),
        ("~//demo", true, Is(ecad(&profile.join("demo")))),
        ("  ~/ecad-projects/demo.ecad  ", true, Is(profile.join("ecad-projects").join("demo.ecad").display().to_string())),
        ("~/Schaltpläne/Anlage ü", true, Is(ecad(&profile.join("Schaltpläne").join("Anlage ü")))),
        // The rest is joined component by component: one separator throughout, the platform's.
        ("~/ecad-projects/demo", true, Is(ecad(&profile.join("ecad-projects").join("demo")))),
        (r"~\ecad-projects/sub\demo", true, Is(ecad(&profile.join("ecad-projects").join("sub").join("demo")))),
        // No home folder known: refused, never a folder named `~`.
        ("~/demo", false, Refused(HOME_UNKNOWN)),
        (r"~\demo", false, Refused(HOME_UNKNOWN)),
        // `~` alone, or a trailing separator, names a folder.
        ("~", true, Refused("not a folder")),
        ("~/", true, Refused("not a folder")),
        (r"C:\dev\ecad-projects\", false, Refused("not a folder")),
        // `~name` is an ordinary name, not a home folder.
        ("~demo", false, Is("~demo.ecad".into())),
        // Drive letters, backslashes, forward slashes, spaces, non-ASCII and UNC paths as typed.
        (r"C:\dev\ecad-projects\demo", false, Is(r"C:\dev\ecad-projects\demo.ecad".into())),
        (r"c:\DEV\ecad-projects\Demo.ECAD", false, Is(r"c:\DEV\ecad-projects\Demo.ECAD".into())),
        ("C:/dev/My Projects/demo", false, Is("C:/dev/My Projects/demo.ecad".into())),
        (r"D:\Projekte\Schaltschrank Ä\plan", false, Is(r"D:\Projekte\Schaltschrank Ä\plan.ecad".into())),
        (r"\\server\share\ecad\demo", false, Is(r"\\server\share\ecad\demo.ecad".into())),
        ("relative/demo", false, Is("relative/demo.ecad".into())),
        // Reserved device names, in any case, with or without the extension.
        ("CON.ecad", false, Refused("reserves for a device")),
        ("con", false, Refused("reserves for a device")),
        (r"C:\dev\Lpt1.ecad", false, Refused("reserves for a device")),
        ("~/nul", true, Refused("reserves for a device")),
        ("aux.backup.ecad", false, Refused("reserves for a device")),
        ("COM10.ecad", false, Is("COM10.ecad".into())),
        ("console.ecad", false, Is("console.ecad".into())),
        // Explorer's "Copy as path" quotes: one surrounding pair is removed.
        (r#""C:\dev\ecad-projects\demo.ecad""#, false, Is(r"C:\dev\ecad-projects\demo.ecad".into())),
        (r#"  "C:\My Projects\demo"  "#, false, Is(r"C:\My Projects\demo.ecad".into())),
        (r#""~/demo""#, true, Is(ecad(&profile.join("demo")))),
        (r#""""#, false, Refused("needs a file name")),
        // `.`, `..` and a bare extension are not file names.
        (".", false, Refused("not .")),
        ("..", false, Refused("not ..")),
        (r"C:\dev\..", false, Refused("not ..")),
        ("~/.", true, Refused("not .")),
        (".ecad", false, Refused("not .ecad")),
        // Drive-relative paths depend on a hidden current folder per drive.
        ("C:", false, Refused("relative to drive C:")),
        ("c:demo", false, Refused("relative to drive c:")),
        ("C:demo.ecad", false, Refused("relative to drive C:")),
        (r"C:\dev\a:b", false, Refused("cannot contain ':'")),
        // A trailing dot would be dropped by Windows.
        ("demo.", false, Refused("trailing dot")),
        ("demo.ecad.", false, Refused("trailing dot")),
        // A dot file is a name like any other.
        (".hidden", false, Is(".hidden.ecad".into())),
        // Other extensions and nothing at all.
        ("plan.dxf", false, Refused(".ecad files, not .dxf")),
        ("   ", false, Refused("needs a file name")),
    ];
    for (typed, known, expect) in rows {
        let got = normalize_with(typed, known.then_some(profile));
        match expect {
            Is(want) => {
                assert_eq!(got.as_deref(), Ok(want.as_str()), "{typed:?}");
                // Normalised once: the `saveas` command normalising again changes nothing.
                assert_eq!(normalize_with(&want, known.then_some(profile)).as_deref(), Ok(want.as_str()), "{typed:?}");
            }
            Refused(why) => {
                let e = got.expect_err(typed);
                assert!(e.contains(why), "{typed:?}: {e}");
            }
        }
    }
}

/// Run by [`the_home_folder_is_known_without_the_home_variable`] in a child process without `HOME`.
#[test]
#[ignore = "run as a child process"]
fn child_prints_its_home_folder() {
    // Only as a child of its test, which sets `ECAD_TEST_CHILD`; run directly, it does nothing.
    if std::env::var_os("ECAD_TEST_CHILD").is_none() {
        return;
    }
    let home = ecad_cadcraft::home().map(|h| h.display().to_string()).unwrap_or_default();
    let p = ecad_cadcraft::normalize("~/p").unwrap_or_default();
    println!("HOME-FOLDER<{home}>\nNORMALIZED<{p}>");
}

#[test]
fn the_home_folder_is_known_without_the_home_variable() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .env("ECAD_TEST_CHILD", "1")
        .args(["--exact", "paths_contracts::child_prints_its_home_folder", "--ignored", "--nocapture", "--test-threads=1"])
        .env_remove("HOME")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    let between = |tag: &str| {
        let start = text.find(&format!("{tag}<")).unwrap() + tag.len() + 1;
        text[start..].split('>').next().unwrap().to_string()
    };
    let home = between("HOME-FOLDER");
    assert!(!home.is_empty(), "the home folder is known from the user profile: {text}");
    if cfg!(windows)
        && let Ok(profile) = std::env::var("USERPROFILE")
    {
        assert_eq!(home, profile);
    }
    let normalized = between("NORMALIZED");
    assert_eq!(normalized, ecad(&Path::new(&home).join("p")));
    assert!(!normalized.starts_with('~'));
}

fn trial_with_home(home: Option<&Path>) -> Trial {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
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

#[test]
fn files_save_and_reopen_under_awkward_folder_names() {
    let d = TempDir::new("awkward");
    let rows = ["My Projects", "Schaltpläne Ä ü", "ünïcödé — 電気"];
    for name in rows {
        let folder = d.0.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        let typed = folder.join("demo").display().to_string();
        let mut t = trial_with_home(None);
        assert_eq!(save_as(&mut t, &typed), None, "{typed}");
        let want = ecad(&folder.join("demo"));
        assert_eq!(t.status().unwrap().file.path.as_deref(), Some(want.as_str()));
        assert!(Path::new(&want).is_file());
        t.close_project();
        t.open_project(&want).unwrap();
        assert_eq!(t.status().unwrap().file.path.as_deref(), Some(want.as_str()));
    }
}

#[test]
fn tilde_paths_save_under_the_home_folder_with_either_separator() {
    let home = TempDir::new("home");
    for (typed, file) in [("~/first", "first.ecad"), (r"~\second", "second.ecad")] {
        let mut t = trial_with_home(Some(&home.0));
        assert_eq!(save_as(&mut t, typed), None, "{typed}");
        let want = home.0.join(file).display().to_string();
        assert_eq!(t.status().unwrap().file.path.as_deref(), Some(want.as_str()), "{typed}");
        // Reopened through the same `~` spelling.
        t.close_project();
        t.open_project(typed).unwrap();
        assert_eq!(t.status().unwrap().file.path.as_deref(), Some(want.as_str()));
    }
    assert!(!home.0.join("~").exists());
}

#[test]
fn no_folder_named_tilde_is_ever_made() {
    let cwd = std::env::current_dir().unwrap();
    let before = cwd.join("~").exists();
    let mut t = trial_with_home(None);
    for typed in ["~/ecad-projects/demo", r"~\ecad-projects\demo", "~/demo"] {
        let e = save_as(&mut t, typed).unwrap_or_default();
        assert!(e.contains(HOME_UNKNOWN), "{typed}: {e}");
    }
    assert_eq!(t.status().unwrap().file.path, None, "nothing was saved");
    assert_eq!(cwd.join("~").exists(), before, "no `~` folder in the working folder");
}

/// On Linux the default folder is `~/ecad-projects`; the first save into it creates it. (On
/// Windows it is `C:\dev\ecad-projects`, which a test must not create.)
#[cfg(not(windows))]
#[test]
fn the_default_folder_is_created_under_the_home_folder() {
    let home = TempDir::new("default");
    let mut t = trial_with_home(Some(&home.0));
    let _ = t.cad.run("qsave", Value::Null);
    t.after_logic();
    assert_eq!(t.dialog().map(|d| d.1.to_string()).as_deref(), Some("~/ecad-projects/untitled.ecad"));
    t.confirm_dialog();
    assert!(t.dialog().is_none(), "{:?}", t.dialog());
    assert!(home.0.join("ecad-projects").join("untitled.ecad").is_file());
    assert!(!home.0.join("~").exists());
    // Another missing folder is not created.
    let e = save_as(&mut t, "~/elsewhere/demo").unwrap_or_default();
    assert!(e.contains("not saved"), "{e}");
    assert!(!home.0.join("elsewhere").exists());
}

#[test]
fn the_dialog_suggests_the_platform_default_folder() {
    let mut t = trial_with_home(None);
    let _ = t.cad.run("qsave", Value::Null);
    t.after_logic();
    let (kind, suggested, _) = t.dialog().unwrap();
    assert_eq!(kind, DialogKind::SaveAs);
    let want = if cfg!(windows) { r"C:\dev\ecad-projects\untitled.ecad" } else { "~/ecad-projects/untitled.ecad" };
    assert_eq!(suggested, want);
    assert_eq!(ecad_trial::DEFAULT_FOLDER, Path::new(want).parent().unwrap().display().to_string());
    t.cancel_dialog();
    t.request_open();
    let sep = std::path::MAIN_SEPARATOR;
    assert_eq!(t.dialog().unwrap().1, format!("{}{sep}", ecad_trial::DEFAULT_FOLDER));
    assert!(ecad_trial::BANNER.contains(ecad_trial::DEFAULT_FOLDER));
    assert!(!ecad_trial::BANNER.contains("Linux"), "platform-neutral: {}", ecad_trial::BANNER);
    assert!(!ecad_trial::USAGE.contains("Linux"), "platform-neutral: {}", ecad_trial::USAGE);
}

#[test]
fn a_case_variant_spelling_of_the_own_file_is_the_same_file() {
    let d = TempDir::new("case");
    let mut t = trial_with_home(None);
    let own = d.0.join("Demo.ecad").display().to_string();
    assert_eq!(save_as(&mut t, &own), None);
    t.cad.run("wire", json!({"points": [[0, 10], [40, 10]]})).unwrap();
    // Typed in another case: on Windows the same file, saved without asking to replace it; on a
    // case-sensitive file system a new file, also without asking.
    let variant = d.0.join("DEMO.ecad").display().to_string();
    assert_eq!(save_as(&mut t, &variant), None, "no replace prompt for the project's own file");
    let s = t.status().unwrap();
    assert_eq!(s.unsaved, 0);
    if cfg!(windows) {
        let files =
            std::fs::read_dir(&d.0).unwrap().filter_map(Result::ok).filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("ecad")));
        assert_eq!(files.count(), 1, "one project file");
    }
    // A different existing file still asks first.
    let other = d.0.join("other.ecad");
    std::fs::write(&other, b"someone else's file\n").unwrap();
    let e = save_as(&mut t, &other.display().to_string()).unwrap_or_default();
    assert!(e.contains("already exists"), "{e}");
}

#[test]
fn a_reserved_device_name_is_refused_with_a_message() {
    let d = TempDir::new("reserved");
    let mut t = trial_with_home(None);
    for name in ["CON.ecad", "con", "Nul.ecad", "LPT1"] {
        let typed = d.0.join(name).display().to_string();
        let e = save_as(&mut t, &typed).unwrap_or_default();
        assert!(e.contains("reserves for a device") && e.contains("choose another"), "{name}: {e}");
    }
    assert_eq!(t.status().unwrap().file.path, None);
    assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 0, "nothing written");
}

#[test]
fn the_banner_names_the_file_as_saved() {
    let home = TempDir::new("banner");
    let mut t = trial_with_home(Some(&home.0));
    assert!(status_line(&t.status().unwrap()).contains("not saved to a file yet, 1 change not saved."));
    assert_eq!(save_as(&mut t, "~/demo"), None);
    let file = home.0.join("demo.ecad").display().to_string();
    assert_eq!(status_line(&t.status().unwrap()), format!("Project \"demo\": 1 page, file {file}, no unsaved changes."));
}

#[test]
fn a_tilde_snapshots_folder_is_under_the_home_folder() {
    let home = TempDir::new("snaps-home");
    for typed in ["~/snaps", r"~\snaps"] {
        ecad_cadcraft::install();
        ecad_cadcraft::views::isolate_for_tests();
        let mut t = Trial::new(Some(PathBuf::from(typed))).unwrap();
        t.home = Some(home.0.clone());
        let written = t.export_snapshot("paths").unwrap();
        assert_eq!(written.parent(), Some(home.0.join("snaps").as_path()), "{typed}");
    }
    assert!(!home.0.join("~").exists());
    // With no home folder known, the export is refused and nothing is made.
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(Some(PathBuf::from("~/snaps"))).unwrap();
    t.home = None;
    assert_eq!(t.export_snapshot("paths").unwrap_err(), HOME_UNKNOWN);
}
