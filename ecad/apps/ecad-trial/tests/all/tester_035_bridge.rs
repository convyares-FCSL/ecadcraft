//! Tester 035: the live MCP bridge, from outside (brief 035 AC 1–11, 13; design 035 §3–§8, §11,
//! §12 checks 1–11 and the trial UI checks). Written from the brief and the design only, before
//! the implementation.
//!
//! Harness (design §12): a headless `Trial` (egui_kittest) with agent access turned on at a free
//! port through the `agentaccess` command, as the ECAD Dev row does; an `ecad-mcp` server in
//! `--connect` mode on its own thread, served over a pipe exactly as over stdio (closing the pipe
//! is stdin closing); the test thread pumps the trial's frames until each reply arrives (10 s
//! guard, longer only for the 5 s hold). Raw control lines go over a real socket with the token
//! read from upstream's token file.
//!
//! Every test here takes [`LOCK`]: the settings folder (`agent-access.json`) is one per test
//! process, and agent access must be Off again before another test builds a trial.
//!
//! Names of the new API are guesses, gathered under "GUESS" so phase 2 adapts them in one place.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ecad_model::{Author, Transaction, TxKind};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use ecad_trial::Trial;

use crate::mouse_tester::harness_with;

type H = Harness<'static, Trial>;

const AGENT: &str = "claude-code";
const GUARD: Duration = Duration::from_secs(10);

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn free_port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    l.local_addr().unwrap().port()
}

fn listening(port: u16) -> bool {
    TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)).is_ok()
}

// ============================================================================================
// GUESS: the trial's side of the new API (design §3, §5.2, §7). Phase 2 adapts these only.
// ============================================================================================

/// The `agentaccess` command (design §3.3): reads with `{}`, sets with `{mode?, port?}`; replies
/// `{mode, port, listening, status}`. Run as the person does, through CadApp's command path.
fn agentaccess(h: &mut H, params: Value) -> Value {
    let r = h.state_mut().cad.run("agentaccess", params.clone()).unwrap_or_else(|e| panic!("agentaccess {params}: {e}"));
    h.run_steps(3);
    r
}

/// The frame-invariant diagnostic counter (§5.2: "a counter the tests read").
fn author_resets(h: &H) -> u64 {
    h.state().agent.author_resets
}

/// The banner's data (§7.1), when one is shown.
fn banner(h: &H) -> Option<ecad_trial::agent::AgentBanner> {
    h.state().agent.banner.clone()
}

/// The settings file (§3.1).
fn access_file() -> std::path::PathBuf {
    ecad_cadcraft::views::Settings::from_env().dir().unwrap().join("agent-access.json")
}

// ============================================================================================

/// A trial with agent access set to `mode` at a fresh port. Dropping it turns access Off, removes
/// the settings file and closes the trial (token revoked).
struct Live {
    h: H,
    port: u16,
}

impl Live {
    fn new(mode: &str) -> Self {
        let port = free_port();
        let mut h = harness_with(None);
        let _ = std::fs::remove_file(access_file());
        let r = agentaccess(&mut h, json!({"mode": mode, "port": port}));
        assert_eq!(r["mode"], json!(mode), "{r}");
        assert_eq!(r["port"], json!(port), "{r}");
        if mode != "off" {
            assert_eq!(r["listening"], json!(true), "{r}");
            assert!(listening(port), "{mode}: the trial listens on {port}");
        }
        Live { h, port }
    }

    fn pump(&mut self) {
        self.h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }

    fn log(&mut self) -> Vec<Transaction> {
        let t = self.h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.ledger.full_log().unwrap()).unwrap()
    }

    fn conductors(&mut self) -> usize {
        crate::mouse_tester::doc(&mut self.h).conductors.len()
    }

    /// The project's author and transaction budget now (the frame invariant, §5.2).
    fn author(&mut self) -> (Author, Option<usize>) {
        let t = self.h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| (p.author.clone(), p.tx_budget)).unwrap()
    }

    /// A person's edit, through the trial's command path (the ECAD command field's path).
    fn person_wire(&mut self, y: f64) -> Transaction {
        let n = self.log().len();
        let e = self.h.state_mut().submit_command(&format!("wire {}", json!({"points": [[0.0, y], [60.0, y]]})));
        assert!(e.is_none(), "{e:?}");
        self.h.run_steps(2);
        let log = self.log();
        assert_eq!(log.len(), n + 1, "the person's wire is one transaction");
        log.last().cloned().unwrap()
    }

    fn echoes(&self) -> Vec<String> {
        self.h.state().cad.session.log.clone()
    }

    /// The text of the element named `name` (its value, else its label), if shown.
    fn element_text(&self, name: &str) -> Option<String> {
        let n = self.h.query_by_label(name)?;
        Some(n.value().unwrap_or_else(|| n.label().unwrap_or_default()))
    }

    fn click(&mut self, name: &str) {
        self.h.get_by_label(name).click();
        self.h.run_steps(3);
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.h.state_mut().cad.run("agentaccess", json!({"mode": "off"}));
        self.h.run_steps(1);
        self.h.state_mut().on_exit();
        let _ = std::fs::remove_file(access_file());
    }
}

// ---------- the agent: an `ecad-mcp --connect` server on its own thread ----------

/// stdin of the server: lines sent over a channel; dropping the sender is end of input.
struct Pipe {
    rx: Receiver<Vec<u8>>,
    buf: Vec<u8>,
    at: usize,
}

