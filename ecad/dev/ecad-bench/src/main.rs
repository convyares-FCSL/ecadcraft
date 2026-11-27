//! `ecad-bench`: the project load benchmark. Run it through `cargo xtask bench`, which builds it
//! in release; see the crate documentation and `ecad/docs/user/quick-start.md`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::process::ExitCode;

fn main() -> ExitCode {
    // Verify mode: the model reads no environment, so the program passes the switch in.
    ecad_model::document::ledger::set_verify_invariants(ecad_model::document::ledger::verify_switch_on(
        std::env::var_os("ECAD_VERIFY_INVARIANTS").as_deref(),
    ));
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The version 2 crash child: save one edit and exit without closing the file.
    if let [flag, file] = args.as_slice()
        && flag == "--crash-save"
    {
        if let Err(e) = ecad_bench::container::crash_save(std::path::Path::new(file)) {
            eprintln!("{e}");
        }
        return ExitCode::FAILURE;
    }
    if let [flag, file, n] = args.as_slice()
        && flag == "--growth"
    {
        let r = n.parse().map_err(|e| format!("{e}")).and_then(|n| ecad_bench::container::growth(std::path::Path::new(file), n));
        return match r {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Only the connector rows (connplace, connpins, connside, pinlabel) on one version 2
    // file.
    // `--write <tier> <lean|churned> <file>`: write one generated project, to time a part on.
    if let [flag, name, variant, file] = args.as_slice()
        && flag == "--write"
    {
        let r = ecad_bench::tier(name).ok_or_else(|| format!("unknown tier {name}")).and_then(|t| {
            let v = match variant.as_str() {
                "lean" => ecad_bench::Variant::Lean,
                "churned" => ecad_bench::Variant::Churned,
                "rich" => return ecad_bench::rich::write_rich(t, std::path::Path::new(file)).map_err(|e| e.to_string()),
                other => return Err(format!("unknown variant {other}")),
            };
            ecad_bench::container::write(&ecad_bench::generate(t, v)?, std::path::Path::new(file))
        });
        return match r {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // `--part <measure|reports|strips|text|wires|plc|release> <file>`: one group of rows on a written
    // project, printed as it is measured; the full table needs a tier run.
    if let [flag, part, file] = args.as_slice()
        && flag == "--part"
    {
        return match ecad_bench::measure_part(part, std::path::Path::new(file)) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // `--views <file>`: the full rebuild of each kept view on a written project.
    if let [flag, file] = args.as_slice()
        && flag == "--views"
    {
        return match ecad_bench::views::measure(std::path::Path::new(file), ecad_bench::measure::RUNS) {
            Ok(rows) => {
                for r in rows {
                    println!("{:<32} {:>9.2} ms", r.view, r.ms);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
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
    // Only the cable manager rows on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--cables"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-cables-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::cables::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "cablenew {:.2} ms, cableedit {:.2} ms, cableassign {:.2} ms, cableslash {:.2} ms ({} cables)",
                    t.cablenew_ms, t.cableedit_ms, t.cableassign_ms, t.cableslash_ms, t.cables
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Only the rating rows on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--ratings"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-ratings-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::ratings::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "rated {}, device_rating {:.2} ms (re-checked {}, re-traced {}), wire_on_net {:.2} ms, panel_default {:.1} ms, checks_build {:.1} ms",
                    t.rated, t.device_rating_ms, t.device_evaluated, t.device_retraced, t.wire_on_net_ms, t.panel_default_ms, t.checks_build_ms
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Only the title block rows on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--title-blocks"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-title-blocks-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::title_blocks::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "house {:.2} ms, own300 {:.2} ms, save {:.1} ms ({} frames worked out), unread key {} frames worked out ({} pages)",
                    t.house_ms, t.own300_ms, t.save_ms, t.save_builds, t.unread_builds, t.pages
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Only the page type and cover rows on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--pages"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-pages-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::pages::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "newpage {:.1} ms, renumber {:.1} ms, typesheet {:.1} ms, coverfirst {:.1} ms, coverupd {:.1} ms, wirecat {:.1} ms ({} pages)",
                    t.newpage_ms, t.renumber_ms, t.typesheet_ms, t.coverfirst_ms, t.coverupd_ms, t.wirecat_ms, t.pages
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Only the style rows on one version 2 file.
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
    // Only the symbol rows (symbolnew, symbolimpact, symboledit, wiredraw) on one
    // version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--symbols"
    {
        let scratch = std::env::temp_dir().join(format!("ecad-bench-symbols-{}", std::process::id()));
        let r = std::fs::create_dir_all(&scratch)
            .map_err(|e| e.to_string())
            .and_then(|()| ecad_bench::symbols::measure(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        return match r {
            Ok(t) => {
                println!(
                    "symbolnew {:.2} ms, symbolimpact {:.2} ms, symboledit {:.1} ms, wiredraw {:.2} ms (baseline {:.2} ms) ({} copies on {} pages, 50 lineages)",
                    t.symbolnew_ms, t.symbolimpact_ms, t.symboledit_ms, t.wiredraw_ms, t.wiredraw_base_ms, t.copies, t.pages
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Only the text rows (textplace, textstyle, propredraw) on one version 2 file.
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
    // Time Verify on one version 2 file.
    if let [flag, file] = args.as_slice()
        && flag == "--verify"
    {
        return match ecad_bench::container::verify_timed(std::path::Path::new(file)) {
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
        return match ecad_bench::container::probe(std::path::Path::new(file)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // `--refusal <file>`: what every generic command copies before it runs, to put back on a refusal.
    if let [flag, file] = args.as_slice()
        && flag == "--refusal"
    {
        let r = ecad_bench::measure::scratch_dir("refusal").and_then(|scratch| {
            let r = ecad_bench::per_edit::refusal_snapshot(std::path::Path::new(file), ecad_bench::measure::RUNS, &scratch);
            let _ = std::fs::remove_dir_all(&scratch);
            r
        });
        return match r {
            Ok(r) => {
                println!(
                    "{} devices: ids {:.1} µs, tag high-water {:.1} µs, designation uses copy {:.1} µs (journalled instead)",
                    r.devices,
                    r.ids_ms * 1000.0,
                    r.tag_high_ms * 1000.0,
                    r.designation_uses_ms * 1000.0
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // `--per-edit <file|new> <counts>`: setstyle, wire and category per edit as landed wires accumulate
    // (counts comma-separated, as 100,200,400).
    if let [flag, file, counts] = args.as_slice()
        && flag == "--per-edit"
    {
        let r =
            counts.split(',').map(|c| c.trim().parse::<usize>().map_err(|e| format!("{c}: {e}"))).collect::<Result<Vec<_>, _>>().and_then(|counts| {
                let scratch = ecad_bench::measure::scratch_dir("per-edit")?;
                let path = (file != "new").then(|| std::path::Path::new(file));
                let r = ecad_bench::per_edit::measure(path, &counts, &scratch);
                let _ = std::fs::remove_dir_all(&scratch);
                r
            });
        return match r {
            Ok(rows) => {
                println!("{:>6} {:>12} {:>9} {:>12}", "wires", "setstyle ms", "wire ms", "category ms");
                for r in rows {
                    println!("{:>6} {:>12.2} {:>9.2} {:>12.2}", r.wires, r.setstyle_ms, r.wire_ms, r.category_ms);
                }
                ExitCode::SUCCESS
            }
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
