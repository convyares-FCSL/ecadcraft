//! `ecadinspect`: read-only views of a version 2 file, and `ecadinspect serve` (see
//! [`ecad_inspector::cli::USAGE`]). A developer tool: it never ships.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    match ecad_inspector::cli::ecadinspect(&args, &mut out) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
