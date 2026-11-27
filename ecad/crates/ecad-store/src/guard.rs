//! [`Halt`]: containing a panic inside the storage engine.
//!
//! redb does not promise to survive damaged input: a damaged page can make it panic on an ordinary read (an
//! out-of-range slice, an `unwrap`, "entered unreachable code"). Every call into redb that this crate and
//! `ecad-file` make runs through [`Halt::run`], which catches such a panic and turns it into an error. A handle
//! that panicked once is halted: every later call through it returns the first error without touching redb, because
//! redb's state after a panic is unknown (half-updated caches, poisoned locks).
//!
//! A caught panic prints nothing: a quiet panic hook, installed once in front of whatever hook is in place, skips
//! the panic message for panics raised inside [`Halt::run`] on that thread and keeps the place it was raised for the
//! error. Panics anywhere else reach the hook that was in place.
//!
//! Excluded: which error type a caller reports it as (`ecad-file` reports it as damage), and any retry.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Once, OnceLock};

thread_local! {
    /// Inside [`Halt::run`] on this thread (a count: runs nest).
    static QUIET: Cell<u32> = const { Cell::new(0) };
    /// Where the last quiet panic was raised (`file:line`).
    static AT: RefCell<Option<String>> = const { RefCell::new(None) };
}

static HOOK: Once = Once::new();

/// Put the quiet hook in front of the hook in place, once per process.
fn install_quiet_hook() {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if QUIET.with(Cell::get) > 0 {
                let at = info.location().map(|l| {
                    let file = std::path::Path::new(l.file()).components().rev().take(2).collect::<Vec<_>>();
                    let file: Vec<_> = file.iter().rev().map(|c| c.as_os_str().to_string_lossy()).collect();
                    format!("{}:{}", file.join("/"), l.line())
                });
                AT.with(|a| *a.borrow_mut() = at);
            } else {
                previous(info);
            }
        }));
    });
}

/// The text of a panic's payload.
fn payload_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| (*s).to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic".into())
}

/// The storage engine panicked: the panic's message and where it was raised (on a halted handle, those of the
/// panic that halted it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Halted(pub String);

impl std::fmt::Display for Halted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One handle's halt flag, shared by the handle and every read or write view it hands out. Clones share it. It
/// keeps what the first panic said.
#[derive(Debug, Clone, Default)]
pub struct Halt(Arc<OnceLock<String>>);

impl Halt {
    /// A new flag, not halted.
    pub fn new() -> Self {
        Self::default()
    }

    /// What the first panic through this handle said, once one did.
    pub fn halted(&self) -> Option<&str> {
        self.0.get().map(String::as_str)
    }

    /// Run `f`, a call into the storage engine. A panic inside it is caught, halts the handle and is returned as
    /// [`Halted`]; on a halted handle `f` is not run.
    ///
    /// # Errors
    /// [`Halted`]: `f` panicked, now or on an earlier call.
    pub fn run<T>(&self, f: impl FnOnce() -> T) -> Result<T, Halted> {
        if let Some(first) = self.halted() {
            return Err(Halted(first.to_string()));
        }
        install_quiet_hook();
        QUIET.with(|q| q.set(q.get().saturating_add(1)));
        // `AssertUnwindSafe`: after a panic nothing `f` touched is used again. The handle is halted, so redb's state
        // (the only state `f` can leave half-updated) is never read or written through it, and its drop runs here,
        // under the same guard ([`Halt::drop_guarded`]).
        let out = catch_unwind(AssertUnwindSafe(f));
        QUIET.with(|q| q.set(q.get().saturating_sub(1)));
        out.map_err(|p| {
            let at = AT.with(|a| a.borrow_mut().take());
            let what = payload_text(p.as_ref());
            let said = match at {
                Some(at) => format!("the storage engine stopped at a damaged page (redb: {what}, at {at})"),
                None => format!("the storage engine stopped at a damaged page (redb: {what})"),
            };
            Halted(self.0.get_or_init(|| said).clone())
        })
    }

    /// Drop `value` (a redb handle or transaction) so that a panic while it closes is caught and halts the handle;
    /// a panic while dropping is never let out.
    pub fn drop_guarded<T>(&self, value: T) {
        install_quiet_hook();
        QUIET.with(|q| q.set(q.get().saturating_add(1)));
        // `AssertUnwindSafe`: the value is consumed, and nothing else is touched.
        if catch_unwind(AssertUnwindSafe(move || drop(value))).is_err() {
            AT.with(|a| a.borrow_mut().take());
            self.0.get_or_init(|| "the storage engine stopped while closing the file".into());
        }
        QUIET.with(|q| q.set(q.get().saturating_sub(1)));
    }
}
