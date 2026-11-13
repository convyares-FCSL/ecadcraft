//! Tester 035, the `ecad-mcp` side without a window (brief 035 AC 11, 12, 13 partly; design 035
//! §3.3, §9, §11, §12 checks 10 and 12). Written from the brief and the design before the
//! implementation: names of the new API are guesses, gathered in [`connect_server`] and
//! [`launch`] so phase 2 adapts them in one place.
//!
//! The bridged checks (a real trial on the port) are in the trial's `tester_035_bridge` module:
//! `ecad-trial` depends on `ecad-mcp`, not the other way round.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ecad_mcp::args::{Invocation, USAGE, parse_args};
use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

/// A loopback port nothing listens on (bound, read, released).
pub(crate) fn free_port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    l.local_addr().unwrap().port()
}

fn os(args: &[&str]) -> Vec<std::ffi::OsString> {
    args.iter().map(Into::into).collect()
}

/// The server `main.rs` starts for `ecad-mcp --connect PORT [--write]` with no project (headless
/// opens an unsaved project when no window answers).
fn connect_server(port: u16, write: bool) -> EcadMcp {
    let p = port.to_string();
    let mut args = vec!["--connect", p.as_str()];
    if write {
        args.push("--write");
    }
    let Ok(Invocation::Serve { project, create, options, connect: Some(port), launch }) = parse_args(os(&args)) else { panic!("{args:?} parses") };
    assert!(project.is_none() && !create && !launch);
    let opener = ecad_mcp::opener(project, create, options.write);
    EcadMcp::connect(port, launch, opener, options).unwrap()
}

fn text(r: &Value) -> String {
    r["content"][0]["text"].as_str().unwrap_or_default().to_string()
}

// ---------- §9.1 command line ----------

#[test]
fn connect_and_launch_parse_and_launch_needs_connect() {
    let Ok(Invocation::Serve { options, connect, launch, .. }) = parse_args(os(&["--connect", "47823", "--write", "--launch"])) else { panic!() };
    assert!(options.write);
    // GUESS: the two new fields of `Options`.
    assert_eq!(connect, Some(47823));
    assert!(launch);
    let Ok(Invocation::Serve { options, connect, launch, .. }) = parse_args(os(&["--write"])) else { panic!() };
    assert!(options.write);
    assert_eq!(connect, None, "headless only unless --connect is given");
    assert!(!launch);
    for bad in [
        &["--launch"][..],
        &["--connect"][..],
        &["--connect", "abc"][..],
        &["--connect", "70000"][..],
        &["--connect", "-1"][..],
        &["--connect", ""][..],
    ] {
        assert!(parse_args(os(bad)).is_err(), "{bad:?} must be a usage error");
    }
    assert!(USAGE.contains("--connect") && USAGE.contains("--launch"), "{USAGE}");
}

// ---------- §3.3: an agent never touches agent access ----------

#[test]
fn agentaccess_is_refused_to_agents_headless() {
    assert!(policy::NEVER_COMMANDS.contains(&"agentaccess"));
    let mut c = Client::new(EcadMcp::in_memory("t035", 35, Options { write: true, ..Default::default() }).unwrap());
    let before = c.log();
    for params in [json!({}), json!({"mode": "write"}), json!({"mode": "write", "port": 47823}), json!({"port": 1})] {
        let m = c.refused("execute", json!({"command": "agentaccess", "params": params}));
        assert!(m.contains("Agent access is set by the person in the ECAD window."), "{params}: {m}");
        assert_eq!(c.log(), before, "{params}");
    }
    // Upper case and an alias spelling reach the same refusal.
    let m = c.refused("execute", json!({"command": "AGENTACCESS", "params": {"mode": "write"}}));
    assert!(m.contains("set by the person"), "{m}");
}

// ---------- §9.2: no window answers → headless, and it says so ----------

