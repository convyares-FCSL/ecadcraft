//! Lane 072 D (design 072 §4.2, §4.6; acceptance A2, A4, A9): the wire's typed values round-trip
//! in their stored form, a stored value in any other form is a read error, and `ModifyWire`
//! changes a wire's values, is its own inverse and never moves its ends.

use ecad_model::text::Text;
use ecad_model::units::Amps;
use ecad_model::wire::{Basis, Bundling, CircuitInputs, Colours, InstallMethod, SupplySystem, WireNumber, WireSize};
use ecad_model::{Author, IdAllocator, Ledger, Op, TerminalId, Wire, WireId, Word};

#[test]
fn sizes_read_typed_and_store_one_form() {
    let cases: &[(&str, Option<&str>)] = &[
        ("2.5", Some("2.5")),
        (" 2.5 mm² ", Some("2.5")),
        ("2,5mm2", Some("2.5")),
        ("0.75", Some("0.75")),
        ("18 AWG", Some("18 AWG")),
        ("awg 18", Some("18 AWG")),
        ("4/0 AWG", Some("4/0 AWG")),
        ("0000 awg", Some("4/0 AWG")),
        ("0 AWG", Some("1/0 AWG")),
        ("41 AWG", None),
        ("0.01", None),
        ("2000", None),
        ("thick", None),
        ("", None),
    ];
    for (typed, want) in cases {
        assert_eq!(WireSize::parse(typed).map(|s| s.to_string()).as_deref(), *want, "{typed:?}");
    }
    for stored in ["\"2.5\"", "\"18 AWG\"", "\"2/0 AWG\""] {
        let s: WireSize = serde_json::from_str(stored).unwrap();
        assert_eq!(serde_json::to_string(&s).unwrap(), stored);
    }
    for not_stored in ["\"2.5 mm²\"", "\"awg18\"", "\"thick\""] {
        assert!(serde_json::from_str::<WireSize>(not_stored).is_err(), "{not_stored} is not the stored form");
    }
}

#[test]
fn colours_are_ordered_lists_stored_by_two_letter_code() {
    let cases: &[(&str, Option<&str>)] = &[
        ("BU", Some("BU")),
        ("blue", Some("BU")),
        ("GNYE", Some("GN/YE")),
        ("gn/ye", Some("GN/YE")),
        ("Green / Yellow", Some("GN/YE")),
        ("GRN-YEL", Some("GN/YE")),
        ("YE GN", Some("YE/GN")),
        ("BK/BN/GY", Some("BK/BN/GY")),
        ("BK/BN/GY/BU", None),
        ("tartan", None),
        ("GNY", None),
        ("", None),
    ];
    for (typed, want) in cases {
        assert_eq!(Colours::parse(typed).map(|c| c.to_string()).as_deref(), *want, "{typed:?}");
    }
    let gy = Colours::parse("GN/YE").unwrap();
    assert_eq!(gy.shown(ecad_model::colour::ColourWords::Code3), "GRN/YEL");
    assert_eq!(gy.shown(ecad_model::colour::ColourWords::Name), "Green/Yellow");
    assert_eq!(serde_json::from_str::<Colours>("\"GN/YE\"").unwrap(), gy);
    for not_stored in ["\"GNYE\"", "\"gn/ye\"", "\"green\"", "\"XX\""] {
        assert!(serde_json::from_str::<Colours>(not_stored).is_err(), "{not_stored} is not the stored form");
    }
}

