//! Full rebuild of each kept view on a written project (the end-gate review's table A, #57):
//! what a view costs when it is made again whole after an edit instead of being updated.
//!
//! # Semantics
//! Each row is the median of `runs` builds of one view from the project's document, in
//! milliseconds, in a release build. Nothing is edited; the project is opened read-only.

use std::path::Path;
use std::time::Instant;

use ecad_electrical::styles::text_style::fonts::FixedWidth;

use crate::measure::median;

/// One view's full-rebuild time.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub view: &'static str,
    pub ms: f64,
}

fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> f64 {
    let mut v: Vec<f64> = (0..runs.max(1))
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(f());
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    median(&mut v)
}

/// Time the full rebuild of every kept view of the project at `path`.
///
/// # Errors
/// The file cannot be opened.
pub fn measure(path: &Path, runs: usize) -> Result<Vec<Row>, crate::rich::CorpusError> {
    let o = crate::container::open(path).map_err(crate::rich::CorpusError::Project)?;
    let doc = o.ledger.document();
    let m = FixedWidth;
    let models = ecad_electrical::views::ReportsView::build(doc, &m);
    let strips = models.model().strips.clone();
    let rows = vec![
        Row { view: "RefsView (references)", ms: time(runs, || ecad_electrical::views::ReferencesView::build(doc)) },
        Row { view: "ConnectorsView", ms: time(runs, || ecad_electrical::views::ConnectorsView::build(doc)) },
        Row { view: "Plc kept", ms: time(runs, || ecad_electrical::plc::kept::Plc::build_with(doc, &m)) },
        Row { view: "front::Uses (cover)", ms: time(runs, || ecad_electrical::pages::front::Uses::build(doc)) },
        Row { view: "rating::Kept", ms: time(runs, || ecad_electrical::circuits::kept::Kept::build(doc)) },
        Row { view: "StripPages", ms: time(runs, || ecad_electrical::terminal_strips::pages::StripPages::build(doc, &strips)) },
        Row {
            view: "Strips (inside Outputs)",
            ms: time(runs, || {
                let cx = ecad_electrical::reports::PageCx::of(doc);
                ecad_electrical::terminal_strips::strip_model::Strips::build(doc, models.model(), &cx)
            }),
        },
        Row { view: "Devices/Outputs (all models)", ms: time(runs.min(3), || ecad_electrical::views::ReportsView::build(doc, &m)) },
    ];
    Ok(rows)
}
