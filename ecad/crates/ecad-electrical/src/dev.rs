//! Test counters, built only with the `test-counts` feature (the engine's tests turn it on through
//! their dev-dependencies; no release or app build has it).
//!
//! [`text_passes`] counts how often a page's derived texts were worked out on the calling thread:
//! the update-map tests run a command and check which pages it worked out again. Projection works
//! a page's texts out on the thread that runs the command, so the test thread sees every pass of
//! the commands it runs, and tests running at the same time (which may reuse page ids) never add
//! to each other's counts. Passes on other threads, such as the background checks, are not
//! projection and are not counted.

use std::cell::RefCell;
use std::collections::BTreeMap;

use ecad_model::PageId;

thread_local! {
    static TEXT_PASSES: RefCell<BTreeMap<PageId, u64>> = const { RefCell::new(BTreeMap::new()) };
}

/// One more pass over `page`'s derived texts on this thread.
pub(crate) fn count_text_pass(page: PageId) {
    TEXT_PASSES.with(|c| *c.borrow_mut().entry(page).or_default() += 1);
}

/// How many times this thread has worked out `page`'s derived texts.
pub fn text_passes(page: PageId) -> u64 {
    TEXT_PASSES.with(|c| c.borrow().get(&page).copied().unwrap_or(0))
}
