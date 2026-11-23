//! The component library build-up, in-process, through the
//! commands' JSON: every new symbol places with its family's prefix, labels and description, the
//! definition is embedded once; change-over contacts join by the sequence digit; ratings; quiet
//! points; fuse banks break the net; BOM rows.

use crate::common;

use common::{H, id};
use ecad_electrical::checks::Kind as CheckKind;
use ecad_electrical::symbols::catalogue::STARTERS;
use ecad_electrical::symbols::{self, Family, STARTER_LIBRARY};
use ecad_model::{ConnectionPointId, DeviceId, Op, Word};
use serde_json::{Value, json};

const SEED: u64 = 34;

fn new_h() -> H {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    h
}

fn place(h: &mut H, symbol: &str, at: [f64; 2]) -> Value {
    h.run("place", json!({"symbol": symbol, "at": at})).unwrap()
}

fn join(h: &mut H, symbol: &str, device: DeviceId, at: [f64; 2]) -> Result<Value, String> {
    h.run("place", json!({"symbol": symbol, "at": at, "device": device.hex()}))
}

fn labels(v: &Value) -> Vec<String> {
    v["connection_points"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap().to_string()).collect()
}

fn connection_point(v: &Value, label: &str) -> ConnectionPointId {
    let t = v["connection_points"].as_array().unwrap().iter().find(|t| t["label"] == label).unwrap();
    id(t, "connection_point")
}

fn setprop(h: &mut H, c: DeviceId, key: &str, value: Value) -> Result<Value, String> {
    h.run("setprop", json!({"component": c.hex(), "key": key, "value": value}))
}

fn prop(h: &mut H, c: DeviceId, key: &str) -> Option<String> {
    h.doc().devices()[&c].value(key)
}

fn embeds(ops: &[Op]) -> usize {
    ops.iter().filter(|o| matches!(o, Op::EmbedDefinition { .. })).count()
}

/// Brief 034's symbols, with the tag their first placement in a fresh project gets.
fn new_keys() -> Vec<&'static symbols::Starter> {
    // Brief 045's channel cells join cards (`plc_contracts`).
    STARTERS.iter().filter(|s| s.family != Family::Basic && ecad_electrical::plc::channels::cell_type(s).is_none()).collect()
}

#[test]
fn every_new_symbol_places_with_its_prefix_labels_and_description() {
    // A fresh project per symbol: the harness checks the whole page after each command, so one
    // page of every symbol would leave the fast set.
    for s in new_keys() {
        let mut h = new_h();
        let r = place(&mut h, s.key.word(), [0.0, 0.0]);
        assert_eq!(r["tag"], json!(format!("-{}01", s.prefix())), "{}", s.key);
        assert_eq!(r["joined"], json!(false), "{}", s.key);
        let c: DeviceId = id(&r, "component");
        // Brief 044 (design 044 §4.6): a connector's description is the registry's `Connector`
        // (each half's BOM description is its own); a pin's label is its pin name.
        if let symbols::Kind::ConnectorHalf { .. } = s.kind {
            assert_eq!(prop(&mut h, c, "description").as_deref(), Some("Connector"), "{}", s.key);
            if !matches!(s.key.word(), "pinpair" | "pins" | "pinp") {
                assert_eq!(labels(&r), s.labels(), "{}", s.key);
            }
        } else {
            assert_eq!(labels(&r), s.labels(), "{}", s.key);
            assert_eq!(prop(&mut h, c, "description").as_deref(), Some(s.name), "{}", s.key);
        }
        let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
        assert_eq!(embeds(&ops), 1, "{}: embedded on first use", s.key);
        let d = ops
            .iter()
            .find_map(|o| match o {
                Op::EmbedDefinition { definition, .. } => Some(definition.clone()),
                _ => None,
            })
            .unwrap();
        let o = d.origin.unwrap();
        assert_eq!((o.library, o.definition), (STARTER_LIBRARY, s.id), "{}", s.key);
        // A second placement reuses the embedded definition and takes the next number.
        let again = place(&mut h, s.key.word(), [0.0, -60.0]);
        assert_eq!(again["tag"], json!(format!("-{}02", s.prefix())), "{}", s.key);
        assert_eq!(again["definition"], r["definition"], "{}", s.key);
        let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
        assert_eq!(embeds(&ops), 0, "{}: reused by origin", s.key);
    }
}

