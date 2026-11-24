//! A motor's rated voltage against the supply typed on its potentials: the Checks row, the offer
//! on the Device tab, and `setprop rating.voltage`.

use std::collections::BTreeSet;

use ecad_model::DeviceId;
use ecad_model::geometry::units::Volts;
use ecad_model::objects::devices::DeviceType;
use serde_json::{Value, json};

use crate::common::{H, id};

fn mismatches(h: &mut H) -> Vec<String> {
    let all = h.run("checks", json!({"kinds": ["supply-mismatch"], "limit": 100})).unwrap();
    all["checks"].as_array().cloned().unwrap_or_default().iter().map(|c| c["which"].as_str().unwrap_or_default().to_string()).collect()
}

fn rated_voltage(h: &mut H, m: DeviceId) -> Option<Volts> {
    match &h.doc().devices()[&m].device_type {
        DeviceType::Motor(x) => x.rated_voltage,
        _ => None,
    }
}

/// The Rated voltage row of `m` on the Device tab: its value and its offer.
fn voltage_row(h: &mut H, m: DeviceId) -> (String, Option<(String, String)>) {
    let doc = h.doc();
    let s = ecad_electrical::properties::device_section(&doc, &ecad_drawing_style::REGISTRY, None, &BTreeSet::from([m]), None);
    let r = s.rows.iter().find(|r| r.key == "rating.voltage").expect("a motor has the row");
    (r.value.clone(), r.proposal.clone())
}

/// A motor wired to L1, L1 supplied with 400/230 V 3~ 50 Hz.
fn fixture() -> (H, DeviceId, ecad_model::WireId, (f64, f64)) {
    let mut h = H::new(0x00d4_0032);
    let mut table: Vec<Value> = h.run("wiresettings", json!({})).unwrap()["potentials"].as_array().unwrap().clone();
    for row in &mut table {
        if row["name"] == "L1" {
            row["supply"] = json!("400/230 V 3~ 50 Hz");
        }
    }
    h.run("wiresettings", json!({"potentials": table})).unwrap();
    let m: DeviceId = id(&crate::common::place(&mut h, "motor3", (100.0, 100.0)), "component");
    let doc = h.doc();
    let at = ecad_model::connectivity::derive::connection_point_positions(&doc, doc.page_order()[0])
        .into_iter()
        .find(|(t, _, _)| doc.connection_points().get(t).and_then(|x| x.device()) == Some(m))
        .map(|(_, _, p)| p)
        .unwrap();
    let w: ecad_model::WireId = id(&h.wire_pp(&[(at.x, at.y), (at.x, at.y + 40.0)]).unwrap(), "conductor");
    h.run("potential", json!({"conductor": w.hex(), "value": "L1"})).unwrap();
    (h, m, w, (at.x, at.y + 40.0))
}

/// An unrated motor is offered its supply's voltage and nothing is written; `setprop` takes it.
#[test]
fn a_motor_is_offered_its_supply_voltage_and_never_given_it() {
    let (mut h, m, _, _) = fixture();
    let (value, offer) = voltage_row(&mut h, m);
    assert_eq!((value.as_str(), offer), ("", Some(("400 V".to_string(), "From L1: 400/230 V 3~ 50 Hz".to_string()))));
    assert_eq!(rated_voltage(&mut h, m), None, "never filled in by itself");
    assert!(mismatches(&mut h).is_empty(), "nothing to compare");
    // The offer's link sets the row's key to its value.
    h.run("setprop", json!({"component": m.hex(), "key": "rating.voltage", "value": "400 V"})).unwrap();
    assert_eq!(rated_voltage(&mut h, m), Some(Volts(400.0)));
    assert_eq!(voltage_row(&mut h, m), ("400 V".to_string(), None));
    // Empty clears it; text that is no voltage is refused.
    h.run("setprop", json!({"component": m.hex(), "key": "rating.voltage", "value": ""})).unwrap();
    assert_eq!(rated_voltage(&mut h, m), None);
    let e = h.run("setprop", json!({"component": m.hex(), "key": "rating.voltage", "value": "15 A"})).unwrap_err();
    assert!(e.contains("Rated voltage takes a voltage"), "{e}");
}

