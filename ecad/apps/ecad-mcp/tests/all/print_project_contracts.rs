//! Brief 033, MCP contracts (the ECAD rule of brief 011): the `contacts` reply's `table` (design
//! 033 §4, AC5 and AC6) and `table`'s refusal (§11, AC13), through `execute`; the writes are the
//! agent's transactions and are refused with writes off.

use ecad_mcp::policy;
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(ecad_mcp::EcadMcp::in_memory("mcp", 33, ecad_mcp::Options { write: true, agent: None }).unwrap())
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

/// A coil at `at` with four contacts drawn away from it; its component.
fn coil_with_contacts(c: &mut Client, at: [f64; 2]) -> String {
    let coil = c.exec("place", json!({"symbol": "coil", "at": at}));
    let k = hex(&coil, "component");
    for (i, x) in [200.0, 230.0, 260.0, 290.0].into_iter().enumerate() {
        let symbol = if i % 2 == 0 { "no" } else { "nc" };
        c.exec("place", json!({"symbol": symbol, "at": [x, 200.0], "device": k}));
    }
    k
}

#[test]
fn an_automatic_table_clears_the_title_block() {
    let mut c = writer();
    // Low on the page: the table below the coil would cross the title block (top line 50 mm).
    let k = coil_with_contacts(&mut c, [75.0, 70.0]);
    let t = c.exec("contacts", json!({"component": k}));
    assert_eq!(t["table"]["shown"], json!("right"), "{t}");
    assert_eq!(t["table"]["placement"], json!("auto"), "{t}");
    assert!(t["table"]["bottom"].as_f64().unwrap() >= 52.0 - 1e-6, "{t}");
    // At the right edge there is no room to its right either: lifted.
    let k2 = coil_with_contacts(&mut c, [400.0, 62.0]);
    let t = c.exec("contacts", json!({"component": k2}));
    assert_eq!(t["table"]["shown"], json!("lifted"), "{t}");
    assert!(t["table"]["bottom"].as_f64().unwrap() >= 52.0 - 1e-6, "{t}");
    // High on the page nothing moves.
    let k3 = coil_with_contacts(&mut c, [75.0, 220.0]);
    let t = c.exec("contacts", json!({"component": k3}));
    assert_eq!(t["table"]["shown"], json!("as placed"), "{t}");
    // No checks row for an automatic table.
    let checks = c.exec("checks", json!({"kinds": ["table-over-title"]}));
    assert_eq!(checks["checks"].as_array().map_or(0, Vec::len), 0, "{checks}");
}

#[test]
fn a_table_the_user_placed_stays_and_is_a_check() {
    let mut c = writer();
    let k = coil_with_contacts(&mut c, [75.0, 120.0]);
    // Into the title block, by Move Mirror Table: one transaction by the agent.
    c.exec("mirrormove", json!({"component": k, "at": [75.0, 45.0]}));
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    assert!(matches!(&last.ops[..], [Op::SetProperty { object: ObjectRef::Component(_), key, .. }] if key == "mirror"), "{:?}", last.ops);
    let t = c.exec("contacts", json!({"component": k}));
    assert_eq!(t["table"]["shown"], json!("as placed"), "{t}");
    assert_eq!(t["table"]["top"], json!([75.0, 45.0]), "{t}");
    let checks = c.exec("checks", json!({"kinds": ["table-over-title"]}));
    let rows = checks["checks"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 1, "{checks}");
    let which = rows[0]["which"].as_str().unwrap();
    assert!(which.starts_with("The contact table of ") && which.contains("overlaps the title block on page"), "{which}");
}

#[test]
fn table_is_refused_on_an_ecad_page_in_clean_words() {
    let mut c = writer();
    let before = c.log();
    let m = c.refused("execute", json!({"command": "table", "params": {"at": [100, 100], "rows": 2, "cols": 2}}));
    assert!(m.contains(ecad_cadcraft::convert::TABLE_REFUSED), "{m}");
    assert!(!m.contains("kinds"), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");
    // With writes off it is refused as every write is.
    let mut r = reader();
    let m = r.refused("execute", json!({"command": "table", "params": {}}));
    assert!(m == policy::WRITES_OFF || m.contains(ecad_cadcraft::convert::TABLE_REFUSED), "{m}");
}

#[test]
fn with_writes_off_the_move_is_refused_and_contacts_reads() {
    let mut c = reader();
    let before = c.log();
    let m = c.refused("execute", json!({"command": "mirrormove", "params": {"component": "1", "at": [0, 0]}}));
    assert_eq!(m, policy::WRITES_OFF);
    assert_eq!(c.log(), before);
}

#[test]
fn continuation_sheets_are_numbered_taken_and_printed_whole() {
    let mut c = writer();
    let contents = c.exec("newpage", json!({"generated": "contents", "number": "C10"}));
    let page = hex(&contents, "page");
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    for _ in 0..45 {
        c.exec("newpage", json!({}));
    }
    let v = c.exec("pageprops", json!({"page": page}));
    assert_eq!(v["sheet_numbers"], json!(["C10", "C11"]), "{v}");
    // Typing a continuation number is refused in words, and nothing is recorded.
    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "newpage", "params": {"number": "c11"}}));
    assert!(m.contains("Number c11 is sheet 2 of page C10 (Contents)."), "{m}");
    let first = c.doc().page_order[0].hex();
    let m = c.refused("execute", json!({"command": "pageprops", "params": {"page": first, "number": "C11"}}));
    assert!(m.contains("Number C11 is sheet 2 of page C10 (Contents)."), "{m}");
    assert_eq!(c.log().len(), before);
    // Print Book's Pages: the continuation number prints its whole page, with a note.
    let p = c.exec("printbook", json!({"scope": {"pages": "C11"}}));
    assert_eq!(p["sheets"], json!(2), "{p}");
    assert!(p["notes"].as_array().unwrap().iter().any(|n| n == "C11 is sheet 2 of page C10; the whole page is printed."), "{p}");
    // A page numbered C12 while it is free; the contents then grows onto it: a warning, not a refusal.
    c.exec("pageprops", json!({"page": first, "number": "C12"}));
    for _ in 0..44 {
        c.exec("newpage", json!({}));
    }
    let v = c.exec("pageprops", json!({"page": page}));
    assert_eq!(v["sheet_numbers"], json!(["C10", "C11", "C12"]), "{v}");
    let checks = c.exec("checks", json!({"kinds": ["continuation-clash"]}));
    let rows = checks["checks"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 1, "{checks}");
    assert_eq!(rows[0]["which"], json!("Page C10 (Contents) now runs to C12, and page C12 has that number."), "{checks}");
}

#[test]
fn with_writes_off_newpage_and_pageprops_are_refused() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [("newpage", json!({"number": "401"})), ("pageprops", json!({"number": "401"}))] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(c.log(), before);
}