#[test]
fn a_change_over_contact_takes_the_next_sequence_digit() {
    let mut h = new_h();
    let k1: DeviceId = id(&place(&mut h, "coil", [0.0, 0.0]), "component");
    assert_eq!(labels(&join(&mut h, "co", k1, [20.0, 0.0]).unwrap()), ["11", "12", "14"]);
    assert_eq!(labels(&join(&mut h, "no", k1, [40.0, 0.0]).unwrap()), ["23", "24"]);
    assert_eq!(labels(&join(&mut h, "co", k1, [60.0, 0.0]).unwrap()), ["31", "32", "34"]);
    assert_eq!(labels(&join(&mut h, "swco", k1, [80.0, 0.0]).unwrap()), ["41", "42", "44"]);
    for i in 5..=9 {
        join(&mut h, "nc", k1, [20.0 * i as f64 + 20.0, 0.0]).unwrap();
    }
    let head = h.head();
    assert_eq!(join(&mut h, "co", k1, [300.0, 0.0]).unwrap_err(), "-K01 already has 9 contacts.");
    assert_eq!(h.head(), head, "a refusal changes nothing");
    // A change-over contact on its own is a new device: 11/12/14.
    let r = place(&mut h, "co", [0.0, -60.0]);
    assert_eq!((r["tag"].as_str(), labels(&r)), (Some("-K02"), vec!["11".to_string(), "12".into(), "14".into()]));
}

#[test]
fn ssr_input_and_outputs_are_one_device() {
    let mut h = new_h();
    let k: DeviceId = id(&place(&mut h, "ssrin", [0.0, 0.0]), "component");
    let out = join(&mut h, "ssrout", k, [30.0, 0.0]).unwrap();
    assert_eq!(labels(&out), ["1", "2"]);
    assert_eq!(join(&mut h, "ssrout", k, [60.0, 0.0]).unwrap_err(), "-K01 already has connection point 1.");
    // A second head is refused with its name.
    let q: DeviceId = id(&place(&mut h, "rcbo2", [0.0, -60.0]), "component");
    assert_eq!(join(&mut h, "rcd2", q, [40.0, -60.0]).unwrap_err(), "-Q01 already has a head symbol.");
}

#[test]
fn a_fuse_bank_breaks_the_net_and_its_spare_points_are_quiet() {
    let mut h = new_h();
    let page = h.first_page();
    let bank = place(&mut h, "fusebank8", [0.0, 0.0]);
    let (lp, m, f1, f2, f3) = (
        connection_point(&bank, "L+"),
        connection_point(&bank, "M"),
        connection_point(&bank, "1"),
        connection_point(&bank, "2"),
        connection_point(&bank, "3"),
    );
    for t in [lp, f1, f2] {
        let at = h.connection_point_pos(page, t);
        let dy = if t == lp { 30.0 } else { -30.0 };
        h.wire_pp(&[(at.x, at.y), (at.x, at.y + dy)]).unwrap();
    }
    let doc = h.doc();
    let conn = doc.connectivity();
    let net = |t| conn.net_of_connection_point(t).unwrap();
    assert_ne!(net(lp), net(f1));
    assert_ne!(net(f1), net(f2));
    assert_ne!(net(lp), net(f2));
    // Spare fuse outputs and M raise no `Unconnected`; a wired point none either.
    for t in [m, f3] {
        assert!(
            !ecad_electrical::checks::nets::connection_point_checks(&doc, t).iter().any(|c| c.kind == CheckKind::Unconnected),
            "quiet point flagged"
        );
    }
    // L+ is not quiet: unwired, it is flagged.
    let mut h2 = new_h();
    let bank2 = place(&mut h2, "fusebank4", [0.0, 0.0]);
    let doc2 = h2.doc();
    assert!(
        ecad_electrical::checks::nets::connection_point_checks(&doc2, connection_point(&bank2, "L+"))
            .iter()
            .any(|c| c.kind == CheckKind::Unconnected)
    );
    assert!(ecad_electrical::checks::nets::connection_point_checks(&doc2, connection_point(&bank2, "M")).is_empty());
    // A non-quiet symbol still raises it (a coil).
    let coil = place(&mut h2, "coil", [100.0, 0.0]);
    let doc2 = h2.doc();
    assert!(
        ecad_electrical::checks::nets::connection_point_checks(&doc2, connection_point(&coil, "A1")).iter().any(|c| c.kind == CheckKind::Unconnected)
    );
}