/// A rated voltage that differs from the line-to-line voltage is a Checks row; the rating read
/// from typed text and the frequency are compared too, and the row goes when they agree.
#[test]
fn a_motor_rated_for_another_voltage_is_a_checks_row() {
    let (mut h, m, _, _) = fixture();
    h.run("setprop", json!({"component": m.hex(), "key": "rating.voltage", "value": "230"})).unwrap();
    let rows = mismatches(&mut h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].ends_with("is rated 230 V; its supply gives 400 V line-to-line, L1 (400/230 V 3~ 50 Hz)"), "{rows:?}");
    h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "7.5 kW 400 V 60 Hz"})).unwrap();
    let rows = mismatches(&mut h);
    assert!(rows.len() == 1 && rows[0].contains("rated 60 Hz; its supply is 50 Hz"), "{rows:?}");
    h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "7.5 kW 400 V 50 Hz"})).unwrap();
    assert!(mismatches(&mut h).is_empty());
}

/// The kept Checks list follows the potential of a wire away from the motor: the row comes and
/// goes as a wire elsewhere on its net changes potential, and the kept list equals a fresh build.
#[test]
fn the_row_follows_a_potential_set_on_another_wire_of_the_net() {
    let (mut h, m, w, end) = fixture();
    let kept_and_fresh = |h: &mut H| {
        h.cfg(|p| {
            let doc = p.ledger.document().clone();
            let of = |ch: &ecad_electrical::checks::kept::Checks| {
                ch.sorted(&doc)
                    .iter()
                    .filter(|c| c.kind == ecad_electrical::checks::Kind::SupplyMismatch)
                    .map(|c| c.which.clone())
                    .collect::<Vec<_>>()
            };
            let kept = of(p.checks());
            (kept, of(ecad_engine::project::engine_views().checks(&doc, p.tolerance)))
        })
    };
    // Shown once: kept from here on.
    h.cfg(|p| {
        p.checks();
    });
    h.run("setprop", json!({"component": m.hex(), "key": "rating.voltage", "value": "230"})).unwrap();
    let (kept, fresh) = kept_and_fresh(&mut h);
    assert_eq!((kept.len(), &kept), (1, &fresh));
    // A second wire on the net, away from the motor, with no potential; then the motor's wire
    // loses L1: no supply, no row.
    let far: ecad_model::WireId = id(&h.wire_pp(&[end, (end.0 + 40.0, end.1)]).unwrap(), "conductor");
    h.run("potential", json!({"conductor": w.hex(), "value": null})).unwrap();
    let (kept, fresh) = kept_and_fresh(&mut h);
    assert!(fresh.is_empty(), "{fresh:?}");
    assert_eq!(kept, fresh);
    // The far wire given L1: the row is back, kept as built.
    h.run("potential", json!({"conductor": far.hex(), "value": "L1"})).unwrap();
    let (kept, fresh) = kept_and_fresh(&mut h);
    assert_eq!((fresh.len(), &kept), (1, &fresh));
    // The project's tolerance widened past 230 against 400: the row goes, kept as built.
    h.run("setdefault", json!({"key": "rating.supply_tolerance", "level": "project", "value": 50})).unwrap();
    let (kept, fresh) = kept_and_fresh(&mut h);
    assert!(fresh.is_empty(), "{fresh:?}");
    assert_eq!(kept, fresh);
}

/// A single-phase motor on a three-phase supply typed with no line-to-neutral voltage is a row,
/// and nothing is offered: no voltage is computed from another.
#[test]
fn a_single_phase_motor_on_a_supply_without_neutral_voltage_is_a_row() {
    let mut h = H::new(0x00d4_0033);
    let mut table: Vec<Value> = h.run("wiresettings", json!({})).unwrap()["potentials"].as_array().unwrap().clone();
    for row in &mut table {
        if row["name"] == "L1" {
            row["supply"] = json!("400 V 3~");
        }
    }
    h.run("wiresettings", json!({"potentials": table})).unwrap();
    let m: DeviceId = id(&crate::common::place(&mut h, "motor1", (100.0, 100.0)), "component");
    let doc = h.doc();
    let at = ecad_model::connectivity::derive::connection_point_positions(&doc, doc.page_order()[0])
        .into_iter()
        .find(|(t, _, _)| doc.connection_points().get(t).and_then(|x| x.device()) == Some(m))
        .map(|(_, _, p)| p)
        .unwrap();
    let w: ecad_model::WireId = id(&h.wire_pp(&[(at.x, at.y), (at.x, at.y + 40.0)]).unwrap(), "conductor");
    h.run("potential", json!({"conductor": w.hex(), "value": "L1"})).unwrap();
    let rows = mismatches(&mut h);
    assert!(rows.len() == 1 && rows[0].ends_with("is single-phase: supply line-to-neutral voltage unknown, L1 (400 V 3~)"), "{rows:?}");
    assert_eq!(voltage_row(&mut h, m), (String::new(), None));
}
