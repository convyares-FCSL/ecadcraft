//! The agent gate (brief 011, brief 035; design 065 §3, cut 12): what an agent may run, the
//! per-call caps, the agent's author name, the ECAD read tools and the agent access preference.
//! The headless server (`ecad-mcp`) and the ECAD window's live bridge (the desktop app) both use it, on
//! whatever session and project they pass, so the same project state gives the same reply.
//!
//! - [`policy`]: which commands run with writes off or on, and the refusals.
//! - [`gate`]: one `execute` call through the policy, its transaction cap and roll-back, and the
//!   agent as the author.
//! - [`tools`]: the ECAD tools (`ecad_project`, `ecad_history`, `ecad_inspect`, `save`).
//! - [`bridge`]: the words both ends of the live bridge share.
//! - [`access`]: the per-user agent access preference.
//!
//! Excluded: the MCP protocol and its server (`ecad-mcp`, over `cadcraft-mcp`), the bridge's
//! transport, launching the window, and anything a host shows.

#[cfg(not(target_arch = "wasm32"))]
pub mod access;
pub mod bridge;
pub mod gate;
pub mod policy;
pub mod tools;

/// Longest request line, in bytes; a longer line is answered with an Invalid Request error and
/// skipped without being kept in memory.
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// Most messages in one JSON-RPC batch.
pub const MAX_BATCH: usize = 64;
/// Most transactions one tool call may record. One `execute` runs one command, which records at
/// most one; a call that records more is undone and refused.
pub const MAX_TRANSACTIONS_PER_CALL: usize = 1;
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
/// rule in `ecad_model::words`) dropped, so a name cannot pose as another; spaces trimmed,
/// at most [`MAX_AGENT_NAME`] characters; empty becomes [`UNNAMED_AGENT`].
pub fn agent_name(given: &str) -> String {
    let s: String = ecad_model::text::strip_hidden(given).chars().take(MAX_AGENT_NAME).collect();
    let s = s.trim();
    if s.is_empty() { UNNAMED_AGENT.to_string() } else { s.to_string() }
}

/// Said when the project cannot be read to check a call; the call is refused (fail closed).
pub const UNREADABLE: &str = "Refused: the project could not be checked just now (it is busy). Nothing was changed; try again.";
