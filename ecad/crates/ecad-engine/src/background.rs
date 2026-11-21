//! Work off the UI thread (design 061, rayon; lane R11): a job computes on copies on a small rayon
//! pool, sends its result back over a channel, and the panel that started it takes it on a later
//! frame.
//!
//! # Semantics
//! - **One pool, one core left for egui.** The pool has one thread fewer than the machine's cores
//!   (at least one). If it cannot be built, a job runs on the calling thread instead.
//! - **Copies only.** A job never sees the live project: its caller copies what it needs through
//!   `ProjectHandle` under `try_lock`, releases the lock, then spawns. The copy's state is the job's
//!   [`At`].
//! - **Stale results are dropped.** [`Jobs::take`] keeps only the results whose [`At`] is the
//!   state the panel shows now; a result computed at an earlier head (a commit landed while it
//!   ran) or for another project is counted in [`Jobs::dropped`] and never applied.
//! - **No crash.** A job that panics is caught; its result is [`JobFailed`].
//!
//! Excludes: anything that changes a project, and cancelling a job (a stale one runs to its end
//! and its result is dropped).

use std::num::NonZero;
use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use ecad_model::ProjectId;

/// The project state a job's inputs were copied at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    pub project: ProjectId,
    pub head: u64,
}

/// A job stopped with an internal error (it panicked) and has no result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobFailed;

impl std::fmt::Display for JobFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the job stopped with an internal error")
    }
}

/// The app's background pool: one thread fewer than the cores, so egui keeps one. `None` when the
/// pool could not be built.
fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism().map_or(1, NonZero::get);
        rayon::ThreadPoolBuilder::new().num_threads(cores.saturating_sub(1).max(1)).thread_name(|i| format!("ecad-background-{i}")).build().ok()
    })
    .as_ref()
}

type Done<T> = (At, Result<T, JobFailed>);

/// The jobs one panel started, and their results not yet taken.
#[derive(Debug)]
pub struct Jobs<T> {
    tx: Sender<Done<T>>,
    rx: Receiver<Done<T>>,
    /// Received but not yet taken (by [`Jobs::wait`]).
    ready: Vec<Done<T>>,
    /// Started and not yet received.
    running: usize,
    dropped: u64,
}

impl<T> Default for Jobs<T> {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, ready: Vec::new(), running: 0, dropped: 0 }
    }
}

impl<T: Send + 'static> Jobs<T> {
    /// Run `job` on the background pool; its result is for state `at`.
    pub fn spawn(&mut self, at: At, job: impl FnOnce() -> T + Send + 'static) {
        let tx = self.tx.clone();
        let run = move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(job)).map_err(|_| JobFailed);
            // The receiver is gone only when the panel was dropped: nobody wants the result.
            let _ = tx.send((at, result));
        };
        self.running += 1;
        match pool() {
            Some(p) => p.spawn(run),
            None => run(),
        }
    }

    /// The results ready now that were computed at `now`, in the order they finished. Results at
    /// any other state are dropped (counted in [`Jobs::dropped`]).
    pub fn take(&mut self, now: At) -> Vec<Result<T, JobFailed>> {
        while let Ok(d) = self.rx.try_recv() {
            self.ready.push(d);
        }
        self.running = self.running.saturating_sub(self.ready.len());
        let mut out = Vec::new();
        for (at, r) in self.ready.drain(..) {
            if at == now {
                out.push(r);
            } else {
                self.dropped += 1;
            }
        }
        out
    }

    /// Jobs started and not yet finished (or finished and not yet taken).
    pub fn running(&self) -> usize {
        self.running
    }

    /// Results dropped because the project had moved on when they were taken.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Wait until every running job has finished, at most `timeout`, keeping the results for
    /// [`Jobs::take`] (tests and headless runs; the app never waits). Whether all finished.
    pub fn wait(&mut self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        while self.ready.len() < self.running {
            let left = end.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(d) => self.ready.push(d),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return false,
            }
        }
        true
    }
}
