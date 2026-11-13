//! Brief 043 (U4d) headless in the trial: the styled agent banner in the shell's slot (035's data
//! contract and element names kept), the agent pill, History's author chips and session groups
//! with Undo session, and the agent's last write outlined on the page.

use std::time::{Duration, Instant};

use ecad_trial::Trial;
use ecad_trial::agent::banner as b;
use ecad_trial::agent::last_write;
use ecad_trial::agent::{AgentBanner, BannerState, LastChange};
use ecad_trial::history_panel as hp;
use ecad_trial::shell::{Workspace, top_bar};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::bridge_contracts::{H, conductors, live_trial, person_wire, server, wire};

/// A trial with no bridge, 1920 × 1080, its banner set by hand.
fn plain() -> H {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let t = Trial::new(None).unwrap();
    let mut h = Harness::builder().with_size(eframe::egui::vec2(1920.0, 1080.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(3);
    h
}

fn main_height(h: &H) -> f32 {
    h.state().shell.slots.map_or(0.0, |s| s.main.height())
}

fn present(h: &H, name: &str) -> bool {
    h.query_by_label(name).is_some()
}

/// AC1: each state's text and controls, found by 035's element names on the styled strip.
#[test]
fn the_styled_banner_keeps_the_contract_through_every_state() {
    let mut h = plain();
    let now = Instant::now();
    let mut banner = AgentBanner::new("0011223344556677", "claude");
    h.state_mut().agent.banner = Some(banner.clone());
    h.run_steps(2);
    // Connected: the name, the state, Undo session (disabled: nothing yet), Pause writes, Disconnect.
    assert!(present(&h, b::ELEMENT) && present(&h, b::STATE));
    assert!(present(&h, b::UNDO) && present(&h, b::PAUSE) && present(&h, b::DISCONNECT) && !present(&h, b::DISMISS));
    assert!(h.query_by_label("Undo session (0)").is_none(), "the element name replaces the text as its label");
    let state_text = |h: &H| h.state().agent_banner().map(|x| x.state_text(x.shown_state(Instant::now()))).unwrap_or_default();
    assert_eq!(state_text(&h), "changing");
    // Writing: the last write is a go-to.
    banner.last_change = Some(LastChange { seq: 3, label: "Wire W1".into(), time_ms: 0, branch: String::new() });
    banner.steps = 2;
    banner.flash_until = Some(now + Duration::from_secs(30));
    h.state_mut().agent.banner = Some(banner.clone());
    h.run_steps(2);
    assert_eq!(h.state().agent_banner().unwrap().shown_state(Instant::now()), BannerState::Writing);
    assert!(present(&h, b::LAST));
    // Held: waits for the person (the trial clears a hold it does not have, so the strip is drawn
    // on its own here).
    let mut held = banner.clone();
    held.held = Some("the designer is in the middle of WIRE".into());
    assert_eq!(held.state_text(held.shown_state(Instant::now())), "waiting for you to finish WIRE");
    let mut strip = Harness::new_ui(move |ui| {
        let _ = b::show(ui, &held, false, Some(0));
    });
    strip.run();
    assert!(strip.query_by_label(b::PAUSE).is_some() && strip.query_by_label(b::UNDO).is_some() && strip.query_by_label(b::STATE).is_some());
    assert_eq!(b::state_tone(BannerState::Held), ecad_ui::prim::Tone::Warning);
    // Paused: Resume in the same place.
    banner.held = None;
    banner.state = BannerState::Paused;
    h.state_mut().agent.banner = Some(banner.clone());
    h.run_steps(2);
    assert_eq!(state_text(&h), "paused \u{2014} agent changes are refused");
    assert!(present(&h, b::PAUSE));
    // Finished: Undo session and Dismiss only; no last write.
    banner.state = BannerState::Finished;
    h.state_mut().agent.banner = Some(banner);
    h.run_steps(2);
    assert_eq!(state_text(&h), "finished \u{b7} 2 steps");
    assert!(present(&h, b::UNDO) && present(&h, b::DISMISS));
    assert!(!present(&h, b::PAUSE) && !present(&h, b::DISCONNECT) && !present(&h, b::LAST));
}

/// AC2 and AC6: the banner takes the 32 px slot on hello, Pause refuses the next write, the
/// last write's go-to opens History and keeps the page's view, the pill hides and shows the
/// banner, and Undo session takes back exactly the session's steps; Dismiss gives the 32 px back.
#[test]
fn the_banner_takes_its_slot_and_its_controls_work() {
    let (mut h, port) = live_trial();
    let before = main_height(&h);
    let base = conductors(&mut h);
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "wire", wire(0.0));
    a.exec(&mut h, "wire", wire(5.0));
    h.run_steps(2);
    assert_eq!(main_height(&h), before - ecad_ui::tokens::size::AGENT_BANNER, "the main view is 32 px shorter");
    assert!(!h.state().developer.open, "the hello no longer opens the Developer section");
    // The pill names the agent and hides the banner; a second click shows it again.
    let pill = format!("{}: Agent tester", top_bar::AGENT_PILL);
    h.get_by_label(&pill).click();
    h.run_steps(2);
    assert_eq!(main_height(&h), before, "hidden");
    h.run_steps(60);
    h.get_by_label(&pill).click();
    h.run_steps(2);
    assert_eq!(main_height(&h), before - ecad_ui::tokens::size::AGENT_BANNER, "shown again");
    // The go-to opens History at the last write; the page keeps its view.
    let view = h.state().cad.canvas.xf.map(|x| (x.center, x.scale));
    h.get_by_label(b::LAST).click();
    h.run_steps(3);
    assert_eq!(h.state().panel.tab, ecad_trial::project_panel::Tab::History);
    assert_eq!(h.state().cad.canvas.xf.map(|x| (x.center, x.scale)), view, "the stored page view does not overwrite it");
    // Pause refuses the next agent write with 035's message.
    h.get_by_label(b::PAUSE).click();
    h.run_steps(2);
    assert_eq!(a.exec_refused(&mut h, "wire", wire(10.0)), ecad_trial::agent::PAUSED);
    h.run_steps(60);
    h.get_by_label(b::PAUSE).click();
    h.run_steps(2);
    // Undo session: exactly the session's two steps.
    h.get_by_label(b::UNDO).click();
    h.run_steps(3);
    assert_eq!(conductors(&mut h), base);
    a.bye(&mut h);
    h.run_steps(2);
    h.get_by_label(b::DISMISS).click();
    h.run_steps(2);
    assert!(h.state().agent_banner().is_none());
    assert_eq!(main_height(&h), before, "the 32 px are given back");
}

/// AC3: 3 person edits, 14 agent writes, 2 person edits: one group with its count and span,
/// expanding shows 14 rows with the agent's chip, the person's rows say "You", and Undo session
/// is offered on the group only while the session is live.
#[test]
fn history_groups_an_agent_session_and_names_every_author() {
    let (mut h, port) = live_trial();
    for i in 0..3 {
        person_wire(&mut h, 100.0 + f64::from(i) * 5.0);
    }
    let mut a = server(port, true, "tester");
    for i in 0..14 {
        a.exec(&mut h, "wire", wire(f64::from(i) * 5.0));
    }
    for i in 0..2 {
        person_wire(&mut h, 200.0 + f64::from(i) * 5.0);
    }
    let head = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
    };
    h.state_mut().open_history_at(head);
    h.run_steps(4);
    let groups = h.state().panel.history.groups().to_vec();
    let sessions: Vec<_> = groups.iter().filter(|g| g.is_session()).collect();
    assert_eq!(sessions.len(), 1, "{groups:?}");
    assert_eq!(sessions[0].seqs.len(), 14);
    assert!(h.query_by_label_contains(&format!("{}: tester \u{b7} 14 changes \u{b7} ", hp::SESSION_GROUP)).is_some(), "the group's row");
    let you = format!("{}: {}", hp::AUTHOR, hp::YOU);
    let agent = format!("{}: tester", hp::AUTHOR);
    assert!(h.query_all_by_label(&you).count() >= 5, "the person's rows say You");
    assert_eq!(h.query_all_by_label(&agent).count(), 1, "collapsed: the group's chip only");
    assert!(present(&h, hp::UNDO_SESSION), "live: Undo session offered");
    h.get_by_label_contains(hp::SESSION_GROUP).click();
    h.run_steps(2);
    assert_eq!(h.query_all_by_label(&agent).count(), 15, "expanded: 14 rows and the group's chip");
    a.bye(&mut h);
    h.run_steps(3);
    assert!(!present(&h, hp::UNDO_SESSION), "the session ended: its steps undo one by one");
}

