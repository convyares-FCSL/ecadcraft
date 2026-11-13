//! Brief 040 (U4a, the canvas), the adapter's part: overlay marks in the token colours and sizes,
//! colour codes and two-colour wires (screen colours, mono and colour print, the information
//! check for an unknown code), overlay never printed, `print.colour`, and text heights as cap
//! heights in the sheet font.

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::doc::color::{Color, Rgb};
use cadcraft_engine::ext::MarkShape;
use ecad_cadcraft::checks::Kind;
use ecad_cadcraft::interact::overlay_marks;
use ecad_cadcraft::look::{self, Painted};
use ecad_cadcraft::printbook;
use ecad_model::{ConductorId, Pt};
use ecad_tokens::sheet::PAPER;
use serde_json::json;

use crate::common::{H, id, pts, small_project};

fn rgb(c: ecad_tokens::Color) -> Rgb {
    Rgb(c.r, c.g, c.b)
}

#[test]
fn colour_codes_are_one_or_two_colours() {
    let one = |c: &str| look::colour_code(c).map(|(a, b)| (look::CODE_COLOURS[a].0, b.map(|i| look::CODE_COLOURS[i].0)));
    for (code, want) in [
        ("BU", Some(("BU", None))),
        ("blu", Some(("BU", None))),
        ("GNYE", Some(("GN", Some("YE")))),
        ("GN/YE", Some(("GN", Some("YE")))),
        ("grn/yel", Some(("GN", Some("YE")))),
        ("GRNYEL", Some(("GN", Some("YE")))),
        ("RDBU", Some(("RD", Some("BU")))),
        ("YE-BN", Some(("YE", Some("BN")))),
        ("PURPLISH", None),
        ("GNXX", None),
        ("", None),
        ("GN/YE/BU", None),
        ("ГН", None),
    ] {
        assert_eq!(one(code), want, "{code}");
    }
    let gnye = look::wire_colours("GNYE", &PAPER).unwrap();
    assert_eq!((gnye.solid, gnye.dashes), (ecad_tokens::wire::PE.paper.solid, ecad_tokens::wire::PE.paper.dashes), "GNYE is the PE look");
    assert_eq!(look::wire_colours("BU", &PAPER).map(|w| w.solid), Some(ecad_tokens::wire::DC_24V.paper.solid));
    assert_eq!(look::wire_colours("BK", &PAPER).map(|w| w.solid), Some(PAPER.ink), "black is ink");
}

#[test]
fn dashes_are_two_on_two_off_round_corners() {
    let d = look::dash_segments(&[Pt::new(0.0, 0.0), Pt::new(5.0, 0.0), Pt::new(5.0, 5.0)], 2.0, 2.0);
    let near = |a: Pt, b: Pt| a.dist(b) < 1e-9;
    let want = [
        (Pt::new(0.0, 0.0), Pt::new(2.0, 0.0)),
        (Pt::new(4.0, 0.0), Pt::new(5.0, 0.0)),
        // The dash that began on the first segment goes on round the corner for its last 1 mm.
        (Pt::new(5.0, 0.0), Pt::new(5.0, 1.0)),
        (Pt::new(5.0, 3.0), Pt::new(5.0, 5.0)),
    ];
    assert_eq!(d.len(), want.len(), "{d:?}");
    for ((a, b), (c, e)) in d.iter().zip(want) {
        assert!(near(*a, c) && near(*b, e), "{d:?}");
    }
    assert!(look::dash_segments(&[Pt::new(0.0, 0.0), Pt::new(1e12, 0.0)], 2.0, 2.0).len() <= look::MAX_DASHES, "bounded");
    assert!(look::dash_segments(&[Pt::new(0.0, 0.0), Pt::new(5.0, 0.0)], 0.0, 2.0).is_empty());
    assert!(look::dash_segments(&[Pt::new(0.0, 0.0), Pt::new(f64::NAN, 0.0)], 2.0, 2.0).is_empty());
}

#[test]
fn marks_take_the_token_colours_and_sizes() {
    let (mut h, ..) = small_project(0x40_0001);
    h.run("wire", json!({"points": pts(&[(0.0, 60.0), (30.0, 60.0)])})).unwrap();
    let uid = h.s.state().unwrap().uid;
    let marks = h.p(|p| overlay_marks(p, uid));
    let of = |s: MarkShape| marks.iter().filter(|m| m.shape == s).map(|m| (m.color, m.size)).collect::<Vec<_>>();
    assert!(of(MarkShape::Ring).iter().all(|m| *m == (rgb(PAPER.connection_point), 8.0)), "{marks:?}");
    assert!(!of(MarkShape::Dot).is_empty() && of(MarkShape::Dot).iter().all(|m| *m == (rgb(PAPER.wire_end_attached), 5.0)));
    assert_eq!(of(MarkShape::Square), vec![(rgb(PAPER.loose_end), 7.0); 2], "the loose wire's two ends");
}

