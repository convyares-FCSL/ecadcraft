//! The wire commands called as types (`ecad_engine::commands::call`): params built as structs, replies
//! read as their types, refusals as `CmdError`; and each typed reply is exactly the JSON the command
//! line and MCP get.

use ecad_electrical::commands::input::{Bounded, Point};
use ecad_electrical::wires::commands::connect::{Disconnect, DisconnectParams, EndParam};
use ecad_electrical::wires::commands::nets::{
    Checks, ChecksParams, Net, NetParams, Nets, NetsParams, Potential, PotentialParams, Renumberwires, RenumberwiresParams, Wiresettings,
    WiresettingsParams,
};
use ecad_engine::commands::{call, reply_of, reply_schema_of, schema_of};
use ecad_engine::wires::commands::{JoinwiresParams, JoinwiresReply, Wire, WireParams};
use ecad_model::Pt;
use serde_json::json;

use crate::common::H;

fn points(p: &[(f64, f64)]) -> Bounded<Point, 2, 4096> {
    Bounded(p.iter().map(|(x, y)| Point(Pt::new(*x, *y))).collect())
}

/// `wire`, `potential`, `net`, `nets`, `renumberwires`, `wiresettings` and `checks` through `call`: each reply is
/// the type, and says what the JSON says.
#[test]
fn the_wire_commands_answer_as_their_types() {
    let mut h = H::new(5);
    let drawn =
        call::<Wire>(&mut h.s, WireParams { points: points(&[(0.0, 0.0), (40.0, 0.0)]), potential: None, join: None, category: None }).unwrap();
    assert!(drawn.created);
    assert!(drawn.attached.is_empty() && drawn.joined.is_empty());

    let settings = call::<Wiresettings>(&mut h.s, WiresettingsParams::default()).unwrap();
    assert!(settings.label_min_length >= 0.0);

    let renumbered = call::<Renumberwires>(&mut h.s, RenumberwiresParams::default()).unwrap();
    let nets = call::<Nets>(&mut h.s, NetsParams { all: Some(true), ..NetsParams::default() }).unwrap();
    assert!(nets.next.is_none());
    assert!(renumbered.renumbered <= nets.nets.len());

    let checks = call::<Checks>(&mut h.s, ChecksParams::default()).unwrap();
    assert_eq!(checks.checks.len().min(checks.total), checks.checks.len());

    // A wire between two device points carries a wire, so its net can be named by it.
    h.run("place", json!({"symbol": "terminal_block", "at": [0, -40], "tag": "X1"})).unwrap();
    h.run("place", json!({"symbol": "terminal_block", "at": [40, -40], "tag": "X2"})).unwrap();
    let w = call::<Wire>(
        &mut h.s,
        WireParams { points: points(&[(0.0, -40.0), (40.0, -40.0)]), potential: Some("24V".into()), join: None, category: None },
    )
    .unwrap();
    let conductor = w.conductor.expect("a wire between two points");
    assert_eq!(w.attached.len(), 2, "{w:?}");
    let set = call::<Potential>(&mut h.s, PotentialParams { conductor, value: Some(Some("L1".into())) }).unwrap();
    assert_eq!(set.potential.as_deref(), Some("L1"));
    let net = call::<Net>(&mut h.s, NetParams { conductor: Some(conductor), ..NetParams::default() }).unwrap();
    assert_eq!((net.potential.as_deref(), net.conductors.clone()), (Some("L1"), vec![conductor]));

    // The same command through its JSON form: the typed reply is exactly that JSON.
    let v = h.run("net", json!({"conductor": conductor.hex()})).unwrap();
    assert_eq!(serde_json::to_value(reply_of::<Net>(v.clone()).unwrap()).unwrap(), v);
}

/// A refusal is a `CmdError` in the command's words, and nothing is recorded.
#[test]
fn a_refused_call_is_a_cmd_error() {
    let mut h = H::new(6);
    let head = h.p(|p| p.ledger.head());
    let e = call::<Disconnect>(&mut h.s, DisconnectParams { route: ecad_model::RouteId(7), end: EndParam::A }).unwrap_err();
    assert!(e.to_string().contains("no such wire"), "{e}");
    assert_eq!(h.p(|p| p.ledger.head()), head);
}

/// `joinwires`' reply is one of two shapes, told apart by `how`.
#[test]
fn joinwires_answers_merged_or_tee() {
    let mut h = H::new(7);
    call::<Wire>(&mut h.s, WireParams { points: points(&[(0.0, 0.0), (50.0, 0.0)]), potential: None, join: None, category: None }).unwrap();
    call::<Wire>(&mut h.s, WireParams { points: points(&[(50.0, 0.0), (50.0, -20.0)]), potential: None, join: Some(false), category: None }).unwrap();
    let r = call::<ecad_engine::wires::commands::Joinwires>(&mut h.s, JoinwiresParams { at: Point(Pt::new(50.0, 0.0)) }).unwrap();
    assert!(matches!(r, JoinwiresReply::Merged { ref removed, .. } if removed.len() == 1), "{r:?}");
}

/// Every wire command has a params schema and a reply schema.
#[test]
fn every_wire_command_has_both_schemas() {
    for id in [
        "wire",
        "joinwires",
        "nets",
        "net",
        "wirenumber",
        "potential",
        "category",
        "wiresettings",
        "renumberwires",
        "checks",
        "connect",
        "disconnect",
    ] {
        assert!(schema_of(id).is_some_and(|s| s.is_object()), "{id}: params schema");
        assert!(reply_schema_of(id).is_some_and(|s| s.is_object()), "{id}: reply schema");
    }
}
