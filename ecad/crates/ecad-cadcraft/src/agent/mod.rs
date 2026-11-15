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

use ecad_model::Author;

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

/// Characters that change how text is shown without being visible: bidirectional overrides and
/// isolates, zero-width spaces and joiners, and the byte-order mark. An author name never holds
/// them, so a name cannot pose as another.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
    )
}

/// An author name as recorded: control, bidirectional and zero-width characters dropped, spaces
/// trimmed, at most [`MAX_AGENT_NAME`] characters; empty becomes [`UNNAMED_AGENT`].
pub fn agent_name(given: &str) -> String {
    let s: String = given.chars().filter(|c| !c.is_control() && !invisible(*c)).take(MAX_AGENT_NAME).collect();
    let s = s.trim();
    if s.is_empty() { UNNAMED_AGENT.to_string() } else { s.to_string() }
}

/// How a transaction's author is shown (as the debugger shows it).
pub fn author_text(a: &Author) -> String {
    match a {
        Author::User => "user".into(),
        Author::Agent(n) => format!("agent {n}"),
    }
}

/// Said when the project cannot be read to check a call; the call is refused (fail closed).
pub const UNREADABLE: &str = "Refused: the project could not be checked just now (it is busy). Nothing was changed; try again.";
