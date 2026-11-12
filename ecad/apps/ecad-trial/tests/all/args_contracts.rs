//! Contracts for `ecad-trial`'s command line (brief 003): arguments arrive as `OsString`
//! (`std::env::args_os`), so a name that is not valid Unicode is a usage error, never a panic.

use std::ffi::OsString;
use std::path::PathBuf;

use ecad_trial::{Invocation, parse_args};

fn run(snapshots: Option<&str>, project: Option<&str>) -> Invocation {
    Invocation::Run { snapshots: snapshots.map(PathBuf::from), project: project.map(str::to_string) }
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

#[test]
fn arguments_are_parsed_or_refused_in_words() {
    // (arguments, expected invocation or a word of the error)
    let rows: Vec<(&[&str], Result<Invocation, &str>)> = vec![
        (&[], Ok(run(None, None))),
        (&["demo.ecad"], Ok(run(None, Some("demo.ecad")))),
        (&[r"C:\dev\ecad-projects\demo.ecad"], Ok(run(None, Some(r"C:\dev\ecad-projects\demo.ecad")))),
        (&[r"C:\Mes projets\schéma Ä.ecad"], Ok(run(None, Some(r"C:\Mes projets\schéma Ä.ecad")))),
        (&["~/ecad-projects/demo.ecad"], Ok(run(None, Some("~/ecad-projects/demo.ecad")))),
        (&["--snapshots", "snaps"], Ok(run(Some("snaps"), None))),
        (&["--snapshots", r"C:\dev\snaps", "demo.ecad"], Ok(run(Some(r"C:\dev\snaps"), Some("demo.ecad")))),
        (&["demo.ecad", "--snapshots", "snaps"], Ok(run(Some("snaps"), Some("demo.ecad")))),
        (&["--help"], Ok(Invocation::Help)),
        (&["-h", "demo.ecad"], Ok(Invocation::Help)),
        (&["--version"], Ok(Invocation::Version)),
        (&["-V"], Ok(Invocation::Version)),
        (&["--snapshots"], Err("--snapshots needs a folder")),
        (&["demo.ecad", "--snapshots"], Err("--snapshots needs a folder")),
        (&["--bogus"], Err("unknown argument \"--bogus\"")),
        (&["/snapshots"], Ok(run(None, Some("/snapshots")))),
        (&["a.ecad", "b.ecad"], Err("only one project file")),
    ];
    for (args, want) in rows {
        let got = parse_args(os(args));
        match want {
            Ok(w) => assert_eq!(got, Ok(w), "{args:?}"),
            Err(word) => {
                let e = got.unwrap_err();
                assert!(e.contains(word), "{args:?}: {e}");
            }
        }
    }
}

/// A file name that is not valid Unicode on this platform.
#[cfg(unix)]
fn not_unicode() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(b"proj\xff.ecad".to_vec())
}

/// A file name that is not valid Unicode on this platform (an unpaired surrogate).
#[cfg(windows)]
fn not_unicode() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(&[0x70, 0xD800, 0x2E, 0x65, 0x63, 0x61, 0x64])
}

#[test]
fn a_name_that_is_not_unicode_is_a_usage_error() {
    let e = parse_args(vec![not_unicode()]).unwrap_err();
    assert!(e.contains("not valid Unicode"), "{e}");
    // As a second argument too, and as a `--snapshots` value it is just a folder.
    assert!(parse_args(vec![OsString::from("demo.ecad"), not_unicode()]).is_err());
    assert_eq!(
        parse_args(vec![OsString::from("--snapshots"), not_unicode()]),
        Ok(Invocation::Run { snapshots: Some(PathBuf::from(not_unicode())), project: None })
    );
}
