//! Brief 025 (design 025 §10): device cross-references on the general reference index, from
//! outside, through the commands' JSON forms. Fixture X: page 100 "Supply" with `-K01`'s coil
//! (its A2 wired straight down to a rail) and its power contacts, page 105 "Motor control" with
//! its NO and NC, and `-Q01` (an MCB) with an auxiliary NC.

use std::collections::BTreeSet;

use crate::common;

use cadcraft_engine::Session;
use common::{H, id, pts};
use ecad_cadcraft::project::Mapped;
use ecad_cadcraft::xref::{self, Group, RefIndex};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{ComponentId, ObjectRef, Op, PageId, RepresentationId};
use serde_json::{Value, json};

const SEED: u64 = 25;

fn close_all(s: &mut Session) {
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
}

struct X {
    h: H,
    p100: PageId,
    p105: PageId,
    k1: ComponentId,
    coil: RepresentationId,
    power: RepresentationId,
    no: RepresentationId,
    nc: RepresentationId,
    q1: ComponentId,
    mcb: RepresentationId,
    q_nc: RepresentationId,
}

fn rep(v: &Value) -> RepresentationId {
    id(v, "representation")
}

fn fixture() -> X {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    let p100 = h.first_page();
    h.run("pageprops", json!({"page": p100.hex(), "number": "100", "title": "Supply"})).unwrap();
    let coil = h.run("place", json!({"symbol": "coil", "at": [75, 180]})).unwrap();
    let k1: ComponentId = id(&coil, "component");
    // A2 straight down to the 0V rail.
    h.run("wire", json!({"points": pts(&[(75.0, 170.0), (75.0, 150.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(40.0, 150.0), (200.0, 150.0)])})).unwrap();
    let power = h.run("place", json!({"symbol": "power3", "at": [125, 180], "device": k1.hex()})).unwrap();
    let p105: PageId = id(&h.run("newpage", json!({"title": "Motor control", "number": "105"})).unwrap(), "page");
    let no = h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k1.hex()})).unwrap();
    let nc = h.run("place", json!({"symbol": "nc", "at": [100, 100], "device": k1.hex()})).unwrap();
    let mcb = h.run("place", json!({"symbol": "mcb1", "at": [250, 150]})).unwrap();
    let q1: ComponentId = id(&mcb, "component");
    let q_nc = h.run("place", json!({"symbol": "nc", "at": [300, 150], "device": q1.hex()})).unwrap();
    X { p100, p105, k1, coil: rep(&coil), power: rep(&power), no: rep(&no), nc: rep(&nc), q1, mcb: rep(&mcb), q_nc: rep(&q_nc), h }
}

/// The texts drawn on `page` by brief 025, by representation and part index.
fn drawn(h: &mut H, page: PageId) -> Vec<(RepresentationId, u32, String)> {
    h.p(|p| {
        xref::parts_on_page(p.ledger.document(), page)
            .into_iter()
            .filter_map(|part| match part.shape {
                xref::Shape::Text { value, .. } => Some((part.rep, part.index, value)),
                xref::Shape::Line(..) => None,
            })
            .collect()
    })
}

fn contact_text(h: &mut H, page: PageId, r: RepresentationId) -> Option<String> {
    drawn(h, page).into_iter().find(|(x, i, _)| *x == r && *i == 0).map(|(_, _, v)| v)
}

fn rows(h: &mut H, c: ComponentId) -> Vec<(String, String, String, String)> {
    let v = h.run("contacts", json!({"component": c.hex()})).unwrap();
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["labels"].as_str().unwrap().to_string(),
                r["kind"].as_str().unwrap().to_string(),
                r["state"].as_str().unwrap().to_string(),
                r["place"].as_str().unwrap_or("—").to_string(),
            )
        })
        .collect()
}

fn index_matches(h: &mut H) {
    h.p(|p| p.xref.verify(p.ledger.document())).unwrap();
}

