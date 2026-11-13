//! The demo project (`cargo xtask demo`): the checked-in command list builds without a refusal,
//! through the MCP `execute` tool, into the control panel it describes.

use ecad_mcp::{EcadMcp, Options, demo};
use ecad_model::{Author, Subject};

#[test]
fn the_demo_command_list_builds_the_control_panel() {
    let mut mcp = EcadMcp::in_memory("demo", 0xDE30, Options { write: true, agent: None }).unwrap();
    let n = demo::run(&mut mcp, demo::COMMANDS).unwrap_or_else(|e| panic!("{e}"));
    assert!(n > 60, "{n} commands");
    let (doc, log) = mcp.with_session(|s, p| p.read(s, |p, _| (p.ledger.document().clone(), p.ledger.full_log().unwrap())).unwrap()).unwrap();
    // Three drawn pages, as named, then the generated revision list and BOM (brief 024).
    let titles: Vec<&str> = doc.page_order.iter().map(|p| doc.pages[p].title.as_str()).collect();
    assert_eq!(titles, ["Supply", "Motor control", "Field", "Revision list", "BOM by location"]);
    // Every transaction after the project's creation is the demo's, as an agent.
    assert!(log.iter().skip(1).all(|t| t.author == Author::Agent(demo::AUTHOR.into())));
    // The devices, by tag.
    let tags: Vec<String> = ecad_cadcraft::devices::devices(&doc, None).into_iter().map(|c| ecad_cadcraft::devices::tag_words(&doc, c)).collect();
    assert_eq!(tags, ["-G01", "-H01", "-H02", "-K01", "-M01", "-Q01", "-Q02", "-S01", "-S02", "-X01", "-X02"], "{tags:?}");
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
        let mut labels: Vec<String> =
            ecad_cadcraft::wires::labels_on_page(&doc, *page).into_iter().map(|l| format!("{}: {}", i + 1, l.text)).collect();
        labels.sort();
        labels.dedup();
        shown.extend(labels);
    }
    let checks: Vec<String> =
        ecad_cadcraft::checks::all(&doc, 1e-9).iter().map(|c| format!("{:?} {} {}", c.kind, c.locator(&doc), c.which)).collect();
    // Recorded from the build (design 022 §14 asks for the actual list). Page 1: L and N on each
    // side of -Q01, PE, and the PSU outputs through continuations 24V and 0V. Page 2: 24V-01 from
    // the continuation, the control nets, 0V-01 through the junction, L1-L3 above the contactor
    // (continuations) and below it (the cable cores show the number only). Page 3: the same 24V-01,
    // 0V-01 and core numbers, the NC-to-lamp wire 24V-05.
    let want = [
        "1: 0V-01 0.5 BU",
        "1: 24V-01 0.5 BU",
        "1: L-01 2.5 BN",
        "1: L-02 2.5 BN",
        "1: N-01 2.5 BU",
        "1: N-02 2.5 BU",
        "1: PE 2.5 GNYE",
        "2: 0V-01 0.5 BU",
        "2: 24V-01 0.5 BU",
        "2: 24V-02 0.5 BU",
        "2: 24V-03 0.5 BU",
        "2: 24V-04 0.5 BU",
        "2: L1-01 2.5 BN",
        "2: L1-02",
        "2: L2-01 2.5 BK",
        "2: L2-02",
        "2: L3-01 2.5 GY",
        "2: L3-02",
        "3: 0V-01 0.5 BU",
        "3: 24V-01 0.5 BU",
        "3: 24V-05 0.5 BU",
        "3: L1-02",
        "3: L1-02 2.5 BN",
        "3: L2-02",
        "3: L2-02 2.5 BK",
        "3: L3-02",
        "3: L3-02 2.5 GY",
    ];
    assert_eq!(shown, want, "{shown:#?}");
    // Owner, 2026-11-09: no two labels touch, on any page (page 2 is the busiest).
    for page in &doc.page_order {
        let labels = ecad_cadcraft::wires::labels_on_page(&doc, *page);
        for (i, x) in labels.iter().enumerate() {
            for y in labels.iter().skip(i + 1) {
                let (p, q) = (ecad_cadcraft::wires::label_box(x), ecad_cadcraft::wires::label_box(y));
                let meet = p.0.x < q.1.x && q.0.x < p.1.x && p.0.y < q.1.y && q.0.y < p.1.y;
                assert!(!meet, "{} and {} touch", x.text, y.text);
            }
        }
    }
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
    // Checks: -M01:PE, and the three L continuations whose partners are not drawn; brief 024's
    // count of devices without a part number (-S01 among them, design 024 §13).
    assert_eq!(
        checks,
        [
            "LonelyContinuation 2-C-4 L1",
            "LonelyContinuation 2-C-4 L2",
            "LonelyContinuation 2-C-4 L3",
            "Unconnected 3-C-5 -M01:PE",
            "NoPartNumber  7 devices have no part number"
        ],
        "{checks:#?}"
    );
}