impl Read for Pipe {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.at >= self.buf.len() {
            match self.rx.recv() {
                Ok(b) => {
                    self.buf = b;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.at);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// stdout of the server: every line goes back over a channel.
struct Lines {
    tx: Sender<String>,
    buf: Vec<u8>,
}

impl Write for Lines {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(b);
        while let Some(i) = self.buf.iter().position(|c| *c == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=i).collect();
            let _ = self.tx.send(String::from_utf8_lossy(&line).trim().to_string());
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Agent {
    to: Option<Sender<Vec<u8>>>,
    from: Receiver<String>,
    next: u64,
    done: Option<std::thread::JoinHandle<()>>,
}

impl Agent {
    /// `ecad-mcp --connect <port> [--write]`, initialized as client `name`.
    fn start(live: &mut Live, name: &str, write: bool) -> Self {
        Self::start_at(live.port, name, write, Some(live))
    }

    /// The same, at `port`, pumping `live` (if any) while waiting.
    fn start_at(port: u16, name: &str, write: bool, live: Option<&mut Live>) -> Self {
        let (to, rx) = channel::<Vec<u8>>();
        let (tx, from) = channel::<String>();
        let done = std::thread::spawn(move || {
            let p = port.to_string();
            let mut args = vec!["--connect".to_string(), p];
            if write {
                args.push("--write".into());
            }
            let Ok(ecad_mcp::args::Invocation::Serve { options, .. }) = ecad_mcp::args::parse_args(args.iter().map(Into::into)) else {
                return;
            };
            // GUESS: the `--connect` constructor (as in tester_035_mcp::connect_server).
            let Ok(mut s) = ecad_mcp::EcadMcp::connect(options) else { return };
            let _ = s.serve(BufReader::new(Pipe { rx, buf: Vec::new(), at: 0 }), Lines { tx, buf: Vec::new() });
        });
        let mut a = Agent { to: Some(to), from, next: 1, done: Some(done) };
        let init =
            a.request(live, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": name, "version": "1"}}));
        assert!(init.get("result").is_some(), "{init}");
        a.send_line(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string());
        a
    }

    fn send_line(&mut self, line: &str) {
        let mut b = line.as_bytes().to_vec();
        b.push(b'\n');
        self.to.as_ref().unwrap().send(b).unwrap();
    }

    /// Wait for the next reply, pumping the trial's frames meanwhile.
    fn wait(&mut self, mut live: Option<&mut Live>, guard: Duration) -> Value {
        let t0 = Instant::now();
        loop {
            if let Ok(line) = self.from.try_recv() {
                return serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {line:?} ({e})"));
            }
            assert!(t0.elapsed() < guard, "no reply within {guard:?}");
            match live.as_deref_mut() {
                Some(l) => l.pump(),
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    fn request(&mut self, live: Option<&mut Live>, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send_line(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string());
        let v = self.wait(live, GUARD);
        assert_eq!(v["id"], json!(id), "{v}");
        v
    }

    fn call(&mut self, live: &mut Live, tool: &str, args: Value) -> Value {
        let r = self.request(Some(live), "tools/call", json!({"name": tool, "arguments": args}));
        r.get("result").cloned().unwrap_or_else(|| json!({"isError": true, "content": [{"type": "text", "text": r["error"]["message"]}]}))
    }

    fn ok(&mut self, live: &mut Live, tool: &str, args: Value) -> Value {
        let r = self.call(live, tool, args.clone());
        assert_ne!(r["isError"], json!(true), "{tool} {args}: {r}");
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap_or(Value::Null)
    }

    fn refused(&mut self, live: &mut Live, tool: &str, args: Value) -> String {
        let r = self.call(live, tool, args.clone());
        assert_eq!(r["isError"], json!(true), "{tool} {args} should be refused: {r}");
        r["content"][0]["text"].as_str().unwrap_or_default().to_string()
    }

    fn exec(&mut self, live: &mut Live, command: &str, params: Value) -> Value {
        self.ok(live, "execute", json!({"command": command, "params": params}))
    }

    fn exec_refused(&mut self, live: &mut Live, command: &str, params: Value) -> String {
        self.refused(live, "execute", json!({"command": command, "params": params}))
    }

    fn wire(&mut self, live: &mut Live, y: f64) -> Value {
        self.exec(live, "wire", json!({"points": [[0.0, y], [60.0, y]]}))
    }

    fn connection(&mut self, live: Option<&mut Live>) -> Value {
        let r = self.request(live, "tools/call", json!({"name": "ecad_connection", "arguments": {}}));
        serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    /// Close stdin (the server says `ecad.bye`) and wait for the server to end, pumping.
    fn close(mut self, live: Option<&mut Live>) {
        self.to = None;
        let t0 = Instant::now();
        let mut live = live;
        while !self.done.as_ref().is_some_and(std::thread::JoinHandle::is_finished) {
            assert!(t0.elapsed() < GUARD, "the server ends when stdin closes");
            match live.as_deref_mut() {
                Some(l) => l.pump(),
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}

// ---------- raw control lines over the socket ----------

fn token(port: u16) -> String {
    cadcraft_mcp::control_token::read(port).unwrap()
}

/// Send `lines` on one connection from another thread; collect one reply per line while pumping.
fn raw(live: &mut Live, lines: Vec<Value>) -> Vec<Value> {
    let port = live.port;
    let n = lines.len();
    let (tx, rx) = channel::<Value>();
    std::thread::spawn(move || {
        let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else { return };
        let _ = s.set_read_timeout(Some(Duration::from_secs(70)));
        let mut r = BufReader::new(s.try_clone().unwrap());
        for l in lines {
            if writeln!(s, "{l}").is_err() {
                return;
            }
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let _ = tx.send(serde_json::from_str(&line).unwrap_or(json!({"unparsed": line})));
        }
    });
    let mut out = Vec::new();
    let t0 = Instant::now();
    while out.len() < n {
        while let Ok(v) = rx.try_recv() {
            out.push(v);
        }
        if out.len() >= n {
            break;
        }
        assert!(t0.elapsed() < GUARD, "raw replies: got {} of {n}: {out:?}", out.len());
        live.pump();
    }
    out
}

fn line(port: u16, id: u64, method: &str, params: Value) -> Value {
    json!({"id": id, "method": method, "params": params, "token": token(port)})
}

fn session_id(n: u64) -> String {
    format!("{n:016x}")
}

fn hello(port: u16, id: u64, session: &str, name: &str, write: bool) -> Value {
    line(port, id, "ecad.hello", json!({"session": session, "name": name, "write": write, "client_version": "t035"}))
}

fn assert_not_run(r: &Value, says: &str) {
    assert_eq!(r["ok"], json!(false), "{r}");
    assert_eq!(r["state"], json!("not-run"), "{r}");
    let e = r["error"].as_str().unwrap_or_default();
    assert!(e.contains(says), "{says:?} not in {e:?}");
}

fn authors(log: &[Transaction]) -> Vec<String> {
    log.iter().map(|t| ecad_mcp::author_text(&t.author)).collect()
}

fn agent_author() -> Author {
    Author::Agent(AGENT.into())
}

// ============================================================================================
// AC 1, 13: off by default; the token is required
// ============================================================================================

#[test]
fn ac1_with_no_preference_nothing_listens() {
    let _l = lock();
    let _ = std::fs::remove_file(access_file());
    let default_before = listening(47823);
    let port = free_port();
    let mut h = harness_with(None);
    h.run_steps(10);
    assert!(!listening(port), "a free port stays closed");
    if default_before {
        eprintln!("47823 was already in use before the trial started (another ECAD?); only the free port is checked");
    } else {
        assert!(!listening(47823), "the default port is not opened without the preference");
    }
    h.state_mut().on_exit();
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac1_access_is_off_by_default_and_the_reply_says_so() {
    let _l = lock();
    let _ = std::fs::remove_file(access_file());
    let mut h = harness_with(None);
    let r = agentaccess(&mut h, json!({}));
    assert_eq!(r["mode"], json!("off"), "{r}");
    assert_eq!(r["port"], json!(47823), "{r}");
    assert_eq!(r["listening"], json!(false), "{r}");
    assert!(!access_file().exists(), "reading the preference writes nothing");
    h.state_mut().on_exit();
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac13_a_request_without_the_token_is_refused() {
    let _l = lock();
    let mut live = Live::new("write");
    let before = live.log();
    let p = live.port;
    let r = raw(&mut live, vec![json!({"id": 1, "method": "ping"})]);
    assert_eq!(r[0]["ok"], json!(false), "{r:?}");
    assert!(r[0]["error"].as_str().unwrap().contains("unauthorized"), "{r:?}");
    let r = raw(
        &mut live,
        vec![json!({"id": 2, "method": "ecad.hello", "params": {"session": session_id(1), "name": "x", "write": true}, "token": "0".repeat(64)})],
    );
    assert!(r[0]["error"].as_str().unwrap().contains("unauthorized"), "{r:?}");
    // With the token, ping answers.
    let r = raw(&mut live, vec![line(p, 3, "ping", json!({}))]);
    assert_ne!(r[0]["ok"], json!(false), "{r:?}");
    assert_eq!(live.log(), before);
}

// ============================================================================================
// AC 2: three wires by the agent, one transaction each, the banner shows the last one
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac2_three_wires_by_the_agent_appear_with_the_banner() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    let conn = a.connection(Some(&mut live));
    assert_eq!(conn["mode"], json!("bridged"), "{conn}");
    assert_eq!(conn["port"], json!(live.port), "{conn}");
    assert_eq!(conn["writer"], json!(true), "{conn}");
    assert_eq!(conn["app"]["writes"], json!(true), "{conn}");
    let start = live.log().len();
    let wires_before = live.conductors();
    let mut last_seq = 0;
    for y in [0.0, 5.0, 10.0] {
        let n = live.log().len();
        let r = a.wire(&mut live, y);
        let log = live.log();
        assert_eq!(log.len(), n + 1, "one transaction per call: {r}");
        let t = log.last().unwrap();
        assert_eq!(t.author, agent_author(), "{}", t.label);
        assert_eq!(r["ecadTransactions"][0]["author"], json!(format!("agent {AGENT}")), "{r}");
        assert_eq!(r["ecadTransactions"][0]["seq"], json!(t.seq), "{r}");
        last_seq = t.seq;
    }
    assert_eq!(live.log().len(), start + 3);
    assert_eq!(live.conductors(), wires_before + 3, "the wires are on the trial's project");
    // The banner (§7.1, §7.2).
    let b = banner(&live.h).expect("a banner while an agent is connected");
    assert_eq!(b.agent, AGENT);
    assert_eq!(b.steps, 3);
    assert_eq!(b.last_change.as_ref().map(|c| c.seq), Some(last_seq));
    let state = live.element_text("agent.banner.state").expect("agent.banner.state shown");
    assert!(state.contains("changing"), "{state}");
    let last = live.element_text("agent.banner.last").expect("agent.banner.last shown");
    assert!(last.contains(&format!("#{last_seq}")) && last.contains("Wire"), "{last}");
    assert!(live.h.query_by_label("agent.banner").is_some());
    // The echo line in the command history (§5.1).
    let echoes = live.echoes();
    assert!(
        echoes.iter().any(|e| e.starts_with(&format!("Agent {AGENT}: WIRE")) && e.contains(&format!("#{last_seq}"))),
        "{:?}",
        &echoes[echoes.len().saturating_sub(8)..]
    );
    // The frame invariant never had to act.
    assert_eq!(author_resets(&live.h), 0);
    assert_eq!(live.author(), (Author::User, None));
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac2_bridged_replies_and_refusals_match_headless_byte_for_byte() {
    let _l = lock();
    // The 011 refusal table and an engine failure: the same words over the bridge and headless.
    let calls: Vec<(&str, Value)> = vec![
        ("open", json!({"path": "x.ecad"})),
        ("saveas", json!({"path": "y.ecad"})),
        ("new", json!({})),
        ("close", json!({})),
        ("export", json!({"path": "z.dxf"})),
        ("setvar", json!({"name": "AUDITCTL", "value": 1})),
        ("audit", json!({"fix": true})),
        ("undo", json!({"count": 2})),
        ("redo", json!({"count": 3})),
        ("agentaccess", json!({"mode": "write"})),
        ("wire", json!({"points": [[0, 0]]})),
        ("nosuchcommand", json!({})),
    ];
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    let bridged: Vec<String> = calls.iter().map(|(c, p)| a.exec_refused(&mut live, c, p.clone())).collect();
    a.close(Some(&mut live));
    drop(live);
    // Headless: the same server binary with nothing on its port.
    let mut h = Agent::start_at(free_port(), AGENT, true, None);
    let mut headless = Vec::new();
    for (c, p) in &calls {
        let r = h.request(None, "tools/call", json!({"name": "execute", "arguments": {"command": c, "params": p}}));
        assert_eq!(r["result"]["isError"], json!(true), "{c}: {r}");
        headless.push(r["result"]["content"][0]["text"].as_str().unwrap().to_string());
    }
    assert_eq!(h.connection(None)["mode"], json!("headless"));
    h.close(None);
    for ((c, _), (b, hl)) in calls.iter().zip(bridged.iter().zip(&headless)) {
        assert_eq!(b, hl, "{c}: bridged and headless differ");
    }
}

// ============================================================================================
// AC 3: a person's edit is the person's, also after failed, refused and panicking agent calls
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac3_a_person_between_two_agent_calls_is_the_user() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    let p = live.person_wire(5.0);
    assert_eq!(p.author, Author::User);
    a.wire(&mut live, 10.0);
    let log = live.log();
    let tail = authors(&log[log.len() - 3..]);
    assert_eq!(tail, vec![format!("agent {AGENT}"), "user".into(), format!("agent {AGENT}")]);
    // A person's mouse-driven wire (interactive, typed points) is the person's too.
    for l in ["wire", "0,20", "60,20", ""] {
        let _ = live.h.state_mut().submit_command(l);
        live.h.run_steps(2);
    }
    assert_eq!(live.log().last().unwrap().author, Author::User);
    assert_eq!(author_resets(&live.h), 0);
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac3_refused_failed_and_panicking_agent_calls_leave_the_person_as_author() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    // Refused by policy.
    a.exec_refused(&mut live, "open", json!({"path": "x.ecad"}));
    assert_eq!(live.author(), (Author::User, None));
    assert_eq!(live.person_wire(0.0).author, Author::User, "after a refusal");
    // Failing in the engine.
    a.exec_refused(&mut live, "wire", json!({"points": [[0, 0]]}));
    assert_eq!(live.author(), (Author::User, None));
    assert_eq!(live.person_wire(5.0).author, Author::User, "after an engine failure");
    // Over the cap (rolled back): the roll-back's own steps are the agent's, the next is the user's.
    // (A batch of two wires on one line: the second is refused and taken back.)
    let batch = json!([
        {"jsonrpc": "2.0", "id": 900, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 30], [60, 30]]}}}},
        {"jsonrpc": "2.0", "id": 901, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 35], [60, 35]]}}}}
    ]);
    a.send_line(&batch.to_string());
    let _ = a.wait(Some(&mut live), GUARD);
    assert_eq!(live.author(), (Author::User, None));
    assert_eq!(live.person_wire(40.0).author, Author::User, "after a rolled-back call");
    // Panicking inside the command (fault injection, design §12 check 3).
    {
        let t = live.h.state_mut();
        let p = t.project().unwrap().clone();
        p.configure(&mut t.cad.session, |p| p.faults.panic_in_command = true).unwrap();
    }
    let n = live.log().len();
    let m = a.exec_refused(&mut live, "wire", json!({"points": [[0, 45], [60, 45]]}));
    assert!(!m.is_empty());
    assert_eq!(live.log().len(), n, "the panicking call recorded nothing");
    assert_eq!(live.author(), (Author::User, None), "restored after the panic");
    assert_eq!(live.person_wire(50.0).author, Author::User, "after a panic");
    // The panic left the bridge usable.
    a.wire(&mut live, 55.0);
    assert_eq!(live.log().last().unwrap().author, agent_author());
    assert_eq!(author_resets(&live.h), 0, "the per-call scope restored it; the frame check never had to");
    a.close(Some(&mut live));
}

// ============================================================================================
// AC 4: caps over the bridge
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac4_a_batch_is_one_call_and_records_at_most_one() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    let wires = live.conductors();
    let batch = json!([
        {"jsonrpc": "2.0", "id": 900, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 0], [60, 0]]}}}},
        {"jsonrpc": "2.0", "id": 901, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 5], [60, 5]]}}}}
    ]);
    a.send_line(&batch.to_string());
    let replies = a.wait(Some(&mut live), GUARD);
    let replies = replies.as_array().unwrap();
    assert_ne!(replies[0]["result"]["isError"], json!(true), "{}", replies[0]);
    assert_eq!(replies[1]["result"]["isError"], json!(true), "{}", replies[1]);
    let m = replies[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(m.contains("one request line, batches included") && m.contains("taken back"), "011's wording: {m}");
    assert_eq!(live.conductors(), wires + 1, "only the first wire stays");
    // The next line is a new call.
    a.wire(&mut live, 10.0);
    assert_eq!(live.conductors(), wires + 2);
    // Several steps in one call are refused before anything runs (011).
    let before = live.log();
    for (cmd, params) in [("undo", json!({"count": 2})), ("redo", json!({"count": 3}))] {
        let m = a.exec_refused(&mut live, cmd, params);
        assert_eq!(m, ecad_mcp::policy::one_step(cmd), "{cmd}");
    }
    assert_eq!(live.log(), before);
    // The banner shows the refusal of the call, cleared by the next accepted one (§7.1).
    let b = banner(&live.h).unwrap();
    assert!(b.last_refusal.as_deref().is_some_and(|r| !r.is_empty()), "{:?}", b.last_refusal);
    a.wire(&mut live, 15.0);
    assert!(banner(&live.h).unwrap().last_refusal.is_none());
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac4_a_burst_is_answered_at_most_eight_per_frame() {
    let _l = lock();
    let mut live = Live::new("write");
    let p = live.port;
    // 12 connections, one ping each, all queued before the next frame.
    let (tx, rx) = channel::<Value>();
    for i in 0..12u64 {
        let tx = tx.clone();
        let l = line(p, i, "ping", json!({}));
        std::thread::spawn(move || {
            let mut s = TcpStream::connect(("127.0.0.1", p)).unwrap();
            writeln!(s, "{l}").unwrap();
            let mut r = BufReader::new(s);
            let mut out = String::new();
            let _ = r.read_line(&mut out);
            let _ = tx.send(serde_json::from_str(&out).unwrap_or(json!(null)));
        });
    }
    std::thread::sleep(Duration::from_millis(500));
    live.h.run_steps(1);
    std::thread::sleep(Duration::from_millis(200));
    let first: Vec<Value> = rx.try_iter().collect();
    assert!(first.len() <= 8, "one frame answers at most 8 agent requests: {}", first.len());
    let mut all = first.len();
    let t0 = Instant::now();
    while all < 12 {
        assert!(t0.elapsed() < GUARD, "the rest are answered in the next frames ({all}/12)");
        live.pump();
        all += rx.try_iter().count();
    }
}

// ============================================================================================
// AC 5: busy rule
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac5_an_agent_write_waits_five_seconds_then_is_refused_while_the_person_draws_a_wire() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    let mut reader = Agent::start(&mut live, "reader", false);
    // The person starts WIRE and leaves it half done.
    let _ = live.h.state_mut().submit_command("wire");
    live.h.run_steps(2);
    let _ = live.h.state_mut().submit_command("0,0");
    live.h.run_steps(2);
    assert!(live.h.state().cad.session.running.is_some(), "WIRE is running");
    let before = live.log();
    // The write is held, the banner says so, reads from another session are answered at once.
    a.send_line(&json!({"jsonrpc": "2.0", "id": 777, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 5], [60, 5]]}}}}).to_string());
    let t0 = Instant::now();
    for _ in 0..20 {
        live.pump();
    }
    let state = live.element_text("agent.banner.state").unwrap_or_default();
    assert!(state.contains("waiting for you to finish WIRE"), "{state}");
    let t_read = Instant::now();
    let project = reader.ok(&mut live, "ecad_project", json!({}));
    assert!(project.is_object());
    assert!(t_read.elapsed() < Duration::from_secs(1), "a read is answered at once, not held");
    let raw_ping = raw(&mut live, vec![line(live.port, 1, "ping", json!({}))]);
    assert_ne!(raw_ping[0]["ok"], json!(false), "{raw_ping:?}");
    // The held write: refused after ≥ 5 s, not-run, nothing changed, the person's WIRE intact.
    let r = a.wait(Some(&mut live), Duration::from_secs(30));
    let held_for = t0.elapsed();
    assert_eq!(r["id"], json!(777));
    assert!(held_for >= Duration::from_secs(5), "held {held_for:?}");
    assert!(held_for < Duration::from_secs(15), "held {held_for:?}");
    let m = r["result"]["content"][0]["text"].as_str().or(r["error"]["message"].as_str()).unwrap().to_string();
    assert!(m.contains("Refused: the designer is in the middle of WIRE; nothing was changed. Try again when they finish."), "{m}");
    assert_eq!(live.log(), before, "history unchanged");
    assert!(live.h.state().cad.session.running.as_ref().is_some_and(|r| r.id.eq_ignore_ascii_case("wire")), "the person's WIRE was not interrupted");
    // The person finishes; the agent's next write runs.
    let _ = live.h.state_mut().cad.run("cancel", json!({}));
    live.h.run_steps(3);
    assert!(live.h.state().cad.session.running.is_none());
    a.wire(&mut live, 5.0);
    assert_eq!(live.log().last().unwrap().author, agent_author());
    reader.close(Some(&mut live));
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac5_held_over_the_wire_carries_not_run() {
    let _l = lock();
    let mut live = Live::new("write");
    let p = live.port;
    let s = session_id(0x35);
    let _ = live.h.state_mut().submit_command("wire");
    live.h.run_steps(2);
    let r = raw(
        &mut live,
        vec![
            hello(p, 1, &s, AGENT, true),
            line(p, 2, "ecad.execute", json!({"session": s, "name": AGENT, "line": 1, "command": "wire", "params": {"points": [[0, 0], [9, 0]]}})),
        ],
    );
    // raw() uses the 10 s guard: the hold is 5 s.
    assert_not_run(&r[1], "the designer is in the middle of WIRE");
}