#[test]
fn ratings_follow_the_rule() {
    let mut h = new_h();
    let f: DeviceId = id(&place(&mut h, "fusebank8", [0.0, 0.0]), "component");
    setprop(&mut h, f, "rating.3", json!(" 2A ")).unwrap();
    assert_eq!(prop(&mut h, f, "rating.3").as_deref(), Some("2 A"));
    let head = h.head();
    assert_eq!(setprop(&mut h, f, "rating.13", json!("2A")).unwrap_err(), "-F01 has no fuse 13.");
    assert_eq!(setprop(&mut h, f, "rating.L+", json!("2A")).unwrap_err(), "-F01 has no fuse L+.");
    assert_eq!(setprop(&mut h, f, "rating.M", json!("2A")).unwrap_err(), "-F01 has no fuse M.");
    assert_eq!(h.head(), head);
    setprop(&mut h, f, "rating.3", Value::Null).unwrap();
    assert_eq!(prop(&mut h, f, "rating.3"), None);
    let q: DeviceId = id(&place(&mut h, "rcbo2", [0.0, -80.0]), "component");
    setprop(&mut h, q, "rating", json!("13A C 30mA")).unwrap();
    assert_eq!(prop(&mut h, q, "rating").as_deref(), Some("curve C, 13 A, 30 mA"));
    assert_eq!(setprop(&mut h, q, "rating.1", json!("2A")).unwrap_err(), "rating.1 is for a fuse bank's fuses; -Q01 is not a fuse bank.");
    // Several devices at once follow the same rule.
    let many = h.run("setprop", json!({"devices": [f.hex(), q.hex()], "key": "rating.1", "value": "1A"}));
    assert!(many.is_err());
    // Design 072 §2.2: a fuse bank's ratings are per fuse; text that is no rating is a note.
    assert!(h.run("setprop", json!({"devices": [f.hex(), q.hex()], "key": "rating", "value": "x"})).is_err());
    let fuse: DeviceId = id(&place(&mut h, "fuse", [80.0, -80.0]), "component");
    h.run("setprop", json!({"devices": [fuse.hex(), q.hex()], "key": "rating", "value": "x"})).unwrap();
    assert_eq!(prop(&mut h, q, "rating").as_deref(), Some("x"));
    assert!(h.doc().devices()[&q].has_no_rating());
}

#[test]
fn hostile_ratings_and_keys_never_panic() {
    let mut h = new_h();
    let f: DeviceId = id(&place(&mut h, "fusebank4", [0.0, 0.0]), "component");
    let head = h.head();
    assert!(h.run("place", json!({"symbol": "fusebank999", "at": [0, 0]})).is_err());
    assert!(h.run("place", json!({"symbol": "nonsense\u{7}", "at": [0, 0]})).is_err());
    assert!(setprop(&mut h, f, "rating", json!("x".repeat(10_000))).is_err());
    assert!(setprop(&mut h, f, "rating", json!("6A\u{1}")).is_err());
    let long = format!("rating.{}", "9".repeat(10_000));
    let e = setprop(&mut h, f, &long, json!("1A")).unwrap_err();
    assert!(e.chars().count() < 80, "{e}");
    assert!(setprop(&mut h, f, "rating.", json!("1A")).is_err());
    assert!(setprop(&mut h, f, "rating.-1", json!("1A")).is_err());
    assert_eq!(h.head(), head);
}

