//! Contracts for the trial's start-up timing (brief 015): the switch, the marks that need no
//! window (1 to 3, 5a and 5b), their sums, the JSON file and the process-age parsing.

use std::time::{Duration, Instant, SystemTime};

use ecad_trial::Trial;
use std::path::{Path, PathBuf};

use ecad_trial::startup::{Adapter, Origin, StartupTiming, Step, auxv_clock_ticks, enabled, preferred_folder, process_age, process_age_from};

/// A fresh temp folder, removed when dropped (also when an assertion fails).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ecad-startup-contracts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_switch_is_on_only_when_asked() {
    // (value of ECAD_STARTUP_TIMING, timing on)
    let rows: &[(Option<&str>, bool)] = &[
        (None, false),
        (Some(""), false),
        (Some("0"), false),
        (Some("off"), false),
        (Some("no"), false),
        (Some("maybe"), false),
        (Some("2"), false),
        (Some("1"), true),
        (Some(" 1 "), true),
        (Some("on"), true),
        (Some("TRUE"), true),
        (Some("yes"), true),
    ];
    for (value, want) in rows {
        assert_eq!(enabled(*value), *want, "{value:?}");
    }
}

fn quiet(before_main: Option<Duration>) -> StartupTiming {
    StartupTiming::starting(Instant::now(), SystemTime::now(), before_main, false)
}

/// The marks are in order, never go back, and their steps add up to the last one's total.
fn assert_consistent(t: &StartupTiming) {
    let mut previous = 0.0;
    let mut sum = 0.0;
    for m in t.marks() {
        assert!(m.since_start_ms >= previous, "{m:?} after {previous}");
        assert!(m.step_ms >= 0.0, "{m:?}");
        assert!((m.since_start_ms - previous - m.step_ms).abs() < 1e-9, "{m:?}");
        previous = m.since_start_ms;
        sum += m.step_ms;
    }
    let total = t.marks().last().map_or(0.0, |m| m.since_start_ms);
    assert!((sum - total).abs() < 1e-6, "steps {sum} vs total {total}");
}

#[test]
fn headless_marks_are_monotonic_and_add_up() {
    // (before main, project file, expected origin)
    let missing = std::env::temp_dir().join("ecad-startup-contracts-no-such-file.ecad");
    let missing = missing.to_string_lossy().into_owned();
    let rows: Vec<(Option<Duration>, Option<&str>, Origin)> = vec![
        (None, None, Origin::Main),
        (Some(Duration::from_millis(250)), None, Origin::ProcessCreation),
        (None, Some(missing.as_str()), Origin::Main),
    ];
    for (before_main, project, origin) in rows {
        let mut t = quiet(before_main);
        assert_eq!(t.origin(), origin);
        t.mark(Step::Main);
        ecad_cadcraft::install();
        t.mark(Step::Install);
        t.mark(Step::RunNative);
        let trial = Trial::start(None, project, Some(&mut t));
        assert!(trial.is_ok(), "{project:?}: {:?}", trial.err());
        let steps: Vec<Step> = t.marks().iter().map(|m| m.step).collect();
        assert_eq!(steps, [Step::Main, Step::Install, Step::RunNative, Step::Shell, Step::Project], "{project:?}");
        let first = t.marks().first().map_or(-1.0, |m| m.since_start_ms);
        let want = before_main.unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
        assert!(first >= want && first < want + 1000.0, "main at {first} ms, before main {want} ms");
        assert_consistent(&t);
    }
}

#[test]
fn every_step_has_a_distinct_number_and_name() {
    let numbers: Vec<_> = Step::ALL.iter().map(|s| s.number()).collect();
    assert_eq!(numbers, ["1", "2", "3", "4", "5a", "5b", "6", "7", "8"]);
    let mut names: Vec<_> = Step::ALL.iter().map(|s| s.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), Step::ALL.len());
    assert!(Step::ALL.iter().all(|s| !s.label().is_empty()));
}

#[test]
fn the_json_file_holds_every_mark_and_the_adapter() {
    let tmp = TempDir::new("json");
    let dir = tmp.path();
    let mut t = quiet(Some(Duration::from_millis(40)));
    for step in Step::ALL {
        t.mark(step);
    }
    t.set_adapter(Adapter { backend: "Vulkan".into(), name: "Test GPU \"1\"".into(), device_type: "IntegratedGpu".into() });
    assert!(t.adapter_line().contains("backend Vulkan"), "{}", t.adapter_line());
    assert!(t.adapter_line().contains("IntegratedGpu"), "{}", t.adapter_line());
    assert!(t.complete());
    let path = t.write_json(dir).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("ecad-startup-") && name.ends_with(".json"), "{name}");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(v["origin"], "process creation");
    assert_eq!(v["adapter"]["backend"], "Vulkan");
    assert_eq!(v["adapter"]["name"], "Test GPU \"1\"");
    assert_eq!(v["before_main_ms"].as_f64(), Some(40.0));
    let marks = v["marks"].as_array().unwrap();
    let numbers: Vec<_> = marks.iter().map(|m| m["mark"].as_str().unwrap()).collect();
    assert_eq!(numbers, ["1", "2", "3", "4", "5a", "5b", "6", "7", "8"]);
    let sum: f64 = marks.iter().map(|m| m["step_ms"].as_f64().unwrap()).sum();
    let total = v["total_ms"].as_f64().unwrap();
    // Each value is rounded to a microsecond.
    assert!((sum - total).abs() < 0.01, "steps {sum} vs total {total}");
    assert!(v["main_unix_ms"].as_u64().is_some_and(|ms| ms > 1_700_000_000_000), "{}", v["main_unix_ms"]);
    assert_eq!(v["complete"], true);
}

