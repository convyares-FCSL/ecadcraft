//! Project ▸ Wires (UI rev 3, board 2.04): every wire, grouped by potential (or none, page,
//! cable), as the one table over the wire report (`Outputs::wire_table`). Number, from, to,
//! colour, size, category, page and status are read from the drawing; ferrules and length belong
//! to the cable manager (brief 2.7), not built yet, and show a dash. A provisional wire number
//! is marked. Nothing here is set: a wire is changed on its page.
//!
//! **Update map:** see [`super::report`]: the copy is taken again only when the report models'
//! revision changed, and only while the tab is shown.

use ecad_cadcraft::outputs::Outputs;
use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};

use super::report::{From, ReportColumns, ReportRows};

pub const WIRE: ColumnId = ColumnId(0);
pub const FROM: ColumnId = ColumnId(1);
pub const TO: ColumnId = ColumnId(2);
pub const COLOUR: ColumnId = ColumnId(3);
pub const SIZE: ColumnId = ColumnId(4);
pub const CATEGORY: ColumnId = ColumnId(5);
pub const FERRULES: ColumnId = ColumnId(6);
pub const LENGTH: ColumnId = ColumnId(7);
pub const PAGE: ColumnId = ColumnId(8);
pub const STATUS: ColumnId = ColumnId(9);
/// Group columns, not shown.
pub const POTENTIAL: ColumnId = ColumnId(20);
pub const CABLE: ColumnId = ColumnId(21);
pub const ON_PAGE: ColumnId = ColumnId(22);

/// The grouping switch (board 2.04: None, Potential, Page, Cable).
pub const GROUPS: [&str; 4] = ["None", "Potential", "Page", "Cable"];

/// The status words.
pub const OK: &str = "ok";
pub const PROVISIONAL: &str = "provisional";
pub const CHECK: &str = "check";

pub fn columns() -> Vec<Column> {
    vec![
        Column::read(WIRE.0, "Wire", Width::Initial(110.0)),
        Column::read(FROM.0, "From", Width::Initial(330.0)),
        Column::read(TO.0, "To", Width::Initial(330.0)),
        Column::read(COLOUR.0, "Colour", Width::Initial(110.0)),
        Column::read(SIZE.0, "Size", Width::Initial(90.0)),
        Column::read(CATEGORY.0, "Category", Width::Initial(110.0)),
        Column::read(FERRULES.0, "Ferrules", Width::Initial(140.0)).unsorted(),
        Column::read(LENGTH.0, "Length", Width::Initial(76.0)).unsorted(),
        Column::read(PAGE.0, "Page", Width::Initial(76.0)),
        Column::read(STATUS.0, "Status", Width::Remainder),
    ]
}

/// The wire report as a table source.
#[derive(Debug)]
pub struct WiresSource {
    cols: Vec<Column>,
    pub rows: ReportRows,
}

impl Default for WiresSource {
    fn default() -> Self {
        Self { cols: columns(), rows: ReportRows::default() }
    }
}

/// The page part of a place (`2/C1` → `2`; `108-B-2` → `108`).
pub fn page_of(place: &str) -> String {
    place.split(['/', '.', '-']).find(|s| !s.is_empty()).unwrap_or(place).to_string()
}

impl WiresSource {
    /// Whether row `k`'s number is provisional.
    pub fn provisional(&self, k: usize) -> bool {
        ecad_model::tag::is_provisional(&self.rows.text(k, "Wire number"))
    }
}

