//! Project ▸ Reports (UI rev 3, board 2.06), a placeholder layout round today's reports: the list
//! of reports (what each reads and how it is grouped), and under it the selected report's preview,
//! which is today's Reports window (brief 024) hosted in place. The report generator (brief 2.9)
//! is not built: its layout, place in the book, generation time and settings show a dash or are
//! greyed, and the PLC rack overview's preview is Project ▸ PLC itself.
//!
//! **Update map.** The list is fixed (it reads nothing); the preview is the Reports window, which
//! keeps its own (design 024 §6.2).

use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};
use eframe::egui;

use crate::outputs_ui::Tab;

/// One report: its name, what it reads, how it is grouped and sorted, what else it exports, and
/// the Reports tab that previews it (`None`: the PLC tab is its preview).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub name: &'static str,
    pub reads: &'static str,
    pub grouped: &'static str,
    pub exports: &'static str,
    pub tab: Option<Tab>,
}

/// The reports there are today.
pub const REPORTS: [Report; 5] = [
    Report { name: "Parts list", reads: "Devices, part sets", grouped: "Location or part", exports: "CSV", tab: Some(Tab::Bom) },
    Report { name: "Terminal plans", reads: "Terminals, cables", grouped: "Strip, then position", exports: "CSV", tab: Some(Tab::Terminals) },
    Report { name: "Cable list", reads: "Cables, cores", grouped: "Cable tag", exports: "CSV", tab: Some(Tab::Cables) },
    Report { name: "PLC rack overview", reads: "PLC cards, channels", grouped: "Rack, then slot", exports: super::report::DASH, tab: None },
    Report { name: "Wire list", reads: "Wires", grouped: "Wire number", exports: "CSV", tab: Some(Tab::Wires) },
];

/// The list's height (its header and rows) and the preview's line under it, px.
pub const LIST_H: f32 = 6.0 * ecad_ui::tokens::size::ROW_TABLE + ecad_ui::tokens::space::XS + PREVIEW_LINE;
pub const PREVIEW_LINE: f32 = 40.0;
/// The preview line's words for the PLC overview.
pub const PLC_PREVIEW: &str = "The rack overview is Project \u{25b8} PLC: open it there.";

pub const REPORT: ColumnId = ColumnId(0);
pub const READS: ColumnId = ColumnId(1);
pub const GROUPED: ColumnId = ColumnId(2);
pub const LAYOUT: ColumnId = ColumnId(3);
pub const IN_BOOK: ColumnId = ColumnId(4);
pub const GENERATED: ColumnId = ColumnId(5);
pub const EXPORTS: ColumnId = ColumnId(6);

/// The list as a table source.
#[derive(Debug)]
pub struct ListSource {
    cols: Vec<Column>,
    keys: Vec<usize>,
}

impl Default for ListSource {
    fn default() -> Self {
        Self {
            cols: vec![
                Column::read(REPORT.0, "Report", Width::Initial(230.0)),
                Column::read(READS.0, "Reads", Width::Initial(180.0)),
                Column::read(GROUPED.0, "Grouped and sorted by", Width::Initial(220.0)),
                Column::read(LAYOUT.0, "Layout", Width::Initial(160.0)).unsorted(),
                Column::read(IN_BOOK.0, "In the book", Width::Initial(180.0)).unsorted(),
                Column::read(GENERATED.0, "Last generated", Width::Initial(500.0)).unsorted(),
                Column::read(EXPORTS.0, "Also exports", Width::Remainder),
            ],
            keys: (0..REPORTS.len()).collect(),
        }
    }
}

impl TableSource for ListSource {
    type Key = usize;
    fn columns(&self) -> &[Column] {
        &self.cols
    }
    fn keys(&self) -> &[usize] {
        &self.keys
    }
    fn cell(&self, key: usize, col: ColumnId) -> Cell {
        let Some(r) = REPORTS.get(key) else { return Cell::Empty };
        match col {
            REPORT => Cell::Text(r.name.into()),
            READS => Cell::read(r.reads),
            GROUPED => Cell::read(r.grouped),
            EXPORTS => Cell::read(r.exports),
            GENERATED => Cell::Badge("on demand".into(), Tone::Neutral),
            _ => Cell::read(super::report::DASH),
        }
    }
}

