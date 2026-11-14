//! Brief 052a (design 052 §3, §4, §10.1–5): representation types and relations, and no `main`
//! role. Every symbol of a device is equal; its type comes from its definition.

use crate::common::{H, id};

use ecad_cadcraft::reps::{self, RepType, View};
use ecad_cadcraft::{connectors, locations, xref};
use ecad_model::{ComponentId, Document, PageId, RepresentationId, Subject};
use serde_json::{Value, json};

const SEED: u64 = 52;

fn rep(v: &Value) -> RepresentationId {
    id(v, "representation")
}

fn page(h: &mut H, number: &str) -> PageId {
    id(&h.run("newpage", json!({"title": number, "number": number})).unwrap(), "page")
}

fn contact_text(h: &mut H, page: PageId, r: RepresentationId) -> Option<String> {
    h.p(|p| {
        xref::parts_on_page(p.ledger.document(), page).into_iter().find_map(|part| match part.shape {
            xref::Shape::Text { value, .. } if part.rep == r && part.index == 0 => Some(value),
            _ => None,
        })
    })
}

fn no_main(d: &Document) {
    let mains: Vec<_> = d.representations.iter().filter(|(_, r)| r.role == "main").map(|(id, _)| *id).collect();
    assert!(mains.is_empty(), "no command writes role main: {mains:?}");
}

/// §10.1: a coil and a joined contact get their starter keys as roles; nothing is `main`.
#[test]
fn roles_are_starter_keys_and_never_main() {
    let mut h = H::new(SEED);
    let p1 = h.first_page();
    h.run("pageprops", json!({"page": p1.hex(), "number": "100"})).unwrap();
    let coil = h.run("place", json!({"symbol": "coil", "at": [75, 180]})).unwrap();
    let k1: ComponentId = id(&coil, "component");
    page(&mut h, "105");
    let no = h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k1.hex()})).unwrap();
    // A connector, a pin of it, a power pole set, an MCB.
    let pins = h.run("place", json!({"symbol": "pins", "at": [10, 10]})).unwrap();
    let x = id::<ComponentId>(&pins, "component");
    h.run("place", json!({"symbol": "pinp", "device": x.hex(), "at": [30, 10]})).unwrap();
    h.run("place", json!({"symbol": "power3", "at": [125, 180], "device": k1.hex()})).unwrap();
    h.run("place", json!({"symbol": "mcb1", "at": [250, 150]})).unwrap();
    h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, 50]})).unwrap();
    let d = h.doc();
    no_main(&d);
    assert_eq!(d.representations[&rep(&coil)].role, "coil");
    assert_eq!(d.representations[&rep(&no)].role, "no");
    assert_eq!(reps::rep_type(&d, &d.representations[&rep(&coil)]), RepType::Head);
    assert_eq!(reps::rep_type(&d, &d.representations[&rep(&no)]), RepType::Contact);
}

/// §3.1: the type comes from the definition kind, never from order or role.
#[test]
fn types_follow_the_definition() {
    let mut h = H::new(SEED);
    let cases = [
        ("coil", RepType::Head),
        ("mcb1", RepType::Head),
        ("no", RepType::Contact),
        ("nc", RepType::Contact),
        ("power3", RepType::Contact),
        ("pins", RepType::Pin),
        ("di", RepType::Channel),
    ];
    for (i, (symbol, want)) in cases.into_iter().enumerate() {
        let v = h.run("place", json!({"symbol": symbol, "at": [20.0 + 30.0 * i as f64, 100.0]}));
        let Ok(v) = v else { continue };
        let d = h.doc();
        let r = &d.representations[&rep(&v)];
        assert_eq!(reps::rep_type(&d, r), want, "{symbol}");
        assert_eq!(reps::view(&d, r), View::Schematic);
        assert_eq!(RepType::of_word(want.word()), Some(want));
    }
    assert_eq!(RepType::Head.numbering_rank(), 0);
    assert_eq!(RepType::Contact.numbering_rank(), 1);
    assert_eq!(RepType::Terminal.numbering_rank(), 1);
    assert_eq!(RepType::Other.numbering_rank(), 1);
}

