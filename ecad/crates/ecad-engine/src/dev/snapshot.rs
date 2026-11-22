//! Project snapshots (from ecad-inspect): a JSON snapshot of a project, labelled with the
//! transaction sequence it shows. A historical snapshot reconstructs that state separately
//! ([`Ledger::state_at`]); the live project is never modified. The snapshot reuses the model's own
//! serialisation (`Document`, `Transaction`), adding an index of readable labels and the results of
//! consistency checks. The inspector page (`ecad/dev/ecad-inspector/page/`, Snapshots mode) reads it.

use ecad_electrical::references::cables::{over_referenced, references_json};
use ecad_model::connectivity::derive::{cable_references, mismatches, nets};
use ecad_model::{Document, Ledger, ModelError, PageId, Subject, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::history::event_log::EventLog;

/// Format tag of snapshot files. Independent of product and schema versions.
///
/// `ecad-inspect/2` adds `persistence` (live snapshots only, `null` otherwise): where the project's
/// file stands (path, header, writer state, record offsets, lock, read-only reason). Everything in
/// `ecad-inspect/1` is unchanged. Brief 010 adds three derived entries to `checks`
/// (`cable_references`, `orphan_references`, `over_referenced_cables`); readers that do not know
/// them ignore them, so the tag is unchanged.
pub(crate) const SNAPSHOT_FORMAT: &str = "ecad-inspect/2";

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
    match s {
        Subject::Device(c) => doc.devices.get(&c).and_then(|x| x.tag_shown()).unwrap_or_else(|| "untagged device".into()),
        Subject::Wire(c) => doc.wires.get(&c).and_then(|x| x.number_text().map(|n| format!("wire {n}"))).unwrap_or_else(|| "unnumbered wire".into()),
        Subject::Cable(c) => doc.cables.get(&c).and_then(|x| x.tag_shown()).unwrap_or_else(|| "untagged cable".into()),
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
    for (id, t) in &doc.connection_points {
        let owner = label_of(doc, t.owner.subject());
        let name = t.label.clone().or_else(|| t.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| id.short());
        push("connection_point", format!("{:032x}", id.0), id.short(), format!("{owner}:{name}"));
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
    for (id, p) in &doc.loose_parts {
        push("loose_part", format!("{:032x}", id.0), id.short(), format!("loose part {}", p.description));
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
    let references = cable_references(doc);
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
            "cable_references": references_json(doc, &references),
            "orphan_references": references.iter().filter(|r| r.orphan).map(|r| json!({
                "reference": r.reference.hex(),
                "cable": r.cable.hex(),
                "page": r.page.hex(),
            })).collect::<Vec<_>>(),
            "over_referenced_cables": over_referenced(&references).iter().map(|k| k.hex()).collect::<Vec<_>>(),
        },
    }))
}

/// Write `snap` as `ecad-desktop-<stamp>-<count>-seq<head>-<why>.json` in `dir`, making the folder: the
/// snapshot writer (the app only says where, why and when). Returns the file written.
///
/// # Errors
/// The folder or file could not be written.
pub fn write_snapshot_file(snap: &Value, dir: &std::path::Path, why: &str, count: u32, stamp: u64) -> std::io::Result<std::path::PathBuf> {
    let head = snap.get("seq").and_then(Value::as_u64).unwrap_or(0);
    let path = dir.join(format!("ecad-desktop-{stamp}-{count:02}-seq{head:03}-{why}.json"));
    std::fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(snap).map_err(std::io::Error::other)?;
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use ecad_model::{Author, ConductorId, ConnectionPoint, ConnectionPointId, Device, DeviceId, IdAllocator, NetId, Op, Wire, WireId};

    /// Design 072 §4.2: a wire's net and conductor have labels in the snapshot's index, the net
    /// named after the wire first in its order.
    #[test]
    fn a_wires_net_and_conductor_are_labelled() {
        let mut ids = IdAllocator::seeded(72);
        let mut l = ecad_model::Ledger::new(72);
        let (k, a, b, c): (DeviceId, ConnectionPointId, ConnectionPointId, ConnectionPointId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
        let (w1, w2, net, c1, c2): (WireId, WireId, NetId, ConductorId, ConductorId) =
            (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
        let point = |label: &str| ConnectionPoint { owner: k.into(), label: Some(label.into()), key: None, accepts: Default::default() };
        let number = |n: &str| ecad_model::naming::text::Text::new(n).ok().map(|text| ecad_model::objects::wire::WireNumber { text, fixed: true });
        let ops = vec![
            Op::CreateDevice { id: k, device: Device::default().with_tag(Some("-K1"), ecad_model::naming::tag::TagState::Automatic) },
            Op::AddConnectionPoint { id: a, connection_point: point("1") },
            Op::AddConnectionPoint { id: b, connection_point: point("2") },
            Op::AddConnectionPoint { id: c, connection_point: point("3") },
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
