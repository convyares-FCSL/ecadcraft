//! `ecad-bench`: the project load benchmark. Run it through `cargo xtask bench`, which builds it
//! in release; see the crate documentation and `ecad/docs/quick-start.md`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The version 2 crash child (brief 021): save one edit and exit without closing the file.
    if let [flag, file] = args.as_slice()
        && flag == "--crash-save"
    {
        if let Err(e) = ecad_bench::v2::crash_save(std::path::Path::new(file)) {
            eprintln!("{e}");
        }
        return ExitCode::FAILURE;
    }
    match ecad_bench::main_with(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
