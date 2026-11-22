//! Brief 045 (PLC racks and cards, design 045): contract tests by part.

mod channels;
mod fixes;
mod iolist;
mod rack;

use ecad_electrical::reports::kept::ReportModels;
use ecad_model::DeviceId;
use serde_json::{Value, json};

use crate::common::{H, id};

/// Place starter `symbol` at `(x, y)` with extra params; its component.
pub(crate) fn place(h: &mut H, symbol: &str, x: f64, y: f64, extra: Value) -> Result<Value, String> {
    let mut p = json!({"symbol": symbol, "at": [x, y]});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    h.run("place", p)
}

pub(crate) fn placed(h: &mut H, symbol: &str, x: f64, y: f64) -> DeviceId {
    let v = place(h, symbol, x, y, json!({})).unwrap_or_else(|e| panic!("place {symbol}: {e}"));
    id(&v, "component")
}

pub(crate) fn prop(h: &mut H, c: DeviceId, k: &str) -> Option<String> {
    crate::common::plc_value(&h.doc(), c, k)
}

pub(crate) fn tag(h: &mut H, c: DeviceId) -> String {
    prop(h, c, "tag").unwrap_or_default()
}

/// The kept PLC model equals a rebuild (verify-mode equality, design 045 §11.2).
pub(crate) fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.reports().plc.clone();
        (kept, ReportModels::build_with(p.ledger.document(), &ecad_engine::styles::draw::EngineMetrics).plc.clone())
    });
    assert!(kept == built, "the kept PLC model differs from a rebuild");
}

pub(crate) fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind]})).unwrap()["checks"].as_array().cloned().unwrap_or_default()
}