// ============================================================================================
// AC 6: raw methods refused over the port
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac6_raw_methods_are_refused_and_change_nothing() {
    let _l = lock();
    let mut live = Live::new("write");
    let p = live.port;
    let before = live.log();
    let dir = std::env::temp_dir().join(format!("t035-raw-{}", std::process::id()));
    let target = dir.join("evil.ecad").to_string_lossy().into_owned();
    let cases: Vec<(&str, Value, &str)> = vec![
        ("cmdline.input", json!({"text": "line 0,0 10,0 "}), "ECAD changes go through"),
        ("cmdline.script", json!({"script": "LINE 0,0 10,0\n"}), "ECAD changes go through"),
        ("cmdline.key", json!({"key": "Escape"}), "not offered over ECAD's port"),
        ("app.open", json!({"path": target}), "files stay with the person"),
        ("app.save", json!({}), "files stay with the person"),
        ("ui.click", json!({"label": "agent.access.mode"}), "the agent does not use the person's mouse and keyboard"),
        ("ui.key", json!({"key": "Escape"}), "the agent does not use the person's mouse and keyboard"),
        ("engine.execute", json!({"command": "agentaccess", "params": {"mode": "write", "port": 1025}}), "not offered over ECAD's port"),
        ("engine.execute", json!({"command": "wire", "params": {"points": [[0, 0], [9, 0]]}}), "not offered over ECAD's port"),
        ("ui.render", json!({"path": target}), "not offered over ECAD's port"),
        ("ui.screenshot", json!({"path": target}), "not offered over ECAD's port"),
        ("app.quit", json!({}), "not offered over ECAD's port"),
        ("ecad.nosuch", json!({}), "not offered over ECAD's port"),
    ];
    let lines: Vec<Value> = cases.iter().enumerate().map(|(i, (m, params, _))| line(p, i as u64, m, params.clone())).collect();
    let replies = raw(&mut live, lines);
    for ((m, _, says), r) in cases.iter().zip(&replies) {
        assert_eq!(r["ok"], json!(false), "{m}: {r}");
        assert_eq!(r["state"], json!("not-run"), "{m}: {r}");
        let e = r["error"].as_str().unwrap_or_default();
        assert!(e.starts_with("not offered over ECAD's port") || e.contains("not offered over ECAD's port"), "{m}: {e}");
        assert!(e.contains(says), "{m}: {says:?} not in {e:?}");
    }
    assert_eq!(live.log(), before, "nothing recorded");
    assert!(!std::path::Path::new(&target).exists(), "no file written");
    assert_eq!(agentaccess(&mut live.h, json!({}))["mode"], json!("write"), "the preference is unchanged");
    assert_eq!(agentaccess(&mut live.h, json!({}))["port"], json!(p));
    assert!(live.h.state().cad.session.running.is_none(), "no command was started");
    // The allowlist answers.
    let ok = raw(
        &mut live,
        vec![
            line(p, 50, "ping", json!({})),
            line(p, 51, "engine.commands", json!({})),
            line(p, 52, "drawing.inspect", json!({})),
            line(p, 53, "control.methods", json!({})),
        ],
    );
    for r in &ok {
        assert_ne!(r["ok"], json!(false), "{r}");
    }
    let methods = ok[3].to_string();
    for m in ["ecad.hello", "ecad.execute", "ecad.tool", "ecad.bye", "drawing.inspect", "engine.commands", "ui.render", "ui.screenshot", "ping"] {
        assert!(methods.contains(m), "control.methods lists {m}: {methods}");
    }
    for m in ["cmdline.input", "app.open", "ui.click"] {
        assert!(!methods.contains(m), "control.methods must not list {m}: {methods}");
    }
}

