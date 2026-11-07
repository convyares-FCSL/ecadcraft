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
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod generate;
pub mod measure;
pub mod report;

use std::path::{Path, PathBuf};

pub use generate::{Generated, TIERS, Tier, Variant, generate, tier};

/// What `cargo xtask bench` accepts.
pub const USAGE: &str = "usage: cargo xtask bench [--tier small|medium|large|all|<list>] [--keep <folder>] \
                         [--compare <earlier.json>]\n  --tier     tiers to measure, comma-separated (default \
                         small,medium)\n  --keep     write the generated projects to <folder> and keep them\n  \
                         --compare  print the change per row against an earlier results file";

/// Parsed options.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    pub tiers: Vec<Tier>,
    pub keep: Option<PathBuf>,
    pub compare: Option<PathBuf>,
}

/// Parse the command-line options.
///
/// # Errors
/// An unknown option or tier, or a missing value.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut tiers = "small,medium".to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{a} needs a value\n{USAGE}"));
        match a.as_str() {
            "--tier" | "--tiers" => tiers = value()?,
            "--keep" => o.keep = Some(PathBuf::from(value()?)),
            "--compare" => o.compare = Some(PathBuf::from(value()?)),
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    for name in tiers.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if name == "all" {
            o.tiers = TIERS.to_vec();
            break;
        }
        let t = tier(name).ok_or_else(|| format!("unknown tier {name}\n{USAGE}"))?;
        if !o.tiers.contains(&t) {
            o.tiers.push(t);
        }
    }
    if o.tiers.is_empty() {
        return Err(format!("no tier given\n{USAGE}"));
    }
    Ok(o)
}

/// The ECAD workspace root (`ecad/`).
pub fn ecad_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Generate, write and measure one tier, lean then churned; checks that both replay to the same
/// document.
///
/// # Errors
/// Generation, writing or a measurement failed, or the two histories replay differently.
pub fn run_tier(tier: Tier, folder: &Path, scratch: &Path, runs: usize) -> Result<Vec<report::Row>, String> {
    let mut rows = Vec::new();
    let mut docs = Vec::new();
    for variant in Variant::BOTH {
        eprintln!("{} {}: generating…", tier.name, variant.name());
        let g = generate(tier, variant)?;
        let path = folder.join(format!("{}-{}.ecad", tier.name, variant.name()));
        measure::remove(&path);
        let bytes = g.file_bytes()?;
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        drop(g);
        eprintln!(
            "{} {}: measuring {} ({} runs of each; a large project takes a long time)…",
            tier.name,
            variant.name(),
            path.display(),
            runs
        );
        let (measured, doc) = measure::measure(&path, runs, scratch)?;
        rows.push(report::Row { tier: tier.name.into(), variant: variant.name().into(), measured });
        docs.push(doc);
    }
    if let [lean, churned] = docs.as_slice()
        && lean != churned
    {
        return Err(format!("tier {}: the lean and churned projects replay to different documents", tier.name));
    }
    Ok(rows)
}

/// The whole benchmark: what `cargo xtask bench` runs.
///
/// # Errors
/// Bad options, or any step failed.
pub fn main_with(args: &[String]) -> Result<(), String> {
    let o = parse(args)?;
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
            rows.extend(run_tier(*t, &folder, &scratch, measure::RUNS)?);
        }
        Ok::<_, String>(rows)
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    let rows = result?;
    println!("{}", report::table(&rows, measure::RUNS));
    let (date, when) = report::now();
    let commit = report::commit(&root);
    let out_dir = root.join("target").join("bench");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let out = out_dir.join(format!("load-{date}-{commit}.json"));
    let value = report::json(&rows, measure::RUNS, &commit, &when);
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("Results written to {}", out.display());
    if let Some(k) = &o.keep {
        println!("Projects kept in {} (open them in the trial with OPEN).", k.display());
    }
    if let Some(c) = &o.compare {
        let text = std::fs::read_to_string(c).map_err(|e| format!("{}: {e}", c.display()))?;
        let earlier: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: not a results file: {e}", c.display()))?;
        println!("{}", report::compare(&rows, &earlier)?);
    }
    Ok(())
}
