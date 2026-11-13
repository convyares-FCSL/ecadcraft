//! Brief 044 (connector devices): the worker's in-process checks of design 044 §12 (rows 1–10,
//! 12, 13), the registry, the F1 guard and the versioned starters. The tester's acceptance tests
//! are separate.

use ecad_cadcraft::connectors;
use ecad_cadcraft::symbols::{self, Side};
use ecad_model::{ComponentId, ConductorId, Document, PageId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// The connector's terminals by (side key, pin name).
fn pins(d: &Document, c: ComponentId) -> Vec<(String, String, TerminalId)> {
    let mut v: Vec<(String, String, TerminalId)> =
        d.terminals_of(c).map(|(t, x)| (x.key.clone().unwrap_or_default(), x.label.clone().unwrap_or_default(), t)).collect();
    v.sort();
    v
}

fn pin(d: &Document, c: ComponentId, key: &str, name: &str) -> TerminalId {
    pins(d, c).into_iter().find(|(k, n, _)| k == key && n == name).map(|(_, _, t)| t).unwrap_or_else(|| panic!("no {key} {name}"))
}

fn net(d: &Document, t: TerminalId) -> Option<ecad_model::NetId> {
    d.connectivity().net_of_terminal(t)
}

/// A connector row of four pins on the active page at (50, 50).
fn row(h: &mut H, extra: Value) -> Value {
    let mut p = json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, 50]});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    h.run("connector", p).unwrap()
}

/// Three vertical wires at x 100, 120, 140 from y 250 to y 150.
fn three_wires(h: &mut H) -> Vec<ConductorId> {
    [100.0, 120.0, 140.0].iter().map(|x| id(&h.run("wire", json!({"points": pts(&[(*x, 250.0), (*x, 150.0)])})).unwrap(), "conductor")).collect()
}

#[test]
fn row_both_makes_one_connector_with_two_halves_in_one_transaction() {
    let mut h = H::new(4401);
    let before = h.head();
    let v = row(&mut h, json!({}));
    assert_eq!(h.head(), before + 1, "one transaction");
    let c: ComponentId = id(&v, "component");
    let d = h.doc();
    let keys: Vec<(String, String)> = pins(&d, c).into_iter().map(|(k, n, _)| (k, n)).collect();
    let want: Vec<(String, String)> = [".p", ".s"].iter().flat_map(|k| (1..=4).map(move |n| (k.to_string(), n.to_string()))).collect();
    assert_eq!(keys, want);
    assert!(connectors::is_connector(&d, c));
    let tag = v["tag"].as_str().unwrap().to_string();
    assert_eq!(connectors::half_tag(&d, c, Side::Socket), format!("{tag}.S"));
    assert_eq!(connectors::pin_name(&d, pin(&d, c, ".p", "3")), Some(format!("{tag}.P:3")));
    // One EmbedDefinition per half definition, version 2.
    let tx = h.p(|p| p.ledger.log().last().cloned()).unwrap();
    let embeds: Vec<u64> = tx
        .ops
        .iter()
        .filter_map(|o| match o {
            ecad_model::Op::EmbedDefinition { definition, .. } => definition.origin.as_ref().map(|o| o.library_seq),
            _ => None,
        })
        .collect();
    assert_eq!(embeds, vec![2, 2]);
    // `u` removes it all.
    h.cmdline("u");
    assert!(!h.doc().components.contains_key(&c));
}

#[test]
fn mated_pins_are_one_net_and_nothing_else_joins() {
    let mut h = H::new(4402);
    let c: ComponentId = id(&row(&mut h, json!({})), "component");
    let d = h.doc();
    let (s3, p3, s4) = (pin(&d, c, ".s", "3"), pin(&d, c, ".p", "3"), pin(&d, c, ".s", "4"));
    assert!(net(&d, s3).is_some());
    assert_eq!(net(&d, s3), net(&d, p3), "pin 3 of both halves is one net");
    assert_ne!(net(&d, s3), net(&d, s4), "pins 3 and 4 are two nets");
    assert_ne!(net(&d, pin(&d, c, ".p", "4")), net(&d, p3));
}