/// A page with a GNYE wire, a loose wire, a terminal and an end that looks connected (a painted
/// warning): every marker kind this page can show.
fn colour_page(seed: u64, code: &str) -> (H, ConductorId) {
    let (mut h, ..) = small_project(seed);
    let w: ConductorId = id(&h.run("wire", json!({"points": pts(&[(0.0, 60.0), (60.0, 60.0)])})).unwrap(), "conductor");
    h.run("setprop", json!({"conductor": w.hex(), "key": "colour", "value": code})).unwrap();
    // An open end lying on the GNYE wire's middle, not joined: looks connected.
    h.run("wire", json!({"points": pts(&[(30.0, 90.0), (30.0, 60.0)]), "join": false})).unwrap();
    (h, w)
}

#[test]
fn a_gnye_wire_draws_both_colours_and_prints_mono_or_colour() {
    let (mut h, w) = colour_page(0x40_0002, "GNYE");
    let page = h.first_page();
    let doc = h.doc();
    let routes = h.p(|p| look::coloured_routes(&doc, page, p.pages.get(&page).map(|v| &v.map), &PAPER));
    let r = routes.iter().find(|r| r.conductor == w).expect("the GNYE wire is coloured");
    assert_eq!((r.colours.solid, r.colours.dashes), (ecad_tokens::wire::PE.paper.solid, ecad_tokens::wire::PE.paper.dashes));
    assert!(r.handle.is_some(), "drawn on the page");

    // Colour print: green copies and yellow dashes on the wire colour layer, the rest in ink.
    let fan = Default::default();
    let (colour, _) = printbook::build_in(&doc, &[page], &fan, 0.01, None, false, true).unwrap();
    let on_layer: Vec<_> = colour.model.iter().filter(|e| e.common.layer == printbook::WIRE_COLOUR_LAYER).collect();
    let has = |c: ecad_tokens::Color| on_layer.iter().any(|e| e.common.color == Color::True(rgb(c)));
    assert!(has(ecad_tokens::wire::PE.paper.solid) && has(ecad_tokens::wire::PE.paper.dashes.unwrap()), "both colours print");
    assert!(colour.model.iter().filter(|e| e.common.layer != printbook::WIRE_COLOUR_LAYER).all(|e| e.common.color == Color::Index(7)));
    // Mono (the default): nothing added; the label carries the code.
    assert!(look::colour_code("GNYE").is_some());
    let (mono, _) = printbook::build_pending(&doc, &[page], &fan, 0.01, None, false).unwrap();
    assert!(mono.model.iter().all(|e| e.common.layer != printbook::WIRE_COLOUR_LAYER));
    let label = mono.model.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value.contains("GNYE")));
    assert!(label, "the mono print's wire label says GNYE");
    assert!(ecad_cadcraft::defaults::print::mono(&doc), "print.colour is mono by default");
    assert_eq!(ecad_cadcraft::defaults::print::mono_param(&doc, &json!({"colour": "colour"})), Ok(false));
    assert!(ecad_cadcraft::defaults::print::mono_param(&doc, &json!({"colour": "sepia"})).is_err());
    // No information check for a known code.
    assert!(!ecad_cadcraft::checks::all(&doc, 0.01).iter().any(|c| c.kind == Kind::UnknownColour));
}

#[test]
fn an_unknown_code_draws_in_ink_and_raises_one_information_check() {
    let (mut h, w) = colour_page(0x40_0003, "PURPLISH");
    let page = h.first_page();
    let doc = h.doc();
    let routes = h.p(|p| look::coloured_routes(&doc, page, p.pages.get(&page).map(|v| &v.map), &PAPER));
    assert!(routes.iter().all(|r| r.conductor != w), "drawn in ink");
    let rows: Vec<_> = ecad_cadcraft::checks::all(&doc, 0.01).into_iter().filter(|c| c.kind == Kind::UnknownColour).collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].severity(), ecad_cadcraft::checks::Severity::Info);
    assert!(rows[0].which.ends_with("PURPLISH"), "{}", rows[0].which);
}