/// Brief 024 (design 024 §13): the demo's locations, its location box, the prefix of -K01's NC
/// contact on page 3, the reports and the generated pages.
#[test]
fn the_demo_locations_reports_and_generated_pages() {
    let mut mcp = EcadMcp::in_memory("demo", 0xDE30, Options { write: true, agent: None }).unwrap();
    demo::run(&mut mcp, demo::COMMANDS).unwrap_or_else(|e| panic!("{e}"));
    let doc = mcp.with_session(|s, p| p.read(s, |p, _| p.ledger.document().clone()).unwrap()).unwrap();
    let at = |tag: &str| {
        let l = ecad_cadcraft::locations::resolve(&doc, doc.components_tagged(tag)[0]);
        (l.value.unwrap_or_default(), l.source.word())
    };
    assert_eq!(at("-K01"), ("+CP1".to_string(), "project"));
    assert_eq!(at("-G01"), ("+CP1".to_string(), "project"));
    assert_eq!(at("-M01"), ("+FIELD".to_string(), "page"));
    assert_eq!(at("-X02"), ("+JB1".to_string(), "box"));
    // -K01's NC contact on page 3 reads +CP1-K01; its coil on page 2 reads -K01.
    let texts = |i: usize| -> Vec<String> { ecad_cadcraft::devices::texts_on_page(&doc, doc.page_order[i]).into_iter().map(|t| t.value).collect() };
    assert!(texts(2).contains(&"+CP1-K01".to_string()), "{:?}", texts(2));
    assert!(texts(1).contains(&"-K01".to_string()));
    // Reports: no wire on the BOM; the cable row.
    let o = ecad_cadcraft::outputs::Outputs::build(&doc);
    let bom = o.bom(ecad_cadcraft::outputs::Group::Location);
    // Brief 028 (Q13): a strip counts its terminals, so -X01 and -X02 add their other terminals.
    let terminals: usize = o.strips.strips.values().map(|s| s.drawn).sum();
    assert_eq!(
        bom.iter().map(|r| usize::try_from(r.qty.0 / 1000).unwrap()).sum::<usize>(),
        11 - o.strips.len() + terminals,
        "the devices and terminals, nothing else"
    );
    let cables = o.cable_table();
    let row: Vec<String> = cables.rows[0].iter().map(|c| c.text.clone()).collect();
    // Brief 028: Cores is the offer read from the type (4G), Used the three members.
    assert_eq!(row[..7], ["-W01", "4G1.5", "4", "+CP1", "+JB1", "3", "1 (GNYE)"], "{row:?}");
    let s01 = o.bom_table(ecad_cadcraft::outputs::Group::Location);
    assert!(s01.rows.iter().any(|r| r[2].text == "-S01" && r[4].text == "?" && r[4].flag.is_some()), "-S01 has no maker");
    // The generated pages.
    let kinds: Vec<_> = doc.page_order.iter().filter_map(|p| ecad_cadcraft::generated::kind_of(&doc, *p)).collect();
    assert_eq!(kinds, [ecad_cadcraft::generated::GenKind::Revisions, ecad_cadcraft::generated::GenKind::BomLocation]);
}
