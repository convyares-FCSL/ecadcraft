//! Brief 035 (design 035 §12): MCP contract tests over the live bridge. Each test builds a trial
//! headless (egui_kittest) with agent access on at a free port, runs an `ecad-mcp --connect`
//! server on a second thread, and pumps the trial's frames until each reply arrives (10 s guard).
//!
//! They live in the trial's test executable (not `ecad-mcp`'s) because they need a `Trial`;
//! `ecad-mcp` stays below the UI layer.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use ecad_mcp::bridge::Mode;
use ecad_mcp::{EcadMcp, Options};
use ecad_model::Author;
use ecad_trial::Trial;
use ecad_trial::agent::{self, BannerState};
use egui_kittest::Harness;
use serde_json::{Value, json};

pub(crate) type H = Harness<'static, Trial>;

/// A loopback port nothing listens on now.
pub(crate) fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

/// A trial with agent access on (Read and change) at a free port, its listener started.
pub(crate) fn live_trial() -> (H, u16) {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    let port = free_port();
    t.agent.control_flag(port);
    t.agent.hold = Duration::from_millis(400);
    let mut h = Harness::builder().with_size(eframe::egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(3);
    assert!(matches!(h.state().agent.listen(), agent::Listen::Listening(p) if p == port), "{:?}", h.state().agent.listen());
    (h, port)
}

/// An `ecad-mcp --connect` server on its own thread (it is not `Send`).
pub(crate) struct Server {
    tx: Sender<Value>,
    rx: Receiver<Value>,
    next: u64,
}

pub(crate) fn server(port: u16, write: bool, name: &str) -> Server {
    let (tx, req_rx) = channel::<Value>();
    let (rep_tx, rx) = channel::<Value>();
    let name = name.to_string();
    std::thread::spawn(move || {
        let opts = Options { write, agent: Some(name) };
        let mut s = EcadMcp::connect(port, false, ecad_mcp::opener(None, false, write), opts).unwrap();
        for msg in req_rx {
            if msg == json!("mode") {
                let m = s.mode().map_or("none", Mode::word);
                let _ = rep_tx.send(json!(m));
                continue;
            }
            if msg == json!("bye") {
                s.bye();
                let _ = rep_tx.send(Value::Null);
                continue;
            }
            let r = s.handle_line(&msg.to_string()).map_or(Value::Null, |r| serde_json::from_str(&r).unwrap());
            if rep_tx.send(r).is_err() {
                break;
            }
        }
    });
    let srv = Server { tx, rx, next: 1 };
    let init = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "x", "version": "1"}}});
    srv.tx.send(init).unwrap();
    srv.rx.recv_timeout(Duration::from_secs(10)).unwrap();
    srv
}

impl Server {
    /// Send `msg` and pump the trial's frames until the reply comes.
    pub(crate) fn send(&mut self, h: &mut H, msg: Value) -> Value {
        self.tx.send(msg).unwrap();
        let start = Instant::now();
        loop {
            if let Ok(v) = self.rx.try_recv() {
                return v;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "no reply within 10 s");
            h.run_steps(1);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub(crate) fn tool(&mut self, h: &mut H, name: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let r = self.send(h, json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}}));
        assert!(r.get("error").is_none(), "{name} {r}");
        r["result"].clone()
    }

    /// A tool's JSON when it worked.
    pub(crate) fn ok(&mut self, h: &mut H, name: &str, args: Value) -> Value {
        let r = self.tool(h, name, args.clone());
        assert_ne!(r["isError"], json!(true), "{name} {args}: {r}");
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    /// A tool's refusal.
    pub(crate) fn refused(&mut self, h: &mut H, name: &str, args: Value) -> String {
        let r = self.tool(h, name, args.clone());
        assert_eq!(r["isError"], json!(true), "{name} {args} should be refused: {r}");
        r["content"][0]["text"].as_str().unwrap().to_string()
    }

    pub(crate) fn exec(&mut self, h: &mut H, command: &str, params: Value) -> Value {
        self.ok(h, "execute", json!({"command": command, "params": params}))
    }

    pub(crate) fn exec_refused(&mut self, h: &mut H, command: &str, params: Value) -> String {
        self.refused(h, "execute", json!({"command": command, "params": params}))
    }

    pub(crate) fn mode(&mut self) -> String {
        self.tx.send(json!("mode")).unwrap();
        self.rx.recv_timeout(Duration::from_secs(10)).unwrap().as_str().unwrap().to_string()
    }

    pub(crate) fn bye(&mut self, h: &mut H) {
        self.send(h, json!("bye"));
    }
}