#[test]
fn bom_has_one_row_per_device_and_contacts_add_none() {
    let mut h = new_h();
    let k: DeviceId = id(&place(&mut h, "coil", [0.0, 0.0]), "component");
    join(&mut h, "co", k, [20.0, 0.0]).unwrap();
    join(&mut h, "power4", k, [60.0, 0.0]).unwrap();
    let s: DeviceId = id(&place(&mut h, "ssrin", [0.0, -60.0]), "component");
    join(&mut h, "ssrout", s, [20.0, -60.0]).unwrap();
    place(&mut h, "rcbo2", [0.0, -120.0]);
    place(&mut h, "fusebank6", [60.0, -120.0]);
    place(&mut h, "fuseterminalblock", [200.0, -120.0]);
    let v = h.run("bom", json!({})).unwrap();
    let rows: Vec<&Value> = v["rows"].as_array().unwrap().iter().filter(|r| r.get("accessory").is_none()).collect();
    let tags: Vec<String> = rows.iter().map(|r| r["tags"].as_str().unwrap_or_default().to_string()).collect();
    assert_eq!(rows.len(), 5, "{tags:?}");
    for t in ["-K01", "-K02", "-Q01", "-F01", "-F02"] {
        assert_eq!(tags.iter().filter(|r| r.split([',', ' ']).any(|x| x == t)).count(), 1, "{t} in {tags:?}");
    }
}

#[test]
fn a_limit_switch_is_two_symbols_of_one_device_and_sensors_take_their_prefixes() {
    let mut h = new_h();
    let b = place(&mut h, "rollerno", [0.0, 0.0]);
    assert_eq!((b["tag"].as_str(), labels(&b)), (Some("-B01"), vec!["13".to_string(), "14".into()]));
    let b1: DeviceId = id(&b, "component");
    assert_eq!(labels(&join(&mut h, "rollernc", b1, [30.0, 0.0]).unwrap()), ["21", "22"]);
    assert_eq!(place(&mut h, "valve", [0.0, -60.0])["tag"], "-MB01");
    assert_eq!(place(&mut h, "valve2", [60.0, -60.0])["tag"], "-MB02");
    assert_eq!(place(&mut h, "heater", [120.0, -60.0])["tag"], "-E01");
    let tc: DeviceId = id(&place(&mut h, "tc", [180.0, -60.0]), "component");
    setprop(&mut h, tc, "rating", json!("K")).unwrap();
    assert_eq!(prop(&mut h, tc, "rating").as_deref(), Some("K"));
    // A second head on a sensor is refused with its name.
    let p: DeviceId = id(&place(&mut h, "prox3", [0.0, -120.0]), "component");
    assert_eq!(join(&mut h, "tc", p, [40.0, -120.0]).unwrap_err(), "-B03 already has a head symbol.");
}

#[test]
fn a_bulkhead_pin_is_one_net_from_both_sides_and_spare_pins_are_quiet() {
    let mut h = new_h();
    let page = h.first_page();
    let rj = place(&mut h, "rj45", [0.0, 0.0]);
    assert_eq!(rj["tag"], "-X01");
    let (p1, p2, sh) = (connection_point(&rj, "1"), connection_point(&rj, "2"), connection_point(&rj, "SH"));
    let at = h.connection_point_pos(page, p1);
    let up: ecad_model::WireId = id(&h.wire_pp(&[(at.x, at.y), (at.x, at.y + 30.0)]).unwrap(), "conductor");
    let down: ecad_model::WireId = id(&h.wire_pp(&[(at.x, at.y), (at.x, at.y - 30.0)]).unwrap(), "conductor");
    let doc = h.doc();
    let conn = doc.connectivity();
    assert_eq!(conn.net_of_conductor(up), conn.net_of_conductor(down), "a feed-through is one net");
    for t in [p2, sh] {
        assert!(ecad_electrical::checks::nets::connection_point_checks(&doc, t).is_empty(), "spare pin flagged");
    }
    // Plugs and sockets are separate devices with their own tags in v0.15.
    assert_eq!(place(&mut h, "socket4", [0.0, -60.0])["tag"], "-X02");
    assert_eq!(place(&mut h, "plug4", [0.0, -80.0])["tag"], "-X03");
}

