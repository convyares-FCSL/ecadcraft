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

/// Brief 022 (design 022 §14): the demo's nets and wire numbers, through devices, cables, cable
/// references, continuations and the junction, and its Checks list.
#[test]
fn the_demo_nets_and_wire_numbers() {
    let mut mcp = EcadMcp::in_memory("demo", 0xDE30, Options { write: true, agent: None }).unwrap();
    demo::run(&mut mcp, demo::COMMANDS).unwrap_or_else(|e| panic!("{e}"));
    let doc = mcp.with_session(|s, p| p.read(s, |p, _| p.ledger.document().clone()).unwrap()).unwrap();
    // Every conductor's label, by page, as the pages show them.
    let mut shown: Vec<String> = Vec::new();
    for (i, page) in doc.page_order.iter().enumerate() {
        let mut labels: Vec<String> = ecad_cadcraft::wires::labels_on_page(&doc, *page)
            .into_iter()
            .map(|l| format!("{}: {}", i + 1, l.text))
            .collect();
        labels.sort();
        labels.dedup();
        shown.extend(labels);
    }
    let checks: Vec<String> = ecad_cadcraft::checks::all(&doc, 1e-9)
        .iter()
        .map(|c| format!("{:?} {} {}", c.kind, c.locator(&doc), c.which))
        .collect();
    // Recorded from the build (design 022 §14 asks for the actual list). Page 1: L and N on each
    // side of -Q01, PE, and the PSU outputs through continuations 24V and 0V. Page 2: 24V-01 from
    // the continuation, the control nets, 0V-01 through the junction, L1-L3 above the contactor
    // (continuations) and below it (the cable cores show the number only). Page 3: the same 24V-01,
    // 0V-01 and core numbers, the NC-to-lamp wire 24V-05.
    let want = [
        "1: 0V-01 0.5 Blue",
        "1: 24V-01 0.5 Blue",
        "1: L-01 2.5 Brown",
        "1: L-02 2.5 Brown",
        "1: N-01 2.5 Blue",
        "1: N-02 2.5 Blue",
        "1: PE 2.5 Green-Yellow",
        "2: 0V-01 0.5 Blue",
        "2: 24V-01 0.5 Blue",
        "2: 24V-02 0.5 Blue",
        "2: 24V-03 0.5 Blue",
        "2: 24V-04 0.5 Blue",
        "2: L1-01 2.5 Brown",
        "2: L1-02",
        "2: L2-01 2.5 Black",
        "2: L2-02",
        "2: L3-01 2.5 Grey",
        "2: L3-02",
        "3: 0V-01 0.5 Blue",
        "3: 24V-01 0.5 Blue",
        "3: 24V-05 0.5 Blue",
        "3: L1-02",
        "3: L1-02 2.5 Brown",
        "3: L2-02",
        "3: L2-02 2.5 Black",
        "3: L3-02",
        "3: L3-02 2.5 Grey",
    ];
    assert_eq!(shown, want, "{shown:#?}");
    // 24V runs over pages 1-3 through continuations; L1-02 through the core and -X02.
    let conn = doc.connectivity();
    let net = |n: &str| conn.nets_numbered(n).map(|(id, _)| id).collect::<Vec<_>>();
    assert_eq!(net("24V-01").len(), 1);
    assert_eq!(net("L1-02").len(), 1);
    let v24 = conn.net(net("24V-01")[0]).unwrap();
    assert_eq!(v24.names.iter().cloned().collect::<Vec<_>>(), ["24V"]);
    // The coil, both contacts' 14 side and the lamp share 24V-04 (design 022 §15 step 1).
    let k04 = conn.net(net("24V-04")[0]).unwrap();
    let points = ecad_cadcraft::wires::net_points(&doc, k04);
    assert_eq!(points, ["-H01:X1", "-K01:14", "-K01:A1", "-S02:14"], "{points:?}");
    // Checks: -M01:PE, and the three L continuations whose partners are not drawn.
    assert_eq!(
        checks,
        [
            "LonelyContinuation /2.3 L1",
            "LonelyContinuation /2.4 L2",
            "LonelyContinuation /2.4 L3",
            "Unconnected /3.4 -M01:PE"
        ],
        "{checks:#?}"
    );
}