pub(crate) fn wire(y: f64) -> Value {
    json!({"points": [[0.0, y], [60.0, y]]})
}

/// The project's log as (label, author) pairs.
pub(crate) fn authors(h: &mut H) -> Vec<Author> {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().iter().map(|t| t.author.clone()).collect()).unwrap()
}

pub(crate) fn log_len(h: &mut H) -> usize {
    authors(h).len()
}

pub(crate) fn conductors(h: &mut H) -> usize {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().conductors.len()).unwrap()
}

/// A person's edit, through the ECAD command field.
pub(crate) fn person_wire(h: &mut H, y: f64) {
    let line = format!("wire {}", wire(y));
    let failed = h.state_mut().submit_command(&line);
    assert!(failed.is_none(), "{failed:?}");
    h.run_steps(1);
}

fn agent(name: &str) -> Author {
    Author::Agent(name.into())
}

// ---------- 1, 2: one transaction per call, by the agent; the person's edits are the person's ----------

#[test]
fn agent_wires_are_the_agents_and_a_persons_edit_between_them_is_the_persons() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    let before = log_len(&mut h);
    let r = a.exec(&mut h, "wire", wire(0.0));
    assert_eq!(r["ecadTransactions"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["ecadTransactions"][0]["author"], json!("agent tester"));
    assert_eq!(a.mode(), "bridged");
    person_wire(&mut h, 5.0);
    a.exec(&mut h, "wire", wire(10.0));
    let log = authors(&mut h);
    assert_eq!(log[before..], [agent("tester"), Author::User, agent("tester")]);
    assert_eq!(conductors(&mut h), 3);
    // The banner shows the last one; the command history names it.
    let b = h.state().agent_banner().unwrap().clone();
    assert_eq!(b.agent, "tester");
    assert_eq!(b.last_change.as_ref().unwrap().seq as usize, log.len());
    assert!(h.state().cad.session.log.iter().any(|l| l.starts_with("Agent tester: WIRE  #")), "{:?}", h.state().cad.session.log);
    assert_eq!(h.state().agent.frame_fixes, 0);
}

#[test]
fn refusals_read_the_same_over_the_bridge_as_headless() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    let mut local = EcadMcp::in_memory("same", 1, Options { write: true, agent: Some("tester".into()) }).unwrap();
    let headless = |s: &mut EcadMcp, cmd: &str, params: Value| -> String {
        let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": cmd, "params": params}}});
        let r: Value = serde_json::from_str(&s.handle_line(&line.to_string()).unwrap()).unwrap();
        r["result"]["content"][0]["text"].as_str().unwrap().to_string()
    };
    for (cmd, params) in [
        ("saveas", json!({})),
        ("line", json!({"path": "x.dxf", "points": [[0, 0], [1, 1]]})),
        ("undo", json!({"count": 3})),
        ("agentaccess", json!({"mode": "write"})),
        ("setvar", json!({"name": "auditctl", "value": 1})),
    ] {
        let bridged = a.exec_refused(&mut h, cmd, params.clone());
        assert_eq!(bridged, headless(&mut local, cmd, params), "{cmd}");
    }
}

// ---------- 3: the person is the author again after every agent call ----------

