//! Lane 072 D (design 072 §4.2–§4.4, §4.6; acceptance A2, A4, A9, A12): the wire's typed values
//! round-trip in their stored form, a stored value in any other form is a read error, `ModifyWire`
//! changes a wire's values, is its own inverse and never moves its ends, and every edit of the
//! routes rewrites the wires of the nets whose points changed, in the same transaction.

use ecad_model::text::Text;
use ecad_model::units::Amps;
use ecad_model::wire::{Basis, Bundling, CircuitInputs, Colours, InstallMethod, SupplySystem, WireNumber, WireSize};
use ecad_model::{Author, ConductorId, IdAllocator, Ledger, NetId, Op, TerminalId, Wire, WireId, Word};

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
        size: WireSize::parse("1.5"),
        colours: Colours::parse("BU"),
        number: Some(WireNumber { text: Text::new("24V-07").unwrap(), fixed: true }),
        potential: Text::new("24V").ok(),
        load: Some(Amps(2.0)),
        ..Wire::new(NetId(1), ConductorId(2), TerminalId(4), TerminalId(5))
    };
    let text = serde_json::to_string(&w).unwrap();
    assert_eq!(serde_json::from_str::<Wire>(&text).unwrap(), w);
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["properties"] = serde_json::json!({});
    assert!(serde_json::from_value::<Wire>(v).is_err(), "an unknown field is a read error");
    let hidden = text.replace("24V-07", "24V\u{202e}07");
    assert!(serde_json::from_str::<Wire>(&hidden).is_err(), "a hidden character is a read error");
    assert!(w.values().iter().any(|(k, v)| *k == "wire number" && v == "24V-07"));
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["ends"] = serde_json::json!([4, null]);
    assert!(serde_json::from_value::<Wire>(v).is_err(), "a wire has two connection points");
}

