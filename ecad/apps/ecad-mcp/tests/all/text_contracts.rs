//! Brief 031, MCP contract (the ECAD rule of brief 011, design 031 §10.7): `textstyle` and
//! `defaults` through `execute`: each write records one transaction by `Author::Agent(<client>)`
//! and is refused with writes off; the reads answer with writes off and record nothing; the
//! property commands refuse `text.` keys.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

const SEED: u64 = 0x0E_CAD0_0031;

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("text", SEED, Options { write: true, agent: None }).unwrap())
}

/// Run a write and check it recorded exactly one transaction by the agent; returns the reply.
fn one_by_agent(c: &mut Client, command: &str, params: Value) -> Value {
    let before = c.log().len();
    let r = c.exec(command, params.clone());
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{command} {params}: {r}");
    assert_eq!(log.last().unwrap().author, Author::Agent(AGENT.into()));
    r
}

#[test]
fn textstyle_writes_one_transaction_by_the_agent_and_reads_with_writes_off() {
    let mut c = writer();
    let r = one_by_agent(&mut c, "textstyle", json!({"kind": "tag", "height": 3}));
    assert_eq!(r["changed"], json!(true));
    let tag = r["kinds"].as_array().unwrap().iter().find(|k| k["kind"] == "tag").unwrap().clone();
    assert_eq!((tag["height"].as_f64(), tag["weight"].as_str(), tag["default"].as_bool()), (Some(3.0), Some("bold"), Some(false)), "{tag}");
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.label, "Text styles");
    assert!(
        matches!(last.ops.as_slice(), [Op::SetProperty { object: ObjectRef::Project, key, after: Some(v), .. }] if key == "text.tag.height" && v == "3"),
        "{:?}",
        last.ops
    );
    // The font, and the wire label height in the wiring setting (one stored value, §3.4).
    one_by_agent(&mut c, "textstyle", json!({"font": "X"}));
    assert_eq!(c.doc().properties.get("text.font").map(String::as_str), Some("X"));
    one_by_agent(&mut c, "textstyle", json!({"kind": "wire_label", "height": 2}));
    assert!(matches!(c.log().last().unwrap().ops.as_slice(), [Op::SetProjectSettings { .. }]));
    assert_eq!(c.doc().settings.wiring.label_height, 2.0);
    assert!(!c.doc().properties.contains_key("text.wire_label.height"));
    // Several fields: one transaction.
    one_by_agent(&mut c, "textstyle", json!({"kind": "tag", "height": null, "weight": "regular"}));
    // Several kinds (the Text section's OK): one transaction.
    one_by_agent(&mut c, "textstyle", json!({"kinds": [{"kind": "point_label", "height": 2}, {"kind": "location", "weight": "bold"}]}));
    let d = c.doc();
    assert_eq!(
        (d.properties.get("text.point_label.height").map(String::as_str), d.properties.get("text.location.weight").map(String::as_str)),
        (Some("2"), Some("bold"))
    );
    let m = c.refused("execute", json!({"command": "textstyle", "params": {"kinds": [{"height": 2}]}}));
    assert!(m.contains("height, weight and show need a kind."), "{m}");
    // The same value again writes nothing.
    let n = c.log().len();
    assert_eq!(c.exec("textstyle", json!({"kind": "tag", "weight": "regular"}))["changed"], json!(false));
    assert_eq!(c.log().len(), n);
    // The read form records nothing.
    let read = c.exec("textstyle", json!({}));
    assert_eq!(read["font"], json!("X"));
    assert!(read["drawn_with"].is_string(), "{read}");
    assert_eq!(c.log().len(), n);

    let mut r = reader();
    for p in [json!({"kind": "tag", "height": 3}), json!({"font": "X"}), json!({"kind": "wire_label", "height": 2})] {
        r.refused("execute", json!({"command": "textstyle", "params": p}));
    }
    let n = r.log().len();
    let read = r.exec("textstyle", json!({}));
    let tag = read["kinds"].as_array().unwrap().iter().find(|k| k["kind"] == "tag").unwrap().clone();
    assert_eq!((tag["height"].as_f64(), tag["default"].as_bool()), (Some(2.5), Some(true)), "{read}");
    assert_eq!(r.log().len(), n);
}

#[test]
fn textstyle_refusals_write_nothing() {
    let mut c = writer();
    let n = c.log().len();
    for (p, words) in [
        (json!({"kind": "tag", "height": 0.1}), "A text height is 0.5 to 10 mm."),
        (json!({"kind": "tag", "height": 11}), "A text height is 0.5 to 10 mm."),
        (json!({"kind": "tag", "height": "big"}), "A text height is 0.5 to 10 mm."),
        (json!({"kind": "nope", "height": 3}), "Unknown text kind \"nope\". Kinds: tag, point_label"),
        (json!({"kind": "tag", "weight": "heavy"}), "Weight is regular or bold."),
        (json!({"kind": "tag", "show": "maybe"}), "Shown is on, off or first (first: tag only)."),
        (json!({"kind": "description", "show": "first"}), "Shown is on, off or first (first: tag only)."),
        (json!({"font": ""}), "A font name is 1 to 64 characters with no control characters."),
        (json!({"font": "a".repeat(65)}), "A font name is 1 to 64 characters with no control characters."),
        (json!({"font": "a\u{7}b"}), "A font name is 1 to 64 characters with no control characters."),
        (json!({"height": 3}), "height, weight and show need a kind."),
    ] {
        let m = c.refused("execute", json!({"command": "textstyle", "params": p}));
        assert!(m.contains(words), "{p}: {m}");
    }
    assert_eq!(c.log().len(), n);
}