#[test]
fn the_person_is_the_author_after_a_refused_a_failing_and_a_panicking_agent_call() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "wire", wire(0.0));
    // Refused by policy.
    a.exec_refused(&mut h, "saveas", json!({}));
    person_wire(&mut h, 1.0);
    // Failing in the engine.
    a.exec_refused(&mut h, "wire", json!({"points": "no"}));
    person_wire(&mut h, 2.0);
    // Panicking inside the ECAD command (fault injection): caught, and the person is the author.
    {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.configure(&mut t.cad.session, |p| p.faults.panic_in_command = true).unwrap();
    }
    let e = a.exec_refused(&mut h, "wire", wire(3.0));
    assert!(!e.is_empty());
    person_wire(&mut h, 4.0);
    let log = authors(&mut h);
    let n = log.len();
    assert_eq!(log[n - 3..], [Author::User, Author::User, Author::User], "{log:?}");
    assert_eq!(h.state().agent.frame_fixes, 0, "the author never had to be reset between frames");
}

// ---------- 4: the cap per call, a batch is one call ----------

#[test]
fn a_batch_of_two_executes_on_one_line_records_at_most_one() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "ecadinspect", json!({}));
    let before = conductors(&mut h);
    let call = |id: u64, y: f64| json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": wire(y)}}});
    let r = a.send(&mut h, json!([call(10, 0.0), call(11, 5.0)]));
    let replies = r.as_array().unwrap();
    assert_eq!(replies.len(), 2);
    assert_ne!(replies[0]["result"]["isError"], json!(true), "{r}");
    assert_eq!(replies[1]["result"]["isError"], json!(true), "{r}");
    assert!(replies[1]["result"]["content"][0]["text"].as_str().unwrap().contains("at most"), "{r}");
    assert_eq!(conductors(&mut h), before + 1);
}

// ---------- 5: writes off ----------

#[test]
fn read_only_access_or_a_server_without_write_refuses_every_change_and_save() {
    let (mut h, port) = live_trial();
    h.state_mut().agent.pref.mode = ecad_cadcraft::agent_access::Mode::Read;
    let mut a = server(port, true, "tester");
    let before = log_len(&mut h);
    assert_eq!(a.exec_refused(&mut h, "wire", wire(0.0)), agent::READ_ONLY);
    assert!(a.refused(&mut h, "save", json!({})).contains("Read only"));
    assert!(a.ok(&mut h, "ecad_project", json!({}))["pages"]["count"].as_u64().unwrap() >= 1);
    assert_eq!(log_len(&mut h), before);

    h.state_mut().agent.pref.mode = ecad_cadcraft::agent_access::Mode::Write;
    let mut b = server(port, false, "reader");
    assert_eq!(b.exec_refused(&mut h, "wire", wire(0.0)), ecad_mcp::policy::WRITES_OFF);
    assert!(b.refused(&mut h, "save", json!({})).contains("read-only"));
    assert!(b.ok(&mut h, "ecad_history", json!({}))["count"].as_u64().is_some());
    assert_eq!(log_len(&mut h), before);
}

// ---------- 6: the busy rule ----------

#[test]
fn a_write_waits_while_the_person_is_in_the_middle_of_wire_then_is_refused_unrun() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "ecadinspect", json!({}));
    h.state_mut().cad.session.cmdline("wire").unwrap();
    h.run_steps(1);
    assert!(h.state().cad.session.running.is_some());
    let before = log_len(&mut h);
    // Reads are answered at once.
    let start = Instant::now();
    a.ok(&mut h, "ecad_project", json!({}));
    assert!(start.elapsed() < Duration::from_millis(350), "{:?}", start.elapsed());
    let start = Instant::now();
    let e = a.exec_refused(&mut h, "wire", wire(0.0));
    assert!(start.elapsed() >= h.state().agent.hold, "held first");
    assert_eq!(e, agent::held_refusal("the designer is in the middle of WIRE"));
    assert_eq!(log_len(&mut h), before);
    h.state_mut().cad.session.cancel();
    a.exec(&mut h, "wire", wire(0.0));
}