// ============================================================================================
// AC 7: Read only preference, or a server without --write
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac7_read_only_preference_refuses_every_change_and_save() {
    let _l = lock();
    let mut live = Live::new("read");
    let mut a = Agent::start(&mut live, AGENT, true);
    let before = live.log();
    let history = a.ok(&mut live, "ecad_history", json!({}));
    for (cmd, params) in
        [("wire", json!({"points": [[0, 0], [60, 0]]})), ("newpage", json!({})), ("u", json!({})), ("setprop", json!({"key": "x", "value": "y"}))]
    {
        let m = a.exec_refused(&mut live, cmd, params);
        assert!(m.contains("Agent access in the ECAD window is Read only."), "{cmd}: {m}");
    }
    let m = a.refused(&mut live, "save", json!({}));
    assert!(m.contains("Read only"), "{m}");
    assert_eq!(live.log(), before);
    assert_eq!(a.ok(&mut live, "ecad_history", json!({})), history, "history unchanged");
    // Reads work.
    assert!(a.ok(&mut live, "ecad_project", json!({})).is_object());
    assert!(a.ok(&mut live, "ecad_inspect", json!({})).is_object());
    a.exec(&mut live, "ecadinspect", json!({}));
    let conn = a.connection(Some(&mut live));
    assert_eq!(conn["mode"], json!("bridged"));
    assert_eq!(conn["app"]["writes"], json!(false), "{conn}");
    assert!(conn["app"]["why"].as_str().is_some_and(|w| w.contains("Read only")), "{conn}");
    let state = live.element_text("agent.banner.state").unwrap_or_default();
    assert!(state.contains("reading"), "{state}");
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac7_a_server_without_write_is_refused_with_011s_words() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, false);
    let before = live.log();
    let m = a.exec_refused(&mut live, "wire", json!({"points": [[0, 0], [60, 0]]}));
    assert_eq!(m, ecad_mcp::policy::WRITES_OFF);
    let m = a.refused(&mut live, "save", json!({}));
    assert!(m.contains("--write"), "{m}");
    assert_eq!(live.log(), before);
    assert!(a.ok(&mut live, "ecad_project", json!({})).is_object());
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac7_turning_access_off_refuses_and_ends_the_session() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    agentaccess(&mut live.h, json!({"mode": "off"}));
    let before = live.log();
    let m = a.exec_refused(&mut live, "wire", json!({"points": [[0, 5], [60, 5]]}));
    assert!(m.contains("agent access is off in the ECAD window") || m.contains("agent access was turned off in the ECAD window"), "{m}");
    assert_eq!(live.log(), before);
    assert!(live.h.query_by_label("agent.banner.pause").is_none(), "the session ended: no live banner");
    // Read and change → Read only takes effect at the next call.
    agentaccess(&mut live.h, json!({"mode": "write"}));
    agentaccess(&mut live.h, json!({"mode": "read"}));
    let m = a.exec_refused(&mut live, "wire", json!({"points": [[0, 5], [60, 5]]}));
    assert!(!m.is_empty());
    assert_eq!(live.log(), before);
    a.close(Some(&mut live));
}