#[test]
fn halves_on_two_pages_still_mate() {
    let mut h = H::new(4403);
    let c: ComponentId = id(&row(&mut h, json!({"halves": "socket"})), "component");
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(p2);
    let v = row(&mut h, json!({"halves": "plug", "device": c.hex(), "family": null, "pins": null}));
    assert_eq!(id::<ComponentId>(&v, "component"), c);
    let d = h.doc();
    assert_eq!(net(&d, pin(&d, c, ".s", "2")), net(&d, pin(&d, c, ".p", "2")));
    // A second socket half is refused, with a way forward.
    let e = h.run("connector", json!({"halves": "socket", "device": c.hex(), "layout": "row", "at": [10, 10]})).unwrap_err();
    assert!(e.contains("already has a socket half") && e.contains("Place a plug half"), "{e}");
}

#[test]
fn pin_pairs_across_wires_split_them_and_keep_one_net() {
    let mut h = H::new(4404);
    let w = three_wires(&mut h);
    let before = h.head();
    let v = h
        .run(
            "connector",
            json!({"halves": "both", "family": "m12", "pins": 4, "names": "numbers", "layout": "pins", "line": [[90, 200], [150, 200]]}),
        )
        .unwrap();
    assert_eq!(h.head(), before + 1);
    let c: ComponentId = id(&v, "component");
    let d = h.doc();
    assert_eq!(d.components[&c].properties.get("pins").map(String::as_str), Some("4"));
    assert_eq!(v["split"].as_array().map(Vec::len), Some(3));
    for (i, n) in ["1", "2", "3"].iter().enumerate() {
        let (s, p) = (pin(&d, c, ".s", n), pin(&d, c, ".p", n));
        assert_eq!(net(&d, s), net(&d, p), "pin {n} mates");
        // Both parts of the wire are on that net.
        let new: ConductorId = id(&v["split"][i], "new");
        assert_eq!(d.connectivity().net_of_conductor(w[i]), d.connectivity().net_of_conductor(new));
        assert_eq!(d.connectivity().net_of_conductor(w[i]), net(&d, s));
    }
    assert_ne!(net(&d, pin(&d, c, ".s", "1")), net(&d, pin(&d, c, ".s", "2")));
    // A plug pin placed alone takes the next pin drawn on neither side… here 4 (1–3 are pairs).
    let v = h.run("place", json!({"symbol": "pinp", "device": c.hex(), "at": [200, 50]})).unwrap();
    assert_eq!(v["terminals"][0]["pin"], "4");
    // A socket pin named 2 is drawn already: refused.
    let e = h.run("place", json!({"symbol": "pins", "device": c.hex(), "pin": "2", "at": [220, 50]})).unwrap_err();
    assert!(e.contains(".S:2 is drawn on"), "{e}");
}

