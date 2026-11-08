//! `ecad-demo [<path.ecad>] [--force]`: write the demo project (`cargo xtask demo`), built from
//! `ecad/examples/demo/commands.jsonl` through the MCP `execute` tool. The default path is
//! `C:\dev\ecad-projects\demo.ecad` on Windows and `~/ecad-projects/demo.ecad` elsewhere. An
//! existing file is never overwritten without `--force`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use ecad_mcp::{EcadMcp, Options, demo};

#[cfg(windows)]
const DEFAULT: &str = r"C:\dev\ecad-projects\demo.ecad";
#[cfg(not(windows))]
const DEFAULT: &str = "~/ecad-projects/demo.ecad";

fn main() {
    if let Err(e) = run() {
        eprintln!("ecad-demo: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut path = None;
    let mut force = false;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--force" => force = true,
            "-h" | "--help" => {
                println!("usage: ecad-demo [<path.ecad>] [--force]   (default {DEFAULT})");
                return Ok(());
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other if path.is_none() => path = Some(other.to_string()),
            other => return Err(format!("only one path ({other:?})")),
        }
    }
    let typed = path.unwrap_or_else(|| DEFAULT.to_string());
    let path = ecad_cadcraft::normalize_with(&typed, ecad_cadcraft::home().as_deref())?;
    if std::path::Path::new(&path).exists() {
        if !force {
            return Err(format!("{path} exists; give --force to replace it (nothing was written)"));
        }
        std::fs::remove_file(&path).map_err(|e| format!("{path}: {e}"))?;
    }
    if let Some(dir) = std::path::Path::new(&path).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut mcp = EcadMcp::create(&path, Options { write: true, agent: Some(demo::AUTHOR.into()) })?;
    let n = demo::run(&mut mcp, demo::COMMANDS)?;
    let saved = mcp
        .handle(serde_json::json!({"jsonrpc": "2.0", "id": 9999, "method": "tools/call",
            "params": {"name": "save", "arguments": {}}}))
        .ok_or("no answer to save")?;
    if saved["result"]["isError"] == serde_json::json!(true) || saved.get("error").is_some() {
        return Err(format!("not saved: {saved}"));
    }
    println!("Demo project written: {path} ({n} commands).");
    Ok(())
}