#[test]
fn the_json_file_never_replaces_an_earlier_one() {
    let tmp = TempDir::new("names");
    let mut t = quiet(None);
    t.mark(Step::Main);
    let paths: Vec<PathBuf> = (0..3).map(|_| t.write_json(tmp.path()).unwrap()).collect();
    let names: Vec<String> = paths.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert!(names[1].ends_with("-1.json") && names[2].ends_with("-2.json"), "{names:?}");
    assert_eq!(names[0].replace(".json", "-1.json"), names[1]);
    for p in &paths {
        assert!(std::fs::read_to_string(p).unwrap().contains("\"complete\": false"), "{}", p.display());
    }
}

#[test]
fn the_json_file_goes_to_the_snapshots_folder_or_else_the_temp_folder() {
    let tmp = TempDir::new("folders");
    let mut t = quiet(None);
    t.mark(Step::Main);
    // A snapshots folder that does not exist yet is created.
    let missing = tmp.path().join("new snaps").join("deeper");
    let (p, why) = t.write_json_preferring(Some(&missing)).unwrap();
    assert_eq!(p.parent(), Some(missing.as_path()));
    assert_eq!(why, None);
    // A "folder" that is a file falls back to the temp folder, saying why.
    let file = tmp.path().join("a file");
    std::fs::write(&file, "x").unwrap();
    let (p, why) = t.write_json_preferring(Some(&file)).unwrap();
    assert_eq!(p.parent(), Some(std::env::temp_dir().as_path()));
    assert!(why.is_some_and(|w| w.contains("a file")));
    std::fs::remove_file(&p).unwrap();
    // No snapshots folder: the temp folder.
    let (p, why) = t.write_json_preferring(None).unwrap();
    assert_eq!(p.parent(), Some(std::env::temp_dir().as_path()));
    assert_eq!(why, None);
    std::fs::remove_file(&p).unwrap();
}

#[test]
fn the_preferred_folder_expands_home_as_snapshot_export_does() {
    let home = Path::new("/home/someone");
    // (snapshots, home, expected)
    let rows: Vec<(Option<&str>, Option<&Path>, Option<PathBuf>)> = vec![
        (None, Some(home), None),
        (Some("snaps"), Some(home), Some(PathBuf::from("snaps"))),
        (Some("~/snaps"), Some(home), Some(home.join("snaps"))),
        (Some(r"~\snaps"), Some(home), Some(home.join("snaps"))),
        (Some("~/snaps"), None, None), // no home known: the temp folder
    ];
    for (snapshots, home, want) in rows {
        assert_eq!(preferred_folder(snapshots.map(Path::new), home), want, "{snapshots:?}");
    }
}

#[test]
fn the_json_file_says_when_it_cannot_be_written() {
    let t = quiet(None);
    let dir = std::env::temp_dir().join("ecad-startup-contracts-no-such-folder").join("deeper");
    let err = t.write_json(&dir).unwrap_err();
    assert!(err.contains("ecad-startup-"), "{err}");
}

#[test]
fn the_process_age_is_read_from_proc_or_refused() {
    // A stat line whose command name holds spaces and parentheses; start time 1000 ticks.
    let stat = "4242 (ecad (trial) x) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 9 0 1000 1 2 3";
    // (stat, uptime, ticks per second, expected age in ms)
    let rows: &[(&str, &str, u64, Option<u64>)] = &[
        (stat, "12.50 99.0\n", 100, Some(2500)),
        (stat, "10.00 99.0", 100, Some(0)),
        (stat, "9.00 1.0", 100, None),     // started after "now": implausible
        (stat, "999999.0 1.0", 100, None), // over a day old: implausible
        (stat, "12.5", 0, None),           // no clock rate
        (stat, "nan 1.0", 100, None),
        (stat, "", 100, None),
        ("4242 (ecad) S 1 2", "12.5", 100, None), // too few fields
        ("no parentheses at all", "12.5", 100, None),
        ("", "", 100, None),
    ];
    for (stat, uptime, ticks, want) in rows {
        let got = process_age_from(stat, uptime, *ticks).map(|d| d.as_millis() as u64);
        assert_eq!(got, *want, "{stat:?} {uptime:?} {ticks}");
    }
    // Linux gives the age; elsewhere the marks are measured from `main`.
    let age = process_age();
    if cfg!(target_os = "linux") {
        let age = age.expect("Linux gives the process age");
        assert!(age < Duration::from_secs(3600), "{age:?}");
    } else {
        assert_eq!(age, None);
    }
}

#[test]
fn the_clock_rate_is_read_from_the_auxiliary_vector() {
    fn vector(pairs: &[(usize, usize)]) -> Vec<u8> {
        pairs.iter().flat_map(|(k, v)| k.to_ne_bytes().into_iter().chain(v.to_ne_bytes())).collect()
    }
    // (pairs, expected AT_CLKTCK)
    let rows: Vec<(Vec<u8>, Option<u64>)> = vec![
        (vector(&[(6, 4096), (17, 100), (0, 0)]), Some(100)),
        (vector(&[(17, 250)]), Some(250)),
        (vector(&[(6, 4096), (0, 0), (17, 100)]), None), // after AT_NULL
        (vector(&[(17, 0)]), None),
        (vector(&[]), None),
        (vec![17, 0, 0], None), // truncated
    ];
    for (bytes, want) in rows {
        assert_eq!(auxv_clock_ticks(&bytes), want, "{bytes:?}");
    }
}