#[test]
fn the_property_commands_refuse_text_keys() {
    let mut c = writer();
    let page = c.doc().page_order[0].hex();
    let comp = c.exec("place", json!({"symbol": "coil", "at": [100, 100]}))["component"].as_str().unwrap().to_string();
    let n = c.log().len();
    for (cmd, p) in [
        ("projectprops", json!({"props": {"text.font": "Arial"}})),
        ("pageprops", json!({"page": page, "props": {"text.tag.show": "off"}})),
        ("setprop", json!({"component": comp, "key": "text.tag.show", "value": "off"})),
        ("setprop", json!({"component": comp, "key": "TEXT.tag.show", "value": "off"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": p}));
        assert!(m.contains("is set with Text styles (textstyle, showtext, movetext)."), "{cmd}: {m}");
    }
    assert_eq!(c.log().len(), n);
}

#[test]
fn defaults_lists_the_registry_with_writes_off() {
    let mut r = reader();
    let n = r.log().len();
    let all = r.exec("defaults", json!({}));
    let rows = all["defaults"].as_array().unwrap();
    assert!(rows.iter().any(|d| d["key"] == "text.tag.height"), "{all}");
    let page = r.doc().page_order[0].hex();
    let text = r.exec("defaults", json!({"group": "Text", "page": page}));
    for d in text["defaults"].as_array().unwrap() {
        assert_eq!(d["from"], json!("builtin"), "{d}");
        assert_eq!(d["value"], d["builtin"], "{d}");
    }
    let one = r.exec("defaults", json!({"key": "text.tag.height"}));
    assert_eq!(
        one["defaults"],
        json!([{
            "key": "text.tag.height", "label": "Tag height", "group": "Text", "brief": "031", "kind": "length",
            "builtin": 2.5, "levels": ["standard", "project"], "value": 2.5, "from": "builtin",
        }])
    );
    let m = r.refused("execute", json!({"command": "defaults", "params": {"key": "text.nope"}}));
    assert!(m.contains("No default \"text.nope\"."), "{m}");
    assert_eq!(r.log().len(), n);

    // After a project change, the value and where it comes from follow.
    let mut c = writer();
    c.exec("textstyle", json!({"kind": "tag", "height": 3}));
    let one = c.exec("defaults", json!({"key": "text.tag.height"}));
    assert_eq!((one["defaults"][0]["value"].as_f64(), one["defaults"][0]["from"].as_str()), (Some(3.0), Some("project")));
}

#[test]
fn showtext_at_each_level_is_one_transaction_by_the_agent_and_refused_with_writes_off() {
    let mut c = writer();
    let page = c.doc().page_order[0].hex();
    let v = c.exec("place", json!({"symbol": "coil", "at": [100, 100]}));
    let comp = v["component"].as_str().unwrap().to_string();
    let rep = c
        .doc()
        .representations
        .iter()
        .find(|(_, r)| matches!(r.subject, ecad_model::Subject::Component(x) if x.hex() == comp))
        .map(|(id, _)| id.hex())
        .unwrap();
    for (p, level) in [
        (json!({"kind": "tag", "show": "off"}), "project"),
        (json!({"kind": "tag", "show": "on", "page": page}), "page"),
        (json!({"kind": "tag", "show": "off", "component": comp}), "device"),
        (json!({"kind": "tag", "show": "on", "representation": rep}), "symbol"),
    ] {
        let r = one_by_agent(&mut c, "showtext", p.clone());
        assert_eq!((r["level"].as_str(), r["changed"].as_bool()), (Some(level), Some(true)), "{p}: {r}");
        let mut w = reader();
        let m = w.refused("execute", json!({"command": "showtext", "params": {"kind": "tag", "show": "off"}}));
        assert!(!m.is_empty());
    }
    // Reads with writes off: the page's texts and the display toggle record nothing.
    let mut r = reader();
    let n = r.log().len();
    let t = r.exec("pagetexts", json!({}));
    assert!(t["texts"].is_array(), "{t}");
    let h = r.exec("hiddentext", json!({"on": true}));
    assert_eq!(h["on"], json!(true));
    assert_eq!(r.log().len(), n);
    // The writer's page lists its tag as shown on the one symbol.
    let t = c.exec("pagetexts", json!({}));
    let tag = t["texts"].as_array().unwrap().iter().find(|x| x["kind"] == "tag").unwrap().clone();
    assert_eq!((tag["shown"].as_bool(), tag["weight"].as_str(), tag["height"].as_f64()), (Some(true), Some("bold"), Some(2.5)), "{tag}");
}

#[test]
fn text_with_justify_is_one_transaction_by_the_agent_and_refused_with_writes_off() {
    let mut c = writer();
    one_by_agent(&mut c, "text", json!({"at": [100, 100], "text": "X", "height": 2.5, "justify": "MC"}));
    let found = c.doc().items.values().any(|i| {
        matches!(
            &i.kind,
            ecad_model::ItemKind::Primitive(ecad_model::Primitive::Text {
                halign: ecad_model::TextHAlign::Center,
                valign: ecad_model::TextVAlign::Middle,
                ..
            })
        )
    });
    assert!(found);
    let mut r = reader();
    r.refused("execute", json!({"command": "text", "params": {"at": [100, 100], "text": "X", "height": 2.5, "justify": "MC"}}));
}