// ---------- 7: raw methods ----------

/// Send raw JSON lines with (or without) the token on one connection while the trial runs.
fn raw(h: &mut H, port: u16, lines: Vec<Value>) -> Vec<Value> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut r = BufReader::new(c.try_clone().unwrap());
        let mut out = Vec::new();
        for l in lines {
            if writeln!(c, "{l}").is_err() {
                break;
            }
            let mut s = String::new();
            if r.read_line(&mut s).unwrap_or(0) == 0 {
                break;
            }
            out.push(serde_json::from_str::<Value>(&s).unwrap());
        }
        tx.send(out).unwrap();
    });
    let start = Instant::now();
    loop {
        if let Ok(v) = rx.try_recv() {
            return v;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn raw_methods_are_refused_and_change_nothing_and_a_request_without_the_token_is_unauthorized() {
    let (mut h, port) = live_trial();
    let token = cadcraft_mcp::control_token::read(port).unwrap();
    let before = log_len(&mut h);
    let reqs: Vec<Value> = [
        ("cmdline.input", json!({"text": "line 0,0 1,1"})),
        ("cmdline.script", json!({"text": "line 0,0 1,1\n"})),
        ("app.open", json!({"path": "x.ecad"})),
        ("app.save", json!({})),
        ("ui.click", json!({"x": 1, "y": 1})),
        ("ui.key", json!({"key": "Enter"})),
        ("engine.execute", json!({"command": "agentaccess", "params": {"mode": "write"}})),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (m, p))| json!({"id": i, "method": m, "params": p, "token": token}))
    .collect();
    for r in raw(&mut h, port, reqs) {
        assert_eq!(r["ok"], json!(false), "{r}");
        assert_eq!(r["state"], json!("not-run"), "{r}");
        assert!(r["error"].as_str().unwrap().starts_with("not offered over ECAD's port"), "{r}");
    }
    assert_eq!(log_len(&mut h), before);
    assert_eq!(h.state().agent.pref.mode, ecad_cadcraft::agent_access::Mode::Write);
    let r = raw(&mut h, port, vec![json!({"id": 1, "method": "ping"})]);
    assert!(r[0]["error"].as_str().unwrap().starts_with("unauthorized"), "{r:?}");
}

// ---------- 8: an agent undoes only its own steps ----------

#[test]
fn an_agent_undo_of_the_persons_step_is_refused_and_of_its_own_works() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "wire", wire(0.0));
    person_wire(&mut h, 5.0);
    let e = a.exec_refused(&mut h, "u", json!({}));
    assert!(e.contains("is the designer's; an agent undoes only its own steps"), "{e}");
    assert_eq!(conductors(&mut h), 2);
    a.exec(&mut h, "wire", wire(10.0));
    let r = a.exec(&mut h, "u", json!({}));
    assert_eq!(r["ecadTransactions"][0]["author"], json!("agent tester"));
    assert_eq!(conductors(&mut h), 2);
    let r = a.exec(&mut h, "redo", json!({}));
    assert_eq!(r["ecadTransactions"].as_array().unwrap().len(), 1);
    assert_eq!(conductors(&mut h), 3);
}

// ---------- 9: one writer ----------

#[test]
fn a_second_writer_reads_until_the_first_says_bye() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "first");
    a.exec(&mut h, "wire", wire(0.0));
    let mut b = server(port, true, "second");
    let e = b.exec_refused(&mut h, "wire", wire(5.0));
    assert_eq!(e, agent::not_writer("first"));
    assert!(b.ok(&mut h, "ecad_project", json!({}))["components"]["count"].is_number());
    a.bye(&mut h);
    b.exec(&mut h, "wire", wire(5.0));
    assert_eq!(conductors(&mut h), 2);
}

// ---------- 10: modes ----------