impl ReportColumns for WiresSource {
    fn rows(&self) -> &ReportRows {
        &self.rows
    }
    fn cols(&self) -> &[Column] {
        &self.cols
    }
    fn from(&self, col: ColumnId) -> From {
        match col {
            WIRE => From::Derived,
            FROM => From::Header("From"),
            TO => From::Header("To"),
            COLOUR => From::Header("Colour"),
            SIZE => From::Header("Cross-section"),
            CATEGORY => From::Header("Category"),
            PAGE => From::Header("Page"),
            POTENTIAL => From::Header("Potential"),
            CABLE => From::Header("Cable"),
            FERRULES | LENGTH => From::Placeholder,
            _ => From::Derived,
        }
    }
    fn derived(&self, k: usize, col: ColumnId) -> Cell {
        match col {
            WIRE if self.provisional(k) => Cell::Provisional(self.rows.text(k, "Wire number")),
            WIRE => self.rows.cell(k, "Wire number"),
            ON_PAGE => Cell::Text(page_of(&self.rows.text(k, "Page"))),
            STATUS if self.provisional(k) => Cell::Badge(PROVISIONAL.into(), Tone::Provisional),
            STATUS if self.rows.flag(k).is_some() => Cell::Badge(CHECK.into(), Tone::Warning),
            STATUS => Cell::Badge(OK.into(), Tone::Ok),
            _ => Cell::Empty,
        }
    }
}

impl TableSource for WiresSource {
    type Key = usize;
    fn columns(&self) -> &[Column] {
        &self.cols
    }
    fn keys(&self) -> &[usize] {
        &self.rows.keys
    }
    fn contains(&self, key: usize) -> bool {
        key < self.rows.rows.len()
    }
    fn cell(&self, key: usize, col: ColumnId) -> Cell {
        super::report::cell(self, key, col)
    }
}

/// The Wires tab's state.
#[derive(Debug)]
pub struct WiresView {
    pub source: WiresSource,
    pub table: Table<usize>,
    /// The grouping (index into [`GROUPS`]).
    pub group: usize,
    /// Where each row's wire is drawn (its go-to).
    pub gos: Vec<Option<(ecad_model::PageId, ecad_model::Pt)>>,
    seen: Option<(u64, u64)>,
    /// How many times the copy was taken (tests: the update map).
    pub copies: u64,
}

impl Default for WiresView {
    fn default() -> Self {
        let mut table = Table::new("ecad.lists.wires", "wires");
        table.toolbar = false;
        table.empty = "No wires yet.";
        Self { source: WiresSource::default(), table, group: 1, gos: Vec::new(), seen: None, copies: 0 }
    }
}

impl WiresView {
    /// Bring the copy up to date (only when the models' revision changed).
    pub fn sync(&mut self, serial: u64, o: &Outputs) {
        let key = (serial, o.build_id ^ o.revision);
        if self.seen == Some(key) {
            return;
        }
        self.seen = Some(key);
        self.source.rows.take(o.wire_table());
        self.gos = o.wires().iter().map(|w| w.go).collect();
        self.copies += 1;
        self.table.update(&self.source, Changed::All);
        self.apply_group();
    }

    /// Forget the copy (shown again, reload).
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.table.reset();
    }

    /// Group by [`GROUPS`]`[i]`.
    pub fn set_group(&mut self, i: usize) {
        self.group = i.min(GROUPS.len() - 1);
        self.apply_group();
    }

    fn apply_group(&mut self) {
        let by = match self.group {
            1 => Some(POTENTIAL),
            2 => Some(ON_PAGE),
            3 => Some(CABLE),
            _ => None,
        };
        self.table.view.set_group(&self.source, by);
    }

    /// The selected wire's row.
    pub fn selected(&self) -> Option<usize> {
        self.table.selection.cursor().or_else(|| self.table.selection.keys().iter().next().copied())
    }

    /// The footer's counts: wires, provisional numbers, rows with a check.
    pub fn counts(&self) -> (usize, usize, usize) {
        let keys = &self.source.rows.keys;
        let prov = keys.iter().filter(|k| self.source.provisional(**k)).count();
        let flagged = keys.iter().filter(|k| self.source.rows.flag(**k).is_some()).count();
        (keys.len(), prov, flagged)
    }

    /// Draw the table. Returns where to go (a double-click or a read cell's go-to).
    pub fn show(&mut self, ui: &mut eframe::egui::Ui) -> Option<(ecad_model::PageId, ecad_model::Pt)> {
        let mut go = None;
        for e in self.table.show(ui, &self.source) {
            match e {
                TableEvent::GoTo { key, .. } | TableEvent::Open(key) => go = self.gos.get(key).copied().flatten(),
                TableEvent::Export(csv) => super::copy_csv(ui, csv),
                _ => {}
            }
        }
        go
    }
}
