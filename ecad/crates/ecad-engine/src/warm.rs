//! The kept views' first builds off the UI thread (lane D2, issue 48): the report models and the
//! Checks list, which a project builds whole the first time something shows them, on the
//! background pool ([`crate::background`]).
//!
//! # Semantics
//! - **Read, copy, release.** [`Warm::pump`] runs inside the caller's one project call: it copies
//!   the document, the text metrics and the tolerance, starts the builds on the pool and returns.
//!   Nothing is computed under the project lock.
//! - **Applied on a later frame.** Each call to [`Warm::pump`] first installs the builds that
//!   finished for the state the project is in now, and says whether both views are kept.
//! - **Stale results are dropped.** A build whose head moved while it ran (a commit, an undo), or
//!   whose views were rebuilt whole (a branch switch), is never installed; the next call starts
//!   it again at the new state. A new build waits until the previous one has ended.
//! - **No crash.** A build that panics leaves the view unbuilt; the next state starts it again,
//!   and a call that needs the view at once ([`crate::Project::reports`]) builds it as before.
//!
//! Excludes: the views' updates after a commit (the pipeline, kept views only), commands and the
//! agent, which still build on first use.

use std::time::Duration;

use ecad_electrical::views::{ChecksView, ReportsView};

use crate::background::{At, Jobs};
use crate::project::Project;

/// A view built off the thread, for the views' [`ecad_electrical::views::Views::epoch`] it was
/// copied at.
enum Built {
    Reports(u64, Box<ReportsView>),
    Checks(u64, Box<ChecksView>),
}

/// What [`Warm::pump`] started for which state, so a state is started once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Asked {
    at: At,
    epoch: u64,
}

/// The first builds the project panel started, and their results not yet taken.
#[derive(Debug, Default)]
pub struct Warm {
    jobs: Jobs<Built>,
    asked: Option<Asked>,
    /// Build on the calling thread instead (a headless run that has no frame to wait for).
    inline: bool,
}

impl std::fmt::Debug for Built {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Built::Reports(..) => "Built::Reports",
            Built::Checks(..) => "Built::Checks",
        })
    }
}

impl Warm {
    /// Builds on the calling thread when `inline` (headless tests that read the views at once).
    pub fn new(inline: bool) -> Self {
        Self { inline, ..Self::default() }
    }

    /// Build on the calling thread from now on (`true`) or on the pool (`false`).
    pub fn set_inline(&mut self, inline: bool) {
        self.inline = inline;
    }

    /// Install what finished, start what is still missing, and say whether the report models and
    /// the Checks list are both kept now.
    pub fn pump(&mut self, p: &mut Project) -> bool {
        if self.inline {
            p.reports();
            p.checks();
            return true;
        }
        let now = p.at();
        let epoch = p.derived().epoch();
        for r in self.jobs.take(now) {
            match r {
                Ok(Built::Reports(e, v)) => {
                    p.derived.install_reports(e, *v);
                }
                Ok(Built::Checks(e, v)) => {
                    p.derived.install_checks(e, *v);
                }
                Err(_) => {}
            }
        }
        let (have_reports, have_checks) = (p.kept_reports().is_some(), p.kept_checks().is_some());
        if have_reports && have_checks {
            return true;
        }
        // One start per state, and none while an earlier build still runs (it ends first).
        if self.jobs.running() > 0 || self.asked == Some(Asked { at: now, epoch }) {
            return false;
        }
        self.asked = Some(Asked { at: now, epoch });
        let doc = p.ledger.document().clone();
        let metrics = p.derived().metrics();
        if !have_reports {
            let (doc, metrics) = (doc.clone(), metrics.clone());
            self.jobs.spawn(now, move || Built::Reports(epoch, Box::new(ReportsView::build(&doc, &*metrics))));
        }
        if !have_checks {
            let tol = p.tolerance;
            self.jobs.spawn(now, move || Built::Checks(epoch, Box::new(ChecksView::build(&doc, tol, &*metrics))));
        }
        false
    }

    /// Builds started and not yet taken.
    pub fn running(&self) -> usize {
        self.jobs.running()
    }

    /// Builds dropped because the project had moved on when they were taken.
    pub fn dropped(&self) -> u64 {
        self.jobs.dropped()
    }

    /// Wait for the running builds, at most `timeout` (tests and headless runs; the app never
    /// waits); the next [`Warm::pump`] installs them. Whether all finished.
    pub fn wait(&mut self, timeout: Duration) -> bool {
        self.jobs.wait(timeout)
    }
}