// ============================================================================================
// AC 8: an agent's u/redo act only on its own steps
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac8_an_agent_undoes_only_its_own_steps() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    let person = live.person_wire(5.0);
    let before = live.log();
    let m = a.exec_refused(&mut live, "u", json!({}));
    assert!(m.starts_with(&format!("Refused: the last step (#{} \"", person.seq)), "{m}");
    assert!(m.ends_with("is the designer's; an agent undoes only its own steps."), "{m}");
    assert_eq!(live.log(), before, "nothing undone");
    // Its own step on top: undone, one Undo transaction by the agent.
    a.wire(&mut live, 10.0);
    let wires = live.conductors();
    a.exec(&mut live, "u", json!({}));
    let last = live.log().last().cloned().unwrap();
    assert!(matches!(last.kind, TxKind::Undo { .. }), "{:?}", last.kind);
    assert_eq!(last.author, agent_author());
    assert_eq!(live.conductors(), wires - 1);
    // Its own redo.
    a.exec(&mut live, "redo", json!({}));
    assert_eq!(live.conductors(), wires);
    // A redo of the person's undo is refused.
    let _ = live.h.state_mut().submit_command("u");
    live.h.run_steps(2);
    let before = live.log();
    let m = a.exec_refused(&mut live, "redo", json!({}));
    assert!(m.contains("an agent undoes only its own steps") || m.contains("designer"), "{m}");
    assert_eq!(live.log(), before);
    a.close(Some(&mut live));
}

