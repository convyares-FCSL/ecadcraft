//! Runtime diagnostics (from ecad-inspect): commands accepted or refused, projection failures,
//! saves and recoveries, correlated with command ids and transaction sequence numbers. Kept apart
//! from the authoritative design history: nothing here is saved in the project or replayed.

use std::collections::VecDeque;

use ecad_model::PageId;
use serde::{Deserialize, Serialize};

/// What happened at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// A command's changes were recorded as a transaction.
    Accepted,
    /// A command's changes were refused and the drawing restored.
    Refused,
    /// A command committed with nothing to record.
    NoChange,
    Undo,
    Redo,
    Saved,
    SaveFailed,
    /// Save or Save As on a project without persistence (the desktop desktop app): nothing was written.
    SaveUnavailable,
    /// Projection failed after the ledger accepted a transaction (PROJ-02 follows).
    ProjectionFailed,
    /// The page drawing and handle map were rebuilt from the document.
    ProjectionRebuilt,
    /// Rebuilding failed too: the page view is unavailable until reopened.
    PageUnavailable,
    /// Normal path: the projected drawing differed from what the engine committed (H2). Compares
    /// entities by handle, which is valid only while the engine's handles are kept.
    FidelityMismatch,
    /// Recovery path: a rebuilt page was checked against the document without relying on handle
    /// numbers (geometry, styles, symbol placement, terminal positions, relationships) and agrees.
    RecoveryVerified,
    /// Recovery path: the rebuilt page disagrees with the document; it is shown as unavailable.
    RecoveryVerificationFailed,
    /// A document-owner callback panicked and the owner recovered.
    Recovered,
    PageOpened,
    PageClosed,
    ClipboardCaptured,
    /// The project was opened from a file; the message is what opening found.
    ProjectOpened,
    /// Verify mode found the kept reference index (or what it derived before a commit) out of
    /// step with the document: a bug. It was rebuilt from the document (brief 025).
    IndexRebuilt,
}

/// One runtime event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Monotonic event number in this session.
    pub n: u64,
    pub kind: EventKind,
    /// The command that caused it, if any.
    pub command: Option<String>,
    /// The ledger head when the event was recorded (the transaction it concerns, when one was
    /// appended).
    pub seq: u64,
    pub page: Option<PageId>,
    pub message: String,
}

/// A bounded log of runtime events (oldest dropped first).
#[derive(Debug, Clone)]
pub struct EventLog {
    events: VecDeque<Event>,
    next: u64,
    cap: usize,
}

impl Default for EventLog {
    fn default() -> Self {
        Self { events: VecDeque::new(), next: 1, cap: 10_000 }
    }
}

impl EventLog {
    /// Record an event.
    pub fn record(&mut self, kind: EventKind, command: Option<&str>, seq: u64, page: Option<PageId>, message: impl Into<String>) {
        if self.events.len() >= self.cap {
            self.events.pop_front();
        }
        self.events.push_back(Event { n: self.next, kind, command: command.map(str::to_string), seq, page, message: message.into() });
        self.next += 1;
    }
    /// All retained events, oldest first.
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }
    /// The most recent event.
    pub fn last(&self) -> Option<&Event> {
        self.events.back()
    }
    /// How many events are retained.
    pub fn len(&self) -> usize {
        self.events.len()
    }
    /// Whether no event is retained.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    /// How many events of `kind` were recorded (retained ones).
    pub fn count(&self, kind: EventKind) -> usize {
        self.events.iter().filter(|e| e.kind == kind).count()
    }
}
