//! Brief 035 on the server side: `--connect` and `--launch` on the command line, a `--connect`
//! server with no window (headless, `ecad_connection`, `screenshot`), and `agentaccess` refused
//! to agents in every mode (MCP contract test of the new command).

use std::ffi::OsString;

use ecad_mcp::args::{Invocation, parse_args};
use ecad_mcp::{EcadMcp, Options};
use serde_json::{Value, json};

fn args(a: &[&str]) -> Result<Invocation, String> {
    parse_args(a.iter().map(OsString::from))
}

#[test]
fn connect_and_launch_parse_or_are_refused_in_words() {
    match args(&["--connect", "47823", "--launch", "--write"]).unwrap() {
        Invocation::Serve { connect, launch, options, .. } => {
            assert_eq!(connect, Some(47823));
            assert!(launch);
            assert!(options.write);
        }
        other => panic!("{other:?}"),
    }
    assert!(args(&["--connect"]).unwrap_err().contains("1024 to 65535"));
    assert!(args(&["--connect", "80"]).unwrap_err().contains("1024 to 65535"));
    assert!(args(&["--connect", "x"]).unwrap_err().contains("1024 to 65535"));
    assert!(args(&["--launch"]).unwrap_err().contains("--launch needs --connect"));
}

fn call(s: &mut EcadMcp, id: u64, tool: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": tool, "arguments": arguments}});
    serde_json::from_str(&s.handle_line(&line.to_string()).unwrap()).unwrap()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

#[test]
fn with_no_window_a_connect_server_is_headless_and_says_so() {
    let port = free_port();
    let mut s = EcadMcp::connect(port, false, ecad_mcp::opener(None, false, true), Options { write: true, agent: Some("tester".into()) }).unwrap();
    let init = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "x", "version": "1"}}});
    let r: Value = serde_json::from_str(&s.handle_line(&init.to_string()).unwrap()).unwrap();
    assert_eq!(r["result"]["serverInfo"]["title"], json!("ECAD"));
    assert!(r["result"]["instructions"].as_str().unwrap().contains("Call ecad_connection to know which"));
    let tools: Value = serde_json::from_str(&s.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap()).unwrap();
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"ecad_connection") && names.contains(&"screenshot"), "{names:?}");
    let c = call(&mut s, 2, "ecad_connection", json!({}));
    let v: Value = serde_json::from_str(c["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(v["mode"], json!("headless"));
    assert_eq!(v["port"], json!(port));
    assert_eq!(v["agent"], json!("tester"));
    let shot = call(&mut s, 3, "screenshot", json!({}));
    assert_eq!(shot["result"]["content"][0]["text"], json!(ecad_mcp::tools::NO_WINDOW));
    // A change headless sticks: the server never bridges after it.
    let w = call(&mut s, 4, "execute", json!({"command": "wire", "params": {"points": [[0, 0], [10, 0]]}}));
    assert_ne!(w["result"]["isError"], json!(true), "{w}");
    assert_eq!(s.mode(), Some(ecad_mcp::bridge::Mode::Headless { stuck: true }));
}

#[test]
fn agentaccess_is_refused_to_agents_with_and_without_write() {
    for write in [true, false] {
        let mut s = EcadMcp::in_memory("aa", 35, Options { write, agent: Some("tester".into()) }).unwrap();
        let before = s.with_session(|s, p| p.read(s, |p, _| p.ledger.log().len()).unwrap()).unwrap();
        for params in [json!({}), json!({"mode": "write"}), json!({"port": 50000})] {
            let r = call(&mut s, 1, "execute", json!({"command": "agentaccess", "params": params}));
            assert_eq!(r["result"]["isError"], json!(true), "{r}");
            assert_eq!(r["result"]["content"][0]["text"], json!(ecad_mcp::policy::AGENT_ACCESS), "write {write}");
        }
        let log = s.with_session(|s, p| p.read(s, |p, _| p.ledger.log().len()).unwrap()).unwrap();
        assert_eq!(log, before, "nothing recorded");
    }
}
