//! The agent bridge against the real window: the MCP server (`ecad_mcp::EcadMcp`) and the app's window
//! in one process (these tests used ecad-app's dev-dependency on ecad-mcp until #53).

pub(crate) mod banner;
pub(crate) mod bridge;
pub(crate) mod bridge_acceptance;
pub(crate) mod common;
pub(crate) mod ui;
