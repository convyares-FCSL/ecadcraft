//! `ecad-bench`: the project load benchmark. Run it through `cargo xtask bench`, which builds it
//! in release; see the crate documentation and `ecad/docs/quick-start.md`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match ecad_bench::main_with(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
