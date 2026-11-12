//! Tester 034 through MCP (brief 011 rule, brief 034 AC 7, design 034 §12), written from the
//! brief and design before the implementation: every new symbol placed by `execute place` records
//! one transaction by `Author::Agent(<client>)` with one `EmbedDefinition` (origin = the starter
//! library and the key's ID), one new device `-<prefix><next>`, the catalogue's labels in order
//! and the name as description; a second placement embeds nothing; a change-over join and ratings
//! are recorded the same way; reopened with writes off every write is refused and nothing is
//! recorded; hostile input is an error, never a panic.

use ecad_cadcraft::symbols::{self, STARTER_LIBRARY};
use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, ComponentId, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

/// (key, prefix, labels in order) for every new key of design 034 §3–§6.
fn table() -> Vec<(String, &'static str, Vec<String>)> {
    let l = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<String>>();
    let n = |k: usize| (1..=k).map(|i| i.to_string()).collect::<Vec<String>>();
    let mut v: Vec<(String, &'static str, Vec<String>)> = vec![
        ("co".into(), "K", l(&["11", "12", "14"])),
        ("swco".into(), "S", l(&["11", "12", "14"])),
        ("power4".into(), "K", n(8)),
        ("rcbo2".into(), "Q", l(&["1", "2", "N1", "N2"])),
        ("rcd2".into(), "Q", l(&["1", "2", "N1", "N2"])),
        ("rcd4".into(), "Q", l(&["1", "2", "3", "4", "5", "6", "N1", "N2"])),
        ("ssrin".into(), "K", l(&["A1", "A2"])),
        ("ssrout".into(), "K", l(&["1", "2"])),
        ("fuseterm".into(), "F", l(&["1", "2"])),
        ("prox3".into(), "B", l(&["1", "3", "4"])),
        ("floatno".into(), "B", l(&["13", "14"])),
        ("floatnc".into(), "B", l(&["11", "12"])),
        ("rollerno".into(), "B", l(&["13", "14"])),
        ("rollernc".into(), "B", l(&["11", "12"])),
        ("interlocknc".into(), "B", l(&["11", "12"])),
        ("interlockno".into(), "B", l(&["13", "14"])),
        ("valve".into(), "MB", l(&["A1", "A2"])),
        ("valve2".into(), "MB", l(&["A1", "A2", "B1", "B2"])),
        ("tc".into(), "B", l(&["+", "-"])),
        ("heater".into(), "E", l(&["1", "2"])),
        ("plccpu".into(), "A", l(&["L+", "M"])),
        ("plcps".into(), "A", l(&["L", "N", "PE", "L+", "M"])),
    ];
    for k in [4, 6, 8, 10, 12] {
        let mut labels = l(&["L+", "M"]);
        labels.extend(n(k));
        v.push((format!("fusebank{k}"), "F", labels));
    }
    for k in [2, 3, 4, 6, 8, 12] {
        v.push((format!("socket{k}"), "X", n(k)));
        v.push((format!("plug{k}"), "X", n(k)));
    }
    for k in [9, 15, 25] {
        v.push((format!("dsub{k}s"), "X", n(k)));
        v.push((format!("dsub{k}p"), "X", n(k)));
    }
    for k in [4, 5, 8] {
        v.push((format!("m12s{k}"), "X", n(k)));
        v.push((format!("m12p{k}"), "X", n(k)));
    }
    let mut rj = n(8);
    rj.push("SH".into());
    v.push(("rj45".into(), "X", rj));
    for k in ["usba", "usbb"] {
        let mut u = n(4);
        u.push("SH".into());
        v.push((k.into(), "X", u));
    }
    for k in ["plcdi", "plcdq", "plcai", "plcao", "plcrtd", "plciol"] {
        v.push((k.into(), "A", l(&["L+", "M"])));
    }
    v
}

/// The command added exactly one transaction, by the agent; its ops.
fn one_by_agent(c: &mut Client, before: usize, what: &str) -> Vec<Op> {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    last.ops
}

fn labels(v: &Value) -> Vec<String> {
    v["terminals"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap().to_string()).collect()
}

fn cid(v: &Value) -> ComponentId {
    ComponentId::from(u128::from_str_radix(v["component"].as_str().unwrap(), 16).unwrap())
}

fn embeds(ops: &[Op]) -> Vec<&Op> {
    ops.iter().filter(|o| matches!(o, Op::EmbedDefinition { .. })).collect()
}

fn sets(ops: &[Op], key: &str, value: &str) -> bool {
    ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Component(_), key: k, after: Some(a), .. } if k == key && a == value))
}

