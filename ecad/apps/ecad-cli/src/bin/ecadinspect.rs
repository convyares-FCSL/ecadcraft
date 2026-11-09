//! `ecadinspect <file> <view>`: read-only views of a version 2 file (see [`ecad_cli::USAGE`]).

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    match ecad_cli::ecadinspect(&args, &mut out) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