/// §10.2: three contacts joined first, the coil last on another page: each contact points at the
/// coil, the coil is the head and the parent, and joining a second coil is refused.
#[test]
fn a_coil_placed_after_its_contacts_is_the_head() {
    let mut h = H::new(SEED);
    let p1 = h.first_page();
    h.run("pageprops", json!({"page": p1.hex(), "number": "100"})).unwrap();
    let first = h.run("place", json!({"symbol": "no", "at": [100, 150]})).unwrap();
    let k: ComponentId = id(&first, "component");
    let c2 = h.run("place", json!({"symbol": "nc", "at": [150, 150], "device": k.hex()})).unwrap();
    let c3 = h.run("place", json!({"symbol": "no", "at": [200, 150], "device": k.hex()})).unwrap();
    let p3 = page(&mut h, "103");
    let coil = h.run("place", json!({"symbol": "coil", "at": [75, 180], "device": k.hex()})).unwrap();
    let coil = rep(&coil);
    let contacts = [rep(&first), rep(&c2), rep(&c3)];
    let d = h.doc();
    no_main(&d);
    assert_eq!(reps::head(&d, k), Some(coil));
    assert_eq!(xref::device(&d, k).head, Some(coil));
    assert_eq!(reps::goto(&d, k, None), Some(coil));
    assert_eq!(locations::main_rep(&d, k).map(|(r, _)| r), Some(coil));
    for c in contacts {
        assert_eq!(reps::parent(&d, c), Some(coil), "{c:?}");
        assert!(reps::siblings(&d, c).iter().all(|s| contacts.contains(s) && *s != c));
        assert!(reps::views(&d, c).is_empty(), "one view only");
        let t = contact_text(&mut h, p1, c).unwrap_or_default();
        assert!(t.starts_with("103"), "{c:?} points at the coil on 103: {t}");
    }
    // Children in book order, then top-left: x 100, 150, 200 on one page.
    assert_eq!(reps::children(&d, coil), contacts.to_vec());
    assert!(reps::children(&d, contacts[0]).is_empty(), "a contact has no children");
    let _ = p3;
    // 018: the head is the coil, whichever was placed first.
    let e = h.run("place", json!({"symbol": "coil", "at": [150, 180], "device": k.hex()})).unwrap_err();
    assert!(e.contains("already has a coil"), "{e}");
}

/// §10.2, Q5: renumbering orders a device by its head's position, not by its first symbol.
#[test]
fn renumber_orders_a_device_by_its_head() {
    let mut h = H::new(SEED);
    // Page 1: X's contact at the left, Y's coil to its right. Page 2: X's coil.
    let x = id::<ComponentId>(&h.run("place", json!({"symbol": "no", "at": [50, 150], "tag": "-K09"})).unwrap(), "component");
    let y = id::<ComponentId>(&h.run("place", json!({"symbol": "coil", "at": [250, 150], "tag": "-K08"})).unwrap(), "component");
    page(&mut h, "2");
    h.run("place", json!({"symbol": "coil", "at": [50, 150], "device": x.hex()})).unwrap();
    h.run("renumber", json!({"kind": "devices", "scope": "all", "start": 1})).unwrap();
    let d = h.doc();
    let tag = |c: ComponentId| d.components[&c].properties.get("tag").cloned().unwrap_or_default();
    assert_eq!(tag(y), "-K01", "Y's coil is on page 1");
    assert_eq!(tag(x), "-K02", "X is numbered by its coil on page 2, not its contact on page 1");
}

/// Two heads (only from a merge or an edited file): the head is the first in book order, never
/// the lowest ID; reorder the pages and the other becomes the head.
#[test]
fn of_two_heads_the_first_in_book_order_is_the_head() {
    let mut h = H::new(SEED);
    let p1 = h.first_page();
    let a = h.run("place", json!({"symbol": "coil", "at": [75, 180]})).unwrap();
    let k: ComponentId = id(&a, "component");
    let no = rep(&h.run("place", json!({"symbol": "no", "at": [150, 150], "device": k.hex()})).unwrap());
    let p2 = page(&mut h, "2");
    let b = h.run("place", json!({"symbol": "coil", "at": [75, 180]})).unwrap();
    let (a, b) = (rep(&a), rep(&b));
    let mut d = h.doc();
    // Give the second coil to device k, as an edited file or a branch merge could.
    if let Some(r) = d.representations.get_mut(&b) {
        r.subject = Subject::Component(k);
    }
    d.rebuild_index();
    assert_eq!(reps::heads(&d, k), vec![a, b]);
    assert_eq!(reps::head(&d, k), Some(a));
    assert_eq!(xref::device(&d, k).head, Some(a));
    assert_eq!(xref::class_in(&d, b).map(|(_, c)| c), Some(xref::Class::Other));
    assert_eq!(reps::parent(&d, no), Some(a));
    // Swap the pages: the coil on page 2 is now first in the book.
    d.page_order = vec![p2, p1];
    assert_eq!(reps::head(&d, k), Some(b));
    assert_eq!(xref::device(&d, k).head, Some(b));
    assert_eq!(reps::parent(&d, no), Some(b));
    assert!(reps::children(&d, a).is_empty(), "a second head has no children");
}

