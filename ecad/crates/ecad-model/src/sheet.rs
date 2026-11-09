//! Sheets (brief 019, design 019 §3): the size and grid of a page's frame, and where a point lies
//! on that grid.
//!
//! # Semantics
//! - A [`SheetSpec`] is stored in whole millimetres whatever the project's length unit; drawing
//!   geometry divides by 25.4 in an inch project ([`unit_scale`]).
//! - The model origin is the **page's bottom-left corner**, the paper edge (owner, 2026-11-08, fix
//!   round): the paper runs from (0, 0) to (W, H); the frame from (10, 10) to (W − 10, H − 7).
//!   Nothing assumes a title block or a bottom margin of any width.
//! - The grid's zones span the **whole frame**, title-block area included: columns run left to
//!   right from the frame's left edge, **row 1 is the top row**, counted down from the frame's top
//!   edge. A point outside the frame is clamped to the nearest column and row.
//! - The project's sheet is [`crate::ProjectSettings::sheet`]; a page may override it with its
//!   `sheet` property ([`SHEET_KEY`]), a text spec ([`SheetSpec::parse`]).
//!
//! This module only computes; it stores nothing and draws nothing (the drawing is
//! `ecad-cadcraft`'s).

use serde::{Deserialize, Serialize};

use crate::doc::{Document, LengthUnit};
use crate::id::PageId;

/// The page property holding a page's own sheet spec (text, [`SheetSpec::parse`]).
pub const SHEET_KEY: &str = "sheet";

/// Margins in millimetres: left, right and bottom (design 019 §3.1).
pub const MARGIN_MM: f64 = 10.0;
/// The top margin, in millimetres, where the column labels sit.
pub const TOP_MARGIN_MM: f64 = 7.0;
/// Height of the title block under the drawing area, in millimetres.
pub const TITLE_BLOCK_MM: f64 = 40.0;

/// Smallest and largest custom sheet (design 019 §3.1, owner answer Q10): smaller sheets cannot
/// hold the title block.
pub const MIN_WIDTH_MM: u32 = 420;
pub const MAX_WIDTH_MM: u32 = 2400;
pub const MIN_HEIGHT_MM: u32 = 250;
pub const MAX_HEIGHT_MM: u32 = 2400;
/// Most columns or rows of a grid (one letter each).
pub const MAX_GRID: u32 = 26;

/// How a grid's columns or rows are labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GridLabels {
    /// `A`, `B`, … `Z`.
    #[default]
    Letters,
    /// `1`, `2`, …
    Numbers,
}

impl GridLabels {
    /// The label of the 1-based column or row `n`.
    pub fn label(self, n: u32) -> String {
        match self {
            GridLabels::Numbers => n.to_string(),
            GridLabels::Letters => letters(n),
        }
    }

    fn word(self) -> &'static str {
        match self {
            GridLabels::Letters => "letters",
            GridLabels::Numbers => "numbers",
        }
    }
}

/// `1 → A`, `26 → Z`, `27 → AA` (0 gives an empty string).
fn letters(mut n: u32) -> String {
    let mut out = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.push(char::from(b'A' + rem as u8));
        n = (n - 1) / 26;
    }
    out.iter().rev().collect()
}

/// A sheet: its paper size in whole millimetres, landscape, and its grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetSpec {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
    pub column_labels: GridLabels,
    pub row_labels: GridLabels,
}

/// The preset sheets (landscape) with their default grids: name, width, height, columns, rows.
pub const PRESETS: [(&str, u32, u32, u32, u32); 4] =
    [("A3", 420, 297, 8, 6), ("A2", 594, 420, 12, 8), ("A1", 841, 594, 16, 12), ("A0", 1189, 841, 24, 16)];

impl Default for SheetSpec {
    /// A3 landscape, columns `A`–`H`, rows `1`–`6` (design 019 §3.1).
    fn default() -> Self {
        SheetSpec { width: 420, height: 297, columns: 8, rows: 6, column_labels: GridLabels::Letters, row_labels: GridLabels::Numbers }
    }
}

/// The refusal for a spec that cannot be read (design 019 §6.1).
pub fn unknown_sheet(text: &str) -> String {
    format!("Sheet \"{text}\" is not known. Use A3, A2, A1, A0 or WxH, then columns x rows.")
}

