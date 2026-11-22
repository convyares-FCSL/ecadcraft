//! Brief 055 timings (design 055 §10.3), through the app's project on a copy of a version 2 file.
//!
//! # Semantics
//! - **house**: one page's frame and title block worked out with the built-in house block
//!   (`FrameInput::of` and `frame::parts`). Target: within 10 % of 019's `frame` row on `dev`.
//! - **own300**: the same with an own frame of 300 items and a 1 000-shape logo. Target: < 2 ms.
//! - **save**: `titleblock {id, body}` of the frame every page uses, with one page on screen:
//!   the commit and that page's frame. `save_builds`: frames worked out (target: 1; the rest stale).
//! - **unread_builds**: frames worked out after `projectprops {props}` on a key no field reads
//!   (target: 0).
//!
//! The editor's frame (< 4 ms with 300 items) is drawn by the desktop app and is not measured here.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_engine::ProjectHandle;
use ecad_engine::pages::frame::{self, FrameInput};
use ecad_model::document::book::Book;
use ecad_model::objects::title_block::{Binding, BlockItem, Rect, TextDraw, TitleBlockLayout};
use ecad_model::{Primitive, Pt};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0055_0000;

/// What brief 055 measured.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredTitleBlocks {
    pub house_ms: f64,
    pub own300_ms: f64,
    pub save_ms: f64,
    pub save_builds: u64,
    pub unread_builds: u64,
    pub pages: usize,
}

/// A block of 300 items and a 1 000-shape logo (design 055 §10.3).
pub fn big_layout() -> TitleBlockLayout {
    let mut items = Vec::new();
    for i in 0..299 {
        let x = f64::from(i % 30) * 6.0;
        let y = f64::from(i / 30) * 4.0;
        items.push(match i % 3 {
            0 => BlockItem::Line { a: (x, y), b: (x + 5.0, y), weight: Default::default() },
            1 => BlockItem::Text { at: Rect::new(x, y, 5.5, 3.5), text: format!("T{i}"), draw: TextDraw::default() },
            _ => BlockItem::Field {
                at: Rect::new(x, y, 5.5, 3.5),
                bind: Binding::Project(ecad_model::objects::title_block::ProjectKey::of_info(ecad_model::InfoField::DrawingNumber)),
                label: None,
                draw: TextDraw::default(),
                if_empty: Default::default(),
            },
        });
    }
    let prims = (0..1000).map(|i| Primitive::Line { a: Pt::new(f64::from(i), 0.0), b: Pt::new(f64::from(i), 10.0) }).collect();
    items.push(BlockItem::Logo { at: Rect::new(150.0, 30.0, 30.0, 9.0), name: "bench".into(), prims });
    TitleBlockLayout { size: (180.0, 40.0), items }
}

fn exec(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<(f64, serde_json::Value), String> {
    let t = Instant::now();
    let v = s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
    Ok((t.elapsed().as_secs_f64() * 1e3, v))
}

fn frame_ms(project: &ProjectHandle, s: &mut Session) -> Result<f64, String> {
    project.read(s, |p, _| {
        let doc = p.ledger.document();
        let page = doc.page_order().first().copied();
        let t = Instant::now();
        let n = page.map(|pg| frame::parts(&FrameInput::of(doc, pg, &Book::of(doc))).len());
        std::hint::black_box(n);
        t.elapsed().as_secs_f64() * 1e3
    })
}

/// Measure brief 055's rows on a copy of `path`.
///
/// # Errors
/// The copy cannot be made or opened, or a command fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredTitleBlocks, String> {
    let _ = ecad_engine::install();
    let copy = scratch.join("title-blocks.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let mut out = MeasuredTitleBlocks::default();
    let result = (|| {
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())?;
        let first = project.read(&mut s, |p, _| p.ledger.document().page_order().first().copied())?.ok_or("the project has no page")?;
        out.pages = project.read(&mut s, |p, _| p.ledger.document().page_order().len())?;
        project.open_page(&mut s, first)?;
        let mut house = Vec::new();
        for _ in 0..runs.max(1) {
            house.push(frame_ms(&project, &mut s)?);
        }
        out.house_ms = median(&mut house);
        // An own frame every page uses.
        let (_, v) = exec(&mut s, "titleblock", json!({"new": {"from": "sheet", "sheet": "A3", "name": "Bench block"}}))?;
        let id = v["id"].as_str().ok_or("titleblock returned no id")?.to_string();
        exec(&mut s, "setdefault", json!({"key": "print.frame.style", "level": "project", "value": id}))?;
        let body = serde_json::to_value(big_layout()).map_err(|e| e.to_string())?;
        exec(&mut s, "titleblock", json!({"id": id, "body": body}))?;
        let mut own = Vec::new();
        for _ in 0..runs.max(1) {
            own.push(frame_ms(&project, &mut s)?);
        }
        out.own300_ms = median(&mut own);
        // Save with one page on screen: one frame worked out.
        project.read(&mut s, |p, _| p.frame_builds)?;
        let mut save = Vec::new();
        let mut builds = Vec::new();
        for i in 0..runs.max(1) {
            let mut l = big_layout();
            if let Some(BlockItem::Line { a, .. }) = l.items.first_mut() {
                a.0 = (i % 5) as f64;
            }
            let before = project.read(&mut s, |p, _| p.frame_builds)?;
            let (ms, _) = exec(&mut s, "titleblock", json!({"id": id, "body": serde_json::to_value(&l).map_err(|e| e.to_string())?}))?;
            save.push(ms);
            builds.push(project.read(&mut s, |p, _| p.frame_builds)? - before);
        }
        out.save_ms = median(&mut save);
        out.save_builds = builds.iter().copied().max().unwrap_or(0);
        let before = project.read(&mut s, |p, _| p.frame_builds)?;
        exec(&mut s, "projectprops", json!({"props": {"bench_unread": "x"}}))?;
        out.unread_builds = project.read(&mut s, |p, _| p.frame_builds)? - before;
        Ok::<(), String>(())
    })();
    remove(&copy);
    result.map(|()| out)
}