// ============================================================================================
// AC 9: Undo session
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac9_undo_session_takes_back_the_agents_steps_as_the_person() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    let wires = live.conductors();
    let first = live.log().len();
    for y in [0.0, 5.0, 10.0] {
        a.wire(&mut live, y);
    }
    let seqs: Vec<u64> = live.log()[first..].iter().map(|t| t.seq).collect();
    live.click("agent.banner.undo");
    assert!(live.h.query_by_label("agent.undo.confirm").is_none(), "nothing of the person on top: no question");
    let log = live.log();
    let undos = &log[log.len() - 3..];
    assert!(undos.iter().all(|t| matches!(t.kind, TxKind::Undo { .. }) && t.author == Author::User), "{:?}", authors(undos));
    assert_eq!(live.conductors(), wires);
    let said = format!("Undid 3 agent steps (#{}–#{}).", seqs[0], seqs[2]);
    assert!(live.echoes().iter().any(|e| e == &said), "{said}");
    assert_eq!(banner(&live.h).unwrap().steps, 0);
    assert!(!live.h.get_by_label("agent.banner.undo").is_enabled() || live.h.get_by_label("agent.banner.undo").is_disabled(), "disabled at 0 steps");
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac9_undo_session_asks_first_when_the_persons_steps_are_on_top() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    a.wire(&mut live, 5.0);
    let mine = live.person_wire(10.0);
    a.wire(&mut live, 15.0);
    let before = live.log();
    // Cancel: nothing happens.
    live.click("agent.banner.undo");
    let dialog = live.element_text("agent.undo.confirm").expect("the confirmation is shown");
    let _ = dialog;
    let all_text = format!("{:?}", live.h.query_all_by_label_contains("#"));
    assert!(all_text.contains(&format!("#{}", mine.seq)), "the person's step is named: {all_text}");
    live.click("agent.undo.cancel");
    assert!(live.h.query_by_label("agent.undo.confirm").is_none());
    assert_eq!(live.log(), before, "Cancel undoes nothing");
    // Esc cancels too.
    live.click("agent.banner.undo");
    live.h.key_press(eframe::egui::Key::Escape);
    live.h.run_steps(3);
    assert!(live.h.query_by_label("agent.undo.confirm").is_none());
    assert_eq!(live.log(), before);
    // "after my last one": only the agent's step after the person's (1).
    live.click("agent.banner.undo");
    live.click("agent.undo.after");
    let log = live.log();
    assert_eq!(log.len(), before.len() + 1);
    assert_eq!(log.last().unwrap().author, Author::User);
    assert!(log.iter().rev().nth(1).is_some_and(|t| t.seq == before.last().unwrap().seq));
    // "Undo all": the person's own step goes too, then the agent's two.
    live.click("agent.banner.undo");
    let n = live.log().len();
    live.click("agent.undo.all");
    let log = live.log();
    assert_eq!(log.len(), n + 3, "the person's step and the agent's two");
    assert!(log[n..].iter().all(|t| matches!(t.kind, TxKind::Undo { .. }) && t.author == Author::User));
    a.close(Some(&mut live));
}

// ============================================================================================
// AC 10, Disconnect, one writer
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac10_pause_refuses_writes_until_resumed() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    live.click("agent.banner.pause");
    let before = live.log();
    let m = a.exec_refused(&mut live, "wire", json!({"points": [[0, 5], [60, 5]]}));
    assert_eq!(m, "Refused: paused by the designer; nothing was changed.");
    assert_eq!(live.log(), before);
    assert!(a.ok(&mut live, "ecad_history", json!({})).is_object(), "reads continue");
    let state = live.element_text("agent.banner.state").unwrap_or_default();
    assert!(state.contains("paused"), "{state}");
    live.click("agent.banner.pause");
    a.wire(&mut live, 5.0);
    assert_eq!(live.log().last().unwrap().author, agent_author());
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn disconnect_ends_the_session_and_releases_the_writer() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    live.click("agent.banner.disconnect");
    let before = live.log();
    let m = a.exec_refused(&mut live, "wire", json!({"points": [[0, 5], [60, 5]]}));
    assert!(m.contains("the designer disconnected this agent; restart the MCP server to connect again"), "{m}");
    assert_eq!(live.log(), before);
    let mut b = Agent::start(&mut live, "second", true);
    b.wire(&mut live, 10.0);
    assert_eq!(live.log().last().unwrap().author, Author::Agent("second".into()), "the writer role was released");
    b.close(Some(&mut live));
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn one_writer_at_a_time_the_second_reads_until_the_first_says_bye() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    let mut b = Agent::start(&mut live, "second", true);
    let before = live.log();
    let m = b.exec_refused(&mut live, "wire", json!({"points": [[0, 5], [60, 5]]}));
    assert!(m.contains(&format!("another agent ({AGENT}) is changing this project; this session can read")), "{m}");
    assert_eq!(live.log(), before);
    assert!(b.ok(&mut live, "ecad_project", json!({})).is_object());
    assert_eq!(b.connection(Some(&mut live))["writer"], json!(false));
    assert_eq!(banner(&live.h).unwrap().others, 1);
    a.close(Some(&mut live));
    b.wire(&mut live, 5.0);
    assert_eq!(live.log().last().unwrap().author, Author::Agent("second".into()));
    b.close(Some(&mut live));
    let state = live.element_text("agent.banner.state").unwrap_or_default();
    assert!(state.contains("finished"), "{state}");
    live.click("agent.banner.dismiss");
    assert!(live.h.query_by_label("agent.banner").is_none());
}

// ============================================================================================
// AC 11: mode
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac11_headless_until_the_window_answers_then_bridged() {
    let _l = lock();
    let port = free_port();
    let mut a = Agent::start_at(port, AGENT, true, None);
    assert_eq!(a.connection(None)["mode"], json!("headless"));
    // A read headless does not stick.
    let _ = a.request(None, "tools/call", json!({"name": "ecad_project", "arguments": {}}));
    // The window opens on that port before anything was recorded.
    let mut h = harness_with(None);
    let r = agentaccess(&mut h, json!({"mode": "write", "port": port}));
    assert_eq!(r["listening"], json!(true), "{r}");
    let mut live = Live { h, port };
    let conn = a.connection(Some(&mut live));
    assert_eq!(conn["mode"], json!("bridged"), "{conn}");
    a.wire(&mut live, 0.0);
    assert_eq!(live.log().last().unwrap().author, agent_author(), "the write reached the window");
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn ac11_after_a_headless_change_the_server_stays_headless() {
    let _l = lock();
    let port = free_port();
    let mut a = Agent::start_at(port, AGENT, true, None);
    let r = a.request(None, "tools/call", json!({"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 0], [60, 0]]}}}));
    assert_ne!(r["result"]["isError"], json!(true), "{r}");
    let mut h = harness_with(None);
    agentaccess(&mut h, json!({"mode": "write", "port": port}));
    let mut live = Live { h, port };
    let before = live.log();
    let conn = a.connection(Some(&mut live));
    assert_eq!(conn["mode"], json!("headless"), "work is never split: {conn}");
    a.wire(&mut live, 5.0);
    assert_eq!(live.log(), before, "the window got nothing");
    assert!(banner(&live.h).is_none(), "no session said hello");
    a.close(Some(&mut live));
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn bridged_sticks_and_a_closed_window_is_said_in_words() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    // The app closes with a call in flight (never pumped again).
    a.send_line(&json!({"jsonrpc": "2.0", "id": 4242, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 5], [60, 5]]}}}}).to_string());
    drop(live);
    let r = a.wait(None, Duration::from_secs(75));
    assert_eq!(r["id"], json!(4242));
    let m = r["result"]["content"][0]["text"].as_str().or(r["error"]["message"].as_str()).unwrap_or_default().to_string();
    assert!(r["result"]["isError"] == json!(true) || r.get("error").is_some(), "{r}");
    assert!(m.contains("The ECAD window at") || m.contains("check ecad_history"), "{m}");
    assert!(!m.contains("CADCraft at"), "reworded: {m}");
    // The next call: still bridged, says the window is not answering.
    let r = a.request(None, "tools/call", json!({"name": "execute", "arguments": {"command": "wire", "params": {"points": [[0, 9], [60, 9]]}}}));
    let m = r["result"]["content"][0]["text"].as_str().or(r["error"]["message"].as_str()).unwrap_or_default().to_string();
    assert!(m.contains("is not answering") && m.contains("nothing was changed"), "{m}");
    assert_eq!(a.connection(None)["mode"], json!("bridged"));
    a.close(None);
}

