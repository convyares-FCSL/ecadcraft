//! Brief 046 through MCP (brief 011 rule, design 046 §11 #14): `newpage` with `style` and
//! `setdefault` of `terminal.plan.style` at page level each record one transaction by
//! `Author::Agent(<client>)`; with writes off both are refused and nothing is recorded.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, PageId};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client};

#[test]
fn a_strip_page_by_the_agent_then_refused_with_writes_off() {
    let mut c = Client::new(EcadMcp::in_memory("mcp 046", 46, Options { write: true, agent: None }).unwrap());
    let n = c.log().len();
    let v = c.exec("newpage", json!({"generated": "terminals", "style": "graphic"}));
    let page = PageId::from(u128::from_str_radix(v["page"].as_str().unwrap(), 16).unwrap());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "one transaction");
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())));
    let d = c.doc();
    assert_eq!(d.pages[&page].properties.get("terminal.plan.style").map(String::as_str), Some("graphic"));
    assert_eq!(d.pages[&page].title, "Terminal strips");
    // Back to the table at page level: one more transaction by the agent.
    let v = c.exec("setdefault", json!({"key": "terminal.plan.style", "level": "page", "page": page.hex(), "value": "table"}));
    assert_eq!((v["changed"].clone(), v["from"].clone()), (json!(true), json!("page")), "{v}");
    let log = c.log();
    assert_eq!(log.len(), n + 2);
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())));
    // `style` on another kind: refused, nothing recorded.
    let m = c.refused("execute", json!({"command": "newpage", "params": {"generated": "cover", "style": "graphic"}}));
    assert!(m.contains("`style` is for a terminal plan page"), "{m}");
    assert_eq!(c.log().len(), n + 2);

    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let m = r.refused("execute", json!({"command": "newpage", "params": {"generated": "terminals", "style": "graphic"}}));
    assert_eq!(m, policy::WRITES_OFF);
    let m = r.refused("execute", json!({"command": "setdefault", "params": {"key": "terminal.plan.style", "level": "project", "value": "graphic"}}));
    assert_eq!(m, policy::WRITES_OFF);
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}