#[test]
fn rating_inputs_are_words_and_bounded_numbers() {
    assert_eq!(Basis::WORDS, ["datasheet", "table", "manual"]);
    assert_eq!(SupplySystem::WORDS, ["ac1", "ac3", "dc"]);
    assert_eq!(InstallMethod::WORDS, ["A1", "A2", "B1", "B2", "C", "D1", "D2", "E", "F", "G"]);
    let inputs = CircuitInputs {
        basis: Some(Basis::Manual),
        manual: Some(Amps(16.0)),
        bundling: Some(Bundling::Circuits(3)),
        system: Some(SupplySystem::Ac3),
        ..CircuitInputs::default()
    };
    let text = serde_json::to_string(&inputs).unwrap();
    assert_eq!(serde_json::from_str::<CircuitInputs>(&text).unwrap(), inputs);
    let refused = [
        r#"{"manual": 0}"#,
        r#"{"manual": 7000}"#,
        r#"{"bundling": {"factor": 0.2}}"#,
        r#"{"bundling": {"circuits": 21}}"#,
        r#"{"ambient": 95}"#,
        r#"{"grouped": 0}"#,
        r#"{"voltage": 0.5}"#,
        r#"{"vd_limit": 25}"#,
        r#"{"system": "ac2"}"#,
        r#"{"basis": "guess"}"#,
        r#"{"colour": "BU"}"#,
    ];
    for r in refused {
        assert!(serde_json::from_str::<CircuitInputs>(r).is_err(), "{r} is refused on reading");
    }
}

#[test]
fn a_wire_is_strict_on_reading() {
    let w = Wire {
        ends: [Some(TerminalId(4)), None],
        size: WireSize::parse("1.5"),
        colours: Colours::parse("BU"),
        number: Some(WireNumber { text: Text::new("24V-07").unwrap(), fixed: true }),
        potential: Text::new("24V").ok(),
        load: Some(Amps(2.0)),
        ..Wire::default()
    };
    let text = serde_json::to_string(&w).unwrap();
    assert_eq!(serde_json::from_str::<Wire>(&text).unwrap(), w);
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["properties"] = serde_json::json!({});
    assert!(serde_json::from_value::<Wire>(v).is_err(), "an unknown field is a read error");
    let hidden = text.replace("24V-07", "24V\u{202e}07");
    assert!(serde_json::from_str::<Wire>(&hidden).is_err(), "a hidden character is a read error");
    assert_eq!(w.values().first().map(|(k, v)| (*k, v.as_str())), Some(("wire number", "24V-07")));
}

#[test]
fn modify_wire_changes_values_undoes_and_keeps_the_ends() {
    let mut ids = IdAllocator::seeded(72);
    let mut l = Ledger::new(72);
    let w: WireId = ids.fresh();
    let created = Wire::between(None, None);
    l.commit(Author::User, "wire", vec![Op::CreateWire { id: w, conductor: created.clone() }]).unwrap();
    let mut after = created.clone();
    after.size = WireSize::parse("2.5");
    // An `after` naming other ends changes the values only.
    after.ends = [Some(TerminalId(99)), None];
    let op = Op::ModifyWire { id: w, before: created.clone(), after };
    assert!(matches!(op.inverse(), Op::ModifyWire { .. }));
    l.commit(Author::User, "size", vec![op]).unwrap();
    let now = l.document().wires.get(&w).unwrap().clone();
    assert_eq!(now.size.map(|s| s.to_string()).as_deref(), Some("2.5"));
    assert_eq!(now.ends, [None, None], "the ends stay");
    // A recorded `before` that no longer holds is refused.
    let stale = Op::ModifyWire { id: w, before: created.clone(), after: created.clone() };
    assert!(l.commit(Author::User, "stale", vec![stale]).is_err());
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().wires.get(&w), Some(&created));
}