/// Check A: each contact shows the coil's place; the coil's table lists its five poles with their
/// places, under the rail; `xrefs` lists ten edges.
#[test]
fn a_contacts_show_the_coil_and_the_coil_shows_its_contacts() {
    let mut x = fixture();
    for (page, r) in [(x.p100, x.power), (x.p105, x.no), (x.p105, x.nc)] {
        assert_eq!(contact_text(&mut x.h, page, r).as_deref(), Some("100-B-3"), "{r:?}");
    }
    let r = rows(&mut x.h, x.k1);
    let labels: Vec<&str> = r.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(labels, ["1/2", "3/4", "5/6", "13/14", "21/22"]);
    assert!(r[..3].iter().all(|r| r.3 == "100-C-3" && r.2 == "drawn"), "{r:?}");
    assert!(r[3].3.starts_with("105-") && r[4].3.starts_with("105-"), "{r:?}");
    // Under the rail the coil's A2 wire meets: T at y = 150 - 4.
    let top = x.h.p(|p| {
        let doc = p.ledger.document();
        xref::table_top(doc, x.k1, x.coil, xref::Placement::Auto).unwrap()
    });
    assert!((top.x - 75.0).abs() < 1e-9 && (top.y - 146.0).abs() < 1e-9, "{top:?}");
    let v = x.h.run("xrefs", json!({"component": x.k1.hex()})).unwrap();
    let refs = v["references"].as_array().unwrap();
    assert_eq!(refs.len(), 10, "five poles to the coil and back: {v}");
    assert!(refs.iter().any(|r| r["kind"] == "contact" && r["text"] == "100-B-3"));
    assert!(refs.iter().any(|r| r["kind"] == "mirror"));
    // The index from the open matches a rebuild.
    index_matches(&mut x.h);
    let sizes = x.h.p(|p| p.xref.sizes());
    assert_eq!(sizes.1, 2, "two device groups");
    // The table is drawn on page 100 only.
    assert!(drawn(&mut x.h, x.p100).iter().any(|(r, _, v)| *r == x.coil && v == "13/14" || *r == x.coil && v == "13"));
    assert!(!drawn(&mut x.h, x.p105).iter().any(|(r, _, _)| *r == x.coil));
}

/// Check B: renumbering 105 changes only the texts that name it, in the same commit, and `u`
/// brings them back.
#[test]
fn b_renumbering_a_page_changes_the_rows_that_name_it() {
    let mut x = fixture();
    let before_100 = drawn(&mut x.h, x.p100);
    let before_105 = drawn(&mut x.h, x.p105);
    let head = x.h.head();
    x.h.run("pageprops", json!({"page": x.p105.hex(), "number": "106"})).unwrap();
    assert_eq!(x.h.head(), head + 1, "one transaction");
    let after_100 = drawn(&mut x.h, x.p100);
    let changed: Vec<_> = after_100.iter().filter(|a| !before_100.contains(a)).collect();
    assert!(!changed.is_empty() && changed.iter().all(|(_, _, v)| v.starts_with("106-")), "{changed:?}");
    // The contacts on 105 still show the coil on 100.
    assert_eq!(contact_text(&mut x.h, x.p105, x.no).as_deref(), Some("100-B-3"));
    // -Q01's pair on that page reads 106 now.
    let q = drawn(&mut x.h, x.p105);
    assert!(q.iter().any(|(r, i, v)| *r == x.q_nc && *i == 0 && v.starts_with("106-")), "{q:?}");
    index_matches(&mut x.h);
    x.h.run("u", json!({})).unwrap();
    assert_eq!(drawn(&mut x.h, x.p100), before_100);
    assert_eq!(drawn(&mut x.h, x.p105), before_105);
}