#[test]
fn headless_until_a_window_answers_then_bridged_and_headless_sticks_after_a_change() {
    let port = free_port();
    let mut a = server(port, true, "tester");
    let (tx, rx) = channel::<Value>();
    // No window: headless (no trial frames needed).
    let call = |s: &mut Server, msg: Value| -> Value {
        s.tx.send(msg).unwrap();
        s.rx.recv_timeout(Duration::from_secs(10)).unwrap()
    };
    let conn = |id: u64| json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "ecad_connection", "arguments": {}}});
    let r = call(&mut a, conn(1));
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("\"headless\""), "{r}");
    drop((tx, rx));
    // The window opens before anything was recorded headless: the next call is bridged.
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    t.agent.control_flag(port);
    let mut h: H = Harness::builder().build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(3);
    a.exec(&mut h, "wire", wire(0.0));
    assert_eq!(a.mode(), "bridged");
    // A fresh server that records headless first stays headless when a window answers later.
    let port2 = free_port();
    let mut b = server(port2, true, "tester");
    let r = call(
        &mut b,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": wire(0.0)}}}),
    );
    assert_ne!(r["result"]["isError"], json!(true), "{r}");
    assert_eq!(b.mode(), "headless");
    h.state_mut().agent = {
        let mut ag = agent::Agent::from_settings(ecad_cadcraft::views::Settings::none());
        ag.control_flag(port2);
        ag
    };
    h.run_steps(3);
    let r = call(&mut b, conn(3));
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("\"headless\""), "{r}");
}

// ---------- 11: off by default ----------

#[test]
fn with_no_preference_and_no_control_flag_nothing_listens() {
    ecad_cadcraft::install();
    let dir = std::env::temp_dir().join(format!("ecad-035-off-{}", std::process::id()));
    let mut t = Trial::new(None).unwrap();
    t.agent.use_settings(ecad_cadcraft::views::Settings::at(&dir));
    let mut h: H = Harness::builder().build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(3);
    assert_eq!(h.state().agent.listen(), agent::Listen::Off);
    assert_eq!(h.state().agent.status_text(), "Off");
    assert!(h.state().agent_banner().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- the banner's states after scripted calls ----------

#[test]
fn the_banner_follows_hello_writes_refusals_and_bye() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "wire", wire(0.0));
    let b = h.state().agent_banner().unwrap().clone();
    assert_eq!(b.steps, 1);
    assert!(matches!(b.shown_state(Instant::now()), BannerState::Writing | BannerState::Connected));
    a.exec_refused(&mut h, "saveas", json!({}));
    assert!(h.state().agent_banner().unwrap().last_refusal.as_deref().unwrap().contains("SAVEAS"));
    a.exec(&mut h, "wire", wire(5.0));
    assert!(h.state().agent_banner().unwrap().last_refusal.is_none(), "cleared by the next accepted call");
    // A person's edit changes nothing in the banner.
    let before = h.state().agent_banner().unwrap().clone();
    person_wire(&mut h, 9.0);
    h.run_steps(2);
    assert_eq!(h.state().agent_banner().unwrap().steps, before.steps);
    assert_eq!(h.state().agent_banner().unwrap().last_change, before.last_change);
    a.bye(&mut h);
    h.run_steps(2);
    assert_eq!(h.state().agent_banner().unwrap().state, BannerState::Finished);
}

// ---------- the frame check resets a stray author and keeps access on (M1 risk 1) ----------

#[test]
fn an_agent_author_found_between_frames_is_reset_and_access_stays_on() {
    let (mut h, port) = live_trial();
    let mut a = server(port, true, "tester");
    a.exec(&mut h, "wire", wire(0.0));
    {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.configure(&mut t.cad.session, |p| p.author = Author::Agent("stray".into())).unwrap();
    }
    h.run_steps(1);
    assert_eq!(h.state().agent.frame_fixes, 1);
    assert!(!h.state().agent.author_pending);
    assert!(h.state().agent.on(), "access is not turned off");
    person_wire(&mut h, 5.0);
    assert_eq!(authors(&mut h).last(), Some(&Author::User));
    a.exec(&mut h, "wire", wire(10.0));
}