// ============================================================================================
// Disconnect mid-call, hostile and oversized input on the port
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn a_client_that_vanishes_mid_call_leaves_the_person_as_author() {
    let _l = lock();
    let mut live = Live::new("write");
    let p = live.port;
    let s = session_id(0xD15C);
    let lines = [
        hello(p, 1, &s, AGENT, true).to_string(),
        line(p, 2, "ecad.execute", json!({"session": s, "name": AGENT, "line": 1, "command": "wire", "params": {"points": [[0, 0], [9, 0]]}}))
            .to_string(),
    ];
    {
        // Send hello and execute, then close the socket before any frame runs.
        let mut sock = TcpStream::connect(("127.0.0.1", p)).unwrap();
        for l in &lines {
            writeln!(sock, "{l}").unwrap();
        }
    }
    for _ in 0..30 {
        live.pump();
    }
    assert_eq!(live.author(), (Author::User, None));
    assert_eq!(live.person_wire(20.0).author, Author::User);
    assert_eq!(author_resets(&live.h), 0);
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn hostile_ecad_requests_are_refused_without_running() {
    let _l = lock();
    let mut live = Live::new("write");
    let p = live.port;
    let s = session_id(0xBAD);
    let before = live.log();
    let deep = (0..5000).fold(json!(0), |v, _| json!([v]));
    let exec = |id: u64, extra: Value| {
        let mut params = json!({"session": s, "name": AGENT, "line": 1, "command": "wire", "params": {"points": [[0, 0], [9, 0]]}});
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        line(p, id, "ecad.execute", params)
    };
    // Before hello: refused.
    let r = raw(&mut live, vec![exec(1, json!({}))]);
    assert_not_run(&r[0], "say ecad.hello first");
    // Malformed hellos: refused or answered, never a session that writes as someone else.
    let bad_hellos = vec![
        line(p, 2, "ecad.hello", json!({})),
        line(p, 3, "ecad.hello", json!({"session": "xyz", "name": AGENT, "write": true})),
        line(p, 4, "ecad.hello", json!({"session": "0123456789abcdef0", "name": AGENT, "write": true})),
        line(p, 5, "ecad.hello", json!({"session": 12, "name": AGENT, "write": true})),
        line(p, 6, "ecad.hello", json!({"session": session_id(7), "name": [1], "write": "yes"})),
        line(p, 7, "ecad.hello", json!({"session": session_id(8), "name": "x".repeat(1 << 20), "write": true})),
    ];
    let r = raw(&mut live, bad_hellos);
    assert_eq!(r.len(), 6);
    // A good hello, then hostile executes.
    let r = raw(&mut live, vec![hello(p, 10, &s, AGENT, true)]);
    assert_ne!(r[0]["ok"], json!(false), "{r:?}");
    let hostile = vec![
        exec(11, json!({"name": "someone else"})),
        exec(12, json!({"command": 5})),
        exec(13, json!({"command": "open", "params": {"path": "x.ecad"}})),
        exec(14, json!({"params": deep})),
        exec(15, json!({"line": -1})),
        exec(16, json!({"line": "x"})),
        exec(17, json!({"params": {"points": [[0, 0], [1e308, 1e308]]}})),
        exec(18, json!({"command": "agentaccess", "params": {"mode": "write"}})),
        line(p, 19, "ecad.tool", json!({"session": s, "name": AGENT, "line": 2, "tool": "save", "arguments": {"path": "elsewhere.ecad"}})),
        line(p, 20, "ecad.tool", json!({"session": s, "name": AGENT, "line": 3, "tool": "nosuchtool", "arguments": {}})),
        line(p, 21, "ecad.tool", json!({"session": s, "name": AGENT, "line": 4, "tool": "ecad_history", "arguments": {"limit": u64::MAX}})),
        line(p, 22, "ecad.bye", json!({"session": session_id(0xFFFF)})),
    ];
    let r = raw(&mut live, hostile);
    assert_eq!(r.len(), 12, "every line answered: {r:?}");
    assert_not_run(&r[0], "");
    for (i, v) in r.iter().enumerate().take(11).skip(1) {
        if i == 10 {
            continue; // ecad_history read with a huge limit: answered (capped), not refused.
        }
        assert_eq!(v["ok"], json!(false), "line {i}: {v}");
    }
    let log = live.log();
    assert!(
        log.len() <= before.len() + 1 && log[before.len()..].iter().all(|t| t.author == agent_author()),
        "at most the 1e308 wire, and only by the hello's name: {:?}",
        authors(&log[before.len()..])
    );
    // A name with invisible characters is recorded cleaned (011's agent_name).
    let s2 = session_id(0x5EC0);
    let r = raw(
        &mut live,
        vec![
            hello(p, 30, &s2, "ad\u{202E}min\u{200B}\u{7}", true),
            line(
                p,
                31,
                "ecad.execute",
                json!({"session": s2, "name": "ad\u{202E}min\u{200B}\u{7}", "line": 1, "command": "wire", "params": {"points": [[0, 40], [9, 40]]}}),
            ),
        ],
    );
    let _ = r;
    let last = live.log().last().cloned().unwrap();
    if last.seq > log.last().unwrap().seq {
        assert_eq!(last.author, Author::Agent("admin".into()));
    }
    assert_eq!(live.author(), (Author::User, None));
    assert_eq!(author_resets(&live.h), 0);
    assert_eq!(live.person_wire(50.0).author, Author::User);
}

// ============================================================================================
// Update maps (design §7.5): no rebuild per agent write beyond what a person's write costs
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn an_agent_write_costs_the_panels_no_more_than_a_persons_and_a_person_edit_leaves_the_banner() {
    let _l = lock();
    let mut live = Live::new("write");
    let mut a = Agent::start(&mut live, AGENT, true);
    a.wire(&mut live, 0.0);
    for _ in 0..5 {
        live.pump();
    }
    let counts = |live: &Live| {
        let t = live.h.state();
        (t.panel.rebuilds, t.panel.history.builds, t.props_rebuilds)
    };
    let c0 = counts(&live);
    for y in [5.0, 10.0, 15.0] {
        a.wire(&mut live, y);
        for _ in 0..3 {
            live.pump();
        }
    }
    let c1 = counts(&live);
    for y in [20.0, 25.0, 30.0] {
        live.person_wire(y);
        for _ in 0..3 {
            live.pump();
        }
    }
    let c2 = counts(&live);
    let agent = (c1.0 - c0.0, c1.1 - c0.1, c1.2 - c0.2);
    let person = (c2.0 - c1.0, c2.1 - c1.1, c2.2 - c1.2);
    assert!(agent.0 <= person.0 && agent.1 <= person.1 && agent.2 <= person.2, "agent {agent:?} vs person {person:?}");
    assert!(agent.0 <= 3 && agent.1 <= 3, "at most one rebuild per write: {agent:?}");
    // The person's edits changed nothing in the banner.
    let b_before = banner(&live.h).unwrap();
    live.person_wire(35.0);
    live.pump();
    let b_after = banner(&live.h).unwrap();
    assert_eq!(b_before.steps, b_after.steps);
    assert_eq!(b_before.last_change.map(|c| c.seq), b_after.last_change.map(|c| c.seq));
    // Idle frames do not rebuild.
    let c3 = counts(&live);
    for _ in 0..20 {
        live.pump();
    }
    assert_eq!(counts(&live), c3, "idle frames rebuild nothing");
    a.close(Some(&mut live));
}

// ============================================================================================
// The preference (design §3) and the ECAD Dev row
// ============================================================================================

#[test]
#[ignore = "tester 035: awaiting build"]
fn the_preference_is_written_once_and_read_back() {
    let _l = lock();
    let live = Live::new("read");
    let port = live.port;
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(access_file()).unwrap()).unwrap();
    assert_eq!(saved, json!({"format": "ecad-agent-access/1", "mode": "read", "port": port}));
    // A second window reads it (its listener finds the port taken: the first window has it).
    let mut h2 = harness_with(None);
    let r = agentaccess(&mut h2, json!({}));
    assert_eq!(r["mode"], json!("read"), "{r}");
    assert_eq!(r["port"], json!(port), "{r}");
    assert!(r["status"].as_str().unwrap().contains(&format!("Port {port} is in use by another program; agent access is off in this window")), "{r}");
    h2.state_mut().on_exit();
    drop(live);
    assert!(
        !access_file().exists() || serde_json::from_str::<Value>(&std::fs::read_to_string(access_file()).unwrap()).unwrap()["mode"] == json!("off")
    );
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn an_unreadable_preference_reads_as_off_with_one_line() {
    let _l = lock();
    let port = free_port();
    let bad: Vec<Vec<u8>> = vec![
        b"not json".to_vec(),
        vec![0xff, 0xfe, 0x00],
        format!(r#"{{"format":"ecad-agent-access/2","mode":"write","port":{port}}}"#).into_bytes(),
        format!(r#"{{"format":"ecad-agent-access/1","mode":"admin","port":{port}}}"#).into_bytes(),
        br#"{"format":"ecad-agent-access/1","mode":"write","port":80}"#.to_vec(),
        br#"{"format":"ecad-agent-access/1","mode":"write","port":70000}"#.to_vec(),
        br#"{"format":"ecad-agent-access/1","mode":"write","port":"47823"}"#.to_vec(),
        format!(r#"{{"format":"ecad-agent-access/1","mode":"write","port":{port},"pad":"{}"}}"#, "x".repeat(5000)).into_bytes(),
    ];
    for (i, bytes) in bad.iter().enumerate() {
        let f = access_file();
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, bytes).unwrap();
        let mut h = harness_with(None);
        h.run_steps(5);
        let r = agentaccess(&mut h, json!({}));
        assert_eq!(r["mode"], json!("off"), "case {i}: {r}");
        assert_eq!(r["listening"], json!(false), "case {i}: {r}");
        assert!(!listening(port), "case {i}: nothing listens");
        let log = &h.state().cad.session.log;
        assert_eq!(
            log.iter().filter(|l| l.as_str() == "Agent access settings could not be read; agent access is off.").count(),
            1,
            "case {i}: one line"
        );
        h.state_mut().on_exit();
    }
    let _ = std::fs::remove_file(access_file());
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn the_ecad_dev_row_shows_mode_port_and_status() {
    let _l = lock();
    let _ = std::fs::remove_file(access_file());
    let mut h = harness_with(None);
    // ECAD Dev tab of Tool Sets (GUESS: opened through the Developer section's header).
    if h.query_by_label("agent.access.mode").is_none() {
        h.get_by_label(ecad_trial::developer::HEADER).click();
        h.run_steps(3);
    }
    let status = |h: &H| {
        let n = h.get_by_label("agent.access.status");
        n.value().unwrap_or_else(|| n.label().unwrap_or_default())
    };
    assert_eq!(status(&h), "Off");
    let port = free_port();
    agentaccess(&mut h, json!({"mode": "write", "port": port}));
    h.run_steps(3);
    assert!(status(&h).contains(&format!("Listening on 127.0.0.1:{port}")), "{}", status(&h));
    // An invalid port: warning under the field, old port kept.
    h.get_by_label("agent.access.port").focus();
    h.run_steps(1);
    h.key_press_modifiers(eframe::egui::Modifiers::COMMAND, eframe::egui::Key::A);
    h.get_by_label("agent.access.port").type_text("80");
    h.key_press(eframe::egui::Key::Enter);
    h.run_steps(3);
    assert!(h.query_by_label_contains("1024–65535").is_some(), "the range is shown");
    assert_eq!(agentaccess(&mut h, json!({}))["port"], json!(port), "the old port is kept");
    // Port in use: a second window on a port someone else holds.
    let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let busy = held.local_addr().unwrap().port();
    agentaccess(&mut h, json!({"mode": "off"}));
    h.state_mut().on_exit();
    let mut h2 = harness_with(None);
    agentaccess(&mut h2, json!({"mode": "write", "port": busy}));
    h2.run_steps(5);
    if h2.query_by_label("agent.access.status").is_none() {
        h2.get_by_label(ecad_trial::developer::HEADER).click();
        h2.run_steps(3);
    }
    assert!(status(&h2).contains(&format!("Port {busy} is in use by another program; agent access is off in this window")), "{}", status(&h2));
    agentaccess(&mut h2, json!({"mode": "off"}));
    h2.state_mut().on_exit();
    drop(held);
    let _ = std::fs::remove_file(access_file());
}

#[test]
#[ignore = "tester 035: awaiting build"]
fn control_flag_parses_and_writes_nothing() {
    // GUESS: `Invocation::Run` gains `control: Option<u16>`.
    let ok = ecad_trial::parse_args(["--control", "47823"].map(std::ffi::OsString::from)).unwrap();
    assert_eq!(ok, ecad_trial::Invocation::Run { snapshots: None, project: None, control: Some(47823) });
    for bad in [&["--control"][..], &["--control", "x"][..], &["--control", "70000"][..]] {
        assert!(ecad_trial::parse_args(bad.iter().map(std::ffi::OsString::from)).is_err(), "{bad:?}");
    }
    assert!(ecad_trial::USAGE.contains("--control"));
}
