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
    // Brief 054 (design 054 §7): a new revisions page is titled Revision history.
    assert_eq!(titles, ["Supply", "Motor control", "Field", "Revision history", "BOM by location"]);
    // Every transaction after the project's creation is the demo's, as an agent.
    assert!(log.iter().skip(1).all(|t| t.author == Author::Agent(demo::AUTHOR.into())));
    // The devices, by tag.
    let tags: Vec<String> = ecad_cadcraft::devices::devices(&doc, None).into_iter().map(|c| ecad_cadcraft::devices::tag_words(&doc, c)).collect();
    assert_eq!(tags, ["-G01", "-H01", "-H02", "-K01", "-M01", "-Q01", "-Q02", "-S01", "-S02", "-X01", "-X02"], "{tags:?}");
    // -K01 is one device across pages 2 and 3: coil, hold-in NO, power contacts, NC.
    let k1 = doc.components_tagged("-K01")[0];
    assert_eq!(doc.representations_of(Subject::Device(k1)).count(), 4);
    assert_eq!(ecad_cadcraft::devices::page_numbers(&doc, k1), [2, 3]);
    assert_eq!(doc.devices[&k1].get("part_number"), Some("CT-9A-24VDC"));
    // Cable -W01 leaves page 2 at its reference and continues on page 3 (design 072 §4.4, F4/F5):
    // two linked references, and each -X02 point has the motor's wire and the cable core's wire.
    assert_eq!(doc.cables.len(), 1);
    let refs = ecad_cadcraft::references::all(&doc);
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert!(refs.iter().all(|r| !r.orphan), "{refs:?}");
    let x2 = doc.components_tagged("-X02")[0];
    let x2_terminals: Vec<_> = doc.terminals_of(x2).map(|(t, _)| t).collect();
    assert_eq!(x2_terminals.len(), 3);
    let w01 = doc.cables_tagged("-W01")[0];
    let cores: Vec<_> = doc.cables[&w01].cores.iter().filter_map(|c| c.wire()).collect();
    assert_eq!(cores.len(), 3, "each continued core is one wire");
    for t in x2_terminals {
        let ends = doc.ends_at(t);
        assert_eq!(ends.len(), 2, "the motor's wire and the core's");
        assert!(ends.iter().any(|(w, _)| cores.contains(w)), "a core of -W01 lands on each -X02 point");
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
    // side of -Q01, PE, and the PSU outputs, which run through continuations 24V and 0V. Page 2:
    // 24V-01 on to the breaker, the control nets, 0V-01 through the T. Page 3: the same 24V-01,
    // 0V-01, the NC-to-lamp wire 24V-05. Continuations are named points (design 072 §4.4, F5): a
    // wire runs device point to device point across pages, and the lines from the lonely L1-L3
    // continuations above the contactor carry no wire. -W01 continues on page 3: each core is one
    // wire from -K01 to -X02 (a core shows its number only), in the net of the motor's wire.
    let want = [
        "1: 0V-01 0.5 BU",
        "1: 24V-01 0.5 BU",
        "1: L-01 2.5 BN",
        "1: L-02 2.5 BN",
        "1: N-01 2.5 BU",
        "1: N-02 2.5 BU",
        "1: PE 2.5 GN/YE",
        "2: 0V-01 0.5 BU",
        "2: 24V-01 0.5 BU",
        "2: 24V-02 0.5 BU",
        "2: 24V-03 0.5 BU",
        "2: 24V-04 0.5 BU",
        "2: L1-01",
        "2: L2-01",
        "2: L3-01",
        "3: 0V-01 0.5 BU",
        "3: 24V-01 0.5 BU",
        "3: 24V-05 0.5 BU",
        "3: L1-01",
        "3: L1-01 2.5 BN",
        "3: L2-01",
        "3: L2-01 2.5 BK",
        "3: L3-01",
        "3: L3-01 2.5 GY",
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
    // 24V runs over pages 1-3 through continuations; L1-01 from -K01 through -W01's core and -X02
    // to the motor.
    let conn = doc.connectivity();
    let net = |n: &str| conn.nets_numbered(n).map(|(id, _)| id).collect::<Vec<_>>();
    assert_eq!(net("24V-01").len(), 1);
    assert_eq!(net("L1-01").len(), 1);
    let l1 = ecad_cadcraft::wires::net_points(&doc, conn.net(net("L1-01")[0]).unwrap());
    assert_eq!(l1.len(), 3, "{l1:?}");
    assert!(l1.iter().any(|p| p == "-K01:2") && l1.iter().any(|p| p.starts_with("-X02:")) && l1.iter().any(|p| p.starts_with("-M01:")), "{l1:?}");
    let v24 = conn.net(net("24V-01")[0]).unwrap();
    assert_eq!(v24.names.iter().cloned().collect::<Vec<_>>(), ["24V"]);
    // The coil, both contacts' 14 side and the lamp share 24V-04 (design 022 §15 step 1).
    let k04 = conn.net(net("24V-04")[0]).unwrap();
    let points = ecad_cadcraft::wires::net_points(&doc, k04);
    assert_eq!(points, ["-H01:X1", "-K01:14", "-K01:A1", "-S02:14"], "{points:?}");
    // Checks: -M01:PE, and the three L continuations whose partners are not drawn; brief 024's
    // count of devices without a part number (-S01 among them, design 024 §13); brief 057's open
    // core (the owner rule: -W01's unwired GNYE core is an error until ticked Not used).
    // Brief 052: -K01's function =PUMP is stored, and no box, page or project says =PUMP.
    // Brief 059: -W01 has no gland size at either end (Info).
    // -W01 continues on page 3 (F4, F5): no orphan reference and no loose lines.
    assert_eq!(
        checks,
        [
            "CoreOpen 2/E3 -W01 core GNYE is not assigned: slash or add a wire, or tick Not used.",
            // Design 072 §2.2: the demo's MCBs have no typed rating.
            "NoRating 1/D1 -Q01 has no rating",
            "NoRating 2/C1 -Q02 has no rating",
            "FunctionUnused 2/E1 -K01 is =PUMP, but none of its symbols is in =PUMP",
            // Design 072 §2.2: the demo's coil has no typed rating either.
            "NoRating 2/E1 -K01 has no rating",
            // Design 072 §2.2: the demo's lamps have no typed rating either.
            "NoRating 2/E2 -H01 has no rating",
            "NoRating 3/E1 -H02 has no rating",
            // Design 072 §2.2: the demo's motor has no typed rating either.
            "NoRating 3/E3 -M01 has no rating",
            "LonelyContinuation 2/D3 L1",
            "LonelyContinuation 2/D3 L2",
            "LonelyContinuation 2/D3 L3",
            "NoGland 2/E3 -W01 has no gland size at the cabinet end.",
            "NoGland 2/E3 -W01 has no gland size at the field end.",
            "Unconnected 3/E3 -M01:PE",
            "NoFunction  10 devices have no function",
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
    let read = |mcp: &mut EcadMcp| mcp.with_session(|s, p| p.read(s, |p, _| p.ledger.document().clone()).unwrap()).unwrap();
    let doc = read(&mut mcp);
    // Brief 052 (design 052 §5.1): nothing is inherited; the project, the page and the box only
    // propose, at each device's go-to symbol.
    let proposal = |doc: &ecad_model::Document, tag: &str| {
        let c = doc.components_tagged(tag)[0];
        let r = &doc.representations[&ecad_cadcraft::reps::goto(doc, c, None).unwrap()];
        let x = ecad_cadcraft::locations::rep_context(doc, r, &mut ecad_cadcraft::locations::BoxCache::default());
        (ecad_cadcraft::locations::resolve(doc, c).value, x.value.unwrap_or_default(), x.source.word())
    };
    // Brief 052d: each device took its proposal when placed; the demo's +JB1 box moved -X02.
    let s = |v: &str| Some(v.to_string());
    assert_eq!(proposal(&doc, "-K01"), (s("+CP1"), "+CP1".to_string(), "project"));
    assert_eq!(proposal(&doc, "-G01"), (s("+CP1"), "+CP1".to_string(), "project"));
    assert_eq!(proposal(&doc, "-M01"), (s("+FIELD"), "+FIELD".to_string(), "page"));
    assert_eq!(proposal(&doc, "-X02"), (s("+JB1"), "+JB1".to_string(), "box"));
    // Accept every device's proposal, as a person does from the inspector's Use link.
    let tagged: Vec<(ecad_model::DeviceId, String)> = doc
        .devices
        .keys()
        .filter(|c| ecad_cadcraft::locations::is_device(&doc, **c))
        .filter_map(|c| {
            let r = &doc.representations[&ecad_cadcraft::reps::goto(&doc, *c, None)?];
            ecad_cadcraft::locations::rep_context(&doc, r, &mut ecad_cadcraft::locations::BoxCache::default()).value.map(|v| (*c, v))
        })
        .collect();
    for (c, v) in tagged {
        mcp.with_session(|s, _| s.execute("setlocation", &serde_json::json!({"component": c.hex(), "value": v})).map(|_| ()))
            .unwrap()
            .unwrap_or_else(|e| panic!("{e}"));
    }
    let doc = read(&mut mcp);
    // -K01 (=PUMP, +CP1): its NC contact on page 3 (+FIELD) reads =PUMP+CP1-K01; its coil on page
    // 2 =PUMP-K01 (no page or project function).
    let texts = |i: usize| -> Vec<String> { ecad_cadcraft::devices::texts_on_page(&doc, doc.page_order[i]).into_iter().map(|t| t.value).collect() };
    assert!(texts(2).contains(&"=PUMP+CP1-K01".to_string()), "{:?}", texts(2));
    assert!(texts(1).contains(&"=PUMP-K01".to_string()), "{:?}", texts(1));
    // Reports: no wire on the BOM; the cable row.
    let o = ecad_cadcraft::outputs::kept::Outputs::build_with(&doc, &ecad_cadcraft::text_style::EngineMetrics);
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
    // Brief 028: Cores is the offer read from the type (4G). -W01 continues on page 3, so each
    // core is a wire from -K01 (+CP1) to -X02 (+JB1): 3 used, GNYE spare.
    assert_eq!(row[..7], ["-W01", "4G1.5", "4", "+CP1", "+JB1", "3", "1 (GNYE)"], "{row:?}");
    let s01 = o.bom_table(ecad_cadcraft::outputs::Group::Location);
    assert!(s01.rows.iter().any(|r| r[2].text == "-S01" && r[4].text == "?" && r[4].flag.is_some()), "-S01 has no maker");
    // The generated pages.
    let kinds: Vec<_> = doc.page_order.iter().filter_map(|p| ecad_cadcraft::generated::kind_of(&doc, *p)).collect();
    assert_eq!(kinds, [ecad_cadcraft::generated::GenKind::Revisions, ecad_cadcraft::generated::GenKind::BomLocation]);
}
