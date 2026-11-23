//! MCP contract: placing each new symbol
//! of the component library through `execute` records one transaction authored by the agent,
//! embedding its definition once; a change-over join and ratings are recorded the same way; every
//! one is refused with writes off; hostile keys and ratings get an error, never a panic.

use ecad_electrical::symbols::{Family, STARTER_LIBRARY, StarterKey};

use ecad_electrical::symbols::catalogue::STARTERS;
use ecad_mcp::policy;
use ecad_model::Op;
use serde_json::{Value, json};

use crate::common::{assert_one_by_agent, writer};
use crate::mcp_contracts::reader;

const WRITER_SEED: u64 = 34;

fn labels(v: &Value) -> Vec<String> {
    v["connection_points"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap().to_string()).collect()
}

fn embeds(ops: &[Op]) -> Vec<&ecad_model::Definition> {
    ops.iter()
        .filter_map(|o| match o {
            Op::EmbedDefinition { definition, .. } => Some(definition),
            _ => None,
        })
        .collect()
}

#[test]
fn each_new_symbol_places_as_the_agent_and_reuses_its_definition() {
    let mut c = writer(WRITER_SEED);
    let mut next: std::collections::BTreeMap<&str, u32> = Default::default();
    // Brief 045's channel cells join cards (`plc_contracts`).
    for (i, s) in STARTERS.iter().filter(|s| s.family != Family::Basic && ecad_electrical::plc::channels::cell_type(s).is_none()).enumerate() {
        let at = json!([80.0 * (i % 8) as f64, -80.0 * (i / 8) as f64]);
        let n = c.log().len();
        let r = c.exec("place", json!({"symbol": s.key, "at": at}));
        let tx = assert_one_by_agent(&mut c, n, "write");
        let e = embeds(&tx.ops);
        assert_eq!(e.len(), 1, "{}", s.key);
        let o = e[0].origin.clone().unwrap();
        assert_eq!((o.library, o.definition), (STARTER_LIBRARY, s.id), "{}", s.key);
        let k = next.entry(s.prefix()).or_insert(0);
        *k += 1;
        let tag = format!("-{}{:02}", s.prefix(), k);
        assert_eq!(r["tag"], json!(tag), "{}", s.key);
        // Brief 044 (design 044 §4.6): a connector half's description is the registry's
        // `Connector` (each half's BOM description is its own); a pin's label is its pin name.
        let connector = matches!(s.kind, ecad_electrical::symbols::Kind::ConnectorHalf { .. });
        if !matches!(s.key, StarterKey::Pinpair | StarterKey::Pins | StarterKey::Pinp) {
            assert_eq!(labels(&r), s.labels(), "{}", s.key);
        }
        let cid: ecad_model::DeviceId = u128::from_str_radix(r["component"].as_str().unwrap(), 16).unwrap().into();
        let doc = c.doc();
        let want = if connector { "Connector" } else { s.name };
        assert_eq!(doc.devices()[&cid].get("description"), Some(want), "{}: description", s.key);
        // Again: no embed.
        let n = c.log().len();
        c.exec("place", json!({"symbol": s.key, "at": [at[0].as_f64().unwrap(), at[1].as_f64().unwrap() - 40.0]}));
        *next.get_mut(s.prefix()).unwrap() += 1;
        let tx = assert_one_by_agent(&mut c, n, "write");
        assert!(embeds(&tx.ops).is_empty(), "{}: reused by origin", s.key);
    }
}

#[test]
fn a_change_over_join_and_ratings_record_as_the_agent() {
    let mut c = writer(WRITER_SEED);
    let k1 = c.exec("place", json!({"symbol": "coil", "at": [0, 0]}))["component"].as_str().unwrap().to_string();
    for (key, want) in [("co", vec!["11", "12", "14"]), ("no", vec!["23", "24"]), ("co", vec!["31", "32", "34"])] {
        let n = c.log().len();
        let r = c.exec("place", json!({"symbol": key, "at": [40, 0], "device": k1}));
        assert_one_by_agent(&mut c, n, "write");
        assert_eq!(labels(&r), want, "{key}");
    }
    let f = c.exec("place", json!({"symbol": "fusebank8", "at": [0, -100]}))["component"].as_str().unwrap().to_string();
    let n = c.log().len();
    c.exec("setprop", json!({"component": f, "key": "rating.3", "value": "2A"}));
    let tx = assert_one_by_agent(&mut c, n, "write");
    assert!(tx.ops.iter().any(|o| matches!(o, Op::ModifyDevice { after, .. } if after.value("rating.3").as_deref() == Some("2 A"))));
    let before = c.log();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"component": f, "key": "rating.13", "value": "2A"}}));
    assert!(m.contains("-F01 has no fuse 13."), "{m}");
    assert_eq!(c.log(), before, "head unchanged");
    let q = c.exec("place", json!({"symbol": "rcbo2", "at": [0, -200]}))["component"].as_str().unwrap().to_string();
    let n = c.log().len();
    c.exec("setprop", json!({"component": q, "key": "rating", "value": "13A C 30mA"}));
    let tx = assert_one_by_agent(&mut c, n, "write");
    assert!(tx.ops.iter().any(|o| matches!(o, Op::ModifyDevice { after, .. } if after.value("rating").as_deref() == Some("curve C, 13 A, 30 mA"))));
}

#[test]
fn with_writes_off_placing_joining_and_ratings_are_refused() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [
        ("place", json!({"symbol": "co", "at": [0, 0]})),
        ("place", json!({"symbol": "fusebank8", "at": [0, 0]})),
        ("place", json!({"symbol": "co", "at": [0, 0], "device": "1"})),
        ("setprop", json!({"component": "1", "key": "rating", "value": "6A"})),
        ("setprop", json!({"component": "1", "key": "rating.3", "value": "2A"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    assert_eq!(c.log(), before, "head unchanged");
}

#[test]
fn hostile_keys_and_ratings_get_an_error_never_a_panic() {
    let mut c = writer(WRITER_SEED);
    let f = c.exec("place", json!({"symbol": "fusebank4", "at": [0, 0]}))["component"].as_str().unwrap().to_string();
    let before = c.log();
    for (cmd, params) in [
        ("place", json!({"symbol": "nonsense", "at": [0, 0]})),
        ("place", json!({"symbol": "fusebank99999999999999999999", "at": [0, 0]})),
        ("setprop", json!({"component": f, "key": "rating", "value": "x".repeat(10_000)})),
        ("setprop", json!({"component": f, "key": "rating", "value": "6A\u{1}"})),
        ("setprop", json!({"component": f, "key": format!("rating.{}", "9".repeat(10_000)), "value": "1A"})),
    ] {
        c.refused("execute", json!({"command": cmd, "params": params}));
    }
    assert_eq!(c.log(), before, "head unchanged");
}
