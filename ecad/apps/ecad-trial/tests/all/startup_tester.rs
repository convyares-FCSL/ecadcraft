//! Tester's contracts for brief 015: marks 5a and 5b around opening an existing project file, and
//! `Trial::start` without timing behaving as before.

use std::time::{Instant, SystemTime};

use ecad_trial::Trial;
use ecad_trial::startup::{StartupTiming, Step};

fn quiet() -> StartupTiming {
    StartupTiming::starting(Instant::now(), SystemTime::now(), None, false)
}

#[test]
fn opening_a_saved_project_marks_the_shell_then_the_project() {
    let dir = std::env::temp_dir().join(format!("ecad-startup-tester-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("timed.ecad").to_string_lossy().into_owned();
    {
        let mut first = Trial::new(None).unwrap();
        first.save_as(&path).unwrap();
    }
    assert!(std::path::Path::new(&path).exists());

    // (project, timing on)
    let rows: &[(Option<&str>, bool)] = &[(Some(path.as_str()), true), (Some(path.as_str()), false), (None, false)];
    for (project, timed) in rows {
        ecad_cadcraft::views::isolate_for_tests();
        let mut t = quiet();
        t.mark(Step::Main);
        let trial = Trial::start(None, *project, timed.then_some(&mut t)).unwrap();
        assert!(trial.project_open(), "{project:?}");
        assert_eq!(trial.notice(), None, "{project:?} opened without a notice");
        let steps: Vec<Step> = t.marks().iter().map(|m| m.step).collect();
        let want: &[Step] = if *timed { &[Step::Main, Step::Shell, Step::Project] } else { &[Step::Main] };
        assert_eq!(steps, want, "{project:?} timed {timed}");
        let mut previous = 0.0;
        for m in t.marks() {
            assert!(m.since_start_ms >= previous && m.step_ms >= 0.0, "{m:?}");
            previous = m.since_start_ms;
        }
        drop(trial);
    }
    std::fs::remove_dir_all(&dir).ok();
}
