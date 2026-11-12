//! ecad-bench
//!
//! The project load benchmark (brief 012), run on demand with `cargo xtask bench` (release
//! build). It generates synthetic projects ([`generate`]), writes them as `.ecad` files, times
//! opening, deriving, the first page and saving one change ([`measure`]), prints a table and
//! writes the results as JSON to `ecad/target/bench/` ([`report`]).
//!
//! It is **not** run by `cargo xtask ci`, `cargo test --workspace` or GitHub CI: only this
//! crate's small unit tests of the generator's shape are. It sets no budgets and has no pass or
//! fail beyond the projects opening cleanly.
//!
//! This crate intentionally contains no file format, op or document code of its own: projects
//! are made of existing ops and opened, derived, projected and saved by the product crates. The
//! generator does not use CADCraft; only the first-page timing does (through `ecad-cadcraft`).
//! Peak memory is not measured (it cannot be read cheaply on Windows without new dependencies).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod generate;
pub mod history;
pub mod measure;
pub mod outputs;
pub mod release;
pub mod report;
pub mod v2;

use std::path::{Path, PathBuf};

pub use generate::{Generated, PRINT_TIER, TIERS, Tier, Variant, generate, tier};

/// What `cargo xtask bench` accepts.
pub const USAGE: &str = "usage: cargo xtask bench [--tier small|medium|large|print|all|<list>] [--keep <folder>] \
                         [--compare <earlier.json>] [--runs N]\n  --tier     tiers to measure, comma-separated \
                         (default small,medium,print; print is the 25-page project Print Book and release are \
                         timed on)\n  --keep     write the generated projects to <folder> and keep \
                         them\n  --compare  print the change per row against an earlier results file\n  --runs     \
                         runs per measurement, the median is reported (default 5)";

/// Parsed options.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub tiers: Vec<Tier>,
    pub keep: Option<PathBuf>,
    pub compare: Option<PathBuf>,
    pub runs: usize,
    /// Measure Print Book and release on the 25-page project ([`PRINT_TIER`]).
    pub print: bool,
}

/// Parse the command-line options.
///
/// # Errors
/// An unknown option or tier, a missing or bad value.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options { tiers: Vec::new(), keep: None, compare: None, runs: measure::RUNS, print: false };
    let mut tiers = "small,medium,print".to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{a} needs a value\n{USAGE}"));
        match a.as_str() {
            "--tier" | "--tiers" => tiers = value()?,
            "--keep" => o.keep = Some(PathBuf::from(value()?)),
            "--compare" => o.compare = Some(PathBuf::from(value()?)),
            "--runs" => {
                let v = value()?;
                o.runs = v
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=measure::MAX_RUNS).contains(n))
                    .ok_or_else(|| format!("--runs must be a whole number from 1 to {}\n{USAGE}", measure::MAX_RUNS))?;
            }
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    for name in tiers.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if name == "all" {
            o.tiers = TIERS.to_vec();
            o.print = true;
            break;
        }
        if name == PRINT_TIER.name {
            o.print = true;
            continue;
        }
        let t = tier(name).ok_or_else(|| format!("unknown tier {name}\n{USAGE}"))?;
        if !o.tiers.contains(&t) {
            o.tiers.push(t);
        }
    }
    if o.tiers.is_empty() && !o.print {
        return Err(format!("no tier given\n{USAGE}"));
    }
    Ok(o)
}

/// The ECAD workspace root (`ecad/`).
pub fn ecad_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