/// Check C: the offer adds spares, joins take its labels, one too many is listed in Checks with
/// go-to, nothing is refused; an unreadable offer is refused with a way forward.
#[test]
fn c_the_offer_spares_join_labels_and_too_many() {
    let mut x = fixture();
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3P+1NO+1NC"})).unwrap();
    assert_eq!(rows(&mut x.h, x.k1).len(), 5, "no rows for poles already drawn");
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3P+2NO+1NC"})).unwrap();
    let r = rows(&mut x.h, x.k1);
    assert!(r.contains(&("23/24".into(), "NO".into(), "spare".into(), "—".into())), "{r:?}");
    assert_eq!(r.len(), 6);
    // Joining a NO takes the free offered 23/24.
    x.h.activate(x.p105);
    let j = x.h.run("place", json!({"symbol": "no", "at": [150, 150], "device": x.k1.hex()})).unwrap();
    assert_eq!(j["offer_slot"], json!(true));
    let labels: Vec<&str> = j["terminals"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["23", "24"]);
    assert!(rows(&mut x.h, x.k1).iter().all(|r| r.2 == "drawn"));
    // Another: 33/34, over the offer, and a Checks row with a place to go to.
    let j = x.h.run("place", json!({"symbol": "no", "at": [200, 150], "device": x.k1.hex()})).unwrap();
    assert_eq!(j["offer_slot"], json!(false));
    let labels: Vec<&str> = j["terminals"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["33", "34"]);
    let r = rows(&mut x.h, x.k1);
    assert!(r.contains(&("33/34".into(), "NO".into(), "over".into(), r.iter().find(|r| r.0 == "33/34").unwrap().3.clone())), "{r:?}");
    let checks = x.h.run("checks", json!({"kinds": ["too-many-contacts"]})).unwrap();
    let list = checks["checks"].as_array().unwrap();
    assert_eq!(list.len(), 1, "{checks}");
    assert_eq!(list[0]["severity"], "warning");
    assert_eq!(list[0]["object"]["component"], json!(x.k1.hex()));
    assert!(list[0]["which"].as_str().unwrap().contains("3 auxiliary NO drawn, part offers 2 (3P+2NO+1NC)"), "{}", list[0]);
    assert!(list[0]["at"].is_array(), "go-to rings the over pole");
    // The kept list (the Checks tab) agrees.
    let kept = x.h.cfg(|p| p.checks().rows().filter(|c| c.kind == ecad_cadcraft::checks::Kind::TooManyContacts).count());
    assert_eq!(kept, 1);
    x.h.run("u", json!({})).unwrap();
    let kept = x.h.cfg(|p| p.checks().rows().filter(|c| c.kind == ecad_cadcraft::checks::Kind::TooManyContacts).count());
    assert_eq!(kept, 0, "the kept list follows the undo");
    // An unreadable offer is refused; nothing changes.
    let head = x.h.head();
    let e = x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3X"})).unwrap_err();
    assert_eq!(e, "\"3X\" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.");
    assert_eq!(x.h.head(), head);
    index_matches(&mut x.h);
}

/// Check D: Move Mirror Table, off and back, and moving the coil are each one transaction; the
/// table follows the coil.
#[test]
fn d_moving_hiding_and_following_the_table() {
    let mut x = fixture();
    x.h.activate(x.p100);
    let head = x.h.head();
    let v = x.h.run("mirrormove", json!({"component": x.k1.hex(), "at": [60, 120]})).unwrap();
    assert_eq!(v["mirror"], json!("at -15,-60"), "{v}");
    assert_eq!(x.h.head(), head + 1);
    assert!(
        matches!(x.h.p(|p| p.ledger.log().last().unwrap().ops.clone()).as_slice(), [Op::SetProperty { object: ObjectRef::Component(_), key, .. }] if key == "mirror")
    );
    let top = |h: &mut H| {
        h.p(|p| {
            xref::table(p.ledger.document(), &mut xref::Places::new(p.ledger.document()), &xref::device(p.ledger.document(), x.k1)).map(|t| t.top)
        })
    };
    assert_eq!(top(&mut x.h), Some(ecad_model::Pt::new(60.0, 120.0)));
    // Moving the coil moves the table with it.
    let coil_handle = x.h.p(|p| {
        let item = p.ledger.document().representations[&x.coil].items[0];
        p.pages[&x.p100].map.item(item).unwrap()
    });
    x.h.run("move", json!({"handles": [coil_handle.hex()], "delta": [10, 0]})).unwrap();
    assert_eq!(top(&mut x.h), Some(ecad_model::Pt::new(70.0, 120.0)), "the table follows the coil");
    index_matches(&mut x.h);
    // Off hides the table; the contact texts remain.
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "mirror", "value": "Off"})).unwrap();
    assert_eq!(top(&mut x.h), None);
    assert!(contact_text(&mut x.h, x.p105, x.no).is_some());
    // Project default: cleared.
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "mirror", "value": "Project default (auto)"})).unwrap();
    assert!(!x.h.doc().components[&x.k1].properties.contains_key("mirror"));
    assert!(top(&mut x.h).is_some());
    // A bad value is refused with a way forward.
    let e = x.h.run("setprop", json!({"component": x.k1.hex(), "key": "mirror", "value": "at NaN,1"})).unwrap_err();
    assert!(e.contains("is not a table placement"), "{e}");
    let e = x.h.run("mirrormove", json!({"component": x.q1.hex()})).unwrap_err();
    assert!(e.contains("`at`"), "{e}");
}