/// AC4 and AC5: the agent's last write is outlined with its chip, stays after the person's edit,
/// is not an entity of the page, is gone over Lists; idle frames read nothing for it.
#[test]
fn the_last_write_is_outlined_on_the_page_and_costs_nothing_idle() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "wire", wire(0.0));
    h.run_steps(3);
    let ctx = h.ctx.clone();
    let outlined = last_write::outlines(&ctx);
    assert!(outlined.iter().any(|o| o.kind == last_write::Kind::AgentWrite && o.chip == "tester" && !o.boxes.is_empty()), "{outlined:?}");
    assert!(present(&h, &format!("{}: tester", last_write::CHIP)));
    // Never an entity: the page's drawing has the wire's entities only.
    let entities = |h: &mut H| {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        let page = t.panel.active.unwrap();
        p.read(&mut t.cad.session, |p, pages| pages.entities_of(p, page).len()).unwrap()
    };
    let n = entities(&mut h);
    // 300 idle frames: no project read for the outline.
    let reads = last_write::reads(&ctx);
    h.run_steps(300);
    assert_eq!(last_write::reads(&ctx), reads, "idle frames read nothing");
    assert_eq!(entities(&mut h), n);
    // The person's next edit: the outline stays (read again for the changed drawing).
    person_wire(&mut h, 50.0);
    h.run_steps(2);
    assert!(last_write::outlines(&ctx).iter().any(|o| o.kind == last_write::Kind::AgentWrite));
    // Over Lists the page is hidden: no outline drawn.
    h.state_mut().set_workspace(Workspace::Lists);
    h.run_steps(2);
    assert!(!present(&h, &format!("{}: tester", last_write::CHIP)));
}

