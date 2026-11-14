//! Brief 032 timings (design 032 §11), through the app's project on a copy of the version 2 file.
//!
//! # Semantics
//! - **wopen**: a two-point wire drawn in open space with `join: false` (nothing to join), the
//!   baseline; **wmerge**: the same wire drawn from another wire's open end (it merges). Target:
//!   within 10% of wopen.
//! - **jjoin**: `joinwires` at two open ends that meet (two nets become one); **junction**:
//!   `junction` at the same place (design 022's repair of the same two ends), the baseline.
//!   Target: as junction.
//! - **lnet** / **leach**: the wire labels of the page with the most routes worked out under the
//!   `net` and `each` rules. Target: lnet within 10% of leach.
//! - **ccont**: `cableslash` continuing a 50-core cable on a second page through continuation
//!   pairs. Target: under 20 ms.
//!
//! Each edit is undone after it is measured; the setup happens on two pages added to the copy.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::PageId;
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0032_0000;
/// Cores of the continued cable.
const CORES: usize = 50;
/// Where the bench draws: right of an A3 sheet, clear of the generated pattern.
const X0: f64 = 600.0;

/// The bench cable's tag. The generator tags its cables `-W1` to `-W<cables>` (2,500 on large), so
/// this stays above every tier's count: `-W932` was a cable of the large project, and continuing it
/// was refused there.
const CABLE: &str = "-W9932";

/// What brief 032 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredWires {
    pub wopen_ms: f64,
    pub wmerge_ms: f64,
    pub junction_ms: f64,
    pub jjoin_ms: f64,
    pub lnet_ms: f64,
    pub leach_ms: f64,
    pub ccont_ms: f64,
    /// Routes on the densest page.
    pub routes: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

fn timed(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<f64, String> {
    let t = Instant::now();
    run(s, cmd, params)?;
    let d = ms(t);
    run(s, "undo", json!({}))?;
    Ok(d)
}

/// Measure brief 032's rows on a copy of `path` (design 032 §11).
///
/// # Errors
/// A command refused or the copy not opened.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredWires, String> {
    let copy = scratch.join("wires-nets.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_cadcraft::install();
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
        let mut out = MeasuredWires::default();
        let runs = runs.max(1);
        // The densest page's labels under both rules (read only, on the document).
        let (lnet, leach, routes) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let dense = doc
                .page_order
                .iter()
                .max_by_key(|pg| doc.representations_on(**pg).filter(|(_, r)| matches!(r.subject, ecad_model::Subject::Conductor(_))).count())
                .copied();
            let Some(page) = dense else { return (Vec::new(), Vec::new(), 0) };
            let routes = doc.representations_on(page).filter(|(_, r)| matches!(r.subject, ecad_model::Subject::Conductor(_))).count();
            let mut each_doc = doc.clone();
            each_doc.settings.wiring.label_rule = ecad_model::wiring::LABEL_RULE_EACH.into();
            let (mut a, mut b) = (Vec::new(), Vec::new());
            for _ in 0..runs {
                let t = Instant::now();
                std::hint::black_box(ecad_cadcraft::wires::labels_on_page(doc, page));
                a.push(ms(t));
                let t = Instant::now();
                std::hint::black_box(ecad_cadcraft::wires::labels_on_page(&each_doc, page));
                b.push(ms(t));
            }
            (a, b, routes)
        })?;
        (out.lnet_ms, out.leach_ms, out.routes) = (median(&mut lnet.clone()), median(&mut leach.clone()), routes);
        // A page of its own for the edits.
        let page: PageId = crate_hex(&run(&mut s, "newpage", json!({"title": "bench 032"}))?, "page")?;
        run(&mut s, "openpage", json!({"page": page.hex()}))?;
        run(&mut s, "wire", json!({"points": [[X0, 0.0], [X0 + 50.0, 0.0]]}))?;
        run(&mut s, "wire", json!({"points": [[X0, 40.0], [X0 + 50.0, 40.0]]}))?;
        run(&mut s, "wire", json!({"points": [[X0 + 50.0, 40.0], [X0 + 50.0, 20.0]], "join": false}))?;
        let (mut wopen, mut wmerge, mut junction, mut jjoin) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for _ in 0..runs {
            wopen.push(timed(&mut s, "wire", json!({"points": [[X0 + 200.0, 0.0], [X0 + 200.0, -20.0]], "join": false}))?);
            wmerge.push(timed(&mut s, "wire", json!({"points": [[X0 + 50.0, 0.0], [X0 + 50.0, -20.0]]}))?);
            junction.push(timed(&mut s, "junction", json!({"at": [X0 + 50.0, 40.0]}))?);
            jjoin.push(timed(&mut s, "joinwires", json!({"at": [X0 + 50.0, 40.0]}))?);
        }
        (out.wopen_ms, out.wmerge_ms, out.junction_ms, out.jjoin_ms) =
            (median(&mut wopen), median(&mut wmerge), median(&mut junction), median(&mut jjoin));
        // A 50-core cable ending on continuations, continued on a second page.
        let y = |i: usize| 100.0 + 5.0 * i as f64;
        for i in 0..CORES {
            run(&mut s, "place", json!({"symbol": "continuation", "at": [X0 + 100.0, y(i)], "name": format!("B{i}")}))?;
            run(&mut s, "wire", json!({"points": [[X0, y(i)], [X0 + 100.0, y(i)]]}))?;
        }
        run(&mut s, "cableslash", json!({"from": [X0 + 50.0, y(0) - 3.0], "to": [X0 + 50.0, y(CORES) + 3.0], "tag": CABLE}))?;
        let second: PageId = crate_hex(&run(&mut s, "newpage", json!({"title": "bench 032 b"}))?, "page")?;
        run(&mut s, "openpage", json!({"page": second.hex()}))?;
        for i in 0..CORES {
            run(&mut s, "place", json!({"symbol": "continuation", "at": [X0, y(i)], "name": format!("B{i}")}))?;
            run(&mut s, "wire", json!({"points": [[X0, y(i)], [X0 + 100.0, y(i)]]}))?;
        }
        let mut ccont = Vec::new();
        for _ in 0..runs {
            ccont.push(timed(&mut s, "cableslash", json!({"from": [X0 + 50.0, y(0) - 3.0], "to": [X0 + 50.0, y(CORES) + 3.0], "tag": CABLE}))?);
        }
        out.ccont_ms = median(&mut ccont);
        drop(project);
        drop(s);
        Ok(out)
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    result
}

fn crate_hex<T: From<u128>>(v: &serde_json::Value, key: &str) -> Result<T, String> {
    v[key].as_str().and_then(|t| u128::from_str_radix(t, 16).ok()).map(T::from).ok_or_else(|| format!("no `{key}` in {v}"))
}

#[cfg(test)]
mod tests {
    /// The bench cable's tag is none of the cables any tier generates (`-W1` to `-W<cables>`).
    #[test]
    fn the_bench_cable_is_not_a_generated_one() {
        let n: usize = super::CABLE.trim_start_matches("-W").parse().unwrap_or(0);
        for t in crate::TIERS.iter().chain([&crate::PRINT_TIER]) {
            assert!(n > t.cables, "{} is a cable of the {} tier ({} cables)", super::CABLE, t.name, t.cables);
        }
    }
}
