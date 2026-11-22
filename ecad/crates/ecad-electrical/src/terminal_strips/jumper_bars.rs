//! Link lines (brief 030, design 030 §5.3): the dashed line a strip's link draws through its
//! terminals on every page that shows them. Derived on every draw of the page, never stored and
//! never edited as geometry; erasing one deletes its link.
//!
//! # Semantics
//! - Through the link's terminals on this page in plan order; between two that are not in one
//!   row or column, an orthogonal dog-leg.
//! - Lane `k` (from the strip's links, greedy by first position then number, as the plan's) is
//!   offset `k × 1.5 mm` up for a horizontal row, left for a vertical one.
//! - Where the link goes on past the first or last terminal shown here, a 4 mm stub in the line's
//!   direction; one terminal shown: a stub to the right.
//!
//! # Update map
//! The lines of a page are part of its projection, which is redrawn when an op touches the page
//! (`crate::project::pages_touched`): a strip property (`jumper_bar.<n>`) touches the strip's pages,
//! a terminal symbol's move its page; nothing else draws them.

use std::collections::BTreeMap;

use ecad_model::connectivity::jumper_bars;
use ecad_model::{DeviceId, Document, PageId, Pt, Subject};

/// Lane spacing and stub length, in millimetres (`ecad.jumper_bar`, `ecad.link-stub`).
pub const LANE_MM: f64 = 1.5;
pub const STUB_MM: f64 = 4.0;
/// The layer the lines are drawn on.
pub const LAYER: &str = "ecad.jumper_bar";
/// Said when a link line is moved or stretched.
pub const JUMPER_BAR_DERIVED: &str =
    "A link line is drawn from its jumper_bar: select it and use Extend, Shorten, Split or Delete (Jumper), or erase it to delete the link.";

/// One segment of a link line.
#[derive(Debug, Clone, PartialEq)]
pub struct JumperBarPart {
    pub strip: DeviceId,
    pub n: u32,
    pub a: Pt,
    pub b: Pt,
}

