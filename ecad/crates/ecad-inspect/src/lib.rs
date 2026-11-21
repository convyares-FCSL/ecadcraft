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
//! The inspector page (`ecad/dev/ecad-inspector/page/`, Snapshots mode) reads these files.
//!
//! This crate intentionally has **no** mutation API, no network server and no CADCraft dependency.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::VecDeque;

use ecad_model::derive::{CableReference, MAX_REFERENCES_PER_CABLE, cable_references, mismatches, nets};
use ecad_model::{Document, Ledger, ModelError, PageId, Subject, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Format tag of snapshot files. Independent of product and schema versions.
///
/// `ecad-inspect/2` adds `persistence` (live snapshots only, `null` otherwise): where the project's
/// file stands (path, header, writer state, record offsets, lock, read-only reason). Everything in
/// `ecad-inspect/1` is unchanged. Brief 010 adds three derived entries to `checks`
/// (`cable_references`, `orphan_references`, `over_referenced_cables`); readers that do not know
/// them ignore them, so the tag is unchanged.
pub const SNAPSHOT_FORMAT: &str = "ecad-inspect/2";

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
    /// Where the project's file stands, as the adapter reports it (`ecad-inspect/2`).
    #[serde(default)]
    pub persistence: Option<Value>,
}

fn label_of(doc: &Document, s: Subject) -> String {
    let tag = |p: &ecad_model::Properties| p.get("tag").cloned();
    match s {
        Subject::Device(c) => doc.devices.get(&c).and_then(|x| x.tag().map(str::to_string)).unwrap_or_else(|| "untagged device".into()),
        Subject::Wire(c) => doc.wires.get(&c).and_then(|x| x.number_text().map(|n| format!("wire {n}"))).unwrap_or_else(|| "unnumbered wire".into()),
        Subject::Cable(c) => doc.cables.get(&c).and_then(|x| tag(&x.properties)).unwrap_or_else(|| "untagged cable".into()),
        Subject::Route(r) => doc.routes.get(&r).map_or_else(|| "line".into(), |x| format!("line on page {}", x.page.short())),
        Subject::Aid(a) => match doc.aids.get(&a) {
            Some(ecad_model::Aid::Continuation { name: Some(n), .. }) => format!("continuation {n}"),
            Some(ecad_model::Aid::Continuation { name: None, .. }) => "unnamed continuation".into(),
            Some(ecad_model::Aid::Box(_)) => "location box".into(),
            None => "drawing aid".into(),
        },
    }
}

