//! The shell's icons (design §3.4), drawn in code from lines, arcs and circles on a 16 px grid
//! with a 1.5 px stroke (`ecad_tokens::line::ICON_STROKE_PX`). One function per icon, in
//! [`tools`], [`workspaces`] and [`shell`]. Each icon is original and drawn here; none is traced
//! from any product, and CADCraft's icon set is not reused (the two evolve apart).
//!
//! egui strokes have no cap style; ends are butt-ended at 1.5 px, which reads the same as round
//! at this size.

pub mod shell;
pub mod tools;
pub mod workspaces;

use ecad_tokens::{Icon, line, size};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

/// Paint `icon` in `rect` (any size; the 16 px grid is scaled to the shorter side and centred).
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, ink: Color32) {
    painter.extend(shapes(rect, icon, ink));
}

/// The shapes of `icon` in `rect`: stroked lines at 1.5 px and, for eyes and bullets, small
/// filled dots.
pub fn shapes(rect: Rect, icon: Icon, ink: Color32) -> Vec<Shape> {
    let mut pen = Pen::new(rect, ink);
    match icon {
        Icon::Select => tools::select(&mut pen),
        Icon::Wire => tools::wire(&mut pen),
        Icon::Place => tools::place(&mut pen),
        Icon::Cable => tools::cable(&mut pen),
        Icon::Draw => tools::draw(&mut pen),
        Icon::Box => tools::boxed(&mut pen),
        Icon::Text => tools::text(&mut pen),
        Icon::WorkspaceDraw => workspaces::draw(&mut pen),
        Icon::WorkspaceLists => workspaces::lists(&mut pen),
        Icon::WorkspaceLibrary => workspaces::library(&mut pen),
        Icon::WorkspacePublish => workspaces::publish(&mut pen),
        Icon::Search => shell::search(&mut pen),
        Icon::Checks => shell::checks(&mut pen),
        Icon::Agent => shell::agent(&mut pen),
        Icon::History => shell::history(&mut pen),
        Icon::Pages => shell::pages(&mut pen),
        Icon::Devices => shell::devices(&mut pen),
        Icon::Collapse => shell::collapse(&mut pen),
        Icon::Close => shell::close(&mut pen),
        Icon::ChevronLeft => shell::chevron(&mut pen, (10.0, 3.0), (5.0, 8.0), (10.0, 13.0)),
        Icon::ChevronRight => shell::chevron(&mut pen, (6.0, 3.0), (11.0, 8.0), (6.0, 13.0)),
        Icon::ChevronUp => shell::chevron(&mut pen, (3.0, 10.0), (8.0, 5.0), (13.0, 10.0)),
        Icon::ChevronDown => shell::chevron(&mut pen, (3.0, 6.0), (8.0, 11.0), (13.0, 6.0)),
        // 041
        Icon::Delete => tools::delete(&mut pen),
        Icon::GoTo => tools::goto(&mut pen),
        Icon::ShowRow => tools::show_row(&mut pen),
        Icon::Edit => tools::edit(&mut pen),
        Icon::Properties => tools::properties(&mut pen),
        Icon::Rotate => tools::rotate(&mut pen),
        Icon::Mirror => tools::mirror(&mut pen),
        Icon::Duplicate => tools::duplicate(&mut pen),
        Icon::Connect => tools::connect(&mut pen),
        Icon::Disconnect => tools::disconnect(&mut pen),
        Icon::Number => tools::number(&mut pen),
        Icon::Move => tools::moving(&mut pen),
        Icon::Paste => tools::paste(&mut pen),
        Icon::NewPage => tools::new_page(&mut pen),
        Icon::Line => tools::line(&mut pen),
        Icon::Polyline => tools::polyline(&mut pen),
        Icon::Rectangle => tools::rectangle(&mut pen),
        Icon::Circle => tools::circle(&mut pen),
        Icon::Arc => tools::arc(&mut pen),
        Icon::Terminal => tools::terminal(&mut pen),
        Icon::Junction => tools::junction(&mut pen),
        Icon::Cloud => tools::cloud(&mut pen),
        Icon::Eye => tools::eye(&mut pen),
    }
    pen.out
}

/// Draws on the 16 px icon grid (x right, y down, 0 to 16) into a screen rect.
pub struct Pen {
    origin: Pos2,
    scale: f32,
    ink: Color32,
    stroke: Stroke,
    out: Vec<Shape>,
}

impl Pen {
    fn new(rect: Rect, ink: Color32) -> Self {
        let side = rect.width().min(rect.height()).max(0.0);
        let scale = side / size::ICON;
        let origin = rect.center() - egui::vec2(side, side) / 2.0;
        Self { origin, scale, ink, stroke: Stroke::new(line::ICON_STROKE_PX, ink), out: Vec::new() }
    }

    /// A grid point on screen.
    pub fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.origin.x + x * self.scale, self.origin.y + y * self.scale)
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        let shape = Shape::line_segment([self.p(a.0, a.1), self.p(b.0, b.1)], self.stroke);
        self.out.push(shape);
    }

    /// An open polyline (`closed` joins the last point to the first).
    pub fn poly(&mut self, points: &[(f32, f32)], closed: bool) {
        let pts: Vec<Pos2> = points.iter().map(|(x, y)| self.p(*x, *y)).collect();
        self.out.push(if closed { Shape::closed_line(pts, self.stroke) } else { Shape::line(pts, self.stroke) });
    }

    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true);
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.out.push(Shape::circle_stroke(self.p(cx, cy), r * self.scale, self.stroke));
    }

    /// A small filled dot (eyes, bullets).
    pub fn dot(&mut self, cx: f32, cy: f32, r: f32) {
        self.out.push(Shape::circle_filled(self.p(cx, cy), r * self.scale, self.ink));
    }

    /// An arc from `from` to `to` degrees (0 = right, 90 = down), as a polyline.
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, from: f32, to: f32) {
        const STEPS: usize = 16;
        let pts: Vec<(f32, f32)> = (0..=STEPS)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / STEPS as f32).to_radians();
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.poly(&pts, false);
    }

    /// A dashed line: `on` and `off` in grid units.
    pub fn dashed(&mut self, a: (f32, f32), b: (f32, f32), on: f32, off: f32) {
        let pts = [self.p(a.0, a.1), self.p(b.0, b.1)];
        self.out.extend(Shape::dashed_line(&pts, self.stroke, on * self.scale, off * self.scale));
    }
}
