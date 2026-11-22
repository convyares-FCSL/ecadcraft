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
    let p = &d.pages()[&page];
    assert_eq!(p.title, "Main supply");
    assert_eq!(p.number.as_ref().map(ecad_model::PageNo::as_str), Some("105a"));
    assert_eq!(p.sheet.map(|s| s.text()).as_deref(), Some("A2"));
    assert_eq!(p.page_type.as_ref().map(ecad_model::StyleRef::text).as_deref(), Some("network"));
    assert!(p.display.is_empty(), "no page field is left in the display bag: {:?}", p.display);

    // A generated page's kind is a field, set when it is made.
    let cover = page_of(&c.exec("newpage", json!({"generated": "cover"})));
    assert_eq!(c.doc().pages()[&cover].kind, PageKind::Generated(GenKind::Cover));
    // A terminal plan of every strip has no strip; a plan of one strip is its own kind.
    let all = page_of(&c.exec("newpage", json!({"generated": "terminal_blocks"})));
    assert_eq!(c.doc().pages()[&all].kind, PageKind::Generated(GenKind::TerminalBlocks));
    c.exec("openpage", json!({"page": page.hex()}));
    for x in [100, 120] {
        c.exec("wire", json!({"points": [[x, 200], [x, 100]]}));
    }
    let f = c.exec("terminalblocks", json!({"from": [90, 150], "to": [130, 150]}));
    let strip = f["strip"].as_str().unwrap().to_string();
    let one = page_of(&c.exec("newpage", json!({"generated": "terminal_blocks", "strip": strip})));
    let want = ecad_model::DeviceId::parse(&strip).unwrap();
    assert_eq!(c.doc().pages()[&one].kind, PageKind::TerminalStrip { strip: want });

    let mut r = reader();
    for (cmd, params) in cases {
        r.refused("execute", json!({"command": cmd, "params": params}));
    }
}

/// Step 2 (design 072 §5.3): the list commands and a page's or the project's location record
/// `ModifyProject` (and `ModifyPage`) by the agent, and are refused with writes off.
#[test]
fn list_commands_through_mcp_record_modify_project_by_the_agent() {
    let mut c = Client::new(EcadMcp::in_memory("lane072e2", 72, Options { write: true, agent: None }).unwrap());
    let page = page_of(&c.exec("newpage", json!({"title": "Supply"})));
    let agent = Author::Agent(AGENT.into());

    c.exec("designationadd", json!({"aspect": "location", "code": "+CP1", "description": "Main panel"}));
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, agent);
    assert!(matches!(last.ops.as_slice(), [Op::ModifyProject { .. }]), "{:?}", last.ops);
    let entry = c.doc().project().locations[0].clone();
    assert_eq!(entry.code, "+CP1");
    assert_eq!(entry.description().map(ToString::to_string).as_deref(), Some("Main panel"));

    c.exec("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP2"}));
    assert!(matches!(c.log().last().unwrap().ops.as_slice(), [Op::ModifyProject { .. }]));
    assert_eq!(c.doc().project().locations[0].code, "+CP2");

    // A page's location: a new code adds its entry (ModifyProject) and the page names it by id.
    c.exec("pageprops", json!({"page": page.hex(), "location": "+CP7"}));
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, agent);
    assert!(matches!(last.ops.as_slice(), [Op::ModifyProject { .. }, Op::ModifyPage { .. }]), "{:?}", last.ops);
    let d = c.doc();
    let cp7 = d.project().locations.iter().find(|e| e.code == "+CP7").map(|e| e.id);
    assert_eq!(d.pages()[&page].location, cp7);
    assert!(d.pages()[&page].display.is_empty());

    // The project's own function: a field of the project.
    c.exec("projectprops", json!({"function": "=F1"}));
    let last = c.log().last().cloned().unwrap();
    assert!(last.ops.iter().all(|o| matches!(o, Op::ModifyProject { .. })), "{:?}", last.ops);
    let d = c.doc();
    assert_eq!(d.project().function.and_then(|f| d.project().function(f)).map(|e| e.code.to_string()).as_deref(), Some("=F1"));
    assert!(!d.display().contains_key("function"));

    // Step 2c: the title and the site are the project's info.
    c.exec("projectprops", json!({"title": "Plant", "props": {"site": "North"}}));
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, agent);
    assert!(last.ops.iter().all(|o| matches!(o, Op::ModifyProject { .. })), "{:?}", last.ops);
    let d = c.doc();
    assert_eq!(d.project().info.get(ecad_model::InfoField::Title), Some("Plant"));
    assert_eq!(d.project().info.get(ecad_model::InfoField::Site), Some("North"));
    assert!(!d.display().contains_key("title") && !d.display().contains_key("site"));

    let mut r = reader();
    for (cmd, params) in [
        ("projectprops", json!({"title": "Other"})),
        ("designationadd", json!({"aspect": "function", "code": "=F9"})),
        ("pageprops", json!({"page": page.hex(), "location": "+CP8"})),
        ("projectprops", json!({"location": "+CP9"})),
    ] {
        r.refused("execute", json!({"command": cmd, "params": params}));
    }
}
