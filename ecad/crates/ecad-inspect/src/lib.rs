//! ecad-inspect
//!
//! Read-only developer inspection for the bounded integration experiment:
//!
//! - [`EventLog`]: **runtime diagnostics** (commands accepted or refused, projection failures,
//!   saves, recoveries), correlated with command ids and transaction sequence numbers. It is kept
//!   apart from the authoritative design history: nothing here is saved in the project or replayed.
//! - [`snapshot`]: a JSON snapshot of a project, labelled with the transaction sequence it shows.
//!   A historical snapshot reconstructs that state separately ([`Ledger::state_at`]); the live
//!   project is never modified.
//!
//! The snapshot reuses the model's own serialisation (`Document`, `Transaction`) instead of a
//! second model, adding only an index of readable labels and the results of consistency checks.
//! The static viewer in `ecad/tools/inspector/` reads these files.
//!
//! This crate intentionally has **no** mutation API, no network server and no CADCraft dependency.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::VecDeque;

use ecad_model::derive::{mismatches, nets};
use ecad_model::{Document, Ledger, ModelError, PageId, Subject, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Format tag of snapshot files. Independent of product and schema versions.
pub const SNAPSHOT_FORMAT: &str = "ecad-inspect/1";

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
    pub fn record(
        &mut self,
        kind: EventKind,
        command: Option<&str>,
        seq: u64,
        page: Option<PageId>,
        message: impl Into<String>,
    ) {
        if self.events.len() >= self.cap {
            self.events.pop_front();
        }
        self.events.push_back(Event {
            n: self.next,
            kind,
            command: command.map(str::to_string),
            seq,
            page,
            message: message.into(),
        });
        self.next += 1;
    }
    /// All retained events, oldest first.
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }
    /// How many events of `kind` were recorded (retained ones).
    pub fn count(&self, kind: EventKind) -> usize {
        self.events.iter().filter(|e| e.kind == kind).count()
    }
}

/// A page as the inspector shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    pub id: PageId,
    pub title: String,
    /// Whether a CADCraft document shows this page now.
    pub open: bool,
    /// Live only: whether that drawing equals a fresh projection of the document.
    pub projection_matches: Option<bool>,
    /// Live only: number of mapped handles.
    pub mapped_handles: Option<usize>,
}

/// Live-only facts the adapter supplies.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveFacts {
    pub pages: Vec<PageInfo>,
    /// The incorporated CADCraft commit, for correlation.
    pub upstream: Option<String>,
}

fn label_of(doc: &Document, s: Subject) -> String {
    let tag = |p: &ecad_model::Properties| p.get("tag").cloned();
    match s {
        Subject::Component(c) => {
            doc.components.get(&c).and_then(|x| tag(&x.properties)).unwrap_or_else(|| "untagged component".into())
        }
        Subject::Conductor(c) => {
            doc.conductors.get(&c).and_then(|x| tag(&x.properties)).unwrap_or_else(|| "untagged conductor".into())
        }
        Subject::Cable(c) => {
            doc.cables.get(&c).and_then(|x| tag(&x.properties)).unwrap_or_else(|| "untagged cable".into())
        }
    }
}

/// Readable labels for every object, for search.
fn index(doc: &Document) -> Vec<Value> {
    let mut out = Vec::new();
    let mut push = |kind: &str, id: String, short: String, label: String| {
        out.push(json!({"kind": kind, "id": id, "short": short, "label": label}));
    };
    for (id, p) in &doc.pages {
        push("page", format!("{:032x}", id.0), id.short(), p.title.clone());
    }
    for id in doc.components.keys() {
        push("component", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Component(*id)));
    }
    for (id, t) in &doc.terminals {
        let owner = label_of(doc, Subject::Component(t.component));
        let name = t.label.clone().or_else(|| t.key.clone()).unwrap_or_else(|| id.short());
        push("terminal", format!("{:032x}", id.0), id.short(), format!("{owner}:{name}"));
    }
    for id in doc.conductors.keys() {
        push("conductor", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Conductor(*id)));
    }
    for id in doc.cables.keys() {
        push("cable", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Cable(*id)));
    }
    for (id, r) in &doc.representations {
        push(
            "representation",
            format!("{:032x}", id.0),
            id.short(),
            format!("{} of {}", r.role, label_of(doc, r.subject)),
        );
    }
    for (id, d) in &doc.definitions {
        push("definition", format!("{:032x}", id.0), id.short(), d.name.clone());
    }
    for (id, i) in &doc.items {
        let what = match &i.kind {
            ecad_model::ItemKind::Primitive(p) => {
                format!("{p:?}").split([' ', '{']).next().unwrap_or("item").to_lowercase()
            }
            ecad_model::ItemKind::Symbol(_) => "symbol".into(),
        };
        push("item", format!("{:032x}", id.0), id.short(), what);
    }
    out
}

/// Build a snapshot.
///
/// # Semantics
/// - `at = None`: the live state at the ledger head, with the adapter's live facts.
/// - `at = Some(seq)`: the state after transaction `seq`, reconstructed separately from the log;
///   the ledger is borrowed immutably and the live document is not touched. Live facts do not
///   apply and are omitted.
///
/// Runtime events are included with their sequence numbers; for a historical snapshot only those
/// recorded at or before `seq`.
///
/// # Errors
/// `seq` past the head, or a log that fails to replay.
pub fn snapshot(
    name: &str,
    ledger: &Ledger,
    at: Option<u64>,
    events: &EventLog,
    live: Option<&LiveFacts>,
) -> Result<Value, ModelError> {
    let seq = at.unwrap_or(ledger.head());
    let reconstructed;
    let doc: &Document = match at {
        None => ledger.document(),
        Some(s) => {
            reconstructed = ledger.state_at(s)?;
            &reconstructed
        }
    };
    let txs: Vec<&Transaction> = ledger.log().iter().take(seq as usize).collect();
    let replay_equals_live = match at {
        None => Some(ledger.replay_matches()?),
        Some(_) => None,
    };
    let pages: Vec<Value> = doc
        .page_order
        .iter()
        .map(|id| {
            let fact = live.and_then(|l| l.pages.iter().find(|p| p.id == *id));
            json!({
                "id": format!("{:032x}", id.0),
                "short": id.short(),
                "title": doc.pages.get(id).map(|p| p.title.clone()).unwrap_or_default(),
                "open": fact.map(|f| f.open),
                "projection_matches": fact.and_then(|f| f.projection_matches),
                "mapped_handles": fact.and_then(|f| f.mapped_handles),
            })
        })
        .collect();
    Ok(json!({
        "format": SNAPSHOT_FORMAT,
        "product_version": env!("CARGO_PKG_VERSION"),
        "schema_version": ecad_model::SCHEMA_VERSION,
        "upstream": live.and_then(|l| l.upstream.clone()),
        "project": name,
        "label": match at { None => format!("live at seq {seq}"), Some(s) => format!("history at seq {s}") },
        "kind": if at.is_none() { "live" } else { "historical" },
        "seq": seq,
        "head": ledger.head(),
        "persisted": ledger.persisted(),
        "unsaved": ledger.is_unsaved(),
        "pages": pages,
        "index": index(doc),
        "document": doc,
        "transactions": txs,
        "events": events.events().filter(|e| at.is_none() || e.seq <= seq).collect::<Vec<_>>(),
        "checks": {
            "replay_equals_live": replay_equals_live,
            "invariants_hold": doc.check().is_ok(),
            "mismatches": mismatches(doc, 1e-6),
            "nets": nets(doc),
        },
    }))
}
