//! Brief 072 lane E (design 072 §5.1): the page commands through MCP record `ModifyPage` with the
//! agent as author, keep a generated page's kind, and are refused with writes off (brief 011).

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{Author, GenKind, Op, PageId, PageKind};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

fn page_of(v: &serde_json::Value) -> PageId {
    PageId::parse(v["page"].as_str().unwrap()).unwrap()
}

#[test]
fn page_commands_through_mcp_record_modify_page_by_the_agent() {
    let mut c = Client::new(EcadMcp::in_memory("lane072e", 72, Options { write: true, agent: None }).unwrap());
    let page = page_of(&c.exec("newpage", json!({"title": "Supply"})));
    let before = c.log().len();

    // Each of these is one transaction holding one ModifyPage of the page, by the agent.
    let cases: &[(&str, serde_json::Value)] = &[
        ("renamepage", json!({"page": page.hex(), "title": "Main supply"})),
        ("pageprops", json!({"page": page.hex(), "number": "105a"})),
        ("pageprops", json!({"page": page.hex(), "sheet": "A2"})),
        ("setdefault", json!({"key": "page.type", "level": "page", "page": page.hex(), "value": "network"})),
    ];
    for (i, (cmd, params)) in cases.iter().enumerate() {
        c.exec(cmd, params.clone());
        let log = c.log();
        assert_eq!(log.len(), before + i + 1, "{cmd}: one transaction");
        let last = log.last().unwrap();
        assert_eq!(last.author, Author::Agent(AGENT.into()), "{cmd}");
        assert!(matches!(last.ops.as_slice(), [Op::ModifyPage { id, .. }] if *id == page), "{cmd}: {:?}", last.ops);
    }
    let d = c.doc();
    let p = &d.pages[&page];
    assert_eq!(p.title, "Main supply");
    assert_eq!(p.number.as_ref().map(ecad_model::PageNo::as_str), Some("105a"));
    assert_eq!(p.sheet.map(|s| s.text()).as_deref(), Some("A2"));
    assert_eq!(p.page_type.as_ref().map(ecad_model::StyleRef::text).as_deref(), Some("network"));
    assert!(p.display.is_empty(), "no page field is left in the display bag: {:?}", p.display);

    // A generated page's kind is a field, set when it is made.
    let cover = page_of(&c.exec("newpage", json!({"generated": "cover"})));
    assert_eq!(c.doc().pages[&cover].kind, PageKind::Generated(GenKind::Cover));
    // A terminal plan of every strip has no strip; a plan of one strip is its own kind.
    let all = page_of(&c.exec("newpage", json!({"generated": "terminals"})));
    assert_eq!(c.doc().pages[&all].kind, PageKind::Generated(GenKind::Terminals));
    c.exec("openpage", json!({"page": page.hex()}));
    for x in [100, 120] {
        c.exec("wire", json!({"points": [[x, 200], [x, 100]]}));
    }
    let f = c.exec("terminals", json!({"from": [90, 150], "to": [130, 150]}));
    let strip = f["strip"].as_str().unwrap().to_string();
    let one = page_of(&c.exec("newpage", json!({"generated": "terminals", "strip": strip})));
    let want = ecad_model::DeviceId::parse(&strip).unwrap();
    assert_eq!(c.doc().pages[&one].kind, PageKind::TerminalStrip { strip: want });

    let mut r = reader();
    for (cmd, params) in cases {
        r.refused("execute", json!({"command": cmd, "params": params}));
    }
}
