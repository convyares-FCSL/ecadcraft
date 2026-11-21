//! `ecad-mcp`: ECAD over MCP on stdin and stdout, headless (brief 011).
//!
//! Usage: `ecad-mcp [--write] [--agent <name>] [--connect <port>] [--launch] [--create] [<project.ecad>]`. Logs go to stderr
//! only; stdout carries JSON-RPC. See `ecad/docs/quick-start.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::process::ExitCode;

use ecad_mcp::EcadMcp;
use ecad_mcp::args::{Invocation, USAGE, parse_args};

fn main() -> ExitCode {
    // Verify mode (brief 013): the model reads no environment, so the program passes the switch in.
    ecad_model::ledger::set_verify_invariants(ecad_model::ledger::verify_switch_on(std::env::var_os("ECAD_VERIFY_INVARIANTS").as_deref()));
    let (project, create, options, connect, launch) = match parse_args(std::env::args_os().skip(1)) {
        Ok(Invocation::Serve { project, create, options, connect, launch }) => (project, create, options, connect, launch),
        Ok(Invocation::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Version) => {
            println!("ecad-mcp {} (CADCraft {})", env!("CARGO_PKG_VERSION"), ecad_engine::UPSTREAM_COMMIT);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if let Some(port) = connect {
        // Brief 035: one server, headless until the window answers on the port. The headless
        // project is opened only at the first headless call, so a file the window has open is
        // never locked here first.
        if let Some(p) = &project {
            ecad_mcp::launch::set_project(p);
        }
        let write = options.write;
        let mut server = match EcadMcp::connect(port, launch, ecad_mcp::opener(project, create, write), options) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("ecad-mcp: {e}");
                return ExitCode::FAILURE;
            }
        };
        let writes = if write { "allowed" } else { "off" };
        eprintln!("ecad-mcp: live on 127.0.0.1:{port} when the ECAD window answers, headless until then; writes {writes}");
        return match server.serve(std::io::stdin().lock(), std::io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ecad-mcp: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let opened = match (&project, create) {
        (Some(p), true) => EcadMcp::create(p, options).map(|s| (s, format!("Created {p}."))),
        (Some(p), false) => EcadMcp::open(p, options),
        (None, _) => EcadMcp::in_memory("Untitled", ecad_engine::session_seed(), options)
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