/// Readable labels for every object, for search.
fn index(doc: &Document) -> Vec<Value> {
    let mut out = Vec::new();
    let mut push = |kind: &str, id: String, short: String, label: String| {
        out.push(json!({"kind": kind, "id": id, "short": short, "label": label}));
    };
    for (id, p) in &doc.pages {
        push("page", format!("{:032x}", id.0), id.short(), p.title.to_string());
    }
    for id in doc.devices.keys() {
        push("device", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Device(*id)));
    }
    for (id, t) in &doc.terminals {
        let owner = label_of(doc, Subject::Device(t.component));
        let name = t.label.clone().or_else(|| t.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| id.short());
        push("terminal", format!("{:032x}", id.0), id.short(), format!("{owner}:{name}"));
    }
    for id in doc.wires.keys() {
        push("wire", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Wire(*id)));
    }
    // A wire's net and its conductor (design 072 §4.2): named after the wire first in the net's
    // order, and after the wire that is the conductor.
    let mut nets: std::collections::BTreeMap<ecad_model::NetId, (u16, String)> = std::collections::BTreeMap::new();
    for (id, w) in &doc.wires {
        let label = label_of(doc, Subject::Wire(*id));
        push("conductor", format!("{:032x}", w.conductor.0), w.conductor.short(), format!("conductor of {label}"));
        let first = nets.entry(w.net).or_insert_with(|| (w.net_order, label.clone()));
        if w.net_order < first.0 {
            *first = (w.net_order, label);
        }
    }
    for (id, (_, label)) in &nets {
        push("net", format!("{:032x}", id.0), id.short(), format!("net of {label}"));
    }
    for id in doc.cables.keys() {
        push("cable", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Cable(*id)));
    }
    for id in doc.routes.keys() {
        push("route", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Route(*id)));
    }
    for id in doc.aids.keys() {
        push("aid", format!("{:032x}", id.0), id.short(), label_of(doc, Subject::Aid(*id)));
    }
    for (id, r) in &doc.representations {
        push("representation", format!("{:032x}", id.0), id.short(), format!("{} of {}", r.role, label_of(doc, r.subject)));
    }
    for (id, d) in &doc.definitions {
        push("definition", format!("{:032x}", id.0), id.short(), d.name.clone());
    }
    for (id, i) in &doc.items {
        let what = match &i.kind {
            ecad_model::ItemKind::Primitive(p) => format!("{p:?}").split([' ', '{']).next().unwrap_or("item").to_lowercase(),
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
pub fn snapshot(name: &str, ledger: &Ledger, at: Option<u64>, events: &EventLog, live: Option<&LiveFacts>) -> Result<Value, ModelError> {
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
    let refs = cable_references(doc);
    let pages: Vec<Value> = doc
        .page_order
        .iter()
        .map(|id| {
            let fact = live.and_then(|l| l.pages.iter().find(|p| p.id == *id));
            json!({
                "id": format!("{:032x}", id.0),
                "short": id.short(),
                "title": doc.pages.get(id).map(|p| p.title.to_string()).unwrap_or_default(),
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
        "persistence": live.and_then(|l| l.persistence.clone()),
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
            "cable_references": references_json(doc, &refs),
            "orphan_references": refs.iter().filter(|r| r.orphan).map(|r| json!({
                "reference": r.reference.hex(),
                "cable": r.cable.hex(),
                "page": r.page.hex(),
            })).collect::<Vec<_>>(),
            "over_referenced_cables": over_referenced(&refs).iter().map(|k| k.hex()).collect::<Vec<_>>(),
        },
    }))
}

/// Cables with more than [`MAX_REFERENCES_PER_CABLE`] references (possible only from a file), in
/// ID order.
pub fn over_referenced(refs: &[CableReference]) -> Vec<ecad_model::CableId> {
    let mut n: std::collections::BTreeMap<ecad_model::CableId, usize> = std::collections::BTreeMap::new();
    for r in refs {
        *n.entry(r.cable).or_default() += 1;
    }
    n.into_iter().filter(|(_, c)| *c > MAX_REFERENCES_PER_CABLE).map(|(k, _)| k).collect()
}

/// The cable references of `doc` as `cablerefs` and the snapshot's `checks.cable_references` list
/// them (design 010 §7.4): one object per reference, in page order, then column, then ID.
pub fn references_json(doc: &Document, refs: &[CableReference]) -> Value {
    Value::Array(
        refs.iter()
            .map(|r| {
                json!({
                    "reference": r.reference.hex(),
                    "cable": r.cable.hex(),
                    "cable_tag": doc.cables.get(&r.cable).and_then(|c| c.properties.get("tag")),
                    "page": r.page.hex(),
                    "page_number": r.page_number,
                    "page_title": doc.pages.get(&r.page).map(|p| p.title.to_string()).unwrap_or_default(),
                    "at": r.at.map(|p| json!([p.x, p.y])),
                    "column": r.column,
                    // Brief 019: the frame's row and labels, and this end's own place.
                    "row": r.row,
                    "column_label": r.column_label,
                    "row_label": r.row_label,
                    "place": r.locator(),
                    "text": r.text,
                    "partner": r.partner().map(|p| p.hex()),
                    "orphan": r.orphan,
                    "wires_here": r.wires_here,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use ecad_model::{Author, ConductorId, Device, DeviceId, IdAllocator, NetId, Op, Terminal, TerminalId, Wire, WireId};

    /// Design 072 §4.2: a wire's net and conductor have labels in the snapshot's index, the net
    /// named after the wire first in its order.
    #[test]
    fn a_wires_net_and_conductor_are_labelled() {
        let mut ids = IdAllocator::seeded(72);
        let mut l = ecad_model::Ledger::new(72);
        let (k, a, b, c): (DeviceId, TerminalId, TerminalId, TerminalId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
        let (w1, w2, net, c1, c2): (WireId, WireId, NetId, ConductorId, ConductorId) =
            (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
        let point = |label: &str| Terminal { component: k, label: Some(label.into()), key: None, accepts: Default::default() };
        let number = |n: &str| ecad_model::text::Text::new(n).ok().map(|text| ecad_model::wire::WireNumber { text, fixed: true });
        let ops = vec![
            Op::CreateDevice { id: k, device: Device::default().with_tag(Some("-K1")) },
            Op::AddTerminal { id: a, terminal: point("1") },
            Op::AddTerminal { id: b, terminal: point("2") },
            Op::AddTerminal { id: c, terminal: point("3") },
            Op::CreateWire { id: w1, wire: Wire { net_order: 2, number: number("102"), ..Wire::new(net, c1, b, c) } },
            Op::CreateWire { id: w2, wire: Wire { net_order: 1, number: number("101"), ..Wire::new(net, c2, a, b) } },
        ];
        l.commit(Author::User, "wires", ops).unwrap();
        let s = super::snapshot("t", &l, None, &super::EventLog::default(), None).unwrap();
        let label = |kind: &str, hex: String| {
            s["index"].as_array().unwrap().iter().find(|x| x["kind"] == kind && x["id"] == hex.as_str()).map(|x| x["label"].clone())
        };
        assert_eq!(label("net", format!("{:032x}", net.0)), Some("net of wire 101".into()));
        assert_eq!(label("conductor", format!("{:032x}", c1.0)), Some("conductor of wire 102".into()));
        assert_eq!(label("conductor", format!("{:032x}", c2.0)), Some("conductor of wire 101".into()));
    }
}