/// §10.4, Q4: a pin-only connector half has no half anchor; each pin shows its mate.
#[test]
fn a_pin_only_half_has_no_half_member() {
    let mut h = H::new(SEED);
    let p1 = h.first_page();
    let s = h.run("place", json!({"symbol": "pins", "at": [10, 10]})).unwrap();
    let x: ComponentId = id(&s, "component");
    let p2 = page(&mut h, "2");
    let p = h.run("place", json!({"symbol": "pinp", "device": x.hex(), "at": [30, 10]})).unwrap();
    let (s, p) = (rep(&s), rep(&p));
    let d = h.doc();
    let classes = connectors::ref_classes(&d, x);
    assert!(classes.iter().all(|(_, c)| matches!(c, xref::Class::Pin(_))), "{classes:?}");
    let edges = connectors::ref_edges(&d, x, &classes);
    assert!(edges.contains(&(s, p)) && edges.contains(&(p, s)), "{edges:?}");
    assert_eq!(reps::rep_type(&d, &d.representations[&s]), RepType::Pin);
    assert_eq!(reps::siblings(&d, s), vec![p], "the mate first");
    assert_eq!(reps::half_anchor(&d, x, ecad_cadcraft::symbols::Side::Socket), None);
    assert_eq!(connectors::half_goto(&d, x, ecad_cadcraft::symbols::Side::Plug), Some(p));
    let _ = (p1, p2);
}

/// §10.5: a project whose contact carries role `main` behaves exactly as with its starter key.
#[test]
fn a_stored_main_role_means_nothing() {
    let mut h = H::new(SEED);
    let p1 = h.first_page();
    let first = h.run("place", json!({"symbol": "no", "at": [50, 150]})).unwrap();
    let k: ComponentId = id(&first, "component");
    h.run("setprop", json!({"component": k.hex(), "key": "tag", "value": "-K05"})).unwrap();
    h.run("place", json!({"symbol": "coil", "at": [200, 150], "device": k.hex()})).unwrap();
    h.run("place", json!({"symbol": "coil", "at": [120, 150], "tag": "-K04"})).unwrap();
    let plain = h.doc();
    let mut edited = plain.clone();
    if let Some(r) = edited.representations.get_mut(&rep(&first)) {
        r.role = "main".into();
    }
    edited.rebuild_index();
    assert_eq!(xref::device(&plain, k), xref::device(&edited, k));
    assert_eq!(reps::goto(&plain, k, None), reps::goto(&edited, k, None));
    assert_eq!(locations::main_rep(&plain, k).map(|(r, _)| r), locations::main_rep(&edited, k).map(|(r, _)| r));
    assert_eq!(locations::resolve(&plain, k), locations::resolve(&edited, k));
    let parts = |d: &Document| xref::parts_on_page(d, p1).into_iter().map(|p| (p.rep, p.index, format!("{:?}", p.shape))).collect::<Vec<_>>();
    assert_eq!(parts(&plain), parts(&edited));
    let req = ecad_model::numbering::Request {
        kinds: [ecad_model::numbering::Kind::Devices].into_iter().collect(),
        scope: ecad_model::numbering::Scope::All,
        start: Some(1),
        digits: None,
        strips: Default::default(),
        positions: Default::default(),
    };
    let high = ecad_model::tag::HighWater::new();
    let plan = |d: &Document| ecad_model::numbering::plan_ranked(d, &req, &high, &|r| reps::numbering_rank(d, r)).unwrap().ops;
    assert_eq!(format!("{:?}", plan(&plain)), format!("{:?}", plan(&edited)));
    assert_eq!(ecad_model::numbering::rep_anchor(&plain, Subject::Component(k)), ecad_model::numbering::rep_anchor(&edited, Subject::Component(k)));
}
