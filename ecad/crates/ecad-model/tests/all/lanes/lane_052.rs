//! Brief 052a (design 052 §4, FP6): numbering anchors a device at its representation of the
//! lowest rank the adapter supplies, and no representation's role is read.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::doc::{Component, Form, Item, ItemKind, Page, Representation, Style, SymbolInstance};
use ecad_model::id::{ComponentId, DefinitionId, ItemId, LayerId, PageId, RepresentationId};
use ecad_model::numbering::{self, Kind, Request, Scope};
use ecad_model::tag::{HighWater, TAG_KEY};
use ecad_model::{Document, Placement, Pt, Subject};

/// Two pages. Each symbol: (representation, device, tag, page, x, y, role).
fn fixture(syms: &[(u128, u128, &str, usize, f64, f64, &str)]) -> Document {
    let mut d = Document::default();
    for i in 0..2u128 {
        let p = PageId(1000 + i);
        d.pages.insert(p, Page::new(format!("P{i}")));
        d.page_order.push(p);
    }
    let style = Style { layer: LayerId(1), color: ecad_model::doc::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    for (rid, c, tag, page, x, y, role) in syms {
        let mut comp = Component::default();
        comp.properties.insert(TAG_KEY.into(), (*tag).into());
        d.components.entry(ComponentId(*c)).or_insert(comp);
        let item = ItemId(5000 + rid);
        let pg = d.page_order[*page];
        let kind = ItemKind::Symbol(SymbolInstance {
            definition: DefinitionId(9),
            placement: Placement { origin: Pt::new(*x, *y), rotation: 0.0, mirror: false, scale: 1.0 },
        });
        d.items.insert(item, Item { page: pg, style: style.clone(), kind });
        let rep = Representation {
            subject: Subject::Component(ComponentId(*c)),
            page: pg,
            form: Form::Symbol,
            role: (*role).into(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        };
        d.representations.insert(RepresentationId(*rid), rep);
    }
    d.rebuild_index();
    d
}

fn req() -> Request {
    Request {
        kinds: [Kind::Devices].into_iter().collect(),
        scope: Scope::All,
        start: Some(1),
        digits: None,
        strips: BTreeSet::new(),
        positions: BTreeMap::new(),
    }
}

fn tags(d: &Document, rank: numbering::Rank<'_>) -> BTreeMap<u128, String> {
    let p = numbering::plan_ranked(d, &req(), &HighWater::new(), rank).unwrap();
    let mut x = d.clone();
    for op in &p.ops {
        op.apply(&mut x).unwrap();
    }
    x.components.iter().filter_map(|(id, c)| c.properties.get(TAG_KEY).map(|t| (id.0, t.clone()))).collect()
}

/// Device 1: a contact at the top-left of page 0 and its coil on page 1. Device 2: a coil on page
/// 0, right of device 1's contact.
fn two_devices(contact_role: &str) -> Document {
    fixture(&[(11, 1, "-K09", 0, 50.0, 200.0, contact_role), (12, 1, "-K09", 1, 50.0, 200.0, "coil"), (21, 2, "-K08", 0, 300.0, 200.0, "coil")])
}

#[test]
fn the_rank_hook_anchors_a_device_at_its_head() {
    let d = two_devices("no");
    let head_first = |r: &Representation| if r.role == "coil" { 0 } else { 1 };
    let t = tags(&d, &head_first);
    assert_eq!(t[&2], "-K01", "device 2's coil is on page 0");
    assert_eq!(t[&1], "-K02", "device 1 is numbered at its coil on page 1");
    // Without a rank: the first symbol in drawing order.
    let t = tags(&d, &|_| 0);
    assert_eq!((t[&1].as_str(), t[&2].as_str()), ("-K01", "-K02"));
    let plain = numbering::plan(&d, &req(), &HighWater::new()).unwrap();
    let ranked_flat = numbering::plan_ranked(&d, &req(), &HighWater::new(), &|_| 0).unwrap();
    assert_eq!(format!("{:?}", plain.ops), format!("{:?}", ranked_flat.ops));
}

#[test]
fn a_main_role_is_not_read() {
    let (a, b) = (two_devices("no"), two_devices("main"));
    assert_eq!(tags(&a, &|_| 0), tags(&b, &|_| 0));
    let n = numbering::plan(&b, &req(), &HighWater::new()).unwrap();
    let m = numbering::plan(&a, &req(), &HighWater::new()).unwrap();
    assert_eq!(format!("{:?}", n.ops), format!("{:?}", m.ops));
    // Checks' anchor: the first in drawing order, whatever the roles.
    let want = Some((PageId(1000), Pt::new(50.0, 200.0)));
    assert_eq!(numbering::rep_anchor(&a, Subject::Component(ComponentId(1))), want);
    assert_eq!(numbering::rep_anchor(&b, Subject::Component(ComponentId(1))), want);
    // The coil (role main in an old file) on page 1 is not preferred over page 0.
    let c = fixture(&[(11, 1, "-K09", 1, 50.0, 200.0, "main"), (12, 1, "-K09", 0, 300.0, 100.0, "no")]);
    assert_eq!(numbering::rep_anchor(&c, Subject::Component(ComponentId(1))), Some((PageId(1000), Pt::new(300.0, 100.0))));
}
