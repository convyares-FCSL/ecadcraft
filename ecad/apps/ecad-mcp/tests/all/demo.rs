//! The demo project (`cargo xtask demo`): the checked-in command list builds without a refusal,
//! through the MCP `execute` tool, into the control panel it describes.

use ecad_mcp::{EcadMcp, Options, demo};
use ecad_model::{Author, Subject};

#[test]
fn the_demo_command_list_builds_the_control_panel() {
    let mut mcp = EcadMcp::in_memory("demo", 0xDE30, Options { write: true, agent: None }).unwrap();
    let n = demo::run(&mut mcp, demo::COMMANDS).unwrap_or_else(|e| panic!("{e}"));
    assert!(n > 60, "{n} commands");
    let (doc, log) = mcp
        .with_session(|s, p| p.read(s, |p, _| (p.ledger.document().clone(), p.ledger.log().to_vec())).unwrap())
        .unwrap();
    // Three pages, as named.
    let titles: Vec<&str> = doc.page_order.iter().map(|p| doc.pages[p].title.as_str()).collect();
    assert_eq!(titles, ["Supply", "Motor control", "Field"]);
    // Every transaction after the project's creation is the demo's, as an agent.
    assert!(log.iter().skip(1).all(|t| t.author == Author::Agent(demo::AUTHOR.into())));
    // The devices, by tag.
    let tags: Vec<String> = ecad_cadcraft::devices::devices(&doc, None)
        .into_iter()
        .map(|c| ecad_cadcraft::devices::tag_words(&doc, c))
        .collect();
    assert_eq!(
        tags,
        ["-G01", "-H01", "-H02", "-K01", "-M01", "-Q01", "-Q02", "-S01", "-S02", "-X01", "-X02"],
        "{tags:?}"
    );
    // -K01 is one device across pages 2 and 3: coil, hold-in NO, power contacts, NC.
    let k1 = doc.components_tagged("-K01")[0];
    assert_eq!(doc.representations_of(Subject::Component(k1)).count(), 4);
    assert_eq!(ecad_cadcraft::devices::page_numbers(&doc, k1), [2, 3]);
    assert_eq!(doc.components[&k1].properties.get("part_number").map(String::as_str), Some("CT-9A-24VDC"));
    // Cable -W01 runs from page 2 to page 3 with linked references, three members connected to
    // -X02 on page 3.
    assert_eq!(doc.cables.len(), 1);
    let refs = ecad_cadcraft::references::all(&doc);
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert!(refs.iter().all(|r| !r.orphan), "{refs:?}");
    let x2 = doc.components_tagged("-X02")[0];
    let x2_terminals: Vec<_> = doc.terminals_of(x2).map(|(t, _)| t).collect();
    assert_eq!(x2_terminals.len(), 3);
    for t in x2_terminals {
        assert!(doc.ends_at(t).len() >= 2, "each -X02 terminal has the cable's wire and the motor's");
    }
    // The drawing agrees with the model everywhere.
    assert!(ecad_model::derive::mismatches(&doc, 1e-9).is_empty(), "{:?}", ecad_model::derive::mismatches(&doc, 1e-9));
}