/// AC 3: with every marker kind the page can show, neither print (mono or colour) nor the raster
/// export holds an overlay colour: the marks and painted markers are never entities.
#[test]
fn overlay_never_prints() {
    let (mut h, _) = colour_page(0x40_0004, "GNYE");
    let page = h.first_page();
    let uid = h.s.state().unwrap().uid;
    let marks = h.p(|p| overlay_marks(p, uid));
    for s in [MarkShape::Ring, MarkShape::Dot, MarkShape::Square] {
        assert!(marks.iter().any(|m| m.shape == s), "a {s:?} is shown");
    }
    assert!(h.p(|p| look::painted_marks(p, uid)).iter().any(|m| matches!(m, Painted::Warning(_))), "a warning is shown");
    let overlay: Vec<Rgb> = [
        PAPER.grid,
        PAPER.connection_point,
        PAPER.wire_end_attached,
        PAPER.loose_end,
        PAPER.error,
        PAPER.warning,
        PAPER.selection,
        PAPER.agent_write,
    ]
    .into_iter()
    .map(rgb)
    .collect();
    let doc = h.doc();
    let fan = Default::default();
    for colour in [false, true] {
        let (d, _) = printbook::build_in(&doc, &[page], &fan, 0.01, None, false, colour).unwrap();
        for e in d.model.iter() {
            if let Color::True(c) = e.common.color {
                assert!(!overlay.contains(&c), "an overlay colour prints ({colour}): {c:?}");
            }
        }
    }
    let drawing = h.p(|p| ecad_cadcraft::preview::preview_drawing(p, page)).unwrap();
    let view = ecad_cadcraft::preview::PreviewView::extents(&drawing, 600, 400);
    let img = ecad_cadcraft::preview::rasterise(&drawing, view, 600, 400, [255, 255, 255], 2.0).unwrap();
    let px = img.rgba.chunks(4).filter(|c| c.len() >= 3).map(|c| Rgb(c[0], c[1], c[2]));
    let found: Vec<Rgb> = px.filter(|c| overlay.contains(c)).take(3).collect();
    assert!(found.is_empty(), "overlay colours in the raster export: {found:?}");
}

/// AC 5: a tag at `text.tag.height` 2.5 is 2.5 mm from the baseline to the top of `K` in the
/// sheet font's SemiBold; run in a child process, as the faces are process-wide.
#[test]
fn a_tag_is_its_height_from_baseline_to_cap_in_the_sheet_font() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .env("ECAD_TEST_CHILD", "1")
        .args(["--exact", "lanes::lane_040::child_cap_height", "--ignored", "--nocapture", "--test-threads=1"])
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.contains("1 passed"), "child failed:\n{text}");
}

#[test]
#[ignore = "run by a_tag_is_its_height_from_baseline_to_cap_in_the_sheet_font in a child process"]
fn child_cap_height() {
    // Only as a child of its test, which sets `ECAD_TEST_CHILD`; run directly, it does nothing.
    if std::env::var_os("ECAD_TEST_CHILD").is_none() {
        return;
    }
    use cadcraft_engine::fonts::ttf::{self, EmbeddedFace};
    // Registered before the project is made, as the app does at start (without them, 031's
    // chain draws: `tester_031_text::t3_font_fallback_never_refuses_and_keeps_the_stored_name`).
    static FACES: [EmbeddedFace; 2] = [
        EmbeddedFace {
            family: "Barlow Semi Condensed",
            style: "Regular",
            bytes: include_bytes!("../../../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-Regular.ttf"),
        },
        EmbeddedFace {
            family: "Barlow Semi Condensed",
            style: "SemiBold",
            bytes: include_bytes!("../../../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-SemiBold.ttf"),
        },
    ];
    ttf::register_faces(&FACES).unwrap();
    let mut h = H::new(0x40_0005);
    let v = h.run("textstyle", json!({})).unwrap();
    assert_eq!((v["font"].as_str(), v["height_is"].as_str()), (Some("Barlow Semi Condensed"), Some("cap height")));
    assert_eq!(v["drawn_with"], "Barlow Semi Condensed", "{v}");
    let tag = v["kinds"].as_array().unwrap().iter().find(|k| k["kind"] == "tag").unwrap().clone();
    assert_eq!((tag["height"].as_f64(), tag["weight"].as_str()), (Some(2.5), Some("bold")));
    let fonts = h.p(|p| ecad_cadcraft::text_style::Fonts::resolve(p.ledger.document()));
    assert_eq!(fonts.bold, "Barlow Semi Condensed SemiBold", "bold is the SemiBold face");
    // The plotted geometry of `K` at the tag height: its outline, baseline at y = 0.
    let face = ttf::find(&fonts.bold).expect("the SemiBold face");
    let shaped = ttf::shape(&face, "K", 2.5, 1.0, 0.0).expect("shaped");
    let ys = shaped.glyphs.iter().flatten().flatten().map(|p| p.y);
    let (lo, hi) = ys.fold((f64::MAX, f64::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
    assert!((lo - 0.0).abs() < 0.02, "baseline at 0: {lo}");
    assert!((hi - 2.5).abs() < 0.02, "K is 2.5 mm tall: {hi}");
    // A project with its own font keeps it.
    h.run("textstyle", json!({"font": "NoSuchFont 040"})).unwrap();
    let own = h.run("textstyle", json!({})).unwrap();
    assert_eq!(own["font"], "NoSuchFont 040");
    assert_eq!(own["drawn_with"], "Barlow Semi Condensed", "a missing family falls back to the sheet font");
}