impl SheetSpec {
    /// The preset `name` (`A3`, `A2`, `A1`, `A0`, any case) with its default grid.
    pub fn preset(name: &str) -> Option<SheetSpec> {
        PRESETS.iter().find(|p| p.0.eq_ignore_ascii_case(name)).map(|&(_, width, height, columns, rows)| SheetSpec {
            width,
            height,
            columns,
            rows,
            ..SheetSpec::default()
        })
    }

    /// The preset name of this paper size, if it is one.
    pub fn preset_name(&self) -> Option<&'static str> {
        PRESETS.iter().find(|p| p.1 == self.width && p.2 == self.height).map(|p| p.0)
    }

    /// Whether the size and grid are within the limits of design 019 §3.1.
    ///
    /// # Errors
    /// The limit broken, in words.
    pub fn check(&self) -> Result<(), String> {
        if !(MIN_WIDTH_MM..=MAX_WIDTH_MM).contains(&self.width) {
            return Err(format!("The width must be {MIN_WIDTH_MM} to {MAX_WIDTH_MM} mm."));
        }
        if !(MIN_HEIGHT_MM..=MAX_HEIGHT_MM).contains(&self.height) {
            return Err(format!("The height must be {MIN_HEIGHT_MM} to {MAX_HEIGHT_MM} mm."));
        }
        if !(1..=MAX_GRID).contains(&self.columns) || !(1..=MAX_GRID).contains(&self.rows) {
            return Err(format!("Columns and rows must be 1 to {MAX_GRID}."));
        }
        Ok(())
    }

    /// Read a text spec: a preset (`A3`) or a size (`594x420`), then optionally a grid (`12x8`),
    /// then optionally the column and row labels (`letters`/`numbers`, in that order). Words are
    /// separated by spaces; `×` may stand for `x`; case does not matter.
    ///
    /// # Errors
    /// [`unknown_sheet`] for text that does not read so, or a size or grid out of range.
    pub fn parse(text: &str) -> Result<SheetSpec, String> {
        let bad = || unknown_sheet(text.trim());
        let lower = text.trim().to_ascii_lowercase().replace('×', "x");
        let mut words = lower.split_whitespace();
        let first = words.next().ok_or_else(bad)?;
        let mut spec = match SheetSpec::preset(first) {
            Some(s) => s,
            None => {
                let (w, h) = pair(first).ok_or_else(bad)?;
                let mut s = SheetSpec { width: w, height: h, ..SheetSpec::default() };
                // A custom size keeps the default grid unless a preset of that size has another.
                if let Some(p) = PRESETS.iter().find(|p| p.1 == w && p.2 == h) {
                    (s.columns, s.rows) = (p.3, p.4);
                }
                s
            }
        };
        let mut labels = Vec::new();
        for w in words {
            if let Some((c, r)) = pair(w) {
                if !labels.is_empty() {
                    return Err(bad());
                }
                (spec.columns, spec.rows) = (c, r);
            } else {
                labels.push(match w {
                    "letters" => GridLabels::Letters,
                    "numbers" => GridLabels::Numbers,
                    _ => return Err(bad()),
                });
            }
        }
        match labels.as_slice() {
            [] => {}
            [c] => spec.column_labels = *c,
            [c, r] => (spec.column_labels, spec.row_labels) = (*c, *r),
            _ => return Err(bad()),
        }
        spec.check()?;
        Ok(spec)
    }

    /// The shortest text [`SheetSpec::parse`] reads back as this spec: `A3`, `A3 10x6`,
    /// `594x420 12x8 numbers letters`.
    pub fn text(&self) -> String {
        let preset = PRESETS.iter().find(|p| p.1 == self.width && p.2 == self.height);
        let mut out = match preset {
            Some(p) => p.0.to_string(),
            None => format!("{}x{}", self.width, self.height),
        };
        let (dc, dr) = preset.map_or((8, 6), |p| (p.3, p.4));
        if (self.columns, self.rows) != (dc, dr) {
            out.push_str(&format!(" {}x{}", self.columns, self.rows));
        }
        let d = SheetSpec::default();
        if (self.column_labels, self.row_labels) != (d.column_labels, d.row_labels) {
            out.push_str(&format!(" {} {}", self.column_labels.word(), self.row_labels.word()));
        }
        out
    }

    /// `A3, 8×6` (or `594×420, 12×8`): how the Properties panel names a sheet.
    pub fn summary(&self) -> String {
        let size = self.preset_name().map_or_else(|| format!("{}×{}", self.width, self.height), str::to_string);
        format!("{size}, {}×{}", self.columns, self.rows)
    }
}

