//! Project ▸ Cables (UI rev 3, board 2.05): every cable with type, cores, length, ends, cores used
//! and checks, as the one table over the cable report (`Outputs::cable_table`); under it the
//! selected cable's core table (`Outputs::core_table`). The cable drawn end to end, the glands and
//! the cores' colours belong to the cable manager (brief 2.7), not built yet: the drawing is a
//! placeholder and the glands show a dash. Nothing here is set: a cable is changed on its page.
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

/// The placeholder where the cable is drawn end to end.
pub const DRAWING_COMES: &str = "The cable drawn end to end, with its glands and each core in its colours, comes with the cable manager.";

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
}

impl Default for CablesSource {
    fn default() -> Self {
        Self { cols: columns(), rows: ReportRows::default() }
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
            COLOUR => From::Placeholder,
            CABINET => From::Header("From"),
            WIRE => From::Header("Wire number"),
            FIELD => From::Header("To"),
            _ => From::Derived,
        }
    }
    fn derived(&self, k: usize, col: ColumnId) -> Cell {
        match (col, self.rows.text(k, "State").as_str()) {
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

    /// Forget the copies.
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.cores_of = None;
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

/// The Cables tab (board 2.05): the cable table, the selected cable's line and its drawing (a
/// placeholder until the cable manager), then its cores. Returns where to go.
pub fn show_tab(t: &mut crate::Trial, ui: &mut eframe::egui::Ui) -> Option<(ecad_model::PageId, ecad_model::Pt)> {
    use ecad_ui::tokens::{line, radius, size, space, ui_type};
    use eframe::egui::{self, Sense, Stroke, pos2, vec2};
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
    let (from, to) = (rows.text(k, "From"), rows.text(k, "To"));
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
            ui.add(ecad_ui::prim::Button::new("Swap ends").small().disabled("Comes with the cable manager."));
            let mut view = 1;
            ui.add_enabled_ui(false, |ui| ecad_ui::prim::segmented(ui, &mut view, &["Cable view", "Cores only"]))
                .response
                .on_disabled_hover_text("The cable view comes with the cable manager.");
        });
    });
    ui.add_space(space::S);
    // The drawing's place: the two ends and the jacket, as a placeholder.
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 200.0), Sense::hover());
    let area = area.shrink2(vec2(space::L, space::XS));
    let p = ui.painter();
    p.rect_stroke(area, ecad_ui::style::corner(radius::CARD), Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)), egui::StrokeKind::Inside);
    let end_w = 240.0_f32.min(area.width() / 4.0);
    let left = egui::Rect::from_min_size(area.min + vec2(space::XL, space::XL), vec2(end_w, area.height() - 2.0 * space::XL));
    let right = egui::Rect::from_min_size(pos2(area.right() - space::XL - end_w, left.top()), left.size());
    let dashed = Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line_strong));
    for (r, title, what) in [(left, "Cabinet end", &from), (right, "Field end", &to)] {
        p.add(egui::Shape::dashed_line(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()], dashed, 4.0, 3.0));
        let font = ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS);
        p.text(r.left_top() + vec2(space::L, space::L), egui::Align2::LEFT_TOP, what.as_str(), font, ecad_ui::color(s.text));
        let small = ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL);
        p.text(r.left_top() + vec2(space::L, space::L + ui_type::BODY.line), egui::Align2::LEFT_TOP, title, small, ecad_ui::color(s.text_3));
    }
    let jacket =
        egui::Rect::from_min_max(pos2(left.right() + space::XXL, area.center().y - 36.0), pos2(right.left() - space::XXL, area.center().y + 36.0));
    if jacket.width() > 40.0 {
        p.add(egui::Shape::dashed_line(
            &[jacket.left_top(), jacket.right_top(), jacket.right_bottom(), jacket.left_bottom(), jacket.left_top()],
            dashed,
            4.0,
            3.0,
        ));
        let g = ecad_ui::table::cells::fitted(ui, DRAWING_COMES, ui_type::SMALL, ecad_ui::color(s.text_3), jacket.width() - 2.0 * space::L);
        let at = jacket.center() - g.size() / 2.0;
        ui.painter().galley(at, g, ecad_ui::color(s.text_3));
    }
    ui.painter().text(
        pos2(jacket.center().x, jacket.top() - space::M),
        egui::Align2::CENTER_BOTTOM,
        format!("{tag} \u{b7} {words}"),
        ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL),
        ecad_ui::color(s.accent_text),
    );
    ui.add_space(space::M);
    v.show_cores(ui);
    go
}
