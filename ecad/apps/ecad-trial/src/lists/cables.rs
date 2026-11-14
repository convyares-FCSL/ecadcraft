//! Project ▸ Cables (UI rev 3, board 2.05): every cable with type, cores, length, ends, cores used
//! and checks, as the one table over the cable report (`Outputs::cable_table`); under it the
//! selected cable's core table (`Outputs::core_table`). Brief 057 (board 2.06): the cable drawn end
//! to end ([`super::cable_view`]) with each core in its cable type's colour; the glands' sizes come
//! with the cable manager (2.7) and show a dash. Nothing here is set: a cable is changed on its
//! page.
//!
//! **Update map:** see [`super::report`]: the cable copy is taken again only when the report
//! models' revision changed, and only while the tab is shown; the core table also when another
//! cable is selected.

use ecad_cadcraft::outputs::Outputs;
use ecad_model::CableId;
use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};

use super::report::{From, ReportColumns, ReportRows};

pub const CABLE: ColumnId = ColumnId(0);
pub const TYPE: ColumnId = ColumnId(1);
pub const CORES: ColumnId = ColumnId(2);
pub const LENGTH: ColumnId = ColumnId(3);
pub const FROM: ColumnId = ColumnId(4);
pub const TO: ColumnId = ColumnId(5);
pub const GLANDS: ColumnId = ColumnId(6);
pub const USED: ColumnId = ColumnId(7);
pub const CHECKS: ColumnId = ColumnId(8);

/// The two views of the selected cable (board 2.06).
pub const VIEWS: [&str; 2] = ["Cable view", "Cores only"];

pub fn columns() -> Vec<Column> {
    vec![
        Column::read(CABLE.0, "Cable", Width::Initial(90.0)),
        Column::read(TYPE.0, "Type", Width::Initial(150.0)),
        Column::read(CORES.0, "Cores", Width::Initial(110.0)),
        Column::read(LENGTH.0, "Length", Width::Initial(80.0)),
        Column::read(FROM.0, "From", Width::Initial(390.0)),
        Column::read(TO.0, "To", Width::Initial(390.0)),
        Column::read(GLANDS.0, "Glands", Width::Initial(120.0)).unsorted(),
        Column::read(USED.0, "Cores used", Width::Initial(110.0)),
        Column::read(CHECKS.0, "Checks", Width::Remainder),
    ]
}

/// The cable report as a table source.
#[derive(Debug)]
pub struct CablesSource {
    cols: Vec<Column>,
    pub rows: ReportRows,
    /// Brief 057: each row's cable, and the rating results.
    pub ids: Vec<CableId>,
    pub rating: super::wires::RatingCopy,
}

impl Default for CablesSource {
    fn default() -> Self {
        Self { cols: columns(), rows: ReportRows::default(), ids: Vec::new(), rating: Default::default() }
    }
}

impl CablesSource {
    /// Brief 057: row `k`'s rating result.
    pub fn rating(&self, k: usize) -> Option<&ecad_cadcraft::rating::Rating> {
        self.ids.get(k).and_then(|c| self.rating.results.get(&ecad_cadcraft::rating::RatedSubject::Cable(*c)))
    }
}

impl ReportColumns for CablesSource {
    fn rows(&self) -> &ReportRows {
        &self.rows
    }
    fn cols(&self) -> &[Column] {
        &self.cols
    }
    fn from(&self, col: ColumnId) -> From {
        match col {
            CABLE => From::Header("Tag"),
            TYPE => From::Header("Type"),
            CORES => From::Header("Cores"),
            FROM => From::Header("From"),
            TO => From::Header("To"),
            GLANDS => From::Placeholder,
            _ => From::Derived,
        }
    }
    fn derived(&self, k: usize, col: ColumnId) -> Cell {
        let r = &self.rows;
        match col {
            LENGTH => match r.get(k, "Length").filter(|c| !c.text.is_empty()) {
                Some(c) => Cell::Text(c.text.clone()),
                None => Cell::read(super::report::DASH),
            },
            USED => Cell::Text(format!("{} of {}", r.text(k, "Used"), r.text(k, "Cores"))),
            CHECKS => {
                let spare = r.text(k, "Spare");
                // Brief 057 (design 057 §2.2): a rating row comes first.
                if let Some(rt) = self.rating(k)
                    && let Some(w) = rt.status()
                {
                    return Cell::Badge(w, if rt.fails() { Tone::Error } else { Tone::Warning });
                }
                match r.flag(k) {
                    Some(_) => Cell::Badge("check".into(), Tone::Warning),
                    None if spare.parse::<u32>().is_ok_and(|n| n > 0) => {
                        Cell::Read { text: format!("{spare} spare core{}", if spare == "1" { "" } else { "s" }), go: false }
                    }
                    None => Cell::Badge("ok".into(), Tone::Ok),
                }
            }
            _ => Cell::Empty,
        }
    }
}