/// `594x420` → (594, 420).
fn pair(w: &str) -> Option<(u32, u32)> {
    let (a, b) = w.split_once('x')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Millimetres to the project's length unit.
pub fn unit_scale(unit: LengthUnit) -> f64 {
    match unit {
        LengthUnit::Millimetre => 1.0,
        LengthUnit::Inch => 1.0 / 25.4,
    }
}

/// The sheet `page` is drawn on: its own `sheet` property when that reads, else the project's.
///
/// The one place a page's sheet is decided, so the frame, the references, the Devices tab and the
/// Properties panel always agree: the page's own spec when it reads and is within the limits
/// ([`SheetSpec::check`]), else the project's when that is within them (a hostile file may hold
/// any numbers), else the default A3.
pub fn sheet_of(doc: &Document, page: PageId) -> SheetSpec {
    doc.pages
        .get(&page)
        .and_then(|p| p.properties.get(SHEET_KEY))
        .and_then(|t| SheetSpec::parse(t).ok())
        .or_else(|| doc.settings.sheet.check().ok().map(|()| doc.settings.sheet))
        .unwrap_or_default()
}

/// The frame's distance from the paper edge on each side, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
    pub top: f64,
}

impl Default for Margins {
    /// 10 mm left, right and bottom, 7 mm at the top (design 019 §3.1).
    fn default() -> Self {
        Margins { left: MARGIN_MM, right: MARGIN_MM, bottom: MARGIN_MM, top: TOP_MARGIN_MM }
    }
}

/// The corner the zones are counted from (owner note of 2026-11-08: a project setting later;
/// today always [`Corner::TopLeft`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A sheet's grid in drawing units: where columns and rows are, and their labels.
///
/// The zone counts and label styles come from the [`SheetSpec`]; the margins and the corner the
/// counting starts from are parameters too ([`Grid::with_layout`]), so a later project setting
/// for them needs no change here (owner note of 2026-11-08).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub spec: SheetSpec,
    /// Millimetres to drawing units.
    pub k: f64,
    pub margins: Margins,
    pub start: Corner,
}

impl Grid {
    /// The grid of `spec` in a project of length unit `unit`, with the default margins, counted
    /// from the top-left.
    pub fn new(spec: SheetSpec, unit: LengthUnit) -> Grid {
        Grid::with_layout(spec, unit, Margins::default(), Corner::TopLeft)
    }

    /// The grid of `spec` with its own margins and starting corner.
    pub fn with_layout(spec: SheetSpec, unit: LengthUnit, margins: Margins, start: Corner) -> Grid {
        Grid { spec, k: unit_scale(unit), margins, start }
    }

    /// The column number of the `i`-th zone from the left (0-based), as counted from the start
    /// corner.
    pub fn column_number(&self, i: u32) -> u32 {
        let c = self.spec.columns.max(1);
        match self.start {
            Corner::TopLeft | Corner::BottomLeft => i + 1,
            Corner::TopRight | Corner::BottomRight => c.saturating_sub(i),
        }
    }

    /// The row number of the `i`-th zone from the top (0-based), as counted from the start corner.
    pub fn row_number(&self, i: u32) -> u32 {
        let r = self.spec.rows.max(1);
        match self.start {
            Corner::TopLeft | Corner::TopRight => i + 1,
            Corner::BottomLeft | Corner::BottomRight => r.saturating_sub(i),
        }
    }

    /// The grid of `page` in `doc`.
    pub fn of(doc: &Document, page: PageId) -> Grid {
        Grid::new(sheet_of(doc, page), doc.settings.length_unit)
    }

    /// The frame's left edge (the grid's first column starts here), in drawing units.
    pub fn frame_left(&self) -> f64 {
        self.margins.left * self.k
    }

    /// The frame's bottom edge, in drawing units.
    pub fn frame_bottom(&self) -> f64 {
        self.margins.bottom * self.k
    }

