//! The report models and the Checks list are first built on the background
//! pool, on copies; a result computed before a commit landed is never installed.

use std::time::Duration;

use ecad_engine::warm::Warm;
use serde_json::json;

use crate::common::H;

const WAIT: Duration = Duration::from_secs(30);

#[test]
fn a_commit_while_the_first_builds_run_drops_the_old_result() {
    let mut h = H::new(48);
    let mut warm = Warm::default();
    // Started at the first head: nothing is kept, nothing is installed in the same call.
    assert!(!h.cfg(|p| warm.pump(p)), "the builds are only started");
    assert!(h.p(|p| p.kept_reports().is_none() && p.kept_checks().is_none()));
    assert_eq!(warm.running(), 2);
    let before = h.head();

    // A commit lands while they run: their results are for the old head.
    h.run("place", json!({"at": [100, 0], "rotation": 90})).unwrap();
    assert_eq!(h.head(), before + 1);
    assert!(warm.wait(WAIT));
    assert!(!h.cfg(|p| warm.pump(p)), "the old results are dropped and the builds start again");
    assert_eq!(warm.dropped(), 2);
    assert!(h.p(|p| p.kept_reports().is_none() && p.kept_checks().is_none()), "the old results were not applied");

    // Started at the new head, they are installed on the next call.
    assert!(warm.wait(WAIT));
    assert!(h.cfg(|p| warm.pump(p)));
    assert_eq!(warm.dropped(), 2, "nothing more dropped");
    assert!(h.p(|p| p.kept_reports().is_some() && p.kept_checks().is_some()));

    // From here the pipeline keeps them: a later commit updates them, nothing is built again.
    h.run("place", json!({"at": [150, 0], "rotation": 0})).unwrap();
    assert!(h.cfg(|p| warm.pump(p)));
    assert_eq!(warm.running(), 0);
}