#[test]
fn with_no_window_the_connect_server_is_headless_and_says_so() {
    let port = free_port();
    let mut c = Client::new(connect_server(port, true));
    let tools = c.call("ecad_connection", json!({}));
    let conn: Value = serde_json::from_str(&text(&tools)).unwrap();
    assert_eq!(conn["mode"], json!("headless"), "{conn}");
    assert_eq!(conn["port"], json!(port), "{conn}");
    assert_eq!(conn["agent"], json!(AGENT), "{conn}");
    // A headless change records as the agent's (011 unchanged).
    c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
    assert_eq!(c.log().last().unwrap().author, Author::Agent(AGENT.into()));
    // screenshot is always listed; headless answers in words.
    let m = c.refused("screenshot", json!({}));
    assert!(m.contains("No ECAD window is connected; use render."), "{m}");
    // Still headless (it has recorded now, so it sticks).
    let conn: Value = serde_json::from_str(&text(&c.call("ecad_connection", json!({})))).unwrap();
    assert_eq!(conn["mode"], json!("headless"), "{conn}");
}

#[test]
fn initialize_and_tools_list_name_the_connection_tool() {
    let mut c = Client::new(connect_server(free_port(), false));
    let r =
        c.s.handle_line(
            &json!({"jsonrpc":"2.0","id":99,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"x"}}}).to_string(),
        )
        .unwrap();
    let r: Value = serde_json::from_str(&r).unwrap();
    assert_eq!(r["result"]["serverInfo"]["title"], json!("ECAD"), "{r}");
    let instr = r["result"]["instructions"].as_str().unwrap();
    assert!(instr.contains("Call ecad_connection to know which."), "{instr}");
    let list = c.s.handle_line(&json!({"jsonrpc":"2.0","id":100,"method":"tools/list"}).to_string()).unwrap();
    let list: Value = serde_json::from_str(&list).unwrap();
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    for want in ["ecad_connection", "screenshot", "ecad_project", "ecad_history", "execute", "save"] {
        assert!(names.contains(&want), "{want} missing: {names:?}");
    }
    for hidden in ["command_line", "script", "cancel", "new_drawing", "open", "ui_click"] {
        assert!(!names.contains(&hidden), "{hidden} offered: {names:?}");
    }
}

// ---------- hostile: a port that answers but is not ECAD ----------

#[test]
fn a_port_that_answers_garbage_never_hangs_or_panics_the_server() {
    // Something else holds the port: it accepts, writes junk, closes.
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in l.incoming().take(32).flatten() {
            let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n\xff\xfe{not json\n");
        }
    });
    let t0 = Instant::now();
    let mut c = Client::new(connect_server(port, true));
    let r = c.call("ecad_connection", json!({}));
    let r2 = c.call("execute", json!({"command": "wire", "params": {"points": [[0, 0], [10, 0]]}}));
    assert!(t0.elapsed() < Duration::from_secs(20), "answered in bounded time");
    // Either refused in words or served headless; never a protocol crash.
    assert!(r.is_object() && r2.is_object(), "{r} {r2}");
    if r2["isError"] == json!(true) {
        assert!(!text(&r2).is_empty());
    }
}

#[test]
fn an_oversized_line_is_capped_in_connect_mode_too() {
    let mut c = Client::new(connect_server(free_port(), true));
    let before = c.log();
    let big = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{{\"name\":\"execute\",\"arguments\":{{\"command\":\"wire\",\"params\":{{\"pad\":\"{}\"}}}}}}}}",
        "x".repeat(ecad_mcp::MAX_REQUEST_BYTES)
    );
    let r: Value = serde_json::from_str(&c.s.handle_line(&big).unwrap()).unwrap();
    assert_eq!(r["error"]["code"], json!(-32600), "{r}");
    let batch: Vec<Value> = (0..=ecad_mcp::MAX_BATCH as u64)
        .map(|i| json!({"jsonrpc":"2.0","id":i,"method":"tools/call","params":{"name":"ecad_connection","arguments":{}}}))
        .collect();
    let r: Value = serde_json::from_str(&c.s.handle_line(&Value::Array(batch).to_string()).unwrap()).unwrap();
    assert_eq!(r["error"]["code"], json!(-32600), "{r}");
    // Junk arguments to the new tool are answered, never a panic.
    for args in [json!(null), json!([1, 2]), json!({"mode": {"x": [1e308]}}), json!("\u{202E}")] {
        let r =
            c.s.handle_line(&json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"ecad_connection","arguments":args}}).to_string());
        assert!(r.is_some());
    }
    assert_eq!(c.log(), before);
}

