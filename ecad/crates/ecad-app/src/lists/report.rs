//! A column set over one of the kept report models' tables (`ecad_cadcraft::outputs::Table`):
//! UI rev 3's Wires and Cables tabs (boards 2.04, 2.05) and the Reports tab's list. Each column
//! names the report header it reads, or is a placeholder for data that is not built yet (the cable
//! manager's ferrules, lengths and glands, brief 2.7), shown as a dash.
//!
//! **Update map.** The rows are a copy of the report table, taken again only when the report
//! models' revision changed (`Outputs::revision`, kept from each change's ops), exactly as the
//! Reports window did (design 024 §6.2); every other commit reads nothing. The copy is the
//! whole table (`Changed::All`): the models do not say which rows a change touched.

use ecad_cadcraft::outputs;
use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Column, ColumnId};

/// What a column shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum From {
    /// The report's column with this header.
    Header(&'static str),
    /// Not built yet: a dash (the cable manager's data).
    Placeholder,
    /// Worked out by the view (status, page group).
    Derived,
}

/// A dash, for a value not built yet.
pub const DASH: &str = "\u{2014}";

/// The rows of one report table, keyed by their index in it.
#[derive(Debug, Default)]
pub struct ReportRows {
    pub headers: Vec<&'static str>,
    pub rows: Vec<Vec<outputs::Cell>>,
    pub keys: Vec<usize>,
}

impl ReportRows {
    /// Copy `t`.
    pub fn take(&mut self, t: outputs::Table) {
        self.keys = (0..t.rows.len()).collect();
        self.headers = t.headers;
        self.rows = t.rows;
    }

    /// The cell of row `k` under `header` (`None`: no such row or column).
    pub fn get(&self, k: usize, header: &str) -> Option<&outputs::Cell> {
        let i = self.headers.iter().position(|h| *h == header)?;
        self.rows.get(k)?.get(i)
    }

    /// Its text, or empty.
    pub fn text(&self, k: usize, header: &str) -> String {
        self.get(k, header).map(|c| c.text.clone()).unwrap_or_default()
    }

    /// Whether any cell of row `k` is flagged (`?`), and the first reason.
    pub fn flag(&self, k: usize) -> Option<String> {
        self.rows.get(k)?.iter().find_map(|c| c.flag.clone())
    }

    /// Row `k`'s cell under `header` as the table shows it: a flagged value in the warning tone.
    pub fn cell(&self, k: usize, header: &str) -> Cell {
        match self.get(k, header) {
            Some(c) if c.flag.is_some() => Cell::Badge(c.text.clone(), Tone::Warning),
            Some(c) => Cell::Text(c.text.clone()),
            None => Cell::Empty,
        }
    }
}

/// The table source: the columns, each with what it shows.
pub trait ReportColumns {
    fn rows(&self) -> &ReportRows;
    fn cols(&self) -> &[Column];
    fn from(&self, col: ColumnId) -> From;
    /// A [`From::Derived`] column's cell.
    fn derived(&self, k: usize, col: ColumnId) -> Cell;
}

/// A report table's cell for column `col`.
pub fn cell<S: ReportColumns + ?Sized>(s: &S, k: usize, col: ColumnId) -> Cell {
    match s.from(col) {
        From::Header(h) => s.rows().cell(k, h),
        From::Placeholder => Cell::read(DASH),
        From::Derived => s.derived(k, col),
    }
}