impl TableSource for CablesSource {
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

pub const CORE: ColumnId = ColumnId(0);
pub const COLOUR: ColumnId = ColumnId(1);
pub const CABINET: ColumnId = ColumnId(2);
pub const WIRE: ColumnId = ColumnId(3);
pub const FIELD: ColumnId = ColumnId(4);
pub const STATUS: ColumnId = ColumnId(5);

/// One cable's cores (`Outputs::core_table`).
#[derive(Debug)]
pub struct CoresSource {
    cols: Vec<Column>,
    pub rows: ReportRows,
    /// Brief 057: the selected cable's cable type colours, in core order.
    pub colours: Vec<String>,
}

impl CoresSource {
    /// Row `k`'s core colour (its label when that is a colour code, else the cable type's).
    pub fn colour(&self, k: usize) -> Option<String> {
        super::cable_view::core_colour(&self.rows.text(k, "Core"), k, &self.colours)
    }
}

impl Default for CoresSource {
    fn default() -> Self {
        Self {
            cols: vec![
                Column::read(CORE.0, "Core", Width::Initial(80.0)),
                Column::read(COLOUR.0, "Colour", Width::Initial(110.0)).unsorted(),
                Column::read(CABINET.0, "From", Width::Initial(440.0)),
                Column::read(WIRE.0, "Wire", Width::Initial(140.0)),
                Column::read(FIELD.0, "To", Width::Initial(440.0)),
                Column::read(STATUS.0, "Status", Width::Remainder),
            ],
            rows: ReportRows::default(),
            colours: Vec::new(),
        }
    }
}

impl ReportColumns for CoresSource {
    fn rows(&self) -> &ReportRows {
        &self.rows
    }
    fn cols(&self) -> &[Column] {
        &self.cols
    }
    fn from(&self, col: ColumnId) -> From {
        match col {
            CORE => From::Header("Core"),
            CABINET => From::Header("From"),
            WIRE => From::Header("Wire number"),
            FIELD => From::Header("To"),
            _ => From::Derived,
        }
    }
    fn derived(&self, k: usize, col: ColumnId) -> Cell {
        match (col, self.rows.text(k, "State").as_str()) {
            (COLOUR, _) => match self.colour(k) {
                Some(c) => Cell::Text(c),
                None => Cell::read(super::report::DASH),
            },
            (STATUS, "used") if self.rows.text(k, "To").is_empty() || self.rows.text(k, "From").is_empty() => {
                Cell::Badge("open end".into(), Tone::Error)
            }
            (STATUS, "used") => Cell::Badge("connected".into(), Tone::Ok),
            (STATUS, "spare") => Cell::read("spare"),
            (STATUS, other) => Cell::Badge(other.to_string(), Tone::Warning),
            _ => Cell::Empty,
        }
    }
}

impl TableSource for CoresSource {
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

/// The Cables tab's state.
#[derive(Debug)]
pub struct CablesView {
    pub source: CablesSource,
    pub table: Table<usize>,
    /// Each row's cable and where it is drawn.
    pub ids: Vec<CableId>,
    pub gos: Vec<Option<(ecad_model::PageId, ecad_model::Pt)>>,
    pub cores: CoresSource,
    pub core_table: Table<usize>,
    /// The cable whose cores are shown, and the copy it was read at.
    cores_of: Option<(CableId, (u64, u64))>,
    seen: Option<(u64, u64)>,
    pub copies: u64,
    /// Brief 057 (board 2.06): Cable view (0) or Cores only (1), the ends swapped, and the
    /// selected cable's colours as read at a change count.
    pub view: usize,
    pub swapped: bool,
    colours_of: Option<(CableId, u64)>,
}

impl Default for CablesView {
    fn default() -> Self {
        let mut table = Table::new("ecad.lists.cables", "cables");
        table.toolbar = false;
        table.empty = "No cables yet.";
        let mut core_table = Table::new("ecad.lists.cables.cores", "cores");
        core_table.toolbar = false;
        core_table.empty = "Select a cable to see its cores.";
        Self {
            source: CablesSource::default(),
            table,
            ids: Vec::new(),
            gos: Vec::new(),
            cores: CoresSource::default(),
            core_table,
            cores_of: None,
            seen: None,
            copies: 0,
            view: 0,
            swapped: false,
            colours_of: None,
        }
    }
}

impl CablesView {
    /// Bring the copies up to date (the cables when the models' revision changed; the cores also
    /// when the selected cable changed).
    pub fn sync(&mut self, serial: u64, o: &Outputs) {
        let key = (serial, o.build_id ^ o.revision);
        if self.seen != Some(key) {
            self.seen = Some(key);
            self.source.rows.take(o.cable_table());
            let rows = o.cables();
            self.ids = rows.iter().map(|r| r.cable).collect();
            self.source.ids = self.ids.clone();
            self.gos = rows.iter().map(|r| r.go).collect();
            self.copies += 1;
            self.table.update(&self.source, Changed::All);
            // The first cable is shown until one is picked (board 2.05).
            if self.table.selection.is_empty()
                && let Some(first) = self.table.view.visible_keys().first().copied()
            {
                self.table.selection.set([first]);
            }
        }
        let selected = self.selected().and_then(|k| self.ids.get(k).copied());
        let want = selected.map(|c| (c, key));
        if want != self.cores_of {
            self.cores_of = want;
            match selected {
                Some(c) => self.cores.rows.take(o.core_table(c)),
                None => self.cores.rows = ReportRows::default(),
            }
            self.core_table.update(&self.cores, Changed::All);
        }
    }

