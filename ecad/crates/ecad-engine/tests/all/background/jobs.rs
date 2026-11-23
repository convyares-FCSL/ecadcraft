//! The background pool's jobs. A result is kept only for the state it
//! was computed at, and a job that panics is a failure, never a crash.

use std::time::Duration;

use ecad_engine::background::{At, JobFailed, Jobs};

const WAIT: Duration = Duration::from_secs(30);

#[test]
fn jobs_keep_results_for_the_state_shown_and_survive_a_panicking_job() {
    let project = ecad_model::ProjectId::from(1_u128);
    let at = |head| At { project, head };
    let mut jobs: Jobs<u64> = Jobs::default();
    jobs.spawn(at(1), || 10);
    jobs.spawn(at(2), || 20);
    jobs.spawn(At { project: ecad_model::ProjectId::from(2_u128), head: 2 }, || 30);
    jobs.spawn(at(2), || panic!("a job that fails"));
    assert!(jobs.wait(WAIT));
    let mut got = jobs.take(at(2));
    got.sort_by_key(|r| r.as_ref().map_or(0, |v| *v));
    assert_eq!(got, vec![Err(JobFailed), Ok(20)]);
    assert_eq!(jobs.dropped(), 2, "the earlier head and the other project");
    assert_eq!(jobs.running(), 0);
}
