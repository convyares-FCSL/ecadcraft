//! Lane 072 G (design 072 §8, §4.6, F30, F36; acceptance A2, A4, A9): style records hold a typed
//! body of their own catalogue. A wire type reads and writes its typed fields, refuses an unknown
//! field or a value in a form it does not store, and cannot be filed under another catalogue.

use ecad_model::style::wire::{Insulation, LineColour};
use ecad_model::style::{StyleBody, WireType};
use ecad_model::units::Mm;
use ecad_model::wire::{Colours, WireSize};
use ecad_model::{Author, Ledger, Op, StyleCatalogue, StyleLayer, StyleRecord, StyleRef};

fn earth() -> WireType {
    WireType {
        potential: Some(ecad_model::text::Text::new("PE").unwrap()),
        size: WireSize::parse("6"),
        colours: Colours::parse("GN/YE"),
        insulation: Some(Insulation::Pvc),
        line_colour: Some(LineColour::Wire),
        line_weight: Some(Mm(0.35)),
        ..WireType::default()
    }
}

#[test]
fn a_wire_type_round_trips_as_its_typed_fields() {
    let rec = StyleRecord::new("Earth 6", StyleBody::Wire(earth()));
    let v = serde_json::to_value(&rec).unwrap();
    assert_eq!(v["body"]["wire"]["colours"], "GN/YE", "{v}");
    assert_eq!(v["body"]["wire"]["insulation"], "pvc", "{v}");
    assert_eq!(v["body"]["wire"]["size"], "6", "{v}");
    let back: StyleRecord = serde_json::from_value(v).unwrap();
    assert_eq!(back, rec);
    assert_eq!(back.catalogue(), StyleCatalogue::Wire);
}

#[test]
fn a_wire_type_with_an_unknown_field_or_word_does_not_read() {
    let good = serde_json::to_value(StyleRecord::new("Earth", StyleBody::Wire(earth()))).unwrap();
    let cases = [
        ("an unknown field", serde_json::json!({"colour": "BU"})),
        ("an unknown insulation", serde_json::json!({"insulation": "paper"})),
        ("a colour not in its stored form", serde_json::json!({"line_colour": "blue"})),
        ("text with a hidden character", serde_json::json!({"potential": "P\u{200b}E"})),
    ];
    for (what, patch) in cases {
        let mut v = good.clone();
        for (k, x) in patch.as_object().unwrap() {
            v["body"]["wire"][k] = x.clone();
        }
        assert!(serde_json::from_value::<StyleRecord>(v).is_err(), "{what} is refused");
    }
    // A body of no catalogue is refused too.
    let mut v = good;
    v["body"] = serde_json::json!({"stamp": {}});
    assert!(serde_json::from_value::<StyleRecord>(v).is_err());
}

#[test]
fn a_record_is_filed_only_under_its_own_catalogue() {
    let mut l = Ledger::new(72);
    let id = StyleRef::Own(7);
    let set = |catalogue| Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue,
        id: id.clone(),
        before: None,
        after: Some(StyleRecord::new("Earth", StyleBody::Wire(earth()))),
    };
    assert!(l.commit(Author::User, "wrong", vec![set(StyleCatalogue::Text)]).is_err(), "a wire type is no text style");
    l.commit(Author::User, "right", vec![set(StyleCatalogue::Wire)]).unwrap();
    let r = ecad_model::style::record(l.document(), StyleLayer::Project, StyleCatalogue::Wire, &id).unwrap();
    assert_eq!(r.body.wire().and_then(|w| w.insulation), Some(Insulation::Pvc));
}

/// Text styles (design 072 §8): typed font, cap height, weight and colour; a value in any other
/// form does not read.
#[test]
fn a_text_style_reads_its_typed_fields_only() {
    use ecad_model::style::TextStyle;
    use ecad_model::style::text::{Font, StyleColour, Weight};
    let t = TextStyle { font: Font::parse("Plex"), height: Some(Mm(3.5)), weight: Some(Weight::Bold), colour: StyleColour::parse("#C62828") };
    let v = serde_json::to_value(StyleRecord::new("Big", StyleBody::Text(t.clone()))).unwrap();
    assert_eq!(v["body"]["text"], serde_json::json!({"font": "Plex", "height": 3.5, "weight": "bold", "colour": "#c62828"}));
    assert_eq!(serde_json::from_value::<StyleRecord>(v.clone()).unwrap().body, StyleBody::Text(t));
    for (k, bad) in [
        ("height", serde_json::json!("3")),
        ("weight", serde_json::json!("heavy")),
        ("colour", serde_json::json!("blue")),
        ("size", serde_json::json!(1)),
    ] {
        let mut w = v.clone();
        w["body"]["text"][k] = bad;
        assert!(serde_json::from_value::<StyleRecord>(w).is_err(), "{k}");
    }
    assert_eq!(StyleColour::parse("none"), Some(StyleColour::None));
    assert_eq!(StyleColour::parse("blu").map(|c| c.to_string()).as_deref(), Some("BU"));
}

/// Location styles (design 072 §8): typed line, colours and label style; strict read.
#[test]
fn a_location_style_reads_its_typed_fields_only() {
    use ecad_model::style::LocationStyle;
    use ecad_model::style::text::StyleColour;
    use ecad_model::style::wire::LineType;
    let l = LocationStyle {
        line_type: Some(LineType::Dashdot),
        line_weight: Some(Mm(0.5)),
        line_colour: StyleColour::parse("RD"),
        label_style: StyleRef::parse("tag"),
        fill: Some(StyleColour::None),
    };
    let v = serde_json::to_value(StyleRecord::new("Panel", StyleBody::Location(l.clone()))).unwrap();
    assert_eq!(v["body"]["location"]["label_style"], "tag");
    assert_eq!(serde_json::from_value::<StyleRecord>(v.clone()).unwrap().body, StyleBody::Location(l));
    for (k, bad) in [("line_type", serde_json::json!("wavy")), ("label_style", serde_json::json!("Tag!")), ("glow", serde_json::json!(1))] {
        let mut w = v.clone();
        w["body"]["location"][k] = bad;
        assert!(serde_json::from_value::<StyleRecord>(w).is_err(), "{k}");
    }
}

