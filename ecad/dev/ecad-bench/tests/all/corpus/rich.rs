//! `Variant::Rich` holds every object kind the kept views hold, and is deterministic.

use ecad_bench::generate::{Tier, Variant, generate};
use ecad_model::Document;
use ecad_model::look::FixedWidth;

/// Ten pages: the bench parts' populations spread over several pages.
const SMALL: Tier = Tier { name: "rich-test", pages: 10, devices: 100, wires: 500, cables: 50 };

fn doc_of(g: &ecad_bench::Generated) -> Document {
    g.ledger.document().clone()
}

#[test]
#[ignore = "slow: generates a rich project through the commands (about 20 s in a debug build)"]
fn rich_has_every_object_kind() {
    let g = generate(SMALL, Variant::Rich).unwrap();
    assert_eq!(g.variant, Variant::Rich);
    let doc = doc_of(&g);
    doc.check().unwrap();

    // The lean pattern is still all there.
    assert!(doc.wires().len() >= SMALL.wires && doc.cables().len() >= SMALL.cables);

    // Connectors: mated halves, with pins.
    let connectors: Vec<_> = doc.devices().keys().filter_map(|c| ecad_electrical::connectors::connector_of(&doc, *c).map(|x| (*c, x))).collect();
    assert!(connectors.iter().any(|(_, x)| x.mating), "a mated connector");
    assert!(connectors.iter().any(|(c, _)| doc.connection_points_of(*c).count() >= 4), "a connector with pins");

    // The reports' models: terminal strips with terminal blocks, and the PLC.
    let models = ecad_electrical::views::ReportsView::build(&doc, &ecad_drawing_style::text::fonts::FontLook(FixedWidth));
    let strips = &models.model().strips;
    assert!(!strips.is_empty(), "terminal strips");
    assert!(strips.strips.values().any(|s| s.drawn > 0), "terminal blocks on a strip");
    let plc = ecad_electrical::plc::kept::Plc::build_with(&doc, &ecad_drawing_style::text::fonts::FontLook(FixedWidth));
    assert!(!plc.racks.is_empty(), "PLC racks");
    assert!(!plc.cards.is_empty(), "PLC cards");
    assert!(plc.naming.values().any(|cards| !cards.is_empty()), "cards in racks");

    // Jumper bars on a page.
    assert!(doc.page_order().iter().any(|p| !ecad_electrical::terminal_strips::jumper_bars::parts_on_page(&doc, *p).is_empty()), "jumper bars");

    // A cover page.
    assert!(doc.pages().values().any(|p| matches!(p.kind, ecad_model::PageKind::Generated(ecad_model::GenKind::Cover))), "a cover page");

    // Ratings: on devices (protective), wires and cables.
    assert!(doc.devices().keys().any(|c| ecad_electrical::circuits::trace::is_protective_component(&doc, *c)), "a protective device");
    let kept = ecad_electrical::circuits::kept::Kept::build(&doc);
    use ecad_electrical::circuits::RatedSubject;
    assert!(kept.results.keys().any(|s| matches!(s, RatedSubject::Wire(_))), "a rated wire");
    assert!(kept.results.keys().any(|s| matches!(s, RatedSubject::Cable(_))), "a rated cable");

    // Contacts: references across pages.
    let refs = ecad_electrical::views::ReferencesView::build(&doc);
    let _ = refs.index();
}

#[test]
#[ignore = "slow: generates two rich projects through the commands"]
fn rich_is_deterministic() {
    let (a, b) = (generate(SMALL, Variant::Rich).unwrap(), generate(SMALL, Variant::Rich).unwrap());
    assert_eq!(a.ledger.document(), b.ledger.document());
    assert_eq!(a.pages, b.pages);
}
