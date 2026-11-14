//! The table's group strip (brief 051, design 051 §3.1, Q9): a horizontal row of tiles over a
//! table, one per group; selecting a tile tells the owner to filter the rows to its group. Part of
//! the table component, like its toolbar and group rows: not a panel.
//!
//! A tile has a small head line, a title, up to two lines under it, a grid of marks (squares,
//! [`MARKS_PER_ROW`] a row) and link lines. The strip draws and reports [`StripEvent`]s; it never
//! runs a command. Generic: it knows nothing of what the groups are.
//!
//! Tokens (design 051 §8, `ecad.plc.*` used generically here): tile width [`TILE_WIDTH`], mark
//! [`MARK`], gap [`MARK_GAP`].

use ecad_tokens::{GRAPHITE, line, radius, space, ui_type};
use egui::{Id, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};

use super::cells;
use crate::color;
use crate::style::corner;

/// `ecad.plc.tile_width`: a tile's width, px.
pub const TILE_WIDTH: f32 = 112.0;
/// `ecad.plc.square`: a mark's side, px.
pub const MARK: f32 = 8.0;
/// `ecad.plc.square_gap`: between marks, px.
pub const MARK_GAP: f32 = 3.0;
/// Marks in a row (one byte, Q16).
pub const MARKS_PER_ROW: usize = 8;

/// How a mark looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mark {
    /// Filled `text_2` (in use).
    On,
    /// An outline (free).
    Off,
    /// Filled `provisional` (reserved).
    Soft,
    /// Filled `error` (has a check).
    Alert,
}

/// A link's colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tone {
    #[default]
    Accent,
    Warning,
    Error,
    /// Plain text, not a link.
    Plain,
}

/// One part of a link line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Link {
    pub text: String,
    pub tone: Tone,
    /// It can be clicked (a greyed one shows `tip`).
    pub enabled: bool,
    pub tip: Option<String>,
    /// Its element name.
    pub name: String,
}

/// One tile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    /// Its element name (`ecad.plc.tile.<slot>`).
    pub name: String,
    pub head: String,
    pub title: String,
    /// The title in the faint colour (a free slot).
    pub faint_title: bool,
    pub lines: Vec<String>,
    pub marks: Vec<Mark>,
    /// Each mark's element name is this followed by its index (`ecad.plc.tile.2.ch.`).
    pub mark_name: String,
    /// One hover line per mark (empty: none).
    pub tips: Vec<String>,
    /// Link lines, each of one or more parts.
    pub links: Vec<Vec<Link>>,
    /// A dashed border and no marks (a free slot).
    pub dashed: bool,
}

/// What the strip reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StripEvent {
    /// Tile `i` was clicked.
    Select(usize),
    /// Mark `mark` of tile `tile` was clicked (`double`: double-clicked).
    Mark { tile: usize, mark: usize, double: bool },
    /// Part `part` of link line `line` of tile `tile` was clicked.
    Link { tile: usize, line: usize, part: usize },
}

fn mark_rows(n: usize) -> usize {
    n.div_ceil(MARKS_PER_ROW)
}

/// A tile's height for `marks` mark rows and `links` link lines, px.
fn tile_height(lines: usize, marks: usize, links: usize) -> f32 {
    let m = if marks > 0 { marks as f32 * (MARK + MARK_GAP) - MARK_GAP + space::S } else { 0.0 };
    2.0 * space::M
        + ui_type::CAPTION.line
        + ui_type::TITLE.line
        + lines as f32 * ui_type::SMALL.line
        + space::XS
        + m
        + links as f32 * ui_type::SMALL.line
}

/// Paint one mark at `r`.
fn paint_mark(ui: &Ui, r: Rect, m: Mark) {
    let s = GRAPHITE;
    let p = ui.painter();
    match m {
        Mark::On => p.rect_filled(r, corner(1.0), color(s.text_2)),
        Mark::Soft => p.rect_filled(r, corner(1.0), color(s.provisional)),
        Mark::Alert => p.rect_filled(r, corner(1.0), color(s.error)),
        Mark::Off => p.rect_stroke(r, corner(1.0), Stroke::new(line::HAIRLINE_PX, color(s.line_strong)), StrokeKind::Inside),
    };
}

/// The legend under the strip: each mark with its words.
pub fn legend(ui: &mut Ui, items: &[(Mark, &str)]) {
    let s = GRAPHITE;
    ui.horizontal(|ui| {
        for (m, words) in items {
            let (r, _) = ui.allocate_exact_size(vec2(MARK, MARK), Sense::hover());
            paint_mark(ui, r, *m);
            let g = cells::fitted(ui, words, ui_type::SMALL, color(s.text_3), 200.0);
            let (gr, resp) = ui.allocate_exact_size(g.size(), Sense::hover());
            resp.widget_info(|| WidgetInfo::labeled(Role::Label, true, *words));
            ui.painter().galley(gr.min, g, color(s.text_3));
            ui.add_space(space::M);
        }
    });
}

/// Draw `tiles` side by side, `selected` with the accent border; the row scrolls sideways when it
/// is wider than the view. Returns what was clicked.
pub fn show(ui: &mut Ui, id: Id, tiles: &[Tile], selected: Option<usize>) -> Vec<StripEvent> {
    let mut out = Vec::new();
    let lines = tiles.iter().map(|t| t.lines.len()).max().unwrap_or(0);
    let marks = tiles.iter().map(|t| mark_rows(t.marks.len())).max().unwrap_or(0);
    let links = tiles.iter().map(|t| t.links.len()).max().unwrap_or(0);
    let h = tile_height(lines, marks, links);
    egui::ScrollArea::horizontal().id_salt(id).auto_shrink([false, true]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = space::M;
            for (i, t) in tiles.iter().enumerate() {
                tile(ui, id.with(i), i, t, selected == Some(i), h, &mut out);
            }
        });
    });
    out
}

