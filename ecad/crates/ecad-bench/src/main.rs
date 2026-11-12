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
    if let [flag, file, n] = args.as_slice()
        && flag == "--growth"
    {
        let r = n.parse().map_err(|e| format!("{e}")).and_then(|n| ecad_bench::v2::growth(std::path::Path::new(file), n));
        return match r {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Brief 029: write one tier's lean version 2 file (for timing Verify on it alone).
    if let [flag, name, file] = args.as_slice()
        && flag == "--write-v2"
    {
        let r = ecad_bench::tier(name)
            .ok_or_else(|| format!("unknown tier {name}"))
            .and_then(|t| ecad_bench::v2::write_lean(t, std::path::Path::new(file)));
        return match r {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Brief 029: time Verify on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--verify"
    {
        return match ecad_bench::v2::verify_timed(std::path::Path::new(file)) {
            Ok((chain, all, checks)) => {
                println!("verify_chain {chain:.0} ms, verify {all:.0} ms, {checks} full checks");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    if let [flag, file] = args.as_slice()
        && flag == "--probe"
    {
        return match ecad_bench::v2::probe(std::path::Path::new(file)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    match ecad_bench::main_with(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