/// Check F: an MCB with an auxiliary NC: the NC shows the MCB's place, the MCB a one-row table.
#[test]
fn f_parent_and_child_mcb_with_an_auxiliary_contact() {
    let mut x = fixture();
    let mcb_place = x.h.p(|p| {
        let doc = p.ledger.document();
        xref::Places::new(doc).text(x.p105, xref::anchor(doc, &doc.representations[&x.mcb]))
    });
    assert_eq!(contact_text(&mut x.h, x.p105, x.q_nc), Some(mcb_place));
    let r = rows(&mut x.h, x.q1);
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].0.as_str(), r[0].1.as_str()), ("11/12", "NC"));
}

/// Check G: hover lines both ways, Properties rows, and Go to Reference both ways.
#[test]
fn g_hover_properties_and_go_to() {
    let mut x = fixture();
    x.h.activate(x.p105);
    // Hover the NO's text: the device's lens with its coil.
    let (uid, text_handle) = x.h.pv(|p, _| {
        let v = &p.pages[&x.p105];
        (v.uid.unwrap(), v.map.xref(x.no, 0).unwrap())
    });
    let lines = x.h.p(|p| ecad_cadcraft::interact::hover_lines(p, uid, text_handle)).unwrap();
    assert!(lines.iter().any(|l| l.starts_with("Coil: 100-B-3") && l.contains("page 100 \"Supply\"")), "{lines:?}");
    // Go to Reference from the NC: the coil's page, ringed and selected.
    let v = x.h.run("refgoto", json!({"source": x.nc.hex()})).unwrap();
    assert_eq!(v["page"], json!(x.p100.hex()));
    assert_eq!(v["target"], json!(x.coil.hex()));
    assert!(x.h.p(|p| p.goto_ring).is_some_and(|(pg, at, _)| pg == x.p100 && at.near(ecad_model::Pt::new(75.0, 180.0), 1e-9)));
    // Hover the coil's table: its contacts.
    let (uid, bar) = x.h.pv(|p, _| {
        let v = &p.pages[&x.p100];
        (v.uid.unwrap(), v.map.xref(x.coil, 0).unwrap())
    });
    let lines = x.h.p(|p| ecad_cadcraft::interact::hover_lines(p, uid, bar)).unwrap();
    assert!(lines[0].starts_with("Contacts: 5 drawn"), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("13/14 NO aux 105-")), "{lines:?}");
    // And back from the coil to a given contact.
    let v = x.h.run("refgoto", json!({"source": x.coil.hex(), "target": x.no.hex()})).unwrap();
    assert_eq!(v["page"], json!(x.p105.hex()));
    // A contact of a device with no coil shows nothing and says so.
    let pb = x.h.run("place", json!({"symbol": "pbno", "at": [20, 20]})).unwrap();
    let e = x.h.run("refgoto", json!({"source": rep(&pb).hex()})).unwrap_err();
    assert!(e.contains("shows no reference"), "{e}");
    // Properties: the coil's section has the offer, the table and its rows with Go to buttons.
    x.h.activate(x.p100);
    let sel = x.h.p(|p| {
        let item = p.ledger.document().representations[&x.coil].items[0];
        vec![p.pages[&x.p100].map.item(item).unwrap()]
    });
    let view = x.h.p(|p| ecad_cadcraft::props::view(p, Some(x.p100), &sel));
    let s = &view.sections[0];
    let keys: Vec<&str> = s.rows.iter().map(|r| r.key.as_str()).collect();
    for k in ["contacts", "mirror", "contactrows"] {
        assert!(keys.contains(&k), "{keys:?}");
    }
    assert!(s.buttons.iter().any(|b| b.starts_with(&format!("refgoto:{}:{}:", x.coil.hex(), x.no.hex()))), "{:?}", s.buttons);
    assert!(view.deps.contains(&ObjectRef::Page(x.p105)), "renumbering 105 refreshes the rows");
    let e = x.h.run("xrefs", json!({"component": "1"})).unwrap_err();
    assert_eq!(e, "no such component");
}