/// A page with a device of two connection points: the page, and the two points.
fn page_with_points(l: &mut Ledger, ids: &mut IdAllocator) -> (ecad_model::PageId, TerminalId, TerminalId) {
    use ecad_model::{Device, DeviceId, Page, PageId, Terminal};
    let (p, d, a, b): (PageId, DeviceId, TerminalId, TerminalId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let t = |label: &str| Terminal { component: d, label: Some(label.into()), key: Some(label.into()) };
    l.commit(
        Author::User,
        "setup",
        vec![
            Op::AddPage { id: p, page: Page::new("1"), index: 0 },
            Op::CreateDevice { id: d, device: Device::default() },
            Op::AddTerminal { id: a, terminal: t("1") },
            Op::AddTerminal { id: b, terminal: t("2") },
        ],
    )
    .unwrap();
    (p, a, b)
}

#[test]
fn routes_are_added_modified_removed_and_undone() {
    use ecad_model::{Pt, Route, RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(31);
    let mut l = Ledger::new(31);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (r1, r2): (RouteId, RouteId) = (ids.fresh(), ids.fresh());
    let ab = Route::new(p, vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)], [RouteEnd::Point(a), RouteEnd::Point(b)]);
    // A T: a second route landing on the middle of the first.
    let tee = Route::new(p, vec![Pt::new(5.0, 10.0), Pt::new(5.0, 0.0)], [RouteEnd::Free, RouteEnd::On { route: r1, at: Pt::new(5.0, 0.0) }]);
    l.commit(Author::User, "draw", vec![Op::AddRoute { id: r1, route: ab.clone() }, Op::AddRoute { id: r2, route: tee.clone() }]).unwrap();
    assert_eq!(l.document().routes.len(), 2);
    l.document().check_index().unwrap();
    let mut moved = ab.clone();
    moved.points.insert(1, Pt::new(5.0, -5.0));
    l.commit(Author::User, "move", vec![Op::ModifyRoute { id: r1, before: ab.clone(), after: moved.clone() }]).unwrap();
    assert_eq!(l.document().routes.get(&r1), Some(&moved));
    // A stale `before` is refused.
    assert!(l.commit(Author::User, "stale", vec![Op::ModifyRoute { id: r1, before: ab.clone(), after: ab.clone() }]).is_err());
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().routes.get(&r1), Some(&ab));
    // Removing the route a T lands on leaves the T dangling: refused by the invariant check.
    let e = l.commit(Author::User, "erase", vec![Op::RemoveRoute { id: r1, route: ab.clone() }]).unwrap_err();
    assert!(e.to_string().contains("not on its page"), "{e}");
    l.commit(Author::User, "erase", vec![Op::RemoveRoute { id: r2, route: tee }, Op::RemoveRoute { id: r1, route: ab }]).unwrap();
    assert!(l.document().routes.is_empty());
    l.document().check_index().unwrap();
}

#[test]
fn a_route_is_well_formed_and_lands_on_what_exists() {
    use ecad_model::{Pt, Route, RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(32);
    let mut l = Ledger::new(32);
    let (p, a, _) = page_with_points(&mut l, &mut ids);
    let r: RouteId = ids.fresh();
    let two = vec![Pt::new(0.0, 0.0), Pt::new(1.0, 0.0)];
    let cases: [(&str, Route); 5] = [
        ("one point", Route::new(p, vec![Pt::new(0.0, 0.0)], [RouteEnd::Free, RouteEnd::Free])),
        ("not finite", Route::new(p, vec![Pt::new(0.0, 0.0), Pt::new(f64::NAN, 0.0)], [RouteEnd::Free, RouteEnd::Free])),
        ("on itself", Route::new(p, two.clone(), [RouteEnd::On { route: r, at: Pt::new(0.0, 0.0) }, RouteEnd::Free])),
        ("missing point", Route::new(p, two.clone(), [RouteEnd::Point(TerminalId(12345)), RouteEnd::Free])),
        ("missing page", Route::new(ecad_model::PageId(999), two.clone(), [RouteEnd::Point(a), RouteEnd::Free])),
    ];
    for (what, route) in cases {
        assert!(l.commit(Author::User, what, vec![Op::AddRoute { id: r, route }]).is_err(), "{what} is refused");
    }
    let ok = Route::new(p, two, [RouteEnd::Point(a), RouteEnd::Free]);
    let text = serde_json::to_string(&ok).unwrap();
    assert_eq!(serde_json::from_str::<Route>(&text).unwrap(), ok);
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["layer"] = serde_json::json!("0");
    assert!(serde_json::from_value::<Route>(v).is_err(), "an unknown field is a read error");
    // Removing the connection point a route lands on is refused while the route lands there.
    l.commit(Author::User, "draw", vec![Op::AddRoute { id: r, route: ok }]).unwrap();
    let term = l.document().terminals.get(&a).cloned().unwrap();
    let e = l.commit(Author::User, "rm", vec![Op::RemoveTerminal { id: a, terminal: term }]).unwrap_err();
    assert!(e.to_string().contains("route"), "{e}");
}
