//! `ecad-mcp`: ECAD over MCP on stdin and stdout, headless (brief 011).
//!
//! Usage: `ecad-mcp [--write] [--agent <name>] [--create] [<project.ecad>]`. Logs go to stderr
//! only; stdout carries JSON-RPC. See `ecad/docs/quick-start.md`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::process::ExitCode;

use ecad_mcp::EcadMcp;
use ecad_mcp::args::{Invocation, USAGE, parse_args};

fn main() -> ExitCode {
    let (project, create, options) = match parse_args(std::env::args_os().skip(1)) {
        Ok(Invocation::Serve { project, create, options }) => (project, create, options),
        Ok(Invocation::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Version) => {
            println!("ecad-mcp {} (CADCraft {})", env!("CARGO_PKG_VERSION"), ecad_cadcraft::UPSTREAM_COMMIT);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let opened = match (&project, create) {
        (Some(p), true) => EcadMcp::create(p, options).map(|s| (s, format!("Created {p}."))),
        (Some(p), false) => EcadMcp::open(p, options),
        (None, _) => EcadMcp::in_memory("Untitled", ecad_cadcraft::session_seed(), options)
            .map(|s| (s, "A new project in memory; it cannot be saved.".to_string())),
    };
    let mut server = match opened {
        Ok((s, summary)) => {
            eprintln!("ecad-mcp: {summary}");
            s
        }
        Err(e) => {
            eprintln!("ecad-mcp: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("ecad-mcp: writes {}", if server.writes() { "on" } else { "off" });
    match server.serve(std::io::stdin().lock(), std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ecad-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
