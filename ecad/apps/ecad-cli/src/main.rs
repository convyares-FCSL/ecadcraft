//! `ecad` (see [`ecad_cli::USAGE`]).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

fn main() -> std::process::ExitCode {
    // Verify mode (brief 013): the model reads no environment, so the program passes the switch in.
    ecad_model::document::ledger::set_verify_invariants(ecad_model::document::ledger::verify_switch_on(std::env::var_os("ECAD_VERIFY_INVARIANTS").as_deref()));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    match ecad_cli::ecad(&args, &mut out) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