#[test]
fn a_card_body_takes_its_channels_as_one_device() {
    let mut h = new_h();
    let card = place(&mut h, "plcdi", [0.0, 0.0]);
    assert_eq!((card["tag"].as_str(), labels(&card)), (Some("-A01"), vec!["L+".to_string(), "M".into()]));
    let a: DeviceId = id(&card, "component");
    // Brief 045 (FP7): channels are labelled with pins.
    assert_eq!(labels(&join(&mut h, "di", a, [40.0, 0.0]).unwrap()), ["1"]);
    assert_eq!(labels(&join(&mut h, "di", a, [50.0, 0.0]).unwrap()), ["2"]);
    let doc = h.doc();
    let all: std::collections::BTreeSet<String> = ecad_electrical::devices::placing::labels_of(&doc, a);
    assert_eq!(all.into_iter().collect::<Vec<_>>(), ["1", "2", "L+", "M"]);
    // A card's supply points are quiet; a CPU's are not.
    assert!(ecad_electrical::checks::nets::connection_point_checks(&doc, connection_point(&card, "L+")).is_empty());
    let cpu = place(&mut h, "plccpu", [100.0, 0.0]);
    let doc = h.doc();
    assert!(
        ecad_electrical::checks::nets::connection_point_checks(&doc, connection_point(&cpu, "L+")).iter().any(|c| c.kind == CheckKind::Unconnected)
    );
}

/// Brief 034 criterion 11: a symbol's prefix is a registered default (`symbol.<key>.prefix`),
/// overridden at the project level through Tag Settings (`settings.tag_prefixes`), and a newly
/// placed device follows it.
#[test]
fn a_changed_registered_prefix_is_followed_by_the_next_device() {
    let mut h = new_h();
    let entry = |h: &mut H| h.run("defaults", json!({"key": "symbol.fusebank8.prefix"})).unwrap()["defaults"][0].clone();
    let e = entry(&mut h);
    assert_eq!((e["value"].as_str(), e["from"].as_str()), (Some("F"), Some("builtin")), "{e}");
    assert_eq!(place(&mut h, "fusebank8", [0.0, 0.0])["tag"], "-F01");
    h.run("tagsettings", json!({"prefixes": {"fusebank8": "FB"}})).unwrap();
    let e = entry(&mut h);
    assert_eq!((e["value"].as_str(), e["from"].as_str()), (Some("FB"), Some("project")), "{e}");
    assert_eq!(place(&mut h, "fusebank8", [0.0, -60.0])["tag"], "-FB01");
    // Other symbols keep their built-in.
    assert_eq!(place(&mut h, "fuseterminalblock", [100.0, 0.0])["tag"], "-F02");
    // Back to the built-in.
    h.run("tagsettings", json!({"prefixes": {"fusebank8": null}})).unwrap();
    assert_eq!(entry(&mut h)["from"], "builtin");
    assert_eq!(place(&mut h, "fusebank8", [100.0, -60.0])["tag"], "-F03");
}

/// Review of PR #63, 2: clearing `rating.<n>` always works, also once the device is no longer a
/// fuse bank (its bank symbol erased, its fault contact kept).
#[test]
fn a_fuse_rating_can_be_cleared_after_the_bank_is_erased() {
    let mut h = new_h();
    let page = h.first_page();
    let bank = place(&mut h, "fusebank8", [0.0, 0.0]);
    let f: DeviceId = id(&bank, "component");
    join(&mut h, "no", f, [150.0, 0.0]).unwrap();
    setprop(&mut h, f, "rating.3", json!("2A")).unwrap();
    let item: ecad_model::ItemId = id(&bank, "item");
    let handle = h.handle_of(page, item);
    h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    assert!(h.doc().devices().contains_key(&f), "the fault contact keeps the device");
    // Setting is still refused: the device is no longer a fuse bank.
    assert!(setprop(&mut h, f, "rating.3", json!("4A")).is_err());
    // Clearing works, by null and by an empty value.
    setprop(&mut h, f, "rating.3", Value::Null).unwrap();
    assert_eq!(prop(&mut h, f, "rating.3"), None);
    setprop(&mut h, f, "rating.3", json!("x")).unwrap_err();
    setprop(&mut h, f, "rating.7", json!("  ")).unwrap();
    // A bank never had fuse 13; clearing it is a no-op, not an error.
    let g: DeviceId = id(&place(&mut h, "fusebank4", [0.0, -100.0]), "component");
    setprop(&mut h, g, "rating.13", Value::Null).unwrap();
}