    /// Brief 057: bring the rating copy up to date and patch the Checks cells of the re-checked
    /// cables only.
    pub fn sync_rating(&mut self, serial: u64, kept: &ecad_cadcraft::rating::Kept) {
        match self.source.rating.sync(serial, kept) {
            None => self.table.update(&self.source, Changed::All),
            Some(v) if v.is_empty() => {}
            Some(v) => {
                let rows: Vec<usize> = self
                    .ids
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| v.contains(&ecad_cadcraft::rating::RatedSubject::Cable(**c)))
                    .map(|(k, _)| k)
                    .collect();
                if !rows.is_empty() {
                    self.table.update(&self.source, Changed::Keys(rows));
                }
            }
        }
    }

    /// Brief 057: the selected cable's core colours, read again when the selection or the project
    /// changed (one cable's type, never the project).
    pub fn sync_colours(&mut self, doc: &ecad_model::Document, count: u64) {
        let selected = self.selected().and_then(|k| self.ids.get(k).copied());
        let want = selected.map(|c| (c, count));
        if want == self.colours_of {
            return;
        }
        self.colours_of = want;
        self.cores.colours = selected.map(|c| ecad_cadcraft::styles::cable::core_colours(doc, c)).unwrap_or_default();
        self.core_table.update(&self.cores, Changed::All);
    }

    /// The selected cable as drawn (board 2.06).
    pub fn drawing(&self, k: usize) -> super::cable_view::Drawing {
        let rows = &self.source.rows;
        let words =
            [rows.text(k, "Type"), rows.text(k, "Length")].into_iter().filter(|w| !w.is_empty() && w != "?").collect::<Vec<_>>().join(" \u{b7} ");
        let c = &self.cores.rows;
        let cores = c
            .keys
            .iter()
            .map(|i| super::cable_view::Core {
                label: c.text(*i, "Core"),
                colour: self.cores.colour(*i),
                wire: c.text(*i, "Wire number"),
                from: Some(c.text(*i, "From")).filter(|f| f != "spare").unwrap_or_default(),
                to: c.text(*i, "To"),
                spare: c.text(*i, "State") == "spare",
            })
            .collect();
        super::cable_view::Drawing { tag: rows.text(k, "Tag"), words, from: rows.text(k, "From"), to: rows.text(k, "To"), cores }
    }

