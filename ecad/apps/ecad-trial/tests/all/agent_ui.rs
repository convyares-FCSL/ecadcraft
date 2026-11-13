//! Brief 035 headless in the trial (design 035 §12, trial UI tests): the Agent access row in the
//! Developer section, the settings file, the banner's controls (Pause, Resume, Disconnect,
//! Dismiss) and Undo session with and without the person's steps on top, by element name.

use std::time::Duration;

use ecad_cadcraft::agent_access::{self, Mode, Preference};
use ecad_cadcraft::views::Settings;
use ecad_trial::agent::banner as b;
use ecad_trial::agent::undo_session as us;
use ecad_trial::agent::{self, BannerAction, BannerState};
use ecad_trial::{Trial, developer};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::bridge_contracts::{H, conductors, live_trial, person_wire, server, wire};

fn dir(what: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ecad-035-{what}-{}", ecad_cadcraft::session_seed()))
}

/// A trial with its own settings folder, the Developer section open.
fn trial_with(settings: &std::path::Path) -> H {
    ecad_cadcraft::install();
    let mut t = Trial::new(None).unwrap();
    t.agent.use_settings(Settings::at(settings));
    t.developer.open = true;
    let mut h = Harness::builder().with_size(eframe::egui::vec2(1400.0, 900.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(3);
    h
}

#[test]
fn the_developer_row_shows_off_listening_and_this_run_and_saves_the_choice() {
    let d = dir("row");
    let mut h = trial_with(&d);
    assert!(h.query_by_label(developer::AGENT_MODE).is_some());
    assert!(h.query_by_label(developer::AGENT_PORT).is_some());
    assert_eq!(h.state().agent.status_text(), "Off");
    // Read and change, as the combo does: saved, and the listener starts.
    h.state_mut().agent_set_mode(Mode::Write);
    h.state_mut().agent.port_text = super::bridge_contracts::free_port().to_string();
    h.state_mut().agent_apply_port();
    h.run_steps(3);
    let port = h.state().agent.pref.port;
    assert_eq!(h.state().agent.status_text(), format!("● Listening on 127.0.0.1:{port}"));
    assert!(h.query_by_label(developer::AGENT_STATUS).is_some());
    assert_eq!(agent_access::load(&Settings::at(&d)).preference, Preference { mode: Mode::Write, port });
    // A bad port keeps the old one and warns.
    h.state_mut().agent.port_text = "80".into();
    h.state_mut().agent_apply_port();
    h.run_steps(2);
    assert!(h.state().agent.port_warning);
    assert_eq!(h.state().agent.pref.port, port);
    assert!(h.query_by_label(developer::AGENT_PORT_RANGE).is_some());
    // Off again: saved; the banner and sessions end.
    h.state_mut().agent_set_mode(Mode::Off);
    h.run_steps(2);
    assert_eq!(agent_access::load(&Settings::at(&d)).preference.mode, Mode::Off);
    assert_eq!(h.state().agent.status_text(), "Off");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn control_flag_shows_this_run_only_and_locks_the_mode() {
    let d = dir("flag");
    let mut h = trial_with(&d);
    let port = super::bridge_contracts::free_port();
    h.state_mut().agent.control_flag(port);
    h.run_steps(3);
    assert_eq!(h.state().agent.status_text(), format!("● Listening on 127.0.0.1:{port} (this run only, --control)"));
    h.state_mut().agent_set_mode(Mode::Off);
    assert_eq!(h.state().agent.pref.mode, Mode::Write, "--control is not changed from the row");
    assert!(!Settings::at(&d).dir().unwrap().join(agent_access::FILE_NAME).exists(), "--control writes nothing");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_port_in_use_says_so_and_an_unreadable_file_reads_as_off() {
    let d = dir("busy");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(agent_access::FILE_NAME), "{ not json").unwrap();
    let mut h = trial_with(&d);
    assert_eq!(h.state().agent.pref.mode, Mode::Off);
    assert!(h.state().cad.session.log.iter().any(|l| l == agent_access::UNREADABLE), "said once in the history");
    let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    h.state_mut().agent.control_flag(port);
    h.run_steps(4);
    assert_eq!(h.state().agent.listen(), agent::Listen::InUse(port));
    assert!(h.state().agent.status_text().contains("is in use by another program"));
    drop(taken);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn pause_refuses_agent_writes_until_resumed_and_disconnect_ends_the_session() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "wire", wire(0.0));
    assert!(h.query_by_label(b::ELEMENT).is_some(), "the banner is drawn");
    h.get_by_label(b::PAUSE).click();
    h.run_steps(2);
    assert_eq!(h.state().agent_banner().unwrap().state, BannerState::Paused);
    assert_eq!(a.exec_refused(&mut h, "wire", wire(5.0)), agent::PAUSED);
    assert!(a.ok(&mut h, "ecad_project", json!({}))["pages"].is_object(), "reads continue");
    h.get_by_label(b::PAUSE).click();
    h.run_steps(2);
    assert!(!h.state().agent.paused, "resumed: {:?}", h.state().agent_banner());
    a.exec(&mut h, "wire", wire(5.0));
    h.get_by_label(b::DISCONNECT).click();
    h.run_steps(2);
    assert_eq!(h.state().agent_banner().unwrap().state, BannerState::Finished);
    assert_eq!(a.exec_refused(&mut h, "wire", wire(10.0)), agent::DISCONNECTED);
    h.get_by_label(b::DISMISS).click();
    h.run_steps(2);
    assert!(h.state().agent_banner().is_none());
}

#[test]
fn undo_session_takes_back_the_agents_steps_as_the_persons_undos() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    let base = conductors(&mut h);
    a.exec(&mut h, "wire", wire(0.0));
    a.exec(&mut h, "wire", wire(5.0));
    assert_eq!(h.state().agent_banner().unwrap().steps, 2);
    h.get_by_label(b::UNDO).click();
    h.run_steps(3);
    assert_eq!(conductors(&mut h), base);
    assert_eq!(h.state().agent_banner().unwrap().steps, 0);
    assert!(h.state().cad.session.log.iter().any(|l| l.starts_with("Undid 2 agent steps")), "{:?}", h.state().cad.session.log);
    let log = crate::bridge_contracts::authors(&mut h);
    let n = log.len();
    assert_eq!(log[n - 2..], [ecad_model::Author::User, ecad_model::Author::User], "the person's undos");
}

#[test]
fn undo_session_asks_first_when_the_persons_steps_are_on_top() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    let base = conductors(&mut h);
    a.exec(&mut h, "wire", wire(0.0));
    person_wire(&mut h, 2.0);
    a.exec(&mut h, "wire", wire(5.0));
    // Cancel changes nothing.
    h.state_mut().agent_action(BannerAction::UndoSession);
    h.run_steps(2);
    let c = h.state().agent.undo_confirm.clone().unwrap();
    assert_eq!(c.plan.steps.len(), 3);
    assert_eq!(c.plan.after, 1);
    assert!(c.text().starts_with("1 of your own steps comes after the agent's: #"), "{}", c.text());
    assert!(h.query_by_label(us::CONFIRM).is_some());
    h.get_by_label(us::CANCEL).click();
    h.run_steps(2);
    assert!(h.state().agent.undo_confirm.is_none());
    assert_eq!(conductors(&mut h), base + 3);
    // Only the agent's steps after my last one.
    h.state_mut().agent_action(BannerAction::UndoSession);
    h.run_steps(2);
    h.get_by_label(us::AFTER).click();
    h.run_steps(2);
    assert_eq!(conductors(&mut h), base + 2);
    // Then all of it.
    h.state_mut().agent_action(BannerAction::UndoSession);
    h.run_steps(2);
    h.get_by_label(us::ALL).click();
    h.run_steps(2);
    assert_eq!(conductors(&mut h), base);
    std::thread::sleep(Duration::from_millis(1));
}