/// Rating tables (design 072 §8): the source is one line, the rows validated text; strict read.
#[test]
fn a_rating_table_reads_its_typed_fields_only() {
    use ecad_model::style::RatingTable;
    use ecad_model::text::{LongText, Text};
    let t = RatingTable { source: Text::new("made up").ok(), rows: LongText::new("kind,a\ncapacity,1").ok() };
    let v = serde_json::to_value(StyleRecord::new("T1", StyleBody::RatingTable(t.clone()))).unwrap();
    assert_eq!(serde_json::from_value::<StyleRecord>(v.clone()).unwrap().body, StyleBody::RatingTable(t));
    for (k, bad) in [("source", serde_json::json!("two\nlines")), ("rows", serde_json::json!("a\u{202e}b")), ("rated", serde_json::json!(1))] {
        let mut w = v.clone();
        w["body"]["ratingtable"][k] = bad;
        assert!(serde_json::from_value::<StyleRecord>(w).is_err(), "{k}");
    }
}

/// Page types (design 072 §8): typed sheet, frame, title block, switches and holds; strict read.
#[test]
fn a_page_type_reads_its_typed_fields_only() {
    use ecad_model::style::PageType;
    use ecad_model::style::page::{Holds, TypeBlock, TypeFrame, TypeSheet};
    let t = PageType {
        sheet: TypeSheet::parse("A2"),
        frame: TypeFrame::parse("project"),
        title_block: TypeBlock::parse("frame"),
        zones: Some(false),
        checks: Some(true),
        holds: Some(Holds::Report),
    };
    let v = serde_json::to_value(StyleRecord::new("Wide", StyleBody::Page(t.clone()))).unwrap();
    assert_eq!(v["body"]["page"]["frame"], "project");
    assert_eq!(serde_json::from_value::<StyleRecord>(v.clone()).unwrap().body, StyleBody::Page(t));
    for (k, bad) in [("zones", serde_json::json!("maybe")), ("holds", serde_json::json!("x")), ("title_block", serde_json::json!("Big!"))] {
        let mut w = v.clone();
        w["body"]["page"][k] = bad;
        assert!(serde_json::from_value::<StyleRecord>(w).is_err(), "{k}");
    }
}

/// Frame styles (design 072 §8): typed weights, colour, switches, sheet, margins, block and title
/// block; strict read.
#[test]
fn a_frame_style_reads_its_typed_fields_only() {
    use ecad_model::style::FrameStyle;
    use ecad_model::style::frame::{BlockAt, FrameMargins, FrameSheet};
    let f = FrameStyle {
        border_weight: Some(Mm(0.7)),
        grid_labels: Some(false),
        sheet: FrameSheet::parse("page"),
        margins: ecad_model::sheet::parse_margins("15,10,12,8").map(FrameMargins),
        block: Some(BlockAt::BottomRight),
        title_block: Some(ecad_model::title_block::TitleBlockLayout::blank(180.0, 40.0)),
        ..FrameStyle::default()
    };
    let v = serde_json::to_value(StyleRecord::new("A2", StyleBody::Frame(f.clone()))).unwrap();
    assert_eq!(v["body"]["frame"]["margins"], "15,10,12,8");
    assert_eq!(serde_json::from_value::<StyleRecord>(v.clone()).unwrap().body, StyleBody::Frame(f));
    for (k, bad) in [
        ("margins", serde_json::json!("1,2")),
        ("sheet", serde_json::json!("A9")),
        ("block", serde_json::json!("top")),
        ("stamp", serde_json::json!({})),
    ] {
        let mut w = v.clone();
        w["body"]["frame"][k] = bad;
        assert!(serde_json::from_value::<StyleRecord>(w).is_err(), "{k}");
    }
}

/// Cable types (design 072 §8, F37): typed size, rating, core colours or scheme, part, offer,
/// shield, gland and diameter; strict read.
#[test]
fn a_cable_type_reads_its_typed_fields_only() {
    use ecad_model::style::CableType;
    use ecad_model::style::cable::{CoreColours, CoreScheme, Shield};
    let c = CableType {
        size: WireSize::parse("1.5"),
        core_colours: CoreColours::parse("BN,BK,GY,GNYE"),
        core_scheme: Some(CoreScheme::Harmonised),
        cores: ecad_model::text::Text::new("4G").ok(),
        shield: Some(Shield::OverallAndPairs),
        outer_diameter: Some(Mm(9.5)),
        ..CableType::default()
    };
    let v = serde_json::to_value(StyleRecord::new("H07", StyleBody::Cable(c.clone()))).unwrap();
    assert_eq!(v["body"]["cable"]["core_colours"], "BN,BK,GY,GN/YE");
    assert_eq!(v["body"]["cable"]["shield"], "overall+pairs");
    assert_eq!(serde_json::from_value::<StyleRecord>(v.clone()).unwrap().body, StyleBody::Cable(c));
    for (k, bad) in
        [("core_scheme", serde_json::json!("rainbow")), ("core_colours", serde_json::json!("BN,GNYE")), ("cross_section", serde_json::json!("1.5"))]
    {
        let mut w = v.clone();
        w["body"]["cable"][k] = bad;
        assert!(serde_json::from_value::<StyleRecord>(w).is_err(), "{k}");
    }
}