/// One tile.
fn tile(ui: &mut Ui, id: Id, i: usize, t: &Tile, selected: bool, h: f32, out: &mut Vec<StripEvent>) {
    let s = GRAPHITE;
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_WIDTH, h), Sense::click());
    let name = t.name.clone();
    resp.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, &name));
    if !ui.is_rect_visible(rect) {
        if resp.clicked() {
            out.push(StripEvent::Select(i));
        }
        return;
    }
    let p = ui.painter();
    let ground = if selected {
        s.accent_soft
    } else if resp.hovered() {
        s.hover
    } else {
        s.raised
    };
    p.rect_filled(rect, corner(radius::CHIP), color(s.raised));
    p.rect_filled(rect, corner(radius::CHIP), color(ground));
    if t.dashed {
        let st = Stroke::new(line::HAIRLINE_PX, color(s.line_strong));
        p.add(egui::Shape::dashed_line(&[rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()], st, 4.0, 3.0));
    } else if selected {
        p.rect_stroke(rect, corner(radius::CHIP), Stroke::new(2.0, color(s.accent_text)), StrokeKind::Inside);
    } else {
        p.rect_stroke(rect, corner(radius::CHIP), Stroke::new(line::HAIRLINE_PX, color(s.line)), StrokeKind::Inside);
    }
    let inner = rect.shrink(space::M);
    let w = inner.width();
    let mut y = inner.top();
    let head_ink = if selected { s.accent_text } else { s.text_3 };
    let head = if selected { format!("{} \u{b7} selected", t.head) } else { t.head.clone() };
    let g = cells::fitted(ui, &head, ui_type::CAPTION, color(head_ink), w);
    ui.painter().galley(pos2(inner.left(), y), g, color(head_ink));
    y += ui_type::CAPTION.line;
    let title_ink = if t.faint_title { s.text_2 } else { s.text };
    let g = cells::fitted(ui, &t.title, ui_type::TITLE, color(title_ink), w);
    ui.painter().galley(pos2(inner.left(), y), g, color(title_ink));
    y += ui_type::TITLE.line;
    for l in &t.lines {
        let g = cells::fitted(ui, l, ui_type::SMALL, color(s.text_2), w);
        ui.painter().galley(pos2(inner.left(), y), g, color(s.text_2));
        y += ui_type::SMALL.line;
    }
    y += space::XS;
    // Marks, a row of eight per byte.
    let mut mark_hit = None;
    for (k, m) in t.marks.iter().enumerate() {
        let (row, col) = (k / MARKS_PER_ROW, k % MARKS_PER_ROW);
        let r = Rect::from_min_size(pos2(inner.left() + col as f32 * (MARK + MARK_GAP), y + row as f32 * (MARK + MARK_GAP)), vec2(MARK, MARK));
        let mr = ui.interact(r.expand(MARK_GAP / 2.0), id.with(("mark", k)), Sense::click());
        paint_mark(ui, r, *m);
        mr.widget_info(|| WidgetInfo::labeled(Role::Button, true, format!("{}{k}", t.mark_name)));
        let mr = match t.tips.get(k).filter(|x| !x.is_empty()) {
            Some(tip) => mr.on_hover_text(tip),
            None => mr,
        };
        if mr.double_clicked() {
            mark_hit = Some((k, true));
        } else if mr.clicked() {
            mark_hit = Some((k, false));
        }
    }
    // Links at the foot.
    let foot = rect.bottom() - space::M - t.links.len() as f32 * ui_type::SMALL.line;
    let mut link_hit = None;
    for (li, parts) in t.links.iter().enumerate() {
        let mut x = inner.left();
        let ly = foot + li as f32 * ui_type::SMALL.line;
        for (pi, part) in parts.iter().enumerate() {
            let ink = match (part.enabled, part.tone) {
                (false, Tone::Accent | Tone::Plain) => s.text_3,
                (_, Tone::Accent) => s.accent_text,
                (_, Tone::Warning) => s.warning,
                (_, Tone::Error) => s.error,
                (_, Tone::Plain) => s.text_2,
            };
            let room = (inner.right() - x).max(0.0);
            let g = cells::fitted(ui, &part.text, ui_type::SMALL, color(ink), room);
            let r = Rect::from_min_size(pos2(x, ly), g.size());
            x = r.right() + space::XS;
            ui.painter().galley(r.min, g, color(ink));
            if part.tone == Tone::Plain && part.tip.is_none() {
                continue;
            }
            let lr = ui.interact(r, id.with(("link", li, pi)), if part.enabled { Sense::click() } else { Sense::hover() });
            let name = if part.name.is_empty() { part.text.clone() } else { part.name.clone() };
            lr.widget_info(|| WidgetInfo::labeled(Role::Link, part.enabled, &name));
            let lr = match &part.tip {
                Some(tip) => lr.on_hover_text(tip),
                None => lr,
            };
            if part.enabled && lr.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if part.enabled && lr.clicked() {
                link_hit = Some((li, pi));
            }
        }
    }
    if let Some((mark, double)) = mark_hit {
        out.push(StripEvent::Mark { tile: i, mark, double });
    } else if let Some((line, part)) = link_hit {
        out.push(StripEvent::Link { tile: i, line, part });
    } else if resp.clicked() {
        out.push(StripEvent::Select(i));
    }
}