#[test]
fn pins_fill_mates_first() {
    let mut h = H::new(4405);
    let v = h.run("place", json!({"symbol": "pins", "at": [10, 10]})).unwrap();
    let c: ComponentId = id(&v, "component");
    h.run("setprop", json!({"component": c.hex(), "key": "pins", "value": "3"})).unwrap();
    for x in [20, 30] {
        h.run("place", json!({"symbol": "pins", "device": c.hex(), "at": [x, 10]})).unwrap();
    }
    // Plug pins pair up 1–3 with the socket pins.
    let names: Vec<String> = (0..3)
        .map(|i| {
            h.run("place", json!({"symbol": "pinp", "device": c.hex(), "at": [10 + 10 * i, 40]})).unwrap()["terminals"][0]["pin"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(names, ["1", "2", "3"]);
    // A fourth socket pin is a new pin past `pins`, with a Checks row.
    let v = h.run("place", json!({"symbol": "pins", "device": c.hex(), "at": [60, 10]})).unwrap();
    assert_eq!(v["terminals"][0]["pin"], "4");
    let rows = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    assert!(rows.iter().any(|r| r.kind.name() == "connector-over"));
    assert!(rows.iter().any(|r| r.kind.name() == "connector-lone-pin"));
}

#[test]
fn mismatched_halves_and_refusals() {
    let mut h = H::new(4406);
    let c: ComponentId = id(&row(&mut h, json!({"halves": "socket", "family": "dsub", "pins": 9})), "component");
    // A plug half of another family and size is placed, with two warnings.
    h.run("place", json!({"symbol": "m12p5", "device": c.hex(), "at": [50, 150]})).unwrap();
    let rows = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    let family = rows.iter().find(|r| r.kind.name() == "connector-family").expect("family row");
    assert!(family.which.contains("D-sub socket") && family.which.contains("M12 plug"), "{}", family.which);
    assert!(rows.iter().any(|r| r.kind.name() == "connector-size"));
    // A half joined to a device that is not a connector.
    let k: ComponentId = id(&h.run("place", json!({"symbol": "coil", "at": [200, 200]})).unwrap(), "component");
    let e = h.run("place", json!({"symbol": "plug4", "device": k.hex(), "at": [0, 0]})).unwrap_err();
    assert!(e.contains("is not a connector") && e.contains("Enter for a new one"), "{e}");
    // A size that is not one of the type's.
    let e = h.run("connector", json!({"family": "dsub", "pins": 10, "layout": "row", "at": [0, 0]})).unwrap_err();
    assert_eq!(e, "A D-sub has 9, 15 or 25 pins. Pick one, or draw pin by pin with Pins.");
    // Names: count and characters.
    let e = h.run("connector", json!({"pins": 4, "names": ["L", "N", "PE"], "layout": "row", "at": [0, 0]})).unwrap_err();
    assert_eq!(e, "Give 4 pin names; got 3.");
    let e = h.run("connector", json!({"pins": 3, "names": "L,P E,PE", "layout": "row", "at": [0, 0]})).unwrap_err();
    assert!(e.contains("cannot contain spaces"), "{e}");
}

#[test]
fn half_tags_follow_the_registry_at_project_and_connector_level() {
    let mut h = H::new(4407);
    let v = row(&mut h, json!({}));
    let c: ComponentId = id(&v, "component");
    let tag = v["tag"].as_str().unwrap().to_string();
    h.run("setdefault", json!({"key": "connector.side.socket", "value": "F"})).unwrap();
    h.run("setdefault", json!({"key": "connector.side.plug", "value": "M"})).unwrap();
    let d = h.doc();
    assert_eq!(connectors::half_tag(&d, c, Side::Socket), format!("{tag}.F"));
    assert_eq!(connectors::half_tag(&d, c, Side::Plug), format!("{tag}.M"));
    let bom = h.run("bom", json!({"limit": 500})).unwrap();
    let tags: Vec<String> = bom["rows"].as_array().unwrap().iter().filter_map(|r| r["tags"].as_str().map(str::to_string)).collect();
    assert!(tags.iter().any(|t| t.contains(&format!("{tag}.F"))) && tags.iter().any(|t| t.contains(&format!("{tag}.M"))), "{tags:?}");
    // A connector's own separator wins over the project's.
    h.run("setprop", json!({"component": c.hex(), "key": "connector.half.separator", "value": "-"})).unwrap();
    assert_eq!(connectors::half_tag(&h.doc(), c, Side::Socket), format!("{tag}-F"));
    // Two equal side names fall back to the built-ins.
    h.run("setdefault", json!({"key": "connector.side.plug", "value": "F"})).unwrap();
    assert_eq!(connectors::half_tag(&h.doc(), c, Side::Plug), format!("{tag}-P"));
}

#[test]
fn typed_names_resolve_and_ambiguous_ones_are_refused() {
    let mut h = H::new(4408);
    let v = row(&mut h, json!({}));
    let c: ComponentId = id(&v, "component");
    let tag = v["tag"].as_str().unwrap().to_string();
    let d = h.doc();
    let p3 = pin(&d, c, ".p", "3");
    assert_eq!(connectors::resolve(&d, &format!("{tag}.P:3")), Some(Ok((c, Some(Side::Plug), Some(p3)))));
    assert_eq!(connectors::resolve(&d, &format!("{tag}.s")), Some(Ok((c, Some(Side::Socket), None))));
    let e = connectors::resolve(&d, &format!("{tag}:3")).unwrap().unwrap_err();
    assert_eq!(e, format!("{tag} has pin 3 on both halves. Type {tag}.S:3 or {tag}.P:3."));
    assert_eq!(connectors::resolve(&d, "-K99"), None);
    // Another device cannot take a half's tag (design 044 §5.1).
    let k: ComponentId = id(&h.run("place", json!({"symbol": "coil", "at": [200, 200]})).unwrap(), "component");
    let e = h.run("setprop", json!({"component": k.hex(), "key": "tag", "value": format!("{tag}.S")})).unwrap_err();
    assert!(e.contains(&format!("is the socket half of {tag}")), "{e}");
}

#[test]
fn bom_has_one_row_per_drawn_half() {
    let mut h = H::new(4409);
    let c: ComponentId = id(&row(&mut h, json!({})), "component");
    h.run("setprop", json!({"component": c.hex(), "key": "half.s.part_number", "value": "SKT-4"})).unwrap();
    h.run("setprop", json!({"component": c.hex(), "key": "half.s.location", "value": "+FIELD"})).unwrap();
    let rows = h.run("bom", json!({"limit": 500})).unwrap()["rows"].as_array().cloned().unwrap();
    let mine: Vec<&Value> = rows.iter().filter(|r| r["devices"].as_array().is_some_and(|a| a.contains(&json!(c.hex())))).collect();
    assert_eq!(mine.len(), 2, "{mine:?}");
    let s = mine.iter().find(|r| r["part_number"] == "SKT-4").expect("socket row");
    assert_eq!(s["location"], "+FIELD");
    assert_eq!(s["description"], "Socket, 4-pin");
    // An unknown side letter is refused; a lone half gives one row.
    let e = h.run("setprop", json!({"component": c.hex(), "key": "half.x.maker", "value": "ACME"})).unwrap_err();
    assert!(e.contains("has no half \"x\""), "{e}");
    let lone: ComponentId = id(&h.run("place", json!({"symbol": "dsub9p", "at": [200, 50]})).unwrap(), "component");
    let rows = h.run("bom", json!({"limit": 500})).unwrap()["rows"].as_array().cloned().unwrap();
    let n = rows.iter().filter(|r| r["devices"].as_array().is_some_and(|a| a.contains(&json!(lone.hex())))).count();
    assert_eq!(n, 1);
}

#[test]
fn halves_reference_each_other_across_pages() {
    let mut h = H::new(4410);
    let c: ComponentId = id(&row(&mut h, json!({"halves": "socket"})), "component");
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(p2);
    row(&mut h, json!({"halves": "plug", "device": c.hex()}));
    let d = h.doc();
    let s = connectors::main_symbol(&d, c, Side::Socket).unwrap();
    let p = connectors::main_symbol(&d, c, Side::Plug).unwrap();
    let list = connectors::ref_classes(&d, c);
    let edges = connectors::ref_edges(&d, c, &list);
    assert!(edges.contains(&(s, p)) && edges.contains(&(p, s)));
    let parts = ecad_cadcraft::xref::parts_on_page(&d, p2);
    assert!(parts.iter().any(|x| x.rep == p), "the plug's reference is drawn on its page");
    assert!(connectors::hover_words(&d, c, Side::Socket).unwrap().contains("mates with"));
}

#[test]
fn pinlabel_renames_a_pin_on_both_halves() {
    let mut h = H::new(4411);
    let c: ComponentId = id(&row(&mut h, json!({})), "component");
    let d0 = h.doc();
    let (s3, p3) = (pin(&d0, c, ".s", "3"), pin(&d0, c, ".p", "3"));
    let before = h.head();
    h.run("pinlabel", json!({"device": c.hex(), "pin": "3", "to": "PE"})).unwrap();
    assert_eq!(h.head(), before + 1);
    let d = h.doc();
    assert_eq!(d.terminals[&s3].label.as_deref(), Some("PE"));
    assert_eq!(d.terminals[&p3].label.as_deref(), Some("PE"));
    assert_eq!(net(&d, s3), net(&d, p3), "still mated");
    let e = h.run("pinlabel", json!({"device": c.hex(), "pin": "PE", "to": "4"})).unwrap_err();
    assert!(e.contains("already has a pin 4"), "{e}");
    h.cmdline("u");
    assert_eq!(h.doc().terminals[&s3].label.as_deref(), Some("3"));
}

#[test]
fn hostile_input_is_refused_without_a_panic() {
    let mut h = H::new(4412);
    let long: Vec<String> = (0..10_000).map(|i| format!("P{i}")).collect();
    for p in [
        json!({"pins": 100000, "layout": "row", "at": [0, 0]}),
        json!({"names": long, "layout": "row", "at": [0, 0]}),
        json!({"names": "x".repeat(10_000), "layout": "row", "at": [0, 0]}),
        json!({"names": ["A\u{1}"], "pins": 1, "layout": "pins", "line": [[0, 0], [1, 1]]}),
        json!({"layout": "row"}),
        json!({"layout": "pins"}),
        json!({"layout": "row", "at": [1e300, 0]}),
        json!({"rotation": 45, "at": [0, 0]}),
        json!({"family": "nonsense", "at": [0, 0]}),
    ] {
        assert!(h.run("connector", p.clone()).is_err(), "{p}");
    }
    assert!(h.run("pinlabel", json!({"device": "nothing", "pin": "1", "to": "2"})).is_err());
    let c: ComponentId = id(&row(&mut h, json!({})), "component");
    for v in ["0", "65", "-1", "1e9", "x"] {
        assert!(h.run("setprop", json!({"component": c.hex(), "key": "pins", "value": v})).is_err(), "{v}");
    }
    assert!(h.run("pinlabel", json!({"device": c.hex(), "pin": "1", "to": "a b"})).is_err());
    assert!(h.run("pinlabel", json!({"device": c.hex(), "pin": "1", "to": "x".repeat(10_000)})).is_err());
}

#[test]
fn a_connector_pin_is_never_read_as_a_strip_level() {
    let mut h = H::new(4413);
    let c: ComponentId = id(&row(&mut h, json!({})), "component");
    let d = h.doc();
    let t = pin(&d, c, ".s", "2");
    let tag = ecad_cadcraft::devices::tag_words(&d, c);
    assert_eq!(ecad_cadcraft::terminal_types::shown_label(&d, t), Some(format!("{tag}.S:2")));
    assert_eq!(ecad_cadcraft::interact::terminal_name(&d, t), format!("{tag}.S:2"));
}

#[test]
fn version_one_halves_stay_separate_and_new_ones_embed_version_two() {
    let mut h = H::new(4414);
    // A project holding 034's version-1 socket: the definition without bridges, seq 1.
    let s = symbols::by_key("socket4").unwrap();
    let mut old = s.definition().unwrap().clone();
    old.bridges.clear();
    let hash = ecad_file::definition_hash(&ecad_model::Definition { origin: None, ..old.clone() }).unwrap();
    if let Some(o) = old.origin.as_mut() {
        o.library_seq = 1;
        o.content_hash = hash;
    }
    assert!(!symbols::is_mating(&old));
    assert_eq!(symbols::kind_of_definition(&old), Some(symbols::Kind::Fixed { exclusive: true }));
    // A new socket half embeds version 2 beside it and is a connector.
    h.cfg(|p| {
        let id: ecad_model::DefinitionId = p.ids.fresh();
        p.ledger.commit(ecad_model::Author::User, "old", vec![ecad_model::Op::EmbedDefinition { id, definition: old.clone() }]).unwrap();
    });
    let c: ComponentId = id(&h.run("place", json!({"symbol": "socket4", "at": [0, 0]})).unwrap(), "component");
    let d = h.doc();
    assert!(connectors::is_connector(&d, c));
    assert_eq!(d.definitions.values().filter(|x| symbols::of_definition(x).is_some_and(|y| y.key == "socket4")).count(), 2);
}

#[test]
fn every_starter_bridges_only_keys_a_placement_writes() {
    for s in symbols::STARTERS {
        let d = s.definition().unwrap();
        for k in d.bridges.iter().flatten() {
            assert!(symbols::bridge_key_ok(d, k), "{} bridges {k}", s.key);
        }
    }
    let mut d = symbols::by_key("plug4").unwrap().definition().unwrap().clone();
    d.bridges = vec![vec![".s".into(), ".x".into()]];
    assert!(!symbols::bridge_key_ok(&d, ".x"));
}

// ------------------------------------------------------------------------------- fix round 1

/// Review of #81, 1: a half's BOM location follows a box and a page `location` on its own page,
/// also when that page is not the connector's first.
#[test]
fn a_half_location_follows_its_own_page() {
    let mut h = H::new(4420);
    let c: ComponentId = id(&row(&mut h, json!({"halves": "socket"})), "component");
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(p2);
    row(&mut h, json!({"halves": "plug", "device": c.hex()}));
    let plug_location = |h: &mut H| -> Value {
        let rows = h.run("bom", json!({"limit": 500})).unwrap()["rows"].as_array().cloned().unwrap();
        rows.into_iter().find(|r| r["tags"].as_str().is_some_and(|t| t.ends_with(".P"))).map(|r| r["location"].clone()).unwrap_or(Value::Null)
    };
    h.run("pageprops", json!({"location": "+FIELD"})).unwrap();
    assert_eq!(plug_location(&mut h), "+FIELD", "a page location on the plug's page");
    h.run("locationbox", json!({"location": "+JB1", "corners": [[0, 0], [300, 200]]})).unwrap();
    assert_eq!(plug_location(&mut h), "+JB1", "a box around the plug");
}

/// Review of #81, 2: the half rows' groups are kept; an unrelated edit groups nothing, and the
/// kept counts equal a rebuild.
#[test]
fn half_row_groups_are_kept_not_regrouped() {
    let mut h = H::new(4421);
    row(&mut h, json!({}));
    let count = |h: &mut H| h.cfg(|p| p.outputs().bom_count(ecad_cadcraft::outputs::Group::Location));
    let n = count(&mut h);
    let g0 = h.cfg(|p| p.outputs().half_groupings.load(std::sync::atomic::Ordering::Relaxed));
    h.run("wire", json!({"points": pts(&[(300.0, 300.0), (350.0, 300.0)])})).unwrap();
    assert_eq!(count(&mut h), n);
    let g1 = h.cfg(|p| p.outputs().half_groupings.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(g0, g1, "an unrelated edit and the counts group no half rows");
    let rebuilt = h.p(|p| ecad_cadcraft::outputs::Outputs::build(p.ledger.document()).bom_count(ecad_cadcraft::outputs::Group::Location));
    assert_eq!(count(&mut h), rebuilt);
}

/// Review of #81, 4 (design 044 §11.1, acceptance 8 and 11): the connector index is kept per
/// commit, recomputes only the connectors an op names, and equals a rebuild.
#[test]
fn the_connector_index_is_kept_from_the_ops() {
    let mut h = H::new(4422);
    let a: ComponentId = id(&row(&mut h, json!({})), "component");
    let b: ComponentId = id(&row(&mut h, json!({"at": [50, 150]})), "component");
    h.run("setprop", json!({"component": b.hex(), "key": "connector.side.socket", "value": "F"})).unwrap();
    let key = ecad_cadcraft::defaults::connectors::SIDE_SOCKET.key;
    let inh = h.p(|p| p.connectors.inheriting(key));
    assert!(inh.contains(&a) && !inh.contains(&b), "{inh:?}");
    h.run("wire", json!({"points": pts(&[(300.0, 300.0), (350.0, 300.0)])})).unwrap();
    assert_eq!(connectors::index_work(), 0, "an unrelated edit recomputes no connector");
    h.run("setprop", json!({"component": a.hex(), "key": "half.s.maker", "value": "ACME"})).unwrap();
    assert_eq!(connectors::index_work(), 1, "an edit of one connector recomputes that one");
    // The project's side name: only the inheriting connector's half tag changes.
    h.run("setdefault", json!({"key": key, "value": "B"})).unwrap();
    let d = h.doc();
    assert!(connectors::half_tag(&d, a, Side::Socket).ends_with(".B"));
    assert!(connectors::half_tag(&d, b, Side::Socket).ends_with(".F"));
    let (kept, built) = h.p(|p| (p.connectors.clone(), connectors::Index::build(p.ledger.document())));
    assert!(kept == built, "the kept index equals a rebuild");
    h.cmdline("u");
    let (kept, built) = h.p(|p| (p.connectors.clone(), connectors::Index::build(p.ledger.document())));
    assert!(kept == built, "also after undo");
}

/// Fix round 1: a typed pin list is kept, in order; its undrawn names come next; pinlabel keeps
/// the list in step (tester D7).
#[test]
fn a_typed_pin_list_is_kept_in_order() {
    let mut h = H::new(4423);
    three_wires(&mut h);
    let v = h
        .run("connector", json!({"halves": "socket", "pins": 4, "names": ["L", "N", "PE", "SH"], "layout": "pins", "line": [[90, 200], [125, 200]]}))
        .unwrap();
    let c: ComponentId = id(&v, "component");
    assert_eq!(h.doc().components[&c].properties.get("pin_names").map(String::as_str), Some("L,N,PE,SH"));
    let v = h.run("place", json!({"symbol": "pins", "device": c.hex(), "at": [200, 50]})).unwrap();
    assert_eq!(v["terminals"][0]["pin"], "PE", "the next listed name, never lost");
    h.run("place", json!({"symbol": "pins", "device": c.hex(), "at": [220, 50]})).unwrap();
    let e = h.run("place", json!({"symbol": "pins", "device": c.hex(), "at": [240, 50]})).unwrap_err();
    assert!(e.contains("(L, N, PE, SH)"), "names in pin order: {e}");
    h.run("pinlabel", json!({"device": c.hex(), "pin": "SH", "to": "S"})).unwrap();
    assert_eq!(h.doc().components[&c].properties.get("pin_names").map(String::as_str), Some("L,N,PE,S"));
}

/// Fix round 1: Ignore and Place mates on the Checks rows; Ctrl+K's device search finds a typed
/// half or pin name.
#[test]
fn checks_fixes_and_typed_search() {
    let mut h = H::new(4424);
    let c: ComponentId = id(&row(&mut h, json!({"halves": "socket", "family": "dsub", "pins": 9})), "component");
    h.run("place", json!({"symbol": "m12p5", "device": c.hex(), "at": [50, 150]})).unwrap();
    let rows = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    let fam = rows.iter().find(|r| r.kind.name() == "connector-family").expect("family row");
    assert!(matches!(fam.fix, Some(ecad_cadcraft::checks::Fix::IgnoreConnector(..))));
    h.run("setprop", json!({"component": c.hex(), "key": "ignore.connector-family", "value": "yes"})).unwrap();
    assert!(ecad_cadcraft::checks::all(&h.doc(), 1e-6).iter().all(|r| r.kind.name() != "connector-family"));
    let lone = ecad_cadcraft::checks::all(&h.doc(), 1e-6).into_iter().find(|r| r.kind.name() == "connector-lone-pin").expect("lone row");
    assert_eq!(lone.fix, Some(ecad_cadcraft::checks::Fix::PlaceMates(c, Side::Plug)));
    let d = h.doc();
    let tag = ecad_cadcraft::devices::tag_words(&d, c);
    assert_eq!(ecad_cadcraft::devices::devices(&d, Some(&format!("{tag}.S:3"))), vec![c]);
    assert_eq!(ecad_cadcraft::devices::devices(&d, Some(&format!("{tag}.P"))), vec![c]);
}

/// Overseer ruling (fix round 1): `pin_names` and `ignore.<check kind>` are plain properties that
/// survive saving and opening, and still act after it.
#[test]
fn pin_names_and_ignore_keys_round_trip() {
    let mut h = H::new(4425);
    let c: ComponentId = id(&row(&mut h, json!({"halves": "socket", "family": "dsub", "pins": 9})), "component");
    h.run("setprop", json!({"component": c.hex(), "key": "pin_names", "value": "A,B,C,D,E,F,G,H,J"})).unwrap();
    h.run("setprop", json!({"component": c.hex(), "key": "ignore.connector-family", "value": "yes"})).unwrap();
    let dir = std::env::temp_dir().join(format!("ecad-044-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roundtrip.ecad").display().to_string();
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    h.s.execute("saveas", &json!({"path": path})).unwrap();
    let before = h.doc().components[&c].properties.clone();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    drop(h);
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, 4426).unwrap();
    let doc = project.read(&mut s, |p, _| p.ledger.document().clone()).unwrap();
    let after = &doc.components[&c].properties;
    assert_eq!(after.get("pin_names"), before.get("pin_names"));
    assert_eq!(after.get("ignore.connector-family").map(String::as_str), Some("yes"));
    assert_eq!(connectors::pin_names(&doc, c), Some(["A", "B", "C", "D", "E", "F", "G", "H", "J"].map(String::from).to_vec()));
    assert!(connectors::ignores(&doc, c, ecad_cadcraft::checks::Kind::ConnectorFamily));
    drop(project);
    let _ = std::fs::remove_dir_all(&dir);
}