/// Review of PR #63, 4 (design 034 §3): a fuse bank's outputs `11`/`12` are not contacts, so a
/// fault contact joined to a 12-way bank is `13`/`14`; a contact never reuses an output's label.
#[test]
fn a_fault_contact_on_a_fuse_bank_ignores_its_outputs() {
    let mut h = new_h();
    let f: DeviceId = id(&place(&mut h, "fusebank12", [0.0, 0.0]), "component");
    assert_eq!(labels(&join(&mut h, "no", f, [200.0, 0.0]).unwrap()), ["13", "14"]);
    assert_eq!(labels(&join(&mut h, "nc", f, [220.0, 0.0]).unwrap()), ["21", "22"]);
    assert_eq!(labels(&join(&mut h, "co", f, [240.0, 0.0]).unwrap()), ["31", "32", "34"]);
    // A change-over first on a 12-way bank skips the digit whose labels outputs 11 and 12 hold.
    let g: DeviceId = id(&place(&mut h, "fusebank12", [0.0, -100.0]), "component");
    assert_eq!(labels(&join(&mut h, "co", g, [200.0, -100.0]).unwrap()), ["21", "22", "24"]);
    // On a 4-way bank nothing collides.
    let k: DeviceId = id(&place(&mut h, "fusebank4", [0.0, -200.0]), "component");
    assert_eq!(labels(&join(&mut h, "nc", k, [100.0, -200.0]).unwrap()), ["11", "12"]);
}

/// Design 034 §13 check 11 (review of PR #63, 3): save, close and reopen keep every new device,
/// its labels, its ratings and the embedded definitions, which still resolve to their catalogue
/// entries by origin.
#[test]
fn save_close_and_reopen_keep_the_library_devices() {
    let mut h = new_h();
    let k: DeviceId = id(&place(&mut h, "coil", [0.0, 0.0]), "component");
    join(&mut h, "co", k, [20.0, 0.0]).unwrap();
    join(&mut h, "no", k, [40.0, 0.0]).unwrap();
    let f: DeviceId = id(&place(&mut h, "fusebank8", [0.0, -60.0]), "component");
    setprop(&mut h, f, "rating.3", json!("2A")).unwrap();
    let q: DeviceId = id(&place(&mut h, "rcbo2", [150.0, -60.0]), "component");
    setprop(&mut h, q, "rating", json!("13A C 30mA")).unwrap();
    for (i, key) in ["prox3", "valve2", "dsub9s", "rj45", "plcdi"].into_iter().enumerate() {
        place(&mut h, key, [60.0 * i as f64, -160.0]);
    }
    h.cfg(|p| p.persistence = ecad_engine::Persistence::File);
    let dir = std::env::temp_dir().join(format!("ecad-034-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("library.ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    let live = h.doc();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_engine::ProjectHandle::open_file(&mut s, &path, 77).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    let doc = h2.doc();
    assert_eq!(doc, live, "devices, labels, ratings and embedded definitions");
    assert!(h2.p(|p| p.ledger.replay_matches().unwrap()));
    let all: std::collections::BTreeSet<String> = ecad_electrical::devices::placing::labels_of(&doc, k);
    assert_eq!(all.into_iter().collect::<Vec<_>>(), ["11", "12", "14", "23", "24", "A1", "A2"]);
    assert_eq!(doc.devices()[&f].value("rating.3").as_deref(), Some("2 A"));
    assert_eq!(doc.devices()[&q].value("rating").as_deref(), Some("curve C, 13 A, 30 mA"));
    // Every embedded definition is still the catalogue's, by origin and content hash.
    assert_eq!(doc.definitions().len(), 10);
    for d in doc.definitions().values() {
        let s = symbols::catalogue::of_definition(d).expect("a catalogue symbol");
        assert_eq!(d.origin, s.definition().unwrap().origin, "{}", s.key);
    }
    h2.check();
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}