#[test]
fn modify_wire_changes_values_undoes_and_never_moves_the_ends() {
    let mut ids = IdAllocator::seeded(72);
    let mut l = Ledger::new(72);
    let (_, a, b) = page_with_points(&mut l, &mut ids);
    let w: WireId = ids.fresh();
    let created = Wire::new(ids.fresh(), ids.fresh(), a, b);
    l.commit(Author::User, "wire", vec![Op::CreateWire { id: w, wire: created.clone() }]).unwrap();
    let mut after = created.clone();
    after.size = WireSize::parse("2.5");
    let op = Op::ModifyWire { id: w, before: created.clone(), after: after.clone() };
    assert!(matches!(op.inverse(), Op::ModifyWire { .. }));
    l.commit(Author::User, "size", vec![op]).unwrap();
    let now = l.document().wires.get(&w).unwrap().clone();
    assert_eq!(now.size.map(|s| s.to_string()).as_deref(), Some("2.5"));
    // Other ends or another conductor id are another wire: refused.
    let moved = Wire { ends: [b, a], ..now.clone() };
    assert!(l.commit(Author::User, "ends", vec![Op::ModifyWire { id: w, before: now.clone(), after: moved }]).is_err());
    let other = Wire { conductor: ids.fresh(), ..now.clone() };
    assert!(l.commit(Author::User, "id", vec![Op::ModifyWire { id: w, before: now.clone(), after: other }]).is_err());
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
    let t = |label: &str| Terminal { component: d, label: Some(label.into()), key: Some(label.parse().unwrap()), accepts: Default::default() };
    l.commit(
        Author::User,
        "setup",
        vec![
            Op::AddPage { id: p, page: Page::titled("1").unwrap(), index: 0 },
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

#[test]
fn routes_join_the_points_they_land_on_into_one_net() {
    use ecad_model::{Pt, Route, RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(33);
    let mut l = Ledger::new(33);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (_, c, _) = page_with_points(&mut l, &mut ids);
    let net = |l: &Ledger, t: TerminalId| l.document().connectivity().net_of_terminal(t);
    // The device bridges nothing: only routes join its points.
    assert_ne!(net(&l, a), net(&l, b));
    let (r1, r2): (RouteId, RouteId) = (ids.fresh(), ids.fresh());
    let ab = Route::new(p, vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)], [RouteEnd::Point(a), RouteEnd::Point(b)]);
    l.commit(Author::User, "a-b", vec![Op::AddRoute { id: r1, route: ab.clone() }]).unwrap();
    assert_eq!(net(&l, a), net(&l, b));
    assert_ne!(net(&l, a), net(&l, c));
    // C joins by a T onto the middle of A–B.
    let tee = Route::new(p, vec![Pt::new(5.0, 9.0), Pt::new(5.0, 0.0)], [RouteEnd::Point(c), RouteEnd::On { route: r1, at: Pt::new(5.0, 0.0) }]);
    l.commit(Author::User, "tee", vec![Op::AddRoute { id: r2, route: tee.clone() }]).unwrap();
    assert_eq!(net(&l, a), net(&l, c));
    l.document().check_index().unwrap();
    // Freeing the T's landing splits C off again.
    let freed = Route { ends: [RouteEnd::Point(c), RouteEnd::Free], ..tee.clone() };
    l.commit(Author::User, "lift", vec![Op::ModifyRoute { id: r2, before: tee, after: freed.clone() }]).unwrap();
    assert_ne!(net(&l, a), net(&l, c));
    assert_eq!(net(&l, a), net(&l, b));
    l.document().check_index().unwrap();
    // Erasing A–B splits A from B.
    l.commit(Author::User, "erase", vec![Op::RemoveRoute { id: r1, route: ab }]).unwrap();
    assert_ne!(net(&l, a), net(&l, b));
    l.document().check_index().unwrap();
    l.undo(Author::User).unwrap();
    assert_eq!(net(&l, a), net(&l, b));
    l.document().check_index().unwrap();
}

/// The wires of `l`, as `(net, net_order, [end, end])`, in net then order. A wire's two ends are
/// a pair: they are listed in ID order.
fn wires(l: &Ledger) -> Vec<(NetId, u16, [TerminalId; 2])> {
    let pair = |e: [TerminalId; 2]| if e[0] <= e[1] { e } else { [e[1], e[0]] };
    let mut v: Vec<_> = l.document().wires.values().map(|w| (w.net, w.net_order, pair(w.ends))).collect();
    v.sort();
    v
}

/// `v` as [`wires`] lists it: each pair in ID order, sorted.
fn want(v: Vec<(NetId, u16, [TerminalId; 2])>) -> Vec<(NetId, u16, [TerminalId; 2])> {
    let mut v: Vec<_> = v.into_iter().map(|(n, o, e)| (n, o, if e[0] <= e[1] { e } else { [e[1], e[0]] })).collect();
    v.sort();
    v
}

/// One route on page `p` from `from` to `to` through `pts`.
fn route(p: ecad_model::PageId, pts: &[(f64, f64)], from: ecad_model::RouteEnd, to: ecad_model::RouteEnd) -> ecad_model::Route {
    ecad_model::Route::new(p, pts.iter().map(|(x, y)| ecad_model::Pt::new(*x, *y)).collect(), [from, to])
}

#[test]
fn drawing_routes_makes_wires_in_drawing_order_and_a_tee_joins_from_the_nearer_end() {
    use ecad_model::{Pt, RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(41);
    let mut l = Ledger::new(41);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (_, c, d) = page_with_points(&mut l, &mut ids);
    let (r1, r2, r3): (RouteId, RouteId, RouteId) = (ids.fresh(), ids.fresh(), ids.fresh());
    // A–B: one wire, first in a new net.
    let ab = route(p, &[(0.0, 0.0), (10.0, 0.0)], RouteEnd::Point(a), RouteEnd::Point(b));
    l.commit(Author::User, "a-b", vec![Op::AddRoute { id: r1, route: ab.clone() }]).unwrap();
    let w = wires(&l);
    assert_eq!(w.len(), 1);
    let net = w[0].0;
    assert_eq!((w[0].1, w[0].2), (1, [a, b]));
    let ab_wire = *l.document().wires.keys().next().unwrap();
    // C onto the middle of A–B, nearer B: 1 = A–B, 2 = B–C.
    let tee = route(p, &[(8.0, 5.0), (8.0, 0.0)], RouteEnd::Point(c), RouteEnd::On { route: r1, at: Pt::new(8.0, 0.0) });
    l.commit(Author::User, "tee", vec![Op::AddRoute { id: r2, route: tee.clone() }]).unwrap();
    assert_eq!(wires(&l), want(vec![(net, 1, [a, b]), (net, 2, [b, c])]));
    assert!(l.document().wires.contains_key(&ab_wire), "A–B keeps its wire");
    // D onto the middle of A–B nearer A: A has room, so 3 = A–D.
    let tee2 = route(p, &[(2.0, 5.0), (2.0, 0.0)], RouteEnd::Point(d), RouteEnd::On { route: r1, at: Pt::new(2.0, 0.0) });
    l.commit(Author::User, "tee2", vec![Op::AddRoute { id: r3, route: tee2 }]).unwrap();
    assert_eq!(wires(&l), want(vec![(net, 1, [a, b]), (net, 2, [b, c]), (net, 3, [a, d])]));
    // One undo takes the route and its wire back together.
    l.undo(Author::User).unwrap();
    assert_eq!(wires(&l), want(vec![(net, 1, [a, b]), (net, 2, [b, c])]));
    l.document().check_index().unwrap();
    // Lifting C's T: C leaves, its wire goes, A–B is untouched.
    let lifted = ecad_model::Route { ends: [RouteEnd::Point(c), RouteEnd::Free], ..tee.clone() };
    l.commit(Author::User, "lift", vec![Op::ModifyRoute { id: r2, before: tee, after: lifted }]).unwrap();
    assert_eq!(wires(&l), want(vec![(net, 1, [a, b])]));
    assert!(l.document().wires.contains_key(&ab_wire));
    // Redrawing A–B through other places on the same points leaves the wires alone.
    let mut bent = ab.clone();
    bent.points.insert(1, Pt::new(5.0, -5.0));
    l.commit(Author::User, "bend", vec![Op::ModifyRoute { id: r1, before: ab, after: bent.clone() }]).unwrap();
    assert_eq!(l.transaction(l.head()).map(|t| t.ops.len()), Some(1), "no wire ops when no point changed");
    // Erasing A–B: its wire goes.
    l.commit(Author::User, "erase", vec![Op::RemoveRoute { id: r1, route: bent }]).unwrap();
    assert!(wires(&l).is_empty());
    l.document().check_index().unwrap();
}

#[test]
fn joining_two_nets_keeps_the_bigger_ones_id_and_splitting_gives_the_smaller_a_new_one() {
    use ecad_model::{RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(42);
    let mut l = Ledger::new(42);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (_, c, d) = page_with_points(&mut l, &mut ids);
    let (_, e, _) = page_with_points(&mut l, &mut ids);
    let r: Vec<RouteId> = (0..4).map(|_| ids.fresh()).collect();
    let line = |x: f64, from: TerminalId, to: TerminalId| route(p, &[(x, 0.0), (x + 10.0, 0.0)], RouteEnd::Point(from), RouteEnd::Point(to));
    // Net 1: A–B–C (two wires); net 2: D–E (one wire).
    l.commit(Author::User, "abc", vec![Op::AddRoute { id: r[0], route: line(0.0, a, b) }, Op::AddRoute { id: r[1], route: line(10.0, b, c) }])
        .unwrap();
    l.commit(Author::User, "de", vec![Op::AddRoute { id: r[2], route: line(40.0, d, e) }]).unwrap();
    let net_of = |l: &Ledger, t: TerminalId| l.document().wires.values().find(|w| w.ends.contains(&t)).map(|w| w.net).unwrap();
    let (n1, n2) = (net_of(&l, a), net_of(&l, d));
    assert_ne!(n1, n2);
    assert_eq!(wires(&l).iter().filter(|w| w.0 == n1).count(), 2);
    let de_conductor = l.document().wires.values().find(|w| w.ends.contains(&d)).unwrap().conductor;
    // C–D joins them: net 1 keeps its id; its order stays, C–D follows, then D–E.
    let cd = line(30.0, c, d);
    l.commit(Author::User, "cd", vec![Op::AddRoute { id: r[3], route: cd.clone() }]).unwrap();
    assert_eq!(wires(&l), want(vec![(n1, 1, [a, b]), (n1, 2, [b, c]), (n1, 3, [c, d]), (n1, 4, [d, e])]));
    let de = l.document().wires.values().find(|w| w.ends.contains(&e)).unwrap();
    assert_eq!(de.conductor, de_conductor, "D–E keeps its conductor id");
    // Erasing C–D splits them again: net 1 keeps A–B–C; D–E gets a new net id.
    l.commit(Author::User, "split", vec![Op::RemoveRoute { id: r[3], route: cd }]).unwrap();
    let n3 = net_of(&l, d);
    assert_eq!(net_of(&l, a), n1);
    assert_ne!(n3, n1);
    assert_eq!(wires(&l).len(), 3);
    l.document().check_index().unwrap();
}

#[test]
fn a_point_cut_out_of_a_chain_joins_its_neighbours() {
    use ecad_model::{RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(43);
    let mut l = Ledger::new(43);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (_, c, _) = page_with_points(&mut l, &mut ids);
    let (r1, r2, r3): (RouteId, RouteId, RouteId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let line = |from: TerminalId, to: TerminalId| route(p, &[(0.0, 0.0), (10.0, 0.0)], RouteEnd::Point(from), RouteEnd::Point(to));
    // A–B–C, then A–C drawn too: no new point, no new wire.
    l.commit(Author::User, "abc", vec![Op::AddRoute { id: r1, route: line(a, b) }, Op::AddRoute { id: r2, route: line(b, c) }]).unwrap();
    l.commit(Author::User, "ac", vec![Op::AddRoute { id: r3, route: line(a, c) }]).unwrap();
    let net = l.document().wires.values().next().unwrap().net;
    assert_eq!(wires(&l), want(vec![(net, 1, [a, b]), (net, 2, [b, c])]));
    // B's routes go: B leaves, A–C takes its place.
    l.commit(Author::User, "cut b", vec![Op::RemoveRoute { id: r1, route: line(a, b) }, Op::RemoveRoute { id: r2, route: line(b, c) }]).unwrap();
    assert_eq!(wires(&l), want(vec![(net, 1, [a, c])]));
    l.document().check_index().unwrap();
}

/// The core of cable `k`, as stored.
fn core_of(l: &Ledger, k: ecad_model::CableId) -> Vec<ecad_model::CorePosition> {
    l.document().cables.get(&k).map(|c| c.cores.iter().map(|m| m.position).collect()).unwrap_or_default()
}

#[test]
fn a_core_on_a_loose_line_moves_onto_the_wire_when_the_second_end_lands() {
    // Owner ruling, 2026-11-18: a loose line carries no wire; a core on it sits on the route with
    // its landed end; the command landing the second end moves it onto the wire, in the same
    // transaction, and one undo takes both back.
    use ecad_model::{Cable, CableId, Core, CorePosition, RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(43);
    let mut l = Ledger::new(43);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (r, k): (RouteId, CableId) = (ids.fresh(), ids.fresh());
    let loose = route(p, &[(0.0, 0.0), (5.0, 0.0)], RouteEnd::Point(a), RouteEnd::Free);
    l.commit(Author::User, "loose", vec![Op::AddRoute { id: r, route: loose.clone() }]).unwrap();
    assert!(wires(&l).is_empty(), "a loose line carries no wire");
    let on_line =
        Core { position: CorePosition::Route { route: r, landed: Some(a) }, ..ecad_model::Core::unplaced(ecad_model::CoreLabel::new("1").ok()) };
    l.commit(
        Author::User,
        "slash",
        vec![Op::CreateCable { id: k, cable: Cable::default() }, Op::AddCableMember { cable: k, index: 0, member: on_line }],
    )
    .unwrap();
    assert_eq!(l.document().cables_on_route(r), vec![k]);
    // The second end lands on B: the wire forms and the core moves onto it, keeping its label.
    let landed = ecad_model::Route { ends: [RouteEnd::Point(a), RouteEnd::Point(b)], ..loose.clone() };
    l.commit(Author::User, "land", vec![Op::ModifyRoute { id: r, before: loose.clone(), after: landed.clone() }]).unwrap();
    let w = *l.document().wires.keys().next().unwrap();
    assert_eq!(core_of(&l, k), vec![CorePosition::Wire(w)]);
    assert_eq!(l.document().cables.get(&k).unwrap().cores[0].label_str(), Some("1"));
    assert_eq!(l.document().cables_of(w), vec![k]);
    assert!(l.document().cables_on_route(r).is_empty());
    l.document().check_index().unwrap();
    // One undo: no wire, the core back on the line.
    l.undo(Author::User).unwrap();
    assert!(wires(&l).is_empty());
    assert_eq!(core_of(&l, k), vec![CorePosition::Route { route: r, landed: Some(a) }]);
    l.redo(Author::User).unwrap();
    // Lifting the end again: the wire goes, the core stays on the line, landed at A.
    l.commit(Author::User, "lift", vec![Op::ModifyRoute { id: r, before: landed, after: loose.clone() }]).unwrap();
    assert!(wires(&l).is_empty());
    assert_eq!(core_of(&l, k), vec![CorePosition::Route { route: r, landed: Some(a) }]);
    // Lifting the other end too: still on the line, no end landed.
    let free = ecad_model::Route { ends: [RouteEnd::Free, RouteEnd::Free], ..loose.clone() };
    l.commit(Author::User, "free", vec![Op::ModifyRoute { id: r, before: loose, after: free.clone() }]).unwrap();
    assert_eq!(core_of(&l, k), vec![CorePosition::Route { route: r, landed: None }]);
    // Erasing the line: the labelled core is one of the cable's core set and stays, unplaced.
    l.commit(Author::User, "erase", vec![Op::RemoveRoute { id: r, route: free }]).unwrap();
    assert_eq!(core_of(&l, k), vec![CorePosition::Unplaced]);
    l.document().check_index().unwrap();
}

#[test]
fn a_core_must_sit_on_something_that_exists_and_once() {
    use ecad_model::{Cable, CableId, Core, CorePosition, RouteId};
    let mut ids = IdAllocator::seeded(44);
    let mut l = Ledger::new(44);
    let (_, a, _) = page_with_points(&mut l, &mut ids);
    let (k, r): (CableId, RouteId) = (ids.fresh(), ids.fresh());
    let missing = Core { position: CorePosition::Route { route: r, landed: Some(a) }, ..ecad_model::Core::unplaced(None) };
    let refused = l.commit(
        Author::User,
        "bad",
        vec![Op::CreateCable { id: k, cable: Cable::default() }, Op::AddCableMember { cable: k, index: 0, member: missing }],
    );
    assert!(refused.is_err(), "a core on a route that does not exist is refused");
    // Unplaced cores are allowed, any number of them.
    let unplaced = Core { position: CorePosition::Unplaced, ..ecad_model::Core::unplaced(None) };
    l.commit(
        Author::User,
        "spare",
        vec![
            Op::CreateCable { id: k, cable: Cable::default() },
            Op::AddCableMember { cable: k, index: 0, member: unplaced.clone() },
            Op::AddCableMember { cable: k, index: 1, member: unplaced },
        ],
    )
    .unwrap();
    assert_eq!(core_of(&l, k), vec![CorePosition::Unplaced, CorePosition::Unplaced]);
}

#[test]
fn a_point_drawn_into_a_wires_line_splits_the_wire_and_a_new_net_starts_where_it_was_drawn() {
    use ecad_model::{RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(45);
    let mut l = Ledger::new(45);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (_, t, _) = page_with_points(&mut l, &mut ids);
    let (r1, r2): (RouteId, RouteId) = (ids.fresh(), ids.fresh());
    // Drawn from B to A: the wire runs B→A, whichever ID is lower.
    let ba = route(p, &[(10.0, 0.0), (0.0, 0.0)], RouteEnd::Point(b), RouteEnd::Point(a));
    l.commit(Author::User, "b-a", vec![Op::AddRoute { id: r1, route: ba.clone() }]).unwrap();
    let w = l.document().wires.values().next().unwrap().clone();
    assert_eq!(w.ends, [b, a], "the wire starts where its line was drawn from");
    // A terminal T placed on the line: B–T and T–A, in the wire's place; no B–A left.
    let bt = route(p, &[(10.0, 0.0), (5.0, 0.0)], RouteEnd::Point(b), RouteEnd::Point(t));
    let ta = route(p, &[(5.0, 0.0), (0.0, 0.0)], RouteEnd::Point(t), RouteEnd::Point(a));
    l.commit(Author::User, "terminal", vec![Op::ModifyRoute { id: r1, before: ba, after: bt }, Op::AddRoute { id: r2, route: ta }]).unwrap();
    assert_eq!(wires(&l), want(vec![(w.net, 1, [b, t]), (w.net, 2, [t, a])]));
}

/// The wire numbers of `l`'s wires, in net order.
fn numbers(l: &Ledger) -> Vec<Option<String>> {
    let mut v: Vec<_> = l.document().wires.values().map(|w| (w.net, w.net_order, w.number_text().map(str::to_string))).collect();
    v.sort();
    v.into_iter().map(|x| x.2).collect()
}

/// One edit as the app commits it: with the numbering pass ([`ecad_model::wiring::assign`]).
fn edit(l: &mut Ledger, label: &str, ops: Vec<Op>) {
    l.commit_extended(Author::User, label, ops, None, ecad_model::wiring::assign).unwrap();
}

/// `l`'s project settings with `wiring.number_by` set to `by`, as one edit.
fn number_by(l: &mut Ledger, by: ecad_model::wiring::NumberBy) {
    let before = l.document().settings.clone();
    let mut after = before.clone();
    after.wiring.number_by = by;
    edit(l, "number by", vec![Op::SetProjectSettings { before, after }]);
}

#[test]
fn number_by_switches_every_shown_number_between_net_and_conductor_in_one_edit() {
    use ecad_model::RouteEnd;
    use ecad_model::wiring::{self, NumberBy};
    let mut ids = IdAllocator::seeded(43);
    let mut l = Ledger::new(43);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (_, c, _) = page_with_points(&mut l, &mut ids);
    let (r1, r2): (ecad_model::RouteId, ecad_model::RouteId) = (ids.fresh(), ids.fresh());
    let ab = route(p, &[(0.0, 0.0), (10.0, 0.0)], RouteEnd::Point(a), RouteEnd::Point(b));
    let bc = route(p, &[(10.0, 0.0), (20.0, 0.0)], RouteEnd::Point(b), RouteEnd::Point(c));
    edit(&mut l, "a-b-c", vec![Op::AddRoute { id: r1, route: ab }, Op::AddRoute { id: r2, route: bc }]);
    // Per net (the default): both wires of the net show the net's one number.
    let per_net = numbers(&l);
    assert_eq!(per_net.len(), 2);
    assert!(per_net[0].is_some() && per_net[0] == per_net[1], "{per_net:?}");
    let first = per_net[0].clone();
    // Per conductor: one edit gives each wire its own; the first in net order keeps the old one.
    number_by(&mut l, NumberBy::Conductor);
    let per_wire = numbers(&l);
    assert_eq!(per_wire[0], first);
    assert!(per_wire[1].is_some() && per_wire[1] != per_wire[0], "{per_wire:?}");
    let tx = l.transaction(l.head()).unwrap();
    assert!(tx.ops.iter().any(|o| matches!(o, Op::ModifyWire { .. })), "the numbers change in the settings edit itself");
    let doc = l.document();
    for (id, w) in &doc.wires {
        assert_eq!(wiring::wire_number(doc, *id).as_deref(), w.number_text(), "a wire shows its own number");
    }
    // Undo puts the settings and the numbers back together.
    l.undo(Author::User).unwrap();
    assert_eq!(numbers(&l), per_net);
    l.redo(Author::User).unwrap();
    assert_eq!(numbers(&l), per_wire);
    // Renumbering per conductor keeps one number per wire.
    let ops = wiring::renumber(l.document(), true);
    if !ops.is_empty() {
        edit(&mut l, "renumber", ops);
    }
    let renumbered = numbers(&l);
    assert!(renumbered.iter().all(Option::is_some) && renumbered[0] != renumbered[1], "{renumbered:?}");
    // Back to per net: every wire shows the net's number again, the first wire's.
    number_by(&mut l, NumberBy::Net);
    let back = numbers(&l);
    assert_eq!(back[0], renumbered[0]);
    assert_eq!(back[0], back[1]);
    l.document().check_index().unwrap();
}

#[test]
fn accepts_is_one_to_sixty_four_and_anything_else_is_a_read_error() {
    use ecad_model::{Accepts, Terminal};
    let read = |json: &str| serde_json::from_str::<Terminal>(json).map(|t| t.accepts.get()).map_err(|e| e.to_string());
    let base = r#"{"component":"00000000000000000000000000000001","label":null,"key":null"#;
    assert_eq!(read(&format!("{base}}}")), Ok(2), "left out: two");
    assert_eq!(read(&format!(r#"{base},"accepts":6}}"#)), Ok(6));
    assert_eq!(read(&format!(r#"{base},"accepts":64}}"#)), Ok(64));
    for hostile in ["0", "65", "255", "256", "18446744073709551615", "-1", "2.5", "\"2\""] {
        let e = read(&format!(r#"{base},"accepts":{hostile}}}"#));
        assert!(e.is_err(), "{hostile} must not read: {e:?}");
    }
    assert!(read(&format!(r#"{base},"accepts":0}}"#)).unwrap_err().contains("1 to 64"));
    let t = Terminal { component: ecad_model::DeviceId(1), label: None, key: None, accepts: Accepts::TWO };
    assert!(!serde_json::to_string(&t).unwrap().contains("accepts"), "two is left out");
}

/// A page with a device of two points, the first accepting `n` conductors.
fn page_with_bar(l: &mut Ledger, ids: &mut IdAllocator, n: u64) -> (ecad_model::PageId, TerminalId, TerminalId) {
    use ecad_model::{Accepts, Device, DeviceId, Page, PageId, Terminal};
    let (p, d, a, b): (PageId, DeviceId, TerminalId, TerminalId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let t = |label: &str, accepts| Terminal { component: d, label: Some(label.into()), key: Some(label.parse().unwrap()), accepts };
    l.commit(
        Author::User,
        "setup",
        vec![
            Op::AddPage { id: p, page: Page::titled("1").unwrap(), index: 0 },
            Op::CreateDevice { id: d, device: Device::default() },
            Op::AddTerminal { id: a, terminal: t("PE", Accepts::new(n).unwrap()) },
            Op::AddTerminal { id: b, terminal: t("2", Accepts::TWO) },
        ],
    )
    .unwrap();
    (p, a, b)
}

#[test]
fn a_point_accepting_more_than_two_fans_out_and_one_accepting_two_chains() {
    // Design 072 §4.4 (F38, A17): Ts onto A–B near A join from A while A has room (a star at a
    // point accepting four); at a point accepting two the second T goes to the far end.
    use ecad_model::{Pt, RouteEnd, RouteId};
    for (n, want_far) in [(4u64, false), (2, true)] {
        let mut ids = IdAllocator::seeded(44 + n);
        let mut l = Ledger::new(44 + n);
        let (p, a, b) = page_with_bar(&mut l, &mut ids, n);
        let (_, c, d) = page_with_points(&mut l, &mut ids);
        let (r1, r2, r3): (RouteId, RouteId, RouteId) = (ids.fresh(), ids.fresh(), ids.fresh());
        l.commit(
            Author::User,
            "a-b",
            vec![Op::AddRoute { id: r1, route: route(p, &[(0.0, 0.0), (10.0, 0.0)], RouteEnd::Point(a), RouteEnd::Point(b)) }],
        )
        .unwrap();
        let tee = |q: TerminalId, x: f64| route(p, &[(x, 5.0), (x, 0.0)], RouteEnd::Point(q), RouteEnd::On { route: r1, at: Pt::new(x, 0.0) });
        l.commit(Author::User, "c", vec![Op::AddRoute { id: r2, route: tee(c, 2.0) }]).unwrap();
        l.commit(Author::User, "d", vec![Op::AddRoute { id: r3, route: tee(d, 3.0) }]).unwrap();
        let w = wires(&l);
        let net = w[0].0;
        let third = if want_far { [b, d] } else { [a, d] };
        assert_eq!(w, want(vec![(net, 1, [a, b]), (net, 2, [a, c]), (net, 3, third)]), "accepts {n}");
        let doc = l.document();
        assert_eq!(ecad_model::wire_order::accepts(doc, a), n as usize);
        assert!(doc.ends_at(a).len() <= n as usize, "never more than it accepts here");
    }
}

#[test]
fn a_cores_purpose_is_kept_apart_from_its_position() {
    // Design 072 §4.5 (c1): moving a core from its line to the wire keeps its purpose and colours;
    // erasing a line keeps a labelled or spare core unplaced and drops an unlabelled in-use one.
    use ecad_model::{Cable, CableId, Core, CoreLabel, CorePosition, Purpose, RouteEnd, RouteId};
    let mut ids = IdAllocator::seeded(46);
    let mut l = Ledger::new(46);
    let (p, a, b) = page_with_points(&mut l, &mut ids);
    let (r, r2, k): (RouteId, RouteId, CableId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let loose = route(p, &[(0.0, 0.0), (5.0, 0.0)], RouteEnd::Point(a), RouteEnd::Free);
    let other = route(p, &[(0.0, 9.0), (5.0, 9.0)], RouteEnd::Free, RouteEnd::Free);
    let spare =
        Core { label: None, colours: Colours::parse("BU"), purpose: Purpose::Spare, position: CorePosition::Route { route: r, landed: Some(a) } };
    let plain = Core { position: CorePosition::Route { route: r2, landed: None }, ..Core::unplaced(None) };
    l.commit(
        Author::User,
        "slash",
        vec![
            Op::AddRoute { id: r, route: loose.clone() },
            Op::AddRoute { id: r2, route: other.clone() },
            Op::CreateCable { id: k, cable: Cable::default() },
            Op::AddCableMember { cable: k, index: 0, member: spare.clone() },
            Op::AddCableMember { cable: k, index: 1, member: plain },
        ],
    )
    .unwrap();
    let landed = ecad_model::Route { ends: [RouteEnd::Point(a), RouteEnd::Point(b)], ..loose.clone() };
    l.commit(Author::User, "land", vec![Op::ModifyRoute { id: r, before: loose, after: landed.clone() }]).unwrap();
    let w = *l.document().wires.keys().next().unwrap();
    let cores = l.document().cables.get(&k).unwrap().cores.clone();
    assert_eq!(cores[0], Core { position: CorePosition::Wire(w), ..spare.clone() }, "purpose and colours ride along");
    // Erase both lines: the spare core stays unplaced, the unlabelled in-use core goes.
    l.commit(Author::User, "erase", vec![Op::RemoveRoute { id: r, route: landed }, Op::RemoveRoute { id: r2, route: other }]).unwrap();
    let cores = l.document().cables.get(&k).unwrap().cores.clone();
    assert_eq!(cores, vec![Core { position: CorePosition::Unplaced, ..spare }]);
    l.document().check_index().unwrap();
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().cables.get(&k).unwrap().cores.len(), 2, "one undo brings both back");
    // Strict reads: a purpose word ECAD does not know, or a label with spaces at its ends, is refused.
    for (bad, why) in [
        (r#"{"label":null,"colours":null,"purpose":"unused","position":"unplaced"}"#, "purpose"),
        (r#"{"label":" 1","colours":null,"purpose":"spare","position":"unplaced"}"#, "label"),
    ] {
        assert!(serde_json::from_str::<Core>(bad).is_err(), "{why}: {bad}");
    }
    let good: Core = serde_json::from_str(r#"{"label":"PE","colours":"GN/YE","purpose":"spare","position":"unplaced"}"#).unwrap();
    assert_eq!(good.label, CoreLabel::new("PE").ok());
    assert!(good.is_spare());
}