// ---------- §9.3: --launch (unit) ----------

#[test]
fn launch_finds_the_trial_in_order_and_starts_it_with_the_port() {
    use ecad_mcp::launch;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    let exe = launch::trial_file_name();
    // Folders under this platform's root: a `C:` folder holds the PATH separator on Unix, where
    // `join_paths` refuses it.
    let root = if cfg!(windows) { PathBuf::from(r"C:\") } else { PathBuf::from("/") };
    let env_path = root.join("named by env").join(exe);
    let server = root.join("beside").join(if cfg!(windows) { "ecad-mcp.exe" } else { "ecad-mcp" });
    let beside = root.join("beside").join(exe);
    let on_path = root.join("on path").join(exe);
    let path_var = std::env::join_paths([root.join("nowhere"), root.join("on path")]).unwrap();
    let all = |p: &Path| p == env_path || p == beside || p == on_path;
    // ECAD_TRIAL first, then beside ecad-mcp, then PATH.
    assert_eq!(launch::find_trial(Some(env_path.clone().into()), Some(&server), Some(path_var.clone()), all), Some(env_path.clone()));
    assert_eq!(launch::find_trial(None, Some(&server), Some(path_var.clone()), all), Some(beside.clone()));
    assert_eq!(launch::find_trial(None, Some(&server), Some(path_var.clone()), |p: &Path| p == on_path), Some(on_path.clone()));
    assert_eq!(launch::find_trial(None, Some(&server), None, |_: &Path| false), None);
    // An ECAD_TRIAL that names nothing is not silently replaced by another copy.
    assert_eq!(launch::find_trial(Some(OsString::from(r"C:\missing\x.exe")), Some(&server), Some(path_var), all), None);
    assert_eq!(launch::NOT_FOUND, "--launch could not find ecad-trial; set ECAD_TRIAL to its path.");
    // The arguments: --control <port>, then the project when one was given.
    assert_eq!(launch::trial_args(47823, None), vec!["--control".to_string(), "47823".to_string()]);
    assert_eq!(launch::trial_args(5000, Some(r"C:\p q\a b.ecad")), vec!["--control".to_string(), "5000".into(), r"C:\p q\a b.ecad".into()]);
    // The command: null stdio (never the MCP stream).
    let cmd = launch::command(&beside, &launch::trial_args(47823, None));
    let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
    assert_eq!(args, ["--control", "47823"]);
    // Linux without a display: refused in words; Windows always has one.
    assert_eq!(launch::NO_DISPLAY, "No display for the ECAD window (DISPLAY/WAYLAND_DISPLAY unset); running headless is not possible with --launch.");
    assert_eq!(launch::has_display(None, None), cfg!(windows));
    assert!(launch::has_display(Some(":0".into()), None));
    assert!(launch::has_display(None, Some("wayland-0".into())));
    assert_eq!(launch::has_display(Some("".into()), Some("".into())), cfg!(windows));
}

// ---------- the binary over real stdio ----------

#[test]
fn stdio_connect_mode_answers_headless_and_exits_when_stdin_closes() {
    let settings = TempDir::new("t035 stdio");
    let port = free_port().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ecad-mcp"))
        .args(["--connect", &port, "--write"])
        .env("ECAD_SETTINGS_DIR", settings.file(""))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut ask = |line: Value| -> Value {
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
        let mut s = String::new();
        out.read_line(&mut s).unwrap();
        serde_json::from_str(&s).unwrap_or_else(|e| panic!("stdout carries JSON-RPC only: {s:?} ({e})"))
    };
    let init = ask(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":AGENT}}}));
    assert_eq!(init["result"]["serverInfo"]["title"], json!("ECAD"), "{init}");
    let r = ask(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ecad_connection","arguments":{}}}));
    let conn: Value = serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(conn["mode"], json!("headless"), "{conn}");
    drop(stdin);
    let t0 = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(5), "ecad-mcp exits when stdin closes");
        std::thread::sleep(Duration::from_millis(50));
    }
}