    /// The frame's top edge (row 1 starts here), in drawing units.
    pub fn frame_top(&self) -> f64 {
        (f64::from(self.spec.height) - self.margins.top) * self.k
    }

    /// Width of the frame (all columns), in drawing units.
    pub fn area_width(&self) -> f64 {
        (f64::from(self.spec.width) - self.margins.left - self.margins.right) * self.k
    }

    /// Height of the frame (all rows, title-block area included), in drawing units.
    pub fn area_height(&self) -> f64 {
        (f64::from(self.spec.height) - self.margins.bottom - self.margins.top) * self.k
    }

    /// Width of one column.
    pub fn column_width(&self) -> f64 {
        self.area_width() / f64::from(self.spec.columns.max(1))
    }

    /// Height of one row.
    pub fn row_height(&self) -> f64 {
        self.area_height() / f64::from(self.spec.rows.max(1))
    }

    /// The 1-based column of `x`, clamped to the grid; `None` when `x` is not finite.
    pub fn column_of(&self, x: f64) -> Option<u32> {
        let i = index_of(x - self.frame_left(), self.column_width(), self.spec.columns)?;
        Some(self.column_number(i - 1))
    }

    /// The 1-based row of `y` (row 1 at the top), clamped to the grid; `None` when `y` is not
    /// finite.
    pub fn row_of(&self, y: f64) -> Option<u32> {
        let i = index_of(self.frame_top() - y, self.row_height(), self.spec.rows)?;
        Some(self.row_number(i - 1))
    }

    /// The label of column `n`.
    pub fn column_label(&self, n: u32) -> String {
        self.spec.column_labels.label(n)
    }

    /// The label of row `n`.
    pub fn row_label(&self, n: u32) -> String {
        self.spec.row_labels.label(n)
    }
}

fn index_of(v: f64, size: f64, count: u32) -> Option<u32> {
    if !(v.is_finite() && size.is_finite() && size > 0.0) {
        return None;
    }
    let n = ((v / size).floor() + 1.0).clamp(1.0, f64::from(count.max(1)));
    // Finite and within 1..=count here, so the conversion is exact.
    Some(n as u32)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn specs_read_and_write_back() {
        for (text, back) in [
            ("A3", "A3"),
            ("a2", "A2"),
            ("A3 10x6", "A3 10x6"),
            ("594x420 12x8 numbers letters", "A2 numbers letters"),
            ("600×400", "600x400"),
            ("1000x500 20x10", "1000x500 20x10"),
        ] {
            let s = SheetSpec::parse(text).unwrap();
            assert_eq!(s.text(), back, "{text}");
            assert_eq!(SheetSpec::parse(&s.text()).unwrap(), s);
        }
        for bad in ["", "A5", "A3 x", "100x100", "A3 30x6", "A3 letters numbers letters", "A3 letters 8x6"] {
            assert!(SheetSpec::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(SheetSpec::parse("A5").unwrap_err(), unknown_sheet("A5"));
    }

    #[test]
    fn the_a3_grid_places_points() {
        let g = Grid::new(SheetSpec::default(), LengthUnit::Millimetre);
        assert_eq!((g.area_width(), g.area_height(), g.column_width()), (400.0, 280.0, 50.0));
        // Column C is x 110..160; row 1 is y 290 down to 243.3, row 6 reaches the frame's bottom.
        assert_eq!(g.column_of(130.0), Some(3));
        assert_eq!(g.column_label(3), "C");
        assert_eq!(g.column_of(5.0), Some(1));
        assert_eq!(g.column_of(1e9), Some(8));
        assert_eq!(g.row_of(289.0), Some(1));
        assert_eq!(g.row_of(120.0), Some(4));
        assert_eq!(g.row_of(15.0), Some(6), "the title-block area is in the last row");
        assert_eq!(g.row_of(-20.0), Some(6));
        assert_eq!(g.row_label(4), "4");
        assert_eq!(g.column_of(f64::NAN), None);
        assert_eq!(letters(27), "AA");
        // Counted from the bottom-right instead: the same point is in column F, row 3.
        let br = Grid::with_layout(SheetSpec::default(), LengthUnit::Millimetre, Margins::default(), Corner::BottomRight);
        assert_eq!((br.column_of(130.0), br.row_of(120.0)), (Some(6), Some(3)));
    }
}
