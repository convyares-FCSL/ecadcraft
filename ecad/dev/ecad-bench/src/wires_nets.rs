//! Brief 032 timings (design 032 §11), through the app's project on a copy of the version 2 file.
//!
//! # Semantics
//! - **wopen**: a two-point wire drawn in open space with `join: false` (nothing to join), the
//!   baseline; **wmerge**: the same wire drawn from another wire's open end (it merges). Target:
//!   within 10% of wopen.
//! - **jjoin**: `joinwires` at two open ends that meet (two nets become one). Reported; its old
//!   baseline, a junction placed there, went with the junction symbol (design 072 D).
//! - **lnet** / **leach**: the wire labels of the page with the most routes worked out under the
//!   `net` and `each` rules. Target: lnet within 10% of leach.
//! - **ccont**: `cablecontinue` of a 50-core cable on a second page (its reference end, one stub
//!   per core and the slash). Target: under 20 ms. Until 072 D (F5) this row timed a `cableslash`
//!   with the cable's tag on the second page, which added members; that form is gone (a slash with
//!   an existing tag is refused where none of the cable's wires runs), and continuing a cable on
//!   another page is `cablecontinue`.
//!
//! Each edit is undone after it is measured; the setup happens on two pages added to the copy.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_engine::ProjectHandle;
use ecad_model::PageId;
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0032_0000;
/// Cores of the continued cable.
const CORES: usize = 50;
/// The continued cable's tag. The generator tags its cables `-W1` to `-W<cables>` (2 500 on
/// large), so this must lie beyond every tier's cables, or `cableslash` shows the generated cable
/// of that tag and is refused where none of its wires runs.
pub const CCONT_TAG: &str = "-W8032";
/// Where the bench draws: right of an A3 sheet, clear of the generated pattern.
const X0: f64 = 600.0;

/// What brief 032 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredWires {
    pub wopen_ms: f64,
    pub wmerge_ms: f64,
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
        ecad_engine::install();
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())?;
        let mut out = MeasuredWires::default();
        let runs = runs.max(1);
        // The densest page's labels under both rules (read only, on the document). Densest by its
        // routes (design 072 D: a wire is drawn by routes, not by representations of the wire); a
        // project whose pages have none is refused, never timed as an empty page.
        let (lnet, leach, routes) = project
            .read(&mut s, |p, _| -> Result<_, LabelPage> {
                let doc = p.ledger.document();
                let (page, routes) =
                    doc.page_order.iter().map(|pg| (*pg, doc.routes_on(*pg).count())).max_by_key(|(_, n)| *n).ok_or(LabelPage::Empty)?;
                if routes == 0 {
                    return Err(LabelPage::Unrouted(page));
                }
                let mut each_doc = doc.clone();
                each_doc.settings.wiring.label_rule = Some(ecad_model::connectivity::wiring::LABEL_RULE_EACH.into());
                for (rule, d) in [("net", doc), ("each", &each_doc)] {
                    if ecad_engine::wires::draw::labels_on_page(d, page).is_empty() {
                        return Err(LabelPage::Unlabelled { page, routes, rule });
                    }
                }
                let (mut a, mut b) = (Vec::new(), Vec::new());
                for _ in 0..runs {
                    let t = Instant::now();
                    std::hint::black_box(ecad_engine::wires::draw::labels_on_page(doc, page));
                    a.push(ms(t));
                    let t = Instant::now();
                    std::hint::black_box(ecad_engine::wires::draw::labels_on_page(&each_doc, page));
                    b.push(ms(t));
                }
                Ok((a, b, routes))
            })?
            .map_err(|e| e.to_string())?;
        (out.lnet_ms, out.leach_ms, out.routes) = (median(&mut lnet.clone()), median(&mut leach.clone()), routes);
        // A page of its own for the edits.
        let page: PageId = crate_hex(&run(&mut s, "newpage", json!({"title": "bench 032"}))?, "page")?;
        run(&mut s, "openpage", json!({"page": page.hex()}))?;
        run(&mut s, "wire", json!({"points": [[X0, 0.0], [X0 + 50.0, 0.0]]}))?;
        run(&mut s, "wire", json!({"points": [[X0, 40.0], [X0 + 50.0, 40.0]]}))?;
        run(&mut s, "wire", json!({"points": [[X0 + 50.0, 40.0], [X0 + 50.0, 20.0]], "join": false}))?;
        let (mut wopen, mut wmerge, mut jjoin) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..runs {
            wopen.push(timed(&mut s, "wire", json!({"points": [[X0 + 200.0, 0.0], [X0 + 200.0, -20.0]], "join": false}))?);
            wmerge.push(timed(&mut s, "wire", json!({"points": [[X0 + 50.0, 0.0], [X0 + 50.0, -20.0]]}))?);
            jjoin.push(timed(&mut s, "joinwires", json!({"at": [X0 + 50.0, 40.0]}))?);
        }
        (out.wopen_ms, out.wmerge_ms, out.jjoin_ms) = (median(&mut wopen), median(&mut wmerge), median(&mut jjoin));
        // A 50-core cable slashed on this page with its reference end, continued on a second page
        // (design 072 §4.4, F4).
        let y = |i: usize| 100.0 + 5.0 * i as f64;
        for i in 0..CORES {
            run(&mut s, "wire", json!({"points": [[X0, y(i)], [X0 + 100.0, y(i)]]}))?;
        }
        let slashed = run(&mut s, "cableslash", json!({"from": [X0 + 50.0, y(0) - 3.0], "to": [X0 + 50.0, y(CORES) + 3.0], "tag": CCONT_TAG}))?;
        let cable = slashed["cable"].clone();
        run(&mut s, "cablereference", json!({"cable": cable, "at": [X0 + 100.0, y(CORES)]}))?;
        let second: PageId = crate_hex(&run(&mut s, "newpage", json!({"title": "bench 032 b"}))?, "page")?;
        run(&mut s, "openpage", json!({"page": second.hex()}))?;
        let mut ccont = Vec::new();
        for _ in 0..runs {
            ccont.push(timed(&mut s, "cablecontinue", json!({"cable": cable, "at": [X0, y(CORES)], "toward": [X0 + 50.0, y(CORES)]}))?);
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

/// Why the densest page cannot be labelled for `lnet` / `leach`.
#[derive(Debug)]
enum LabelPage {
    Empty,
    Unrouted(PageId),
    Unlabelled { page: PageId, routes: usize, rule: &'static str },
}

impl std::fmt::Display for LabelPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LabelPage::Empty => write!(f, "lnet/leach: the project has no page"),
            LabelPage::Unrouted(page) => write!(f, "lnet/leach: no page has a route (densest page {}): nothing to label", page.hex()),
            LabelPage::Unlabelled { page, routes, rule } => {
                write!(f, "lnet/leach: the densest page {} ({routes} routes) has no wire labels under the {rule} rule", page.hex())
            }
        }
    }
}
