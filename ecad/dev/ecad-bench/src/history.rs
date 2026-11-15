//! History and branch timings (brief 020a, design 020 §8.4), through the app's project on a copy
//! of the version 2 file, with a history of 5 branches and 20 versions added first.
//!
//! # Semantics
//! - **histshow**: the History graph built and laid out, as on the tab's first show.
//! - **histupd**: a commit that names a version, then the graph read again (the update map's
//!   costliest case; an ordinary commit only relabels the Now row).
//! - **histsel**: a version selected: its details and change summary.
//! - **pointshow**: a version shown read-only (its snapshot loaded); **pointstep**: a step up to
//!   1 999 past the nearest snapshot shown (that snapshot and the replay after it).
//! - **brswitch**: switching to another branch and back, each saved first (nothing unsaved), as
//!   opening does; the median of each switch.
//! - **restore**: a version 500 steps back restored: the plan, one transaction, the page redrawn.
//! - **mergeprev** and **merge** (brief 020b): a branch with 20 device changes, main 2 000 steps
//!   ahead of the fork with one of them changed differently (one conflict). `mergeprev` is the
//!   dialog's first preview (the ancestor's and the branch's states read, both stretches since
//!   the fork, the comparison); `merge` is Merge with the conflict chosen (one merge
//!   transaction, the views updated). Each run's merge is undone before the next.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::history;
use serde_json::json;

use crate::measure::{median, path_text, release_lock_file, remove};

const OPEN_SEED: u64 = 0x0020_A000;

/// What the history measured, in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredHistory {
    pub histshow_ms: f64,
    pub histupd_ms: f64,
    pub histsel_ms: f64,
    pub pointshow_ms: f64,
    pub pointstep_ms: f64,
    pub brswitch_ms: f64,
    pub restore_ms: f64,
    pub mergeprev_ms: f64,
    pub merge_ms: f64,
    /// Changes and conflicts the merge preview listed.
    pub merge_changes: usize,
    pub merge_conflicts: usize,
    /// Graph rows and branches measured on.
    pub nodes: usize,
    pub branches: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

/// A property toggle: one ordinary transaction.
fn toggle(s: &mut Session, n: u64) -> Result<(), String> {
    run(s, "projectprops", json!({"props": {"bench_history": format!("{n}")}})).map(|_| ())
}

/// Measure on a copy of `path` (a version 2 file), `runs` times each where it repeats.
///
/// # Errors
/// The copy cannot be made or opened, or a step fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredHistory, String> {
    let copy = scratch.join("history.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let r = measure_on(&copy, runs);
    release_lock_file(&copy);
    remove(&copy);
    r
}