/// The Reports tab's list.
#[derive(Debug)]
pub struct ReportsList {
    pub source: ListSource,
    pub table: Table<usize>,
    /// The report picked last frame: a new pick sets Reports' tab; otherwise the list follows
    /// the tab (Show plan, a report opened from elsewhere).
    last: Option<usize>,
}

impl Default for ReportsList {
    fn default() -> Self {
        let mut table = Table::new("ecad.lists.reports", "reports");
        table.toolbar = false;
        let source = ListSource::default();
        table.update(&source, Changed::All);
        table.selection.set([0]);
        Self { source, table, last: Some(0) }
    }
}

impl ReportsList {
    /// The selected report.
    pub fn selected(&self) -> Option<&'static Report> {
        let k = self.table.selection.cursor().or_else(|| self.table.selection.keys().iter().next().copied())?;
        REPORTS.get(k)
    }
}

/// The Reports tab: the list, then the preview's line; the preview itself is the Reports window,
/// hosted under it ([`super::workspace::reports_preview`]).
pub fn show(t: &mut crate::Trial, ui: &mut egui::Ui) {
    use ecad_ui::tokens::{size, space, ui_type};
    let s = ecad_ui::SHELL;
    let Some(r) = t.reports.as_mut() else { return };
    let list_h = 6.0 * size::ROW_TABLE + space::XS;
    let mut open_plc = false;
    ui.allocate_ui(egui::vec2(ui.available_width(), list_h), |ui| {
        ui.set_min_height(list_h);
        for e in r.list.table.show(ui, &r.list.source) {
            if let TableEvent::Open(k) = e
                && REPORTS.get(k).is_some_and(|x| x.tab.is_none())
            {
                open_plc = true;
            }
        }
    });
    let now = r.list.table.selection.cursor().or_else(|| r.list.table.selection.keys().iter().next().copied());
    if now != r.list.last {
        // A new pick: its preview.
        if let Some(Report { tab: Some(tab), .. }) = now.and_then(|k| REPORTS.get(k))
            && r.tab != *tab
        {
            r.tab = *tab;
            r.strip = None;
            r.sort = None;
            r.export = None;
            r.cores_of = None;
        }
    } else if r.list.selected().and_then(|p| p.tab) != Some(r.tab)
        && let Some(k) = REPORTS.iter().position(|p| p.tab == Some(r.tab))
    {
        // The tab was set elsewhere: the list follows it.
        r.list.table.selection.set([k]);
    }
    r.list.last = r.list.table.selection.cursor().or_else(|| r.list.table.selection.keys().iter().next().copied());
    let picked = r.list.selected().copied();
    // The preview's line.
    let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), PREVIEW_LINE), egui::Sense::hover());
    super::super::shell::in_rect(ui, line.shrink2(egui::vec2(space::L, 0.0)), "ecad.project.reports.line", |ui| {
        ui.horizontal_centered(|ui| {
            let name = picked.map_or("", |p| p.name);
            ui.label(egui::RichText::new(name).font(ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS)).color(ecad_ui::color(s.text)));
            let words = if picked.is_some_and(|p| p.tab.is_none()) { PLC_PREVIEW } else { "\u{b7} preview" };
            ui.label(egui::RichText::new(words).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_2)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ecad_ui::prim::Button::new("Open its page").small().disabled(super::workspace::REPORTS_COME));
                let mut i = 0;
                ui.add_enabled_ui(false, |ui| ecad_ui::prim::segmented(ui, &mut i, &["As a table", "On the sheet"]))
                    .response
                    .on_disabled_hover_text(super::workspace::REPORTS_COME);
                if picked.is_some_and(|p| p.tab.is_none()) && ui.add(ecad_ui::prim::Button::new("Open Project \u{25b8} PLC").small()).clicked() {
                    open_plc = true;
                }
            });
        });
    });
    if open_plc {
        t.show_project_tab(crate::shell::ListsView::Plc);
    }
}

/// Whether the preview (the hosted Reports window) shows: a report with a Reports tab.
pub fn preview_shown(t: &crate::Trial) -> bool {
    t.reports.as_ref().and_then(|r| r.list.selected()).is_some_and(|p| p.tab.is_some())
}
