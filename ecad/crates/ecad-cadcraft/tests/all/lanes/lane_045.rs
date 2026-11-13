//! Brief 045 (PLC racks and cards, design 045): contract tests by part.

mod channels;
mod rack;

use ecad_cadcraft::outputs::Outputs;
use ecad_model::ComponentId;
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

pub(crate) fn placed(h: &mut H, symbol: &str, x: f64, y: f64) -> ComponentId {
    let v = place(h, symbol, x, y, json!({})).unwrap_or_else(|e| panic!("place {symbol}: {e}"));
    id(&v, "component")
}

pub(crate) fn prop(h: &mut H, c: ComponentId, k: &str) -> Option<String> {
    h.doc().components.get(&c).and_then(|x| x.properties.get(k).cloned())
}

pub(crate) fn tag(h: &mut H, c: ComponentId) -> String {
    prop(h, c, "tag").unwrap_or_default()
}

/// The kept PLC model equals a rebuild (verify-mode equality, design 045 §11.2).
pub(crate) fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().plc.clone();
        (kept, Outputs::build(p.ledger.document()).plc)
    });
    assert!(kept.same_as(&built), "the kept PLC model differs from a rebuild");
}

pub(crate) fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind]})).unwrap()["checks"].as_array().cloned().unwrap_or_default()
}