/// Generate one tier, lean and churned, check that both histories end in the same document, write
/// both files to `folder`, then measure each.
///
/// # Errors
/// Generation fails, the two histories end differently (before anything is measured), a target
/// file is open in another session, or a measurement fails.
pub fn run_tier(tier: Tier, folder: &Path, scratch: &Path, runs: usize) -> Result<Vec<report::Row>, String> {
    let mut paths = Vec::new();
    {
        eprintln!("{}: generating the lean and churned projects…", tier.name);
        let lean = generate(tier, Variant::Lean)?;
        let churned = generate(tier, Variant::Churned)?;
        if lean.ledger.document() != churned.ledger.document() {
            return Err(format!("tier {}: the lean and churned histories end in different documents", tier.name));
        }
        for g in [&lean, &churned] {
            let path = folder.join(format!("{}-{}.ecad", tier.name, g.variant.name()));
            measure::claim(&path)?;
            let bytes = g.file_bytes()?;
            std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            paths.push((g.variant, path));
        }
    }
    let mut rows = Vec::new();
    for (variant, path) in paths {
        // The app opens version 2 files (brief 021): convert first (timed), the converter's checks
        // included.
        let v2_path = path.with_file_name(format!("{}-{}.v2.ecad", tier.name, variant.name()));
        measure::claim(&v2_path)?;
        eprintln!("{} {}: converting to {}…", tier.name, variant.name(), v2_path.display());
        let convert_ms = v2::convert(&path, &v2_path)?;
        eprintln!("{} {}: measuring {} ({runs} runs of each)…", tier.name, variant.name(), path.display());
        let measured = measure::measure_with(&path, &v2_path, runs, scratch);
        // The opens leave an empty lock file beside a kept project: remove it once released.
        measure::release_lock_file(&path);
        measure::release_lock_file(&v2_path);
        let (measured, _) = measured?;
        eprintln!("{} {}: measuring version 2…", tier.name, variant.name());
        let v2m = v2::measure(&v2_path, runs, scratch, convert_ms);
        measure::release_lock_file(&v2_path);
        let compression = v2::compression(&v2_path)?;
        eprintln!("{} {}: measuring history and branches…", tier.name, variant.name());
        let hist = history::measure(&v2_path, runs, scratch);
        measure::release_lock_file(&v2_path);
        eprintln!("{} {}: measuring locations, reports and printing…", tier.name, variant.name());
        let outs = outputs::measure(&v2_path, runs, scratch);
        measure::release_lock_file(&v2_path);
        eprintln!("{} {}: measuring the pre-release check and the release…", tier.name, variant.name());
        let rel = release::measure(&v2_path, runs, scratch);
        measure::release_lock_file(&v2_path);
        rows.push(report::Row {
            tier: tier.name.into(),
            variant: variant.name().into(),
            measured,
            v2: Some(v2m?),
            compression: Some(compression),
            history: Some(hist?),
            outputs: Some(outs?),
            release: Some(rel?),
        });
    }
    Ok(rows)
}

/// Generate the 25-page project (lean), convert it, and time Print Book and release on it.
///
/// # Errors
/// Generation, conversion or a measurement failed.
pub fn measure_print(folder: &Path, scratch: &Path, runs: usize) -> Result<release::MeasuredPrint, String> {
    eprintln!("{}: generating the 25-page project…", PRINT_TIER.name);
    let g = generate(PRINT_TIER, Variant::Lean)?;
    let path = folder.join(format!("{}-{}.ecad", PRINT_TIER.name, Variant::Lean.name()));
    measure::claim(&path)?;
    std::fs::write(&path, g.file_bytes()?).map_err(|e| format!("{}: {e}", path.display()))?;
    let v2_path = path.with_file_name(format!("{}-{}.v2.ecad", PRINT_TIER.name, Variant::Lean.name()));
    measure::claim(&v2_path)?;
    v2::convert(&path, &v2_path)?;
    eprintln!("{}: measuring Print Book and release ({runs} runs)…", PRINT_TIER.name);
    let m = release::measure_print(&v2_path, runs, scratch);
    measure::release_lock_file(&v2_path);
    m
}

/// The whole benchmark: what `cargo xtask bench` runs.
///
/// # Errors
/// Bad options, an unreadable `--compare` file, or any step failed.
pub fn main_with(args: &[String]) -> Result<(), String> {
    let o = parse(args)?;
    // Read the earlier results first, so a bad path fails before a long run.
    let earlier = match &o.compare {
        Some(c) => {
            let text = std::fs::read_to_string(c).map_err(|e| format!("{}: {e}", c.display()))?;
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: not a results file: {e}", c.display()))?;
            if v.get("rows").and_then(serde_json::Value::as_array).is_none() {
                return Err(format!("{}: not a results file (no `rows`)", c.display()));
            }
            Some(v)
        }
        None => None,
    };
    let root = ecad_root();
    let scratch = measure::scratch_dir("work")?;
    let folder = match &o.keep {
        Some(k) => {
            std::fs::create_dir_all(k).map_err(|e| format!("{}: {e}", k.display()))?;
            k.clone()
        }
        None => scratch.clone(),
    };
    let result = (|| {
        let mut rows = Vec::new();
        for t in &o.tiers {
            rows.extend(run_tier(*t, &folder, &scratch, o.runs)?);
        }
        let print = if o.print { Some(measure_print(&folder, &scratch, o.runs)?) } else { None };
        Ok::<_, String>((rows, print))
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    let (rows, print) = result?;
    println!("{}", report::table(&rows, o.runs, print.as_ref()));
    let commit = report::commit(&root);
    let (stem, when) = report::stem_and_time(&commit);
    let out_dir = root.join("target").join("bench");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    // Never overwrite an earlier run, which may be the one being compared with.
    let out = report::unused_path(&out_dir, &stem);
    let value = report::json(&rows, o.runs, &commit, &when, print.as_ref());
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("Results written to {}", out.display());
    if let Some(k) = &o.keep {
        println!("Projects kept in {} (open them in the trial with Open project…).", k.display());
    }
    if let Some(earlier) = &earlier {
        println!("{}", report::compare(&rows, earlier)?);
    }
    Ok(())
}
