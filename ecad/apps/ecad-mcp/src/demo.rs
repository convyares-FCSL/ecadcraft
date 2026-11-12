//! The demo project (`cargo xtask demo`, owner request of 2026-11-08): a small control panel
//! built from a checked-in list of commands (`ecad/examples/demo/commands.jsonl`) through the
//! same `execute` tool an MCP client uses, so the demo is rebuilt from source whenever the model or
//! the file format changes and never goes stale.
//!
//! # Semantics
//! One JSON object per line: `{"command": name, "params": {…}, "as": name?}`. Blank lines and
//! lines starting with `//` are skipped. A string parameter `$name.path` is replaced by a value of
//! an earlier result kept with `"as": name` (`$coil.component`, `$x2.terminals.0.terminal`,
//! `$cont.routes.1.conductor`); `$page1` is the first page's ID. Any refused command stops the
//! build with its line number and message.
//!
//! Excluded: anything a command cannot do (no direct edits of the model).

use serde_json::{Map, Value, json};

use crate::EcadMcp;

/// The checked-in command list.
pub const COMMANDS: &str = include_str!("../../../examples/demo/commands.jsonl");

/// The author the demo's transactions are recorded as.
pub const AUTHOR: &str = "ecad-demo";

/// Run `script` through `mcp`'s `execute` tool, as an MCP client named [`AUTHOR`]. Returns how many
/// commands ran.
///
/// # Errors
/// A malformed line, an unknown `$name`, or a refused command, with its line number.
pub fn run(mcp: &mut EcadMcp, script: &str) -> Result<usize, String> {
    let init = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": AUTHOR, "version": "1"}}});
    mcp.handle(init).ok_or("the server did not answer initialize")?;
    mcp.handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let page1 = mcp
        .with_session(|s, p| p.read(s, |p, _| p.ledger.document().page_order.first().map(|x| x.hex())))?
        .map_err(|e| e.to_string())?
        .ok_or("the project has no page")?;
    let mut vars: Map<String, Value> = Map::new();
    vars.insert("page1".into(), Value::String(page1));
    let mut ran = 0;
    for (n, line) in script.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        let step: Value = serde_json::from_str(line).map_err(|e| at(e.to_string()))?;
        let command = step.get("command").and_then(Value::as_str).ok_or_else(|| at("no `command`".into()))?;
        let params = substitute(step.get("params").cloned().unwrap_or(json!({})), &vars).map_err(at)?;
        let reply = mcp
            .handle(json!({"jsonrpc": "2.0", "id": n + 1, "method": "tools/call",
                "params": {"name": "execute", "arguments": {"command": command, "params": params}}}))
            .ok_or_else(|| at("no answer".into()))?;
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        if reply.get("error").is_some() || result["isError"] == json!(true) {
            return Err(at(format!("{command}: {}", if text.is_empty() { reply.to_string() } else { text.into() })));
        }
        if let Some(name) = step.get("as").and_then(Value::as_str) {
            let v: Value = serde_json::from_str(text).map_err(|e| at(format!("{command}'s result: {e}")))?;
            vars.insert(name.to_string(), v);
        }
        ran += 1;
    }
    Ok(ran)
}

/// Replace every `$name.path` string in `v` from `vars`.
fn substitute(v: Value, vars: &Map<String, Value>) -> Result<Value, String> {
    Ok(match v {
        Value::String(s) if s.starts_with('$') => lookup(&s, vars)?,
        Value::Array(a) => Value::Array(a.into_iter().map(|x| substitute(x, vars)).collect::<Result<_, _>>()?),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| Ok((k, substitute(x, vars)?))).collect::<Result<_, String>>()?),
        other => other,
    })
}

fn lookup(s: &str, vars: &Map<String, Value>) -> Result<Value, String> {
    let mut parts = s.trim_start_matches('$').split('.');
    let name = parts.next().unwrap_or_default();
    let mut v = vars.get(name).ok_or_else(|| format!("unknown {s}: no earlier result named {name}"))?;
    if name == "page1" {
        return Ok(v.clone());
    }
    for p in parts {
        v = match p.parse::<usize>() {
            Ok(i) => v.get(i),
            Err(_) => v.get(p),
        }
        .ok_or_else(|| format!("{s}: no `{p}` in that result"))?;
    }
    Ok(v.clone())
}
