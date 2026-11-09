//! `ecad`: `ecad upgrade <file>` and `ecad inspect <file> <view>` (see [`ecad_cli::USAGE`]).

fn main() -> std::process::ExitCode {
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