/// Edits of a drawn reference are refused with words; a window move of a coil with its table
/// leaves the table out and moves the coil.
#[test]
fn drawn_references_cannot_be_edited_alone() {
    let mut x = fixture();
    x.h.activate(x.p105);
    let h_text = x.h.p(|p| p.pages[&x.p105].map.xref(x.no, 0).unwrap());
    assert_eq!(x.h.mapped(x.p105, h_text), Some(Mapped::Xref(x.no)));
    let e = x.h.run("move", json!({"handles": [h_text.hex()], "delta": [5, 0]})).unwrap_err();
    assert!(e.contains(xref::TEXT_DERIVED), "{e}");
    x.h.activate(x.p100);
    let bar = x.h.p(|p| p.pages[&x.p100].map.xref(x.coil, 0).unwrap());
    let e = x.h.run("erase", json!({"handles": [bar.hex()]})).unwrap_err();
    assert!(e.contains(xref::TABLE_DERIVED), "{e}");
}

/// Check J: save and reopen keep the offer and the placement; the project default is written
/// only when it is not `auto`.
#[test]
fn j_save_and_reopen_keep_contacts_and_mirror() {
    let mut x = fixture();
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3P+1NO+1NC"})).unwrap();
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "mirror", "value": "right"})).unwrap();
    let v = x.h.run("projectprops", json!({})).unwrap();
    assert_eq!(v["contact_mirror"], "auto");
    x.h.run("projectprops", json!({"contact_mirror": "off"})).unwrap();
    assert_eq!(x.h.doc().settings.contact_mirror, ecad_model::ContactMirror::Off);
    let e = x.h.run("projectprops", json!({"contact_mirror": "sideways"})).unwrap_err();
    assert!(e.contains("contact_mirror"), "{e}");
    let dir = std::env::temp_dir().join(format!("ecad-025-{}-{}", std::process::id(), ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("x.ecad").to_string_lossy().into_owned();
    x.h.cfg(|p| p.persistence = Persistence::File);
    x.h.run("saveas", json!({"path": path})).unwrap();
    let live = x.h.doc();
    let idx = x.h.p(|p| p.xref.clone());
    close_all(&mut x.h.s);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 99).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(h2.doc(), live);
    assert_eq!(h2.p(|p| p.xref.clone()), idx, "the index rebuilt on open is the kept one");
    let c = &h2.doc().components[&x.k1];
    assert_eq!(c.properties.get("contacts").map(String::as_str), Some("3P+1NO+1NC"));
    assert_eq!(c.properties.get("mirror").map(String::as_str), Some("right"));
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Check K: hostile values never panic: a long offer, too many poles, a bad offset, two heads, a
/// stored value that cannot be read.
#[test]
fn k_hostile_values() {
    let mut x = fixture();
    for v in ["1NO+".repeat(2_500), "33NO".into(), "at NaN,1".into()] {
        let key = if v.starts_with("at") { "mirror" } else { "contacts" };
        assert!(x.h.run("setprop", json!({"component": x.k1.hex(), "key": key, "value": v})).is_err(), "{key} {v:.20}");
    }
    // Stored unreadable values (a hostile file) count as absent.
    let before = x.h.doc().components[&x.k1].properties.clone();
    let ops = vec![
        Op::SetProperty {
            object: ObjectRef::Component(x.k1),
            key: "contacts".into(),
            before: before.get("contacts").cloned(),
            after: Some("3X".into()),
        },
        Op::SetProperty {
            object: ObjectRef::Component(x.k1),
            key: "mirror".into(),
            before: before.get("mirror").cloned(),
            after: Some("at inf,0".into()),
        },
    ];
    x.h.project.apply_ops(&mut x.h.s, "hostile", ops).unwrap();
    assert_eq!(rows(&mut x.h, x.k1).len(), 5);
    // A second head is refused by Place (a file may still hold one; the index takes the lower
    // ID as the head, see `xref::device`).
    x.h.activate(x.p105);
    let e = x.h.run("place", json!({"symbol": "coil", "at": [10, 10], "device": x.k1.hex()})).unwrap_err();
    assert!(e.contains("already has a coil"), "{e}");
    index_matches(&mut x.h);
    let groups: BTreeSet<Group> = x.h.p(|p| p.xref.groups().collect());
    assert!(groups.contains(&Group::Device(x.k1)) && groups.contains(&Group::Device(x.q1)));
}

/// Check E: cable reference commits are scoped per cable, cable ops included (brief 025 made
/// them per cable); the index's cable groups follow.
#[test]
fn e_cable_groups_are_kept_on_the_index() {
    let (mut h, _, _, _) = common::small_project(SEED);
    let p1 = h.first_page();
    h.run("wire", json!({"points": pts(&[(0.0, 50.0), (60.0, 50.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(0.0, 55.0), (60.0, 55.0)])})).unwrap();
    let slash = h.run("cableslash", json!({"from": [30, 45], "to": [32, 60]})).unwrap();
    let k: ecad_model::CableId = id(&slash, "cable");
    h.run("cableref", json!({"cable": k.hex(), "at": [60, 52]})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "2"})).unwrap(), "page");
    h.run("cablecontinue", json!({"cable": k.hex(), "at": [10, 50], "toward": [60, 50]})).unwrap();
    let _ = (p1, p2);
    let members: Vec<RepresentationId> = h.p(|p| p.xref.members(Group::Cable(k)).collect());
    assert_eq!(members.len(), 2);
    h.p(|p| p.xref.verify(p.ledger.document())).unwrap();
    let doc = h.doc();
    let scope = ecad_cadcraft::references::cables_touched(&doc, &[Op::DeleteCable { id: k, cable: doc.cables[&k].clone() }]);
    assert_eq!(scope, Some(BTreeSet::from([k])), "a cable op touches its cable only");
    let v = h.run("xrefs", json!({"cable": k.hex()})).unwrap();
    assert_eq!(v["references"].as_array().unwrap().len(), 2);
    assert!(v["references"].as_array().unwrap().iter().all(|r| r["kind"] == "cable"));
    h.run("u", json!({})).unwrap();
    h.p(|p| p.xref.verify(p.ledger.document())).unwrap();
    assert_eq!(h.p(|p| RefIndex::build(p.ledger.document()).members(Group::Cable(k)).count()), 1);
}

/// Review finding 2: a kept index out of step with the document (a bug, forced here by emptying
/// it) is reported as an `IndexRebuilt` event and rebuilt in the same commit; the command still
/// succeeds, nothing panics, and the next commit finds nothing wrong.
#[test]
fn a_stale_index_is_reported_and_rebuilt() {
    use ecad_inspect::EventKind;
    let mut x = fixture();
    x.h.cfg(|p| p.xref = RefIndex::default());
    let head = x.h.head();
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3P+2NO+2NC"})).unwrap();
    assert_eq!(x.h.head(), head + 1);
    let events: Vec<_> = x.h.p(|p| p.events.events().filter(|e| e.kind == EventKind::IndexRebuilt).cloned().collect());
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(events[0].message.contains("reference index was out of step"), "{events:?}");
    index_matches(&mut x.h);
    x.h.run("setprop", json!({"component": x.k1.hex(), "key": "contacts", "value": "3P+1NO+1NC"})).unwrap();
    assert_eq!(x.h.p(|p| p.events.count(EventKind::IndexRebuilt)), 1, "nothing more to repair");
    index_matches(&mut x.h);
}
