//! The default projects folder, the banner that names it, and the usage text: platform-neutral words.

use ecad_desktop::folder::{BANNER, DEFAULT_FOLDER};
use ecad_desktop::{Invocation, USAGE, parse_args};

#[test]
fn the_default_folder_is_the_platforms_and_the_words_are_neutral() {
    let want = if cfg!(windows) { r"C:\dev\ecad-projects" } else { "~/ecad-projects" };
    assert_eq!(DEFAULT_FOLDER, want);
    assert!(BANNER.contains(DEFAULT_FOLDER));
    assert!(!BANNER.contains("Linux"), "platform-neutral: {BANNER}");
    assert!(!USAGE.contains("Linux"), "platform-neutral: {USAGE}");
    assert_eq!(ecad_desktop::host::host().default_folder, DEFAULT_FOLDER);
}

#[test]
fn the_cad_ui_flag_turns_the_developer_switch_on() {
    assert!(parse_args(["--cad-ui".into()]).is_ok_and(|i| matches!(i, Invocation::Run { cad_ui: true, .. })));
}

#[test]
fn control_flag_parses_and_writes_nothing() {
    let ok = parse_args(["--control", "47823"].map(std::ffi::OsString::from)).unwrap();
    assert_eq!(ok, Invocation::Run { snapshots: None, project: None, control: Some(47823), cad_ui: false });
    for bad in [&["--control"][..], &["--control", "x"][..], &["--control", "70000"][..], &["--control", "80"][..]] {
        assert!(parse_args(bad.iter().map(std::ffi::OsString::from)).is_err(), "{bad:?}");
    }
    assert!(USAGE.contains("--control"));
}

/// The command line never panics on odd arguments, and hands a `~` snapshots folder over as typed.
#[test]
fn odd_arguments_never_panic_and_a_tilde_folder_is_kept_as_typed() {
    use std::ffi::OsString;
    for args in [vec!["-"], vec!["--"], vec!["--snapshots", "--help"], vec!["--snapshots="], vec![""], vec!["", ""]] {
        let _ = parse_args(args.into_iter().map(OsString::from).collect::<Vec<_>>());
    }
    for typed in ["~/snaps", r"~\snaps2"] {
        let Ok(Invocation::Run { snapshots, .. }) = parse_args(vec![OsString::from("--snapshots"), OsString::from(typed)]) else { panic!("{typed}") };
        assert_eq!(snapshots, Some(std::path::PathBuf::from(typed)));
    }
}