/// AC 7, §12 items 1–4.
#[test]
fn every_family_places_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t034 Bibliothek mit Leerzeichen");
    let path = dir.file("Bauteile -F01 (\u{e4}).ecad");
    let table = table();
    assert_eq!(table.len(), 60);
    let (fb, rcbo, k1) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let mut next: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        let mut fb = None;
        let mut rcbo = None;
        for (i, (key, prefix, want)) in table.iter().enumerate() {
            let s = symbols::by_key(key).unwrap_or_else(|| panic!("{key} not in the catalogue"));
            for round in 0..2 {
                let at = [200 * ((2 * i + round) % 8), 100 * ((2 * i + round) / 8) + 50];
                let n = c.log().len();
                let v = c.exec("place", json!({"symbol": key, "at": at}));
                let ops = one_by_agent(&mut c, n, &format!("place {key} round {round}"));
                let e = embeds(&ops);
                if round == 0 {
                    assert_eq!(e.len(), 1, "{key}: one EmbedDefinition: {ops:?}");
                    let Op::EmbedDefinition { definition, .. } = e[0] else { panic!("{key}: not an embed") };
                    let o = definition.origin.as_ref().expect("origin");
                    assert_eq!((o.library, o.definition), (STARTER_LIBRARY, s.id), "{key}: origin");
                } else {
                    assert!(e.is_empty(), "{key}: a second placement reuses the definition");
                }
                let k = next.entry(*prefix).or_insert(0);
                *k += 1;
                let tag = format!("-{prefix}{:02}", *k);
                assert_eq!(v["tag"], json!(tag), "{key}: {v}");
                assert_eq!(&labels(&v), want, "{key}: labels");
                let props = c.doc().components[&cid(&v)].properties.clone();
                assert_eq!(props.get("description").map(String::as_str), Some(s.name), "{key}: default description");
                assert_eq!(props.get("tag"), Some(&tag), "{key}: tag");
                if round == 0 && key == "fusebank8" {
                    fb = Some(cid(&v));
                }
                if round == 0 && key == "rcbo2" {
                    rcbo = Some(cid(&v));
                }
            }
        }
        let (fb, rcbo) = (fb.unwrap(), rcbo.unwrap());
        let fb_tag = c.doc().components[&fb].properties.get("tag").cloned().unwrap();

        // §12 item 2: change-over join, each by the agent.
        let kv = c.exec("place", json!({"symbol": "coil", "at": [0, 2000]}));
        let k1 = cid(&kv);
        for (key, x, want) in [("co", 30, vec!["11", "12", "14"]), ("no", 60, vec!["23", "24"]), ("co", 90, vec!["31", "32", "34"])] {
            let n = c.log().len();
            let v = c.exec("place", json!({"symbol": key, "at": [x, 2000], "device": k1.hex()}));
            one_by_agent(&mut c, n, &format!("join {key}"));
            assert_eq!(labels(&v), want, "{key}");
        }

        // §12 item 3: ratings.
        let n = c.log().len();
        c.exec("setprop", json!({"component": fb.hex(), "key": "rating.3", "value": "2A"}));
        assert!(sets(&one_by_agent(&mut c, n, "rating.3"), "rating.3", "2A"));
        let n = c.log().len();
        let m = c.refused("execute", json!({"command": "setprop", "params": {"component": fb.hex(), "key": "rating.13", "value": "2A"}}));
        assert!(m.contains(&format!("{fb_tag} has no fuse 13.")), "{m}");
        assert_eq!(c.log().len(), n, "refused: head unchanged");
        let n = c.log().len();
        c.exec("setprop", json!({"component": rcbo.hex(), "key": "rating", "value": "13A C 30mA"}));
        assert!(sets(&one_by_agent(&mut c, n, "rating"), "rating", "13A C 30mA"));

        // §12 item 5: hostile input, each an error, nothing recorded.
        let n = c.log().len();
        for params in [
            json!({"command": "place", "params": {"symbol": "nosuchsymbol", "at": [0, 0]}}),
            json!({"command": "place", "params": {"symbol": "fusebank7", "at": [0, 0]}}),
            json!({"command": "setprop", "params": {"component": rcbo.hex(), "key": "rating", "value": "A".repeat(10_000)}}),
            json!({"command": "setprop", "params": {"component": rcbo.hex(), "key": "rating", "value": "13A\u{7}"}}),
            json!({"command": "setprop", "params": {"component": fb.hex(), "key": format!("rating.{}", "9".repeat(10_000)), "value": "2A"}}),
            json!({"command": "setprop", "params": {"component": fb.hex(), "key": "rating.abc", "value": "2A"}}),
            json!({"command": "setprop", "params": {"component": rcbo.hex(), "key": "rating.1", "value": "2A"}}),
        ] {
            c.refused("execute", params);
        }
        assert_eq!(c.log().len(), n, "hostile input records nothing");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (fb, rcbo, k1)
    };

    // §12 item 4: writes off.
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    let defs = c.doc().definitions.len();
    for (cmd, params) in [
        ("place", json!({"symbol": "fusebank8", "at": [0, 3000]})),
        ("place", json!({"symbol": "dsub25s", "at": [0, 3000]})),
        ("place", json!({"symbol": "co", "at": [120, 2000], "device": k1.hex()})),
        ("setprop", json!({"component": fb.hex(), "key": "rating.4", "value": "4A"})),
        ("setprop", json!({"component": rcbo.hex(), "key": "rating", "value": "16A B 30mA"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    assert_eq!(c.log(), log, "nothing recorded with writes off");
    assert_eq!(c.doc().definitions.len(), defs);
    // Reads still work.
    let v = c.exec("symbols", json!({"query": "protection"}));
    assert!(v["symbols"].as_array().unwrap().iter().any(|s| s["key"] == "fusebank8"), "{v}");
}
