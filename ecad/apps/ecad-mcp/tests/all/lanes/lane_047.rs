//! Brief 047 through MCP (brief 011 rule, design 047 §7.5): `partline` (add, kit, edit, remove),
//! `bomitem` (add, edit, delete) and `article` each record one transaction by
//! `Author::Agent(<client>)` and are refused with writes off; a `for` naming a fuse the bank lacks
//! is refused in words and writes nothing; `bom` works with writes off and records nothing;
//! hostile values are errors in words.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client};

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("mcp 047", 47, Options { write: true, agent: None }).unwrap())
}

/// One transaction since `n`, by the agent, labelled `label`.
fn one_by_agent(c: &mut Client, n: usize, label: &str) {
    let log = c.log();
    assert_eq!(log.len(), n + 1, "one transaction for {label}");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    assert_eq!(t.label, label);
}

fn component(v: &Value) -> String {
    v["component"].as_str().unwrap().to_string()
}

#[test]
fn partline_bomitem_and_article_by_the_agent() {
    let mut c = writer();
    let k = component(&c.exec("place", json!({"symbol": "coil", "at": [100, 100]})));
    let f = component(&c.exec("place", json!({"symbol": "fusebank8", "at": [200, 100]})));

    let n = c.log().len();
    let v = c.exec("partline", json!({"component": k, "add": {"part_number": "AB-1NO", "maker": "Generic", "description": "Aux block"}}));
    assert_eq!(v["lines"], json!([1]));
    one_by_agent(&mut c, n, "Add part line");

    let n = c.log().len();
    c.exec("partline", json!({"component": k, "kit": "Starter set", "add": [{"part_number": "SUP-1"}, {"part_number": "LBL-1"}]}));
    one_by_agent(&mut c, n, "Add kit Starter set");

    let n = c.log().len();
    c.exec("partline", json!({"component": k, "line": 2, "set": {"qty": 2}}));
    one_by_agent(&mut c, n, "Edit part line");

    let n = c.log().len();
    c.exec("partline", json!({"component": k, "line": 3, "remove": true}));
    one_by_agent(&mut c, n, "Remove part line");

    // A fuse the bank lacks: refused in words, nothing written.
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": "partline", "params": {"component": f, "add": {"part_number": "FU", "for": "13"}}}));
    assert!(m.contains("has no fuse 13. Its fuses are 1-8."), "{m}");
    assert_eq!(c.log().len(), n);

    let n = c.log().len();
    let item = c.exec("bomitem", json!({"description": "DIN rail 35x7.5", "qty": 2, "unit": "m"}))["item"].as_str().unwrap().to_string();
    one_by_agent(&mut c, n, "Add item");
    let n = c.log().len();
    c.exec("bomitem", json!({"item": item, "set": {"part_number": "DR-35"}}));
    one_by_agent(&mut c, n, "Edit item");

    let n = c.log().len();
    let v = c.exec("article", json!({"part_number": "AB-1NO", "article_number": "A-0110"}));
    assert_eq!(v["message"], "Set A-0110 on 1 part (1 line).");
    one_by_agent(&mut c, n, "Set article number");

    // `bom` reads the new fields, with the article column for this call only.
    let n = c.log().len();
    let rows = c.exec("bom", json!({"columns": ["article"]}))["rows"].as_array().unwrap().clone();
    let aux = rows.iter().find(|r| r["part_number"] == "AB-1NO").unwrap();
    assert_eq!(aux["article_number"], "A-0110");
    let rail = rows.iter().find(|r| r["part_number"] == "DR-35").unwrap();
    assert_eq!((rail["qty"].clone(), rail["unit"].clone(), rail["entries"][0]["item"].clone()), (json!(2), json!("m"), json!(true)));
    assert_eq!(c.log().len(), n, "reading records nothing");

    let n = c.log().len();
    c.exec("bomitem", json!({"item": item, "delete": true}));
    one_by_agent(&mut c, n, "Delete item");

    // Hostile values: errors in words, never a panic, nothing written.
    let n = c.log().len();
    for q in [json!("NaN"), json!(-1), json!(1e308), json!("0.0001"), json!("9".repeat(10_000))] {
        let m = c.refused("execute", json!({"command": "partline", "params": {"component": k, "add": {"part_number": "P", "qty": q}}}));
        assert!(m.contains("quantity") || m.contains("decimals"), "{m}");
    }
    let m = c.refused("execute", json!({"command": "partline", "params": {"component": f, "add": {"part_number": "P", "for": "1-99999"}}}));
    assert!(m.contains("at most 999"), "{m}");
    for key in ["part.0.maker", "part.007.maker", "part.100.maker"] {
        let m = c.refused("execute", json!({"command": "setprop", "params": {"component": k, "key": key, "value": "X"}}));
        assert!(m.contains("is not a part line key"), "{key}: {m}");
    }
    assert_eq!(c.log().len(), n);
}

#[test]
fn writes_are_refused_with_writes_off_and_bom_still_reads() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let any = "0123456789abcdef0123456789abcdef";
    for (command, params) in [
        ("partline", json!({"component": any, "add": {"part_number": "AB-1NO"}})),
        ("bomitem", json!({"description": "DIN rail"})),
        ("article", json!({"part_number": "AB-1NO", "article_number": "A-0110"})),
    ] {
        let m = r.refused("execute", json!({"command": command, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{command}");
    }
    let v = r.exec("bom", json!({"columns": ["article"], "lines": "nested"}));
    assert!(v["rows"].is_array(), "{v}");
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}