fn measure_on(copy: &Path, runs: usize) -> Result<MeasuredHistory, String> {
    let runs = runs.max(3);
    ecad_cadcraft::install();
    let mut s = Session::empty();
    let (p, _) = ecad_cadcraft::ProjectHandle::open_file_in(&mut s, path_text(copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
    // The history: 20 versions on main, a step apart by 25 toggles, and 5 branches from some of
    // them, each with a few steps of its own.
    let mut n = 0u64;
    for v in 0..20 {
        for _ in 0..25 {
            n += 1;
            toggle(&mut s, n)?;
        }
        run(&mut s, "version", json!({"name": format!("Bench {v}")}))?;
    }
    run(&mut s, "qsave", json!({}))?;
    for b in 0..5 {
        run(&mut s, "branch", json!({"name": format!("Bench branch {b}"), "from": {"version": format!("Bench {}", b * 3)}, "switch": true}))?;
        for _ in 0..10 {
            n += 1;
            toggle(&mut s, n)?;
        }
        run(&mut s, "switchbranch", json!({"branch": "main", "save": true}))?;
    }

    let mut show = Vec::with_capacity(runs);
    let mut nodes = 0;
    let mut branches = 0;
    for _ in 0..runs {
        let t = Instant::now();
        let (l, g) = p.read(&mut s, |p, _| history::graph(p, history::Show::default()).map(|g| (history::layout(&g), g)))??;
        show.push(ms(t));
        nodes = g.nodes.len();
        branches = g.branches.len();
        std::hint::black_box(l);
    }

    let mut upd = Vec::with_capacity(runs);
    for i in 0..runs {
        n += 1;
        toggle(&mut s, n)?;
        let t = Instant::now();
        run(&mut s, "version", json!({"name": format!("Bench upd {i}")}))?;
        let g = p.read(&mut s, |p, _| history::graph(p, history::Show::default()))??;
        upd.push(ms(t));
        std::hint::black_box(g);
    }

    let mut sel = Vec::with_capacity(runs);
    for i in 0..runs {
        let t = Instant::now();
        let v = run(&mut s, "pointinfo", json!({"version": format!("Bench {}", 10 + i % 9)}))?;
        sel.push(ms(t));
        std::hint::black_box(v);
    }

    let mut point = Vec::with_capacity(runs);
    for i in 0..runs {
        let t = Instant::now();
        run(&mut s, "showpoint", json!({"version": format!("Bench {}", 5 + i % 9)}))?;
        point.push(ms(t));
        run(&mut s, "closepoint", json!({}))?;
    }
    // A step up to 1 999 past the nearest snapshot before it.
    let head = p.read(&mut s, |p, _| p.ledger.head())?;
    let step = head.saturating_sub(1).max(1);
    let mut stepped = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        run(&mut s, "showpoint", json!({"seq": step}))?;
        stepped.push(ms(t));
        run(&mut s, "closepoint", json!({}))?;
    }

    run(&mut s, "qsave", json!({}))?;
    let mut switch = Vec::with_capacity(2 * runs);
    for _ in 0..runs {
        let t = Instant::now();
        run(&mut s, "switchbranch", json!({"branch": "Bench branch 2", "save": true}))?;
        switch.push(ms(t));
        let t = Instant::now();
        run(&mut s, "switchbranch", json!({"branch": "main", "save": true}))?;
        switch.push(ms(t));
    }

    // Restore a version 500 steps back (each run: a fresh version, 500 steps, restore).
    let mut restore = Vec::with_capacity(runs);
    for i in 0..runs {
        let name = format!("Bench restore {i}");
        run(&mut s, "version", json!({"name": name}))?;
        for _ in 0..500 {
            n += 1;
            toggle(&mut s, n)?;
        }
        let t = Instant::now();
        run(&mut s, "restore", json!({"version": name}))?;
        restore.push(ms(t));
    }
    // Merge: `runs` branches from Now, each changing 20 devices' descriptions; then main 2 000
    // steps ahead, one of the 20 changed there too.
    let devices: Vec<String> = p.read(&mut s, |p, _| p.ledger.document().components.keys().take(21).map(|c| c.hex()).collect())?;
    if devices.len() < 21 {
        return Err("the bench project has fewer than 21 devices".into());
    }
    run(&mut s, "qsave", json!({}))?;
    for i in 0..runs {
        run(&mut s, "branch", json!({"name": format!("Bench merge {i}"), "switch": true}))?;
        for (j, d) in devices.iter().take(20).enumerate() {
            run(&mut s, "setprop", json!({"component": d, "key": "description", "value": format!("Branch {i} device {j}")}))?;
        }
        run(&mut s, "switchbranch", json!({"branch": "main", "save": true}))?;
    }
    for k in 0..2_000u64 {
        if k == 1_000 {
            run(&mut s, "setprop", json!({"component": devices[0], "key": "description", "value": "Main's description"}))?;
        } else {
            n += 1;
            toggle(&mut s, n)?;
        }
    }
    run(&mut s, "qsave", json!({}))?;
    let mut prev = Vec::with_capacity(runs);
    let mut merged = Vec::with_capacity(runs);
    let (mut merge_changes, mut merge_conflicts) = (0, 0);
    for i in 0..runs {
        let from = format!("Bench merge {i}");
        let t = Instant::now();
        let v = run(&mut s, "merge", json!({"from": from, "preview": true}))?;
        prev.push(ms(t));
        merge_changes = v["changes"].as_array().map_or(0, Vec::len);
        merge_conflicts = v["conflicts"].as_array().map_or(0, Vec::len);
        let mut choices = serde_json::Map::new();
        for c in v["conflicts"].as_array().cloned().unwrap_or_default() {
            if let Some(id) = c["id"].as_str() {
                choices.insert(id.to_string(), json!("branch"));
            }
        }
        let t = Instant::now();
        run(&mut s, "merge", json!({"from": from, "choices": choices, "close": false}))?;
        merged.push(ms(t));
        run(&mut s, "undomerge", json!({}))?;
    }
    drop(p);
    Ok(MeasuredHistory {
        mergeprev_ms: median(&mut prev),
        merge_ms: median(&mut merged),
        merge_changes,
        merge_conflicts,
        histshow_ms: median(&mut show),
        histupd_ms: median(&mut upd),
        histsel_ms: median(&mut sel),
        pointshow_ms: median(&mut point),
        pointstep_ms: median(&mut stepped),
        brswitch_ms: median(&mut switch),
        restore_ms: median(&mut restore),
        nodes,
        branches,
    })
}
