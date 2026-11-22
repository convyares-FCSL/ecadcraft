//! Project ▸ Wires (UI rev 3, board 2.04): every wire, grouped by potential (or none, page,
//! cable), as the one table over the wire report (`ReportModels::wire_table`). Number, from, to,
//! colour, size, category, page and status are read from the drawing; ferrules and length belong
//! to the cable manager (brief 2.7), not built yet, and show a dash. A provisional wire number
//! is marked. Nothing here is set: a wire is changed on its page.
//!
//! **Update map:** see [`crate::reports::report`]: the copy is taken again only when the report models'
//! revision changed, and only while the tab is shown.

use ecad_electrical::reports::kept::ReportModels;
use ecad_widgets::prim::Tone;
use ecad_widgets::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};

use crate::reports::report::{From, ReportColumns, ReportRows};

/// Brief 057: a copy of the kept rating results (`Checks::rating`), patched by subject: at the
/// next epoch only the subjects the update re-checked are copied again (design 057 §10).
#[derive(Debug, Default)]
pub struct RatingCopy {
    pub results: std::collections::BTreeMap<ecad_electrical::circuits::RatedSubject, ecad_electrical::circuits::circuit::Rating>,
    seen: Option<(u64, u64)>,
    /// Full copies and patches (tests: the update map).
    pub copies: u64,
    pub patches: u64,
    /// The methods the project's rating table has rows for.
    pub methods: Vec<&'static str>,
}

impl RatingCopy {
    /// Bring the copy up to date. Returns the subjects copied again, or `None` for a full copy.
    pub fn sync(&mut self, serial: u64, kept: &ecad_electrical::circuits::Kept) -> Option<Vec<ecad_electrical::circuits::RatedSubject>> {
        if self.methods != kept.methods {
            self.methods = kept.methods.clone();
        }
        match self.seen {
            Some((s, e)) if s == serial && e == kept.epoch => Some(Vec::new()),
            Some((s, e)) if s == serial && e + 1 == kept.epoch => {
                self.seen = Some((serial, kept.epoch));
                self.patches += 1;
                let v: Vec<_> = kept.last.iter().copied().collect();
                for subj in &v {
                    match kept.results.get(subj) {
                        Some(r) => self.results.insert(*subj, r.clone()),
                        None => self.results.remove(subj),
                    };
                }
                Some(v)
            }
            _ => {
                self.seen = Some((serial, kept.epoch));
                self.copies += 1;
                self.results = kept.results.clone();
                None
            }
        }
    }
}

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
    /// Brief 057: each row's conductor, and the rating results.
    pub ids: Vec<ecad_model::WireId>,
    pub rating: RatingCopy,
}

impl Default for WiresSource {
    fn default() -> Self {
        Self { cols: columns(), rows: ReportRows::default(), ids: Vec::new(), rating: RatingCopy::default() }
    }
}

/// The page part of a place (`2/C1` → `2`; `108-B-2` → `108`).
pub fn page_of(place: &str) -> String {
    place.split(['/', '.', '-']).find(|s| !s.is_empty()).unwrap_or(place).to_string()
}

impl WiresSource {
    /// Whether row `k`'s number is provisional.
    pub fn provisional(&self, k: usize) -> bool {
        ecad_model::naming::tag::is_provisional(&self.rows.text(k, "Wire number"))
    }

    /// Brief 057: row `k`'s rating result.
    pub fn rating(&self, k: usize) -> Option<&ecad_electrical::circuits::circuit::Rating> {
        self.ids.get(k).and_then(|c| self.rating.results.get(&ecad_electrical::circuits::RatedSubject::Wire(*c)))
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
            // Brief 057 (design 057 §2.1): the worst rating row in words.
            STATUS if let Some((w, fails)) = self.rating(k).and_then(|r| r.status().map(|w| (w, r.fails()))) => {
                Cell::Badge(w, if fails { Tone::Error } else { Tone::Warning })
            }
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
        crate::reports::report::cell(self, key, col)
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
    /// Whether each row's colour and cross-section are its own, overriding its category's (the
    /// right panel's hints: "set here" or "from category").
    pub own: Vec<(bool, bool)>,
    seen: Option<(u64, u64)>,
    /// How many times the copy was taken (tests: the update map).
    pub copies: u64,
}

impl Default for WiresView {
    fn default() -> Self {
        let mut table = Table::new("ecad.lists.wires", "wires");
        table.toolbar = false;
        table.empty = "No wires yet.";
        Self { source: WiresSource::default(), table, group: 1, gos: Vec::new(), own: Vec::new(), seen: None, copies: 0 }
    }
}

impl WiresView {
    /// Bring the copy up to date (only when the models' revision changed).
    pub fn sync(&mut self, serial: u64, o: &ReportModels) {
        let key = (serial, o.build_id ^ o.revision);
        if self.seen == Some(key) {
            return;
        }
        self.seen = Some(key);
        self.source.rows.take(o.wire_table());
        self.gos = o.wires().iter().map(|w| w.go).collect();
        self.source.ids = o.wires().iter().map(|w| w.conductor).collect();
        self.own = o.wires().iter().map(|w| (w.colour_own, w.cross_section_own)).collect();
        self.copies += 1;
        self.table.update(&self.source, Changed::All);
        self.apply_group();
    }

    /// Brief 057: bring the rating copy up to date and patch the Status cells of the re-checked
    /// wires only.
    pub fn sync_rating(&mut self, serial: u64, kept: &ecad_electrical::circuits::Kept) {
        match self.source.rating.sync(serial, kept) {
            None => self.table.update(&self.source, Changed::All),
            Some(v) if v.is_empty() => {}
            Some(v) => {
                let rows: Vec<usize> = self
                    .source
                    .ids
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| v.contains(&ecad_electrical::circuits::RatedSubject::Wire(**c)))
                    .map(|(k, _)| k)
                    .collect();
                if !rows.is_empty() {
                    self.table.update(&self.source, Changed::Keys(rows));
                }
            }
        }
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
                TableEvent::Export(csv) => crate::workspace::copy_csv(ui, csv),
                _ => {}
            }
        }
        go
    }
}