/// Screenshots for the owner's check (written only when `ECAD_TESTER_SHOTS` names a folder and a
/// wgpu adapter exists; nothing is asserted about the pictures): the banner with the last write
/// outlined, History's session group open, and the finished banner.
#[test]
fn screenshots_of_the_banner_history_and_outline() {
    let Some(out) = crate::project_panel::shots() else { return };
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    let port = crate::bridge_contracts::free_port();
    t.agent.control_flag(port);
    let mut h =
        Harness::builder().with_size(eframe::egui::vec2(1920.0, 1080.0)).renderer(crate::mouse_tester::Collect(deltas.clone())).build_ui_state(
            |ui, t: &mut Trial| {
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            },
            t,
        );
    h.run_steps(3);
    person_wire(&mut h, 40.0);
    let mut a = server(port, true, "claude-code");
    for i in 0..3 {
        a.exec(&mut h, "wire", wire(f64::from(i) * 10.0));
    }
    h.state_mut().zoom_to_drawing();
    h.run_steps(5);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("043-banner.png"));
    let head = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
    };
    h.state_mut().open_history_at(head);
    h.run_steps(4);
    h.get_by_label_contains(hp::SESSION_GROUP).click();
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("043-history.png"));
    h.get_by_label(b::PAUSE).click();
    h.run_steps(3);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("043-paused.png"));
    a.bye(&mut h);
    h.run_steps(3);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("043-finished.png"));
}

/// Review of PR #77: two bridge sessions under one name, back to back, are two groups; Undo
/// session is offered on the live one only.
#[test]
fn back_to_back_sessions_of_one_agent_are_two_groups() {
    let (mut h, port) = live_trial();
    let mut first = server(port, true, "tester");
    first.exec(&mut h, "wire", wire(0.0));
    first.exec(&mut h, "wire", wire(5.0));
    first.bye(&mut h);
    h.run_steps(2);
    let mut second = server(port, true, "tester");
    for i in 0..3 {
        second.exec(&mut h, "wire", wire(20.0 + f64::from(i) * 5.0));
    }
    let head = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
    };
    h.state_mut().open_history_at(head);
    h.run_steps(4);
    let panel = &h.state().panel.history;
    let sessions: Vec<(usize, bool)> = panel.groups().iter().filter(|g| g.is_session()).map(|g| (g.seqs.len(), panel.group_is_live(g))).collect();
    assert_eq!(sessions, [(3, true), (2, false)], "{:?}", panel.groups());
    assert_eq!(h.query_all_by_label(hp::UNDO_SESSION).count(), 1, "Undo session on the live session only");
}
