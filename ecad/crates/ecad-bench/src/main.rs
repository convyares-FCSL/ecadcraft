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
    // Brief 044: only the connector rows (connplace, connpins, connside, pinlabel) on one version 2
    // file.
    if let [flag, file] = args.as_slice()
        && flag == "--connectors"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-connectors-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::connectors::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "connplace {:.2} ms, connpins {:.2} ms, connside {:.2} ms, pinlabel {:.2} ms ({} connectors on {} pages)",
                    t.connplace_ms, t.connpins_ms, t.connside_ms, t.pinlabel_ms, t.connectors, t.pages
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Brief 050: only the style rows (design 050 §11) on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--styles"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-styles-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::styles::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "style_tag {:.1} ms, textstyle {:.1} ms, propredraw {:.1} ms, standard_remove {:.1} ms, open {:.0} ms, open_styled {:.0} ms, wire_category {:.1} ms, category_net {:.1} ms, potential_net {:.1} ms",
                    t.style_tag_ms,
                    t.textstyle_ms,
                    t.propredraw_ms,
                    t.standard_remove_ms,
                    t.open_ms,
                    t.open_styled_ms,
                    t.wire_category_ms,
                    t.category_net_ms,
                    t.potential_net_ms
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Brief 031: only the text rows (textplace, textstyle, propredraw) on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--text"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-text-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::text::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "textplace {:.2} ms, textstyle {:.1} ms, propredraw {:.1} ms (densest page: {} symbols, {} texts)",
                    t.textplace_ms, t.textstyle_ms, t.propredraw_ms, t.symbols, t.texts
                );
                ExitCode::SUCCESS
            }
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