fn stub(p: Pt, toward: Pt, back: bool) -> Pt {
    let (dx, dy) = (toward.x - p.x, toward.y - p.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 || !len.is_finite() {
        return Pt::new(p.x + STUB_MM, p.y);
    }
    let s = if back { -STUB_MM } else { STUB_MM };
    Pt::new(p.x + dx / len * s, p.y + dy / len * s)
}

/// The link lines of every strip shown on `page`.
pub fn parts_on_page(doc: &Document, page: PageId) -> Vec<JumperBarPart> {
    let mut strips: Vec<DeviceId> = doc
        .representations_on(page)
        .filter_map(|(_, r)| match r.subject {
            Subject::Device(c) => Some(c),
            _ => None,
        })
        .collect();
    strips.sort();
    strips.dedup();
    let mut out = Vec::new();
    for c in strips {
        let Some(comp) = doc.devices.get(&c) else { continue };
        let found = jumper_bars::of(&comp.properties);
        if found.is_empty() || crate::terminal_strips::strip_of(doc, c).is_none() {
            continue;
        }
        let order = crate::terminal_strips::accessories::positions(doc, c);
        // Brief 036: each level's symbols; the in point stands for a two-point level.
        let here: BTreeMap<(String, u32), Pt> = crate::terminal_strips::drawn_points(doc, c)
            .into_iter()
            .filter(|(t, _, p, _)| *p == page && crate::terminal_strips::terminal_blocks::level_of(doc, *t).1 != Some('o'))
            .map(|(t, _, _, at)| {
                ((crate::terminal_strips::terminal_blocks::position_label(doc, t), crate::terminal_strips::terminal_blocks::level_of(doc, t).0), at)
            })
            .collect();
        // Cross-level links (design 036 §5.4): a dog-leg between the levels shown, else stubs.
        for (n, p) in &found {
            for sp in p.spans() {
                let jumper_bars::Span::Levels { position, lo, hi } = sp else { continue };
                let pts: Vec<Pt> = (lo..=hi).filter_map(|l| here.get(&(position.clone(), l)).copied()).collect();
                if pts.len() >= 2 {
                    for w in pts.windows(2) {
                        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
                        let knee = Pt::new(a.x + STUB_MM, a.y);
                        let knee2 = Pt::new(a.x + STUB_MM, b.y);
                        out.push(JumperBarPart { strip: c, n: *n, a: *a, b: knee });
                        out.push(JumperBarPart { strip: c, n: *n, a: knee, b: knee2 });
                        out.push(JumperBarPart { strip: c, n: *n, a: knee2, b: *b });
                    }
                } else if let Some(a) = pts.first() {
                    out.push(JumperBarPart { strip: c, n: *n, a: *a, b: Pt::new(a.x + STUB_MM, a.y) });
                }
            }
        }
        // Lanes over the whole strip, as the plan's (§5.3), per level (brief 036).
        let mut spans: Vec<(u32, usize, usize, u32, Vec<String>)> = found
            .iter()
            .filter(|(_, p)| !p.crosses_levels())
            .filter_map(|(n, p)| {
                let level = p.level().unwrap_or(1);
                let sel: Vec<String> = p.select(&order).into_iter().cloned().collect();
                let first = order.iter().position(|o| Some(o) == sel.first())?;
                let last = order.iter().position(|o| Some(o) == sel.last())?;
                Some((level, first, last, *n, sel))
            })
            .collect();
        spans.sort_by_key(|(lv, f, _, n, _)| (*lv, *f, *n));
        let mut lanes_of: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (level, first, last, n, sel) in spans {
            let lanes = lanes_of.entry(level).or_default();
            let lane = match lanes.iter().position(|e| *e < first) {
                Some(k) => {
                    if let Some(e) = lanes.get_mut(k) {
                        *e = last;
                    }
                    k
                }
                None => {
                    lanes.push(last);
                    lanes.len() - 1
                }
            };
            let shown: Vec<(usize, Pt)> = sel.iter().enumerate().filter_map(|(i, l)| here.get(&(l.clone(), level)).map(|p| (i, *p))).collect();
            let (Some((fi, fp)), Some((li, lp))) = (shown.first().copied(), shown.last().copied()) else { continue };
            let horizontal = shown.len() < 2 || (lp.x - fp.x).abs() >= (lp.y - fp.y).abs();
            let off = lane as f64 * LANE_MM;
            let shift = |p: Pt| if horizontal { Pt::new(p.x, p.y + off) } else { Pt::new(p.x - off, p.y) };
            let mut seg = |a: Pt, b: Pt| out.push(JumperBarPart { strip: c, n, a: shift(a), b: shift(b) });
            for w in shown.windows(2) {
                let (Some((_, a)), Some((_, b))) = (w.first(), w.get(1)) else { continue };
                if (a.x - b.x).abs() < 1e-6 || (a.y - b.y).abs() < 1e-6 {
                    seg(*a, *b);
                } else {
                    let knee = if horizontal { Pt::new(b.x, a.y) } else { Pt::new(a.x, b.y) };
                    seg(*a, knee);
                    seg(knee, *b);
                }
            }
            // Continuation stubs where the link goes on beyond what this page shows.
            if shown.len() == 1 {
                if sel.len() > 1 {
                    let to = if horizontal { Pt::new(fp.x + 1.0, fp.y) } else { Pt::new(fp.x, fp.y + 1.0) };
                    seg(fp, stub(fp, to, false));
                }
                continue;
            }
            if fi > 0 {
                seg(fp, stub(fp, lp, true));
            }
            if li + 1 < sel.len() {
                seg(lp, stub(lp, fp, true));
            }
        }
    }
    out
}

/// The hover line of a link (§5.3): `-X01 link 1-4 (4 positions, 1 bar JB-10)`.
pub fn hover(doc: &Document, data: Option<&crate::terminal_strips::StripData>, c: DeviceId, n: u32) -> String {
    let tag = crate::devices::tag_words(doc, c);
    let Some(l) = data.and_then(|d| d.acc.jumper_bars.iter().find(|l| l.n == n)) else { return format!("{tag} link {n}") };
    let bars = if l.bars == 1 { "1 bar".to_string() } else { format!("{} bars", l.bars) };
    let part = l.part_number.clone().map_or_else(String::new, |p| format!(" {p}"));
    // Brief 036: the level a link runs on (`level N`) when its strip has levels.
    let level = match (l.level, data) {
        (Some(n), Some(d)) => d.rows.iter().find(|r| r.level == n && r.ty.multi()).map(|r| format!("level {}, ", r.level_name)),
        (None, _) => Some("levels of one position, ".to_string()),
        _ => None,
    };
    format!("{tag} link {} ({}{} positions, {bars}{part})", l.text, level.unwrap_or_default(), l.labels.len())
}
