//! Brief 018 headless in the trial (design 018 §12 check J and the reference screenshots): the
//! Components group's new buttons, the Find Symbol window, Place Symbol from a search row, the
//! device lens, and a duplicate refusal at the command line. Screenshots are written when
//! `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; they are for the owner's
//! approval, never committed.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_model::{ComponentId, Subject};
use ecad_trial::{search, tools};
use eframe::egui::{self, Event};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Deltas, button, doc, harness_with, ok, pointer_to, prompt, screen, shot, type_enter};

type H = Harness<'static, ecad_trial::Trial>;

fn out() -> Option<PathBuf> {
    let dir = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from)?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn save(h: &H, deltas: &Deltas, name: &str) {
    if let Some(dir) = out()
        && shot(h, deltas, &dir.join(name)).is_none()
    {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn device(h: &mut H, tag: &str) -> ComponentId {
    let d = doc(h);
    d.components_tagged(tag).first().copied().unwrap_or_else(|| panic!("no device {tag}"))
}

fn zoom(h: &mut H) {
    assert!(h.state_mut().zoom_to_drawing(), "zoom to the drawing");
    h.run_steps(3);
}

#[test]
fn every_starter_symbol_on_one_page() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    // The 19 device symbols (brief 028 added the PE terminal); the junction and continuation
    // (brief 022) are not devices.
    let devices = ecad_cadcraft::symbols::STARTERS.iter().filter(|s| s.kind != ecad_cadcraft::symbols::Kind::Tagless);
    for (i, s) in devices.enumerate() {
        let (x, y) = (60 * (i % 6), -60 * (i / 6) as i64);
        ok(&mut h, &format!(r#"place {{"symbol":"{}","at":[{x},{y}]}}"#, s.key));
    }
    let d = doc(&mut h);
    assert_eq!(d.components.len(), 19);
    zoom(&mut h);
    save(&h, &deltas, "018-symbols-all.png");
}

#[test]
fn search_place_hover_and_duplicate_by_the_ui() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    // Check J: the Components group has the new buttons.
    button(&mut h, tools::TAB);
    for label in ["Find Symbol", "Place Symbol", "Device Properties", "Delete Device"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    // Page 1: -K01's coil.
    ok(&mut h, r#"place {"symbol":"coil","at":[0,0]}"#);
    let k1 = device(&mut h, "-K01");
    zoom(&mut h);
    ok(&mut h, "zoom s 0.5x");
    save(&h, &deltas, "018-k1-page1.png");
    // Page 2: its NO and NC contacts.
    ok(&mut h, r#"newpage {"title":"Control"}"#);
    ok(&mut h, &format!(r#"place {{"symbol":"no","at":[0,0],"device":"{}"}}"#, k1.hex()));
    ok(&mut h, &format!(r#"place {{"symbol":"nc","at":[30,0],"device":"{}"}}"#, k1.hex()));
    assert_eq!(doc(&mut h).representations_of(Subject::Component(k1)).count(), 3);
    zoom(&mut h);
    ok(&mut h, "zoom s 0.5x");
    save(&h, &deltas, "018-k1-page2.png");

    // Find Symbol: the window and its field.
    button(&mut h, "Find Symbol");
    assert!(h.state().search.open);
    h.get_by_label(search::FIELD_LABEL).click();
    h.run_steps(2);
    h.event(Event::Text("cont".into()));
    h.run_steps(3);
    assert_eq!(h.state().search.query, "cont");
    let rows = search::rows(Some(&doc(&mut h)), "cont").0;
    assert!(rows.iter().any(|r| r.pick == search::Pick::Symbol("no")));
    save(&h, &deltas, "018-search.png");

    // A symbol row starts Place Symbol at the insertion point; D and Enter join -K01.
    h.state_mut().start_pick(search::Pick::Symbol("no")).unwrap();
    h.run_steps(2);
    assert!(prompt(&h).starts_with("Specify insertion point (new device -K02"), "{}", prompt(&h));
    type_enter(&mut h, "D");
    type_enter(&mut h, "");
    assert!(prompt(&h).starts_with("Specify insertion point (joins -K01 as 33/34"), "{}", prompt(&h));
    let at = screen(&h, 60.0, 0.0);
    pointer_to(&mut h, at);
    h.run_steps(3);
    save(&h, &deltas, "018-place-preview.png");
    h.event(Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(2);
    h.state_mut().cad.session.cancel();
    h.state_mut().search.open = false;
    h.run_steps(2);

    // The lens on the NO contact (its upper lead).
    let lead = screen(&h, 0.0, 7.0);
    pointer_to(&mut h, lead);
    h.run_steps(3);
    let lines = h.state().hover_lines().map(<[String]>::to_vec).unwrap_or_default();
    assert_eq!(lines.first().map(String::as_str), Some("Device -K01   Contactor / relay coil"), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("This symbol: NO contact 13/14")), "{lines:?}");
    save(&h, &deltas, "018-hover.png");

    // A duplicate: a second coil tagged -K01 is refused, naming -K01.
    ok(&mut h, r#"place {"symbol":"coil","at":[0,-40]}"#);
    let k2 = device(&mut h, "-K02");
    let e = h.state_mut().submit_command(&format!(r#"setprop {{"component":"{}","key":"tag","value":"K01"}}"#, k2.hex()));
    let e = e.unwrap_or_default();
    assert!(e.contains("Tag -K01 is already used by -K01"), "{e}");
    let away = screen(&h, 200.0, 200.0);
    pointer_to(&mut h, away);
    h.run_steps(3);
    save(&h, &deltas, "018-duplicate.png");
}

/// Owner's trial of 2026-11-08: picking a symbol in Find Symbol lists "New device" and the devices
/// it can join; one click picks, the next click on the page places. No Enter.
#[test]
fn find_symbol_offers_new_device_and_the_devices_to_join_by_click() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    ok(&mut h, r#"place {"symbol":"coil","at":[0,0]}"#);
    ok(&mut h, r#"place {"symbol":"fuse","at":[40,0]}"#);
    zoom(&mut h);
    let k1 = device(&mut h, "-K01");
    let choices = |h: &mut H, pick: search::Pick| {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| search::choices(p, &pick)).unwrap()
    };
    let c = choices(&mut h, search::Pick::Symbol("no"));
    assert_eq!(c[0].text, "New device (-K02)");
    assert_eq!(c[0].device, None);
    assert_eq!(c[1].device.as_deref(), Some("-K01"), "the coil's device comes next: {c:?}");
    assert!(c[1].text.ends_with("13/14"), "{c:?}");
    assert_eq!(c.iter().filter(|x| x.device.is_some()).count(), 2, "the fuse can take a contact too, after: {c:?}");
    // A second coil cannot join -K01: it is not offered.
    let coil = choices(&mut h, search::Pick::Symbol("coil"));
    assert!(coil.iter().all(|x| x.device.as_deref() != Some("-K01")), "{coil:?}");
    // A device row lists what it can take, its own prefix's symbols first.
    let d = choices(&mut h, search::Pick::Device("-K01".into()));
    assert!(d.iter().all(|x| x.symbol != "coil") && d.first().is_some_and(|x| x.symbol == "no"), "{d:?}");

    // The window: a row click shows the second step; one click on a choice starts placing.
    button(&mut h, tools::TAB);
    button(&mut h, "Find Symbol");
    h.state_mut().search.picked = Some(search::Pick::Symbol("no"));
    h.run_steps(3);
    let label = c[1].text.clone();
    h.get_by_label(&label).click();
    h.run_steps(3);
    assert!(prompt(&h).starts_with("Specify insertion point (joins -K01 as 13/14"), "{}", prompt(&h));
    assert!(h.state().search.picked.is_none(), "back to the search after a choice");
    // The window may sit over the page; here it is closed so the click reaches the drawing.
    h.state_mut().search.open = false;
    h.run_steps(2);
    let at = screen(&h, 20.0, 0.0);
    crate::mouse_tester::click(&mut h, at);
    let log: Vec<String> = h.state().cad.session.log.iter().rev().take(4).cloned().collect();
    assert_eq!(doc(&mut h).representations_of(Subject::Component(k1)).count(), 2, "the click placed it: {} {log:?}", prompt(&h));
    save(&h, &deltas, "018-choices.png");
    // The prompt's default device is a keyword: a click on it (sent as the keyword) answers.
    type_enter(&mut h, "D");
    assert_eq!(prompt(&h), "Enter device to join or [-K01/New] <-K01>:");
    h.state_mut().cad.session.input(cadcraft_engine::Input::Keyword("-K01".into())).unwrap();
    h.run_steps(2);
    assert!(prompt(&h).starts_with("Specify insertion point (joins -K01 as 23/24"), "{}", prompt(&h));
}
