//! The agent gate: what an agent may run, the
//! per-call caps, the agent's author name, the ECAD read tools and the agent access preference.
//! The headless server (`ecad-mcp`) and the ECAD window's live bridge (the desktop app) both use it, on
//! whatever session and project they pass, so the same project state gives the same reply.
//!
//! - [`policy`]: the allowlist of commands an agent may run, which run with writes off or on, and
//!   the refusals.
//! - [`upstream`]: the reviewed CADCraft drawing commands offered to agents.
//! - [`gate`]: one `execute` call through the policy, its transaction cap and roll-back, and the
//!   agent as the author.
//! - [`tools`]: the ECAD tools (`ecad_project`, `ecad_history`, `ecad_inspect`, `save`).
//! - [`bridge`]: the words both ends of the live bridge share.
//! - [`access`]: the per-user agent access preference.
//!
//! Excluded: the MCP protocol and its server (`ecad-mcp`, over `cadcraft-mcp`), the bridge's
//! transport, launching the window, and anything a host shows.

pub mod access;
pub mod bridge;
pub mod gate;
pub mod policy;
pub mod tools;
pub(crate) mod upstream;

/// Longest request line, in bytes; a longer line is answered with an Invalid Request error and
/// skipped without being kept in memory.
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// Most messages in one JSON-RPC batch.
pub const MAX_BATCH: usize = 64;
/// Most transactions one tool call may record. One `execute` runs one command, which records at
/// most one; a call that records more is undone and refused.
pub(crate) const MAX_TRANSACTIONS_PER_CALL: usize = 1;
/// Longest agent name recorded as an author, in characters.
pub const MAX_AGENT_NAME: usize = 64;
/// The author name before (or without) a client name.
pub const UNNAMED_AGENT: &str = "unnamed MCP client";

/// CADCraft's tools that ECAD's server offers; the others are refused as unknown tools.
pub const CADCRAFT_TOOLS: &[&str] = &["execute", "list_commands", "inspect_drawing", "query_entities", "find_text", "render"];

/// Said when an agent asks for a CADCraft tool ECAD's server does not offer.
pub fn not_offered(name: &str) -> String {
    let why = match name {
        "command_line" | "script" | "cancel" => " ECAD changes go through `execute` only (one command with JSON parameters per call).",
        "new_drawing" | "open" => " The project is opened by the person who starts the server.",
        _ => "",
    };
    format!("unknown tool `{name}`.{why} `tools/list` shows the tools this server offers.")
}

/// An author name as recorded: hidden characters (control, bidirectional, zero-width: the one
/// rule in `ecad_model::naming::words`) dropped, so a name cannot pose as another; spaces trimmed,
/// at most [`MAX_AGENT_NAME`] characters; empty becomes [`UNNAMED_AGENT`].
pub fn agent_name(given: &str) -> String {
    let s: String = ecad_model::naming::text::strip_hidden(given).chars().take(MAX_AGENT_NAME).collect();
    let s = s.trim();
    if s.is_empty() { UNNAMED_AGENT.to_string() } else { s.to_string() }
}

/// Said when the project cannot be read to check a call; the call is refused (fail closed).
pub const UNREADABLE: &str = "Refused: the project could not be checked just now (it is busy). Nothing was changed; try again.";

/// A pathless screenshot's reply (a private file upstream) turned into the PNG inline, as `ui.render`
/// replies: reads the file back and removes it, so the app never touches the disk.
///
/// # Errors
/// An unreadable file gives `{"ok": false, "error": …}`.
#[must_use]
pub fn inline_screenshot(v: serde_json::Value) -> serde_json::Value {
    use serde_json::{Value, json};
    let Some(path) = v.pointer("/result/path").and_then(Value::as_str).map(str::to_string) else { return v };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return json!({"ok": false, "error": format!("the screenshot could not be read back: {e}")}),
    };
    let _ = std::fs::remove_file(&path);
    let w = v.pointer("/result/width").cloned().unwrap_or(Value::Null);
    let h = v.pointer("/result/height").cloned().unwrap_or(Value::Null);
    json!({"ok": true, "result": {"pngBase64": crate::export::printbook::base64(&bytes), "width": w, "height": h, "bytes": bytes.len()}})
}