    /// Forget the copies.
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.cores_of = None;
        self.colours_of = None;
        self.table.reset();
        self.core_table.reset();
    }

    /// The selected cable's row.
    pub fn selected(&self) -> Option<usize> {
        self.table.selection.cursor().or_else(|| self.table.selection.keys().iter().next().copied())
    }

    /// The footer's counts: cables, cables with a check, cables without a length.
    pub fn counts(&self) -> (usize, usize, usize) {
        let keys = &self.source.rows.keys;
        let flagged = keys.iter().filter(|k| self.source.rows.flag(**k).is_some()).count();
        let lengths = self.source.rows.headers.contains(&"Length");
        let no_length = keys.iter().filter(|k| !lengths || self.source.rows.text(**k, "Length").is_empty()).count();
        (keys.len(), flagged, no_length)
    }

    /// The cable table. Returns where to go.
    pub fn show_cables(&mut self, ui: &mut eframe::egui::Ui) -> Option<(ecad_model::PageId, ecad_model::Pt)> {
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

    /// The selected cable's core table.
    pub fn show_cores(&mut self, ui: &mut eframe::egui::Ui) {
        for e in self.core_table.show(ui, &self.cores) {
            if let TableEvent::Export(csv) = e {
                super::copy_csv(ui, csv);
            }
        }
    }
}

/// The Cables tab (board 2.06): the cable table, the selected cable's line and its drawing, then
/// its cores. Returns where to go.
pub fn show_tab(t: &mut crate::Trial, ui: &mut eframe::egui::Ui) -> Option<(ecad_model::PageId, ecad_model::Pt)> {
    use ecad_ui::tokens::{size, space, ui_type};
    use eframe::egui::{self, vec2};
    let s = ecad_ui::SHELL;
    let r = t.reports.as_mut()?;
    let v = &mut r.cables;
    let full = ui.available_rect_before_wrap();
    // The cable table: its rows, at most two fifths of the body.
    let rows = v.source.rows.keys.len().max(1) as f32;
    let table_h = ((rows + 1.0) * size::ROW_TABLE + space::XS).min(full.height() * 0.4).max(3.0 * size::ROW_TABLE);
    let mut go = ui
        .allocate_ui(vec2(full.width(), table_h), |ui| {
            ui.set_min_height(table_h);
            v.show_cables(ui)
        })
        .inner;
    let Some(k) = v.selected() else { return go };
    let rows = &v.source.rows;
    let tag = rows.text(k, "Tag");
    let words = [rows.text(k, "Type"), rows.text(k, "Length")].into_iter().filter(|w| !w.is_empty() && w != "?").collect::<Vec<_>>().join(" \u{b7} ");
    ui.add_space(space::M);
    ui.horizontal(|ui| {
        ui.add_space(space::L);
        ui.label(egui::RichText::new(&tag).font(ecad_ui::fonts::font(ui.ctx(), ui_type::TITLE)).color(ecad_ui::color(s.accent_text)));
        ui.label(egui::RichText::new(&words).size(ui_type::BODY.size).color(ecad_ui::color(s.text_2)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(space::L);
            if let Some(target) = v.gos.get(k).copied().flatten()
                && ui.add(ecad_ui::prim::Button::new("Go to its page").small()).clicked()
            {
                go = Some(target);
            }
            if ui.add(ecad_ui::prim::Button::new("Swap ends").small()).clicked() {
                v.swapped = !v.swapped;
            }
            ecad_ui::prim::segmented(ui, &mut v.view, &VIEWS);
        });
    });
    ui.add_space(space::S);
    // Brief 057 (board 2.06): the cable drawn end to end, unless Cores only.
    if v.view == 0 {
        let d = v.drawing(k);
        super::cable_view::show(ui, &d, v.swapped);
    }
    ui.add_space(space::M);
    v.show_cores(ui);
    go
}
