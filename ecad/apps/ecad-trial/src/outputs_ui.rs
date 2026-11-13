//! Brief 024's windows (design 024 §4.1, §6.2): **Reports** (BOM, wires, cables, each exported to
//! CSV) and **Print Book**. Both read the project and record nothing. Brief 028 adds the
//! **Terminals** tab (the terminal plan, per strip or all) and a cable's core list on the Cables
//! tab (double-click a cable).
//!
//! The Reports window shows a copy of a table of the project's kept report models
//! ([`ecad_cadcraft::outputs::Outputs`], built on first show and then kept from each change's
//! ops); the copy is taken again only when the models' revision, the tab or the grouping changed.
//! Live while open: no refresh button. Rows are virtualised.

use crate::shell::Hosted;
use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::outputs::{BomView, Group, Table};
use eframe::egui::{self, Color32, RichText, Sense, pos2, vec2};

/// The tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Bom,
    Wires,
    Cables,
    /// Brief 028: the terminal plan.
    Terminals,
}

impl Tab {
    pub fn name(self) -> &'static str {
        match self {
            Tab::Bom => "bom",
            Tab::Wires => "wires",
            Tab::Cables => "cables",
            Tab::Terminals => "terminals",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Tab::Bom => "BOM",
            Tab::Wires => "Wires",
            Tab::Cables => "Cables",
            Tab::Terminals => "Terminals",
        }
    }
    fn empty(self) -> &'static str {
        match self {
            Tab::Bom => "No devices yet.",
            Tab::Wires => "No wires yet.",
            Tab::Cables => "No cables yet.",
            Tab::Terminals => "No terminal strips yet.",
        }
    }
    fn file(self) -> &'static str {
        match self {
            Tab::Bom => "bom",
            Tab::Wires => "wires",
            Tab::Cables => "cables",
            Tab::Terminals => "terminals",
        }
    }
}

/// The tabs in order.
const TABS: [Tab; 4] = [Tab::Bom, Tab::Wires, Tab::Cables, Tab::Terminals];

/// `ecad.check-warning`.
const WARNING: Color32 = Color32::from_rgb(0xFF, 0xFF, 0x00);
/// Row height (design 024 §6.2).
const ROW: f32 = 20.0;

/// What the window asks the trial for.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Go to a row (index into the shown rows).
    Goto(Tab, usize),
    /// Show a row's object in Properties.
    Select(Tab, usize),
    /// Write the shown table as CSV to this path.
    Export(String),
    /// Brief 030: run this command line (the Accessory menu; brief 039: a terminal plan edit,
    /// whose refusal goes back through [`Reports::ran`]).
    Run(String),
    /// Brief 039: show this page, centred on this point (a Terminals row's place).
    GotoAt(ecad_model::PageId, ecad_model::Pt),
    Close,
}

/// The Reports window's state.
#[derive(Debug, Default)]
pub struct Reports {
    /// Brief 038: the shell's slot it fills (no title bar, not movable), or `None` to float.
    pub host: Option<egui::Rect>,
    /// Brief 038: its workspace is not shown, so it is not drawn.
    pub hidden: bool,
    pub tab: Tab,
    pub group: Group,
    pub find: String,
    pub only_flagged: bool,
    /// Sort column and direction (kept for the session).
    pub sort: Option<(usize, bool)>,
    /// The project's table copy and the (serial, revision, tab, group) it was taken at.
    table: Table,
    counts: [usize; 4],
    seen: Option<(u64, u64, Tab, Group, Option<ecad_model::ComponentId>, Option<ecad_model::CableId>)>,
    /// The (project, models revision) the tab counts were taken at.
    counted: Option<(u64, u64, Group)>,
    /// The strips revision the strip picker was taken at.
    strips_seen: Option<(u64, u64)>,
    /// Brief 028: the strip the Terminals tab shows (`None`: all), and the strips to pick from.
    pub strip: Option<ecad_model::ComponentId>,
    pub strips: Vec<(ecad_model::ComponentId, String)>,
    /// Brief 030: the Accessories check box is off (gap rows hidden; view only).
    pub hide_accessories: bool,
    /// Brief 028: the cable whose core list the Cables tab shows instead of the schedule.
    pub cores_of: Option<ecad_model::CableId>,
    /// Where each Terminals row goes.
    pub gos: Vec<Option<(ecad_model::PageId, ecad_model::Pt)>>,
    /// The rows shown after Find, Only ? and sort: indexes into `table.rows`.
    pub shown: Vec<usize>,
    /// The Export path box, when open.
    pub export: Option<String>,
    /// A message under the toolbar (the export echo or a refusal).
    pub note: Option<String>,
    /// How many times the copy was taken (tests: only when the models changed).
    pub copies: u64,
    pub rows_drawn: usize,
    /// The project folder and name, for the export path's default.
    pub stem: String,
    /// Brief 039: the Terminals tab as the one table's terminal manager.
    pub terminals: crate::lists::terminals::TerminalsView,
    /// Brief 047 (design 047 §6.1): the project's BOM report settings (the Columns menu reads
    /// them; ticking writes `setdefault`), whether the project can be written (Columns and
    /// + Item are greyed when not), each BOM row's item when it lists one item only, and the BOM
    /// as CSV (Qty a number, a Unit column), row for row with the shown table.
    pub view: BomView,
    pub writable: bool,
    pub items: Vec<Option<ecad_model::ComponentId>>,
    csv: Option<Table>,
}

/// Brief 047's element names (design 047 §11).
pub const COLUMNS: &str = "ecad.reports.columns";
pub const COLUMN_ARTICLE: &str = "ecad.reports.columns.article";
pub const COLUMN_NESTED: &str = "ecad.reports.columns.nested";
pub const ADD_ITEM: &str = "ecad.reports.additem";
/// The status words for an item row (it is on no page).
pub const ITEM_NO_PAGE: &str = "Items are on no page.";
pub const READ_ONLY: &str = "The project is read-only.";

impl Reports {
    /// The table shown now (all rows; [`Reports::shown`] says which are visible).
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Bring the copy up to date with the project's kept models (built on the first call).
    pub fn sync(&mut self, serial: u64, p: &mut ecad_cadcraft::Project) {
        let reference_style = p.ledger.document().settings.reference_style;
        let style = reference_style as u64;
        let o = p.outputs();
        // The counts on the tabs, from kept counts (no rows made).
        if self.counted != Some((serial, o.build_id ^ o.revision, self.group)) {
            self.counts = [o.bom_count(self.group), o.wire_count(), o.cable_count(), o.strips.len()];
            self.counted = Some((serial, o.build_id ^ o.revision, self.group));
        }
        // The Terminals tab follows only the strips it shows (review of PR #55, finding 2; design
        // 028 §11.1): an edit that changes none leaves its copy; the others follow the models.
        let revision = match self.tab {
            Tab::Terminals => o.build_id.wrapping_mul(1 << 40).wrapping_add(o.strips.changed_at(self.strip).wrapping_mul(8)).wrapping_add(style),
            _ => o.revision,
        };
        let revision = revision.wrapping_add(u64::from(self.hide_accessories));
        if self.tab == Tab::Terminals {
            self.terminals.sync(serial, o, self.strip, !self.hide_accessories, reference_style);
        }
        let key = (serial, revision, self.tab, self.group, self.strip, self.cores_of);
        if self.seen == Some(key) {
            self.filter();
            return;
        }
        self.gos.clear();
        self.view = o.view;
        self.items.clear();
        self.csv = None;
        if self.tab == Tab::Bom {
            self.items =
                o.bom(self.group).iter().map(|r| if r.item && r.entries.len() == 1 { r.entries.first().map(|e| e.0) } else { None }).collect();
            self.csv = Some(o.bom_table_with(self.group, o.view, true));
        }
        self.table = match (self.tab, self.cores_of) {
            (Tab::Bom, _) => o.bom_table(self.group),
            (Tab::Wires, _) => o.wire_table(),
            (Tab::Cables, Some(k)) => o.core_table(k),
            (Tab::Cables, None) => o.cable_table(),
            (Tab::Terminals, _) => {
                let (t, gos) = o.terminal_table_with(self.strip, !self.hide_accessories);
                self.gos = gos;
                t
            }
        };
        if self.strips_seen != Some((serial, o.strips.revision)) {
            self.strips = o.strips.sorted().iter().map(|s| (s.component, s.tag.clone())).collect();
            self.strips_seen = Some((serial, o.strips.revision));
        }
        self.seen = Some(key);
        self.copies += 1;
        self.filter();
    }

    /// Forget the copy.
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.counted = None;
        self.strips_seen = None;
        self.terminals.invalidate();
    }

    /// The result of the last [`Out::Run`]: a terminal plan edit's refusal shows under its cell.
    pub fn ran(&mut self, refusal: Option<String>) {
        self.terminals.ran(refusal);
    }

    fn filter(&mut self) {
        let q = self.find.trim().to_lowercase();
        let mut v: Vec<usize> = (0..self.table.rows.len())
            .filter(|i| {
                let Some(r) = self.table.rows.get(*i) else { return false };
                (q.is_empty() || r.iter().any(|c| c.text.to_lowercase().contains(&q))) && (!self.only_flagged || r.iter().any(|c| c.flag.is_some()))
            })
            .collect();
        if let Some((col, desc)) = self.sort {
            v.sort_by(|a, b| {
                let get = |i: &usize| self.table.rows.get(*i).and_then(|r| r.get(col)).map(|c| c.text.clone()).unwrap_or_default();
                let o = ecad_cadcraft::locations::natural_cmp(&get(a), &get(b));
                if desc { o.reverse() } else { o }
            });
        }
        self.shown = v;
    }

    /// The row `i` of the shown rows, as an index into the table (and so the report model order).
    pub fn row_index(&self, i: usize) -> Option<usize> {
        self.shown.get(i).copied()
    }

    /// The table as shown (grouping, sort and filter), for CSV. Brief 047: the BOM in its CSV
    /// form (design 047 §6.4), the same rows.
    pub fn shown_table(&self) -> Table {
        let t = self.csv.as_ref().filter(|_| self.tab == Tab::Bom).unwrap_or(&self.table);
        Table { headers: t.headers.clone(), rows: self.shown.iter().filter_map(|i| t.rows.get(*i).cloned()).collect() }
    }

    /// Brief 047: the BOM tab's Columns menu and + Item (design 047 §6.1).
    fn bom_tools(&mut self, ui: &mut egui::Ui, out: &mut Option<Out>) {
        let writable = self.writable;
        let r = ui.add_enabled_ui(writable, |ui| {
            ui.menu_button(RichText::new("Columns v").small(), |ui| {
                let mut article = self.view.article;
                if ui.checkbox(&mut article, "Article number").accessible_name(COLUMN_ARTICLE).changed() {
                    let v = if article { "on" } else { "off" };
                    *out = Some(Out::Run(format!("setdefault {}", serde_json::json!({"key": "bom.column.article", "level": "project", "value": v}))));
                    ui.close();
                }
                if self.group == Group::Location {
                    let mut nested = self.view.nested;
                    if ui.checkbox(&mut nested, "Lines under devices").accessible_name(COLUMN_NESTED).changed() {
                        let v = if nested { "nested" } else { "flat" };
                        *out = Some(Out::Run(format!("setdefault {}", serde_json::json!({"key": "bom.lines", "level": "project", "value": v}))));
                        ui.close();
                    }
                }
            })
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, COLUMNS));
            let add = ui.button("+ Item").accessible_name(ADD_ITEM);
            if add.clicked() {
                *out = Some(Out::Run("bomitem".into()));
            }
        });
        if !writable {
            r.response.on_hover_text(READ_ONLY);
        }
    }

    /// The window. Returns what it asks for.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let mut out = None;
        let mut open = true;
        let t = Tokens::get();
        egui::Window::new("REPORTS")
            .id(egui::Id::unique("ecad.reports"))
            .default_size(vec2(900.0, 480.0))
            .resizable(true)
            .collapsible(false)
            .open(&mut open)
            .hosted(self.host)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, tab) in TABS.into_iter().enumerate() {
                        let n = self.counts.get(i).copied().unwrap_or(0);
                        let b = ui
                            .selectable_label(self.tab == tab, format!("{} ({n})", tab.label()))
                            .on_hover_text(format!("ecad.reports.{}", tab.name()));
                        let b = b.accessible_name(format!("ecad.reports.{}", tab.name()));
                        if b.clicked() && (self.tab != tab || self.cores_of.is_some()) {
                            self.tab = tab;
                            self.sort = None;
                            self.export = None;
                            self.cores_of = None;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    // Brief 028: which strip, or all.
                    if self.tab == Tab::Terminals {
                        let shown = self.strip.and_then(|c| self.strips.iter().find(|(x, _)| *x == c)).map_or("All", |(_, t)| t.as_str()).to_string();
                        ui.menu_button(RichText::new(format!("{shown} v")).small(), |ui| {
                            if ui.button("All").clicked() {
                                self.strip = None;
                                ui.close();
                            }
                            for (c, tag) in self.strips.clone() {
                                if ui.button(tag).clicked() {
                                    self.strip = Some(c);
                                    ui.close();
                                }
                            }
                        })
                        .response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "ecad.reports.strip"));
                        // Brief 030 (design 030 §2.4): accessories at the strip's ends and markers,
                        // for the strip picked (or the only one).
                        let only = match self.strips.as_slice() {
                            [(c, _)] => Some(*c),
                            _ => None,
                        };
                        let one = self.strip.or(only);
                        let mut acc = !self.hide_accessories;
                        if ui.checkbox(&mut acc, "Accessories").accessible_name("ecad.reports.terminals.acc").changed() {
                            self.hide_accessories = !acc;
                        }
                        ui.add_enabled_ui(one.is_some(), |ui| {
                            ui.menu_button(RichText::new("Accessory v").small(), |ui| {
                                let Some(c) = one else { return };
                                for (label, params) in accessory_menu(c) {
                                    if ui.button(label).clicked() {
                                        out = Some(Out::Run(format!("accessory {params}")));
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_disabled_hover_text("Pick one strip (or place terminals) first.")
                            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "ecad.reports.terminals.accessory"));
                        });
                    }
                    if self.tab == Tab::Cables
                        && self.cores_of.is_some()
                        && ui.button("\u{2190} All cables").accessible_name("ecad.reports.cables.back").clicked()
                    {
                        self.cores_of = None;
                    }
                    if self.tab == Tab::Bom {
                        ui.label(RichText::new("Group").small());
                        let words = if self.group == Group::Location { "By location" } else { "By part" };
                        ui.menu_button(RichText::new(format!("{words} v")).small(), |ui| {
                            if ui.button("By location").clicked() {
                                self.group = Group::Location;
                                ui.close();
                            }
                            if ui.button("By part").clicked() {
                                self.group = Group::Part;
                                ui.close();
                            }
                        })
                        .response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "ecad.reports.group"));
                        self.bom_tools(ui, &mut out);
                    }
                    ui.label(RichText::new("Find").small());
                    let find = ui.add(egui::TextEdit::singleline(&mut self.find).desired_width(120.0)).accessible_name("ecad.reports.find");
                    if find.changed() {
                        self.filter();
                    }
                    if ui.checkbox(&mut self.only_flagged, "Only ?").accessible_name("ecad.reports.flagged").changed() {
                        self.filter();
                    }
                    let flagged = self.table.flagged();
                    if flagged > 0 {
                        ui.label(RichText::new(format!("{flagged} ?")).small().color(WARNING));
                    }
                    if ui.button("Export CSV\u{2026}").accessible_name("ecad.reports.export").clicked() {
                        self.export = Some(format!("{}-{}.csv", self.stem, self.tab.file()));
                    }
                });
                if let Some(path) = &mut self.export {
                    let mut go = false;
                    let mut cancel = false;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Save as").small());
                        let r = ui.add(egui::TextEdit::singleline(path).desired_width(420.0)).accessible_name("ecad.reports.export.path");
                        go = ui.button("Save").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        cancel = ui.button("Cancel").clicked();
                    });
                    if go {
                        out = Some(Out::Export(path.clone()));
                    }
                    if cancel {
                        self.export = None;
                    }
                }
                if let Some(n) = &self.note {
                    ui.label(RichText::new(n).small().color(t.text_dim));
                }
                ui.separator();
                // Brief 039: the terminal manager is the one table, mirrored (design 036 §9).
                if self.tab == Tab::Terminals {
                    for a in self.terminals.show(ui, true) {
                        out = Some(match a {
                            crate::lists::ListAction::Run(line) => Out::Run(line),
                            crate::lists::ListAction::GoTo(pg, at) => Out::GotoAt(pg, at),
                            crate::lists::ListAction::Panel(_) => continue,
                        });
                    }
                    self.rows_drawn = self.terminals.table.rows_drawn;
                    return;
                }
                if self.table.rows.is_empty() {
                    ui.label(RichText::new(self.tab.empty()).color(t.text_dim));
                    self.rows_drawn = 0;
                    return;
                }
                let widths = self.widths(ui);
                let font = egui::FontId::monospace(12.0);
                // Header: click sorts, again reverses.
                let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
                let mut x = hr.left();
                for (col, h) in self.table.headers.clone().iter().enumerate() {
                    let w = widths.get(col).copied().unwrap_or(80.0);
                    let r = egui::Rect::from_min_size(pos2(x, hr.top()), vec2(w, ROW));
                    let resp = ui.interact(r, ui.scope_id().with(("hdr", col)), Sense::click());
                    let arrow = match self.sort {
                        Some((c, false)) if c == col => " ^",
                        Some((c, true)) if c == col => " v",
                        _ => "",
                    };
                    ui.painter().text(pos2(x + 2.0, hr.center().y), egui::Align2::LEFT_CENTER, format!("{h}{arrow}"), font.clone(), t.text_dim);
                    if resp.clicked() {
                        self.sort = match self.sort {
                            Some((c, d)) if c == col => Some((col, !d)),
                            _ => Some((col, false)),
                        };
                        self.filter();
                    }
                    x += w;
                }
                let tab = self.tab;
                let items = std::mem::take(&mut self.items);
                let writable = self.writable;
                let mut note = None;
                let mut drawn = 0;
                egui::ScrollArea::vertical().id_salt("ecad.reports.rows").auto_shrink([false, false]).show_rows(
                    ui,
                    ROW,
                    self.shown.len(),
                    |ui, range| {
                        for i in range {
                            let Some(row) = self.shown.get(i).and_then(|k| self.table.rows.get(*k)) else { continue };
                            drawn += 1;
                            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                            let p = ui.painter_at(rect);
                            if resp.hovered() {
                                p.rect_filled(rect, 0.0, t.chrome);
                            }
                            let mut x = rect.left();
                            let mut tip: Vec<String> = Vec::new();
                            for (col, c) in row.iter().enumerate() {
                                let w = widths.get(col).copied().unwrap_or(80.0);
                                let colour = if c.flag.is_some() { WARNING } else { t.text };
                                let text = cut(&c.text, w);
                                p.text(pos2(x + 2.0, rect.center().y), egui::Align2::LEFT_CENTER, text, font.clone(), colour);
                                if let Some(f) = &c.flag {
                                    tip.push(f.clone());
                                }
                                x += w;
                            }
                            let label = format!("{}: {}", row.first().map_or("", |c| c.text.as_str()), row.get(1).map_or("", |c| c.text.as_str()));
                            let resp = resp.accessible_name(format!("ecad.reports.{}.row.{i}", tab.name()));
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                            let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip.join("\n")) };
                            // Brief 047: an item row has no page; its menu deletes it.
                            let item = if tab == Tab::Bom { self.shown.get(i).and_then(|k| items.get(*k).copied().flatten()) } else { None };
                            if let Some(c) = item {
                                if resp.clicked() || resp.double_clicked() {
                                    note = Some(ITEM_NO_PAGE.to_string());
                                }
                                resp.context_menu(|ui| {
                                    let d = ui.add_enabled(writable, egui::Button::new("Delete item"));
                                    let d = d.accessible_name(format!("ecad.reports.bom.row.{i}.delete"));
                                    if d.clicked() {
                                        out = Some(Out::Run(format!("bomitem {}", serde_json::json!({"item": c.hex(), "delete": true}))));
                                        ui.close();
                                    }
                                });
                            } else if resp.double_clicked() {
                                out = Some(Out::Select(tab, i));
                            } else if resp.clicked() {
                                out = Some(Out::Goto(tab, i));
                            }
                        }
                    },
                );
                self.items = items;
                if note.is_some() {
                    self.note = note;
                }
                self.rows_drawn = drawn;
            });
        if !open {
            out = Some(Out::Close);
        }
        out
    }

    fn widths(&self, ui: &egui::Ui) -> Vec<f32> {
        let total = ui.available_width().max(200.0);
        let weights: Vec<f32> = match self.tab {
            Tab::Bom => {
                let mut v = vec![1.2, 0.6, 2.0, 2.5, 1.2, 1.6];
                v.extend(std::iter::repeat_n(1.2, self.table.headers.len().saturating_sub(6)));
                v
            }
            Tab::Wires => vec![1.0, 1.4, 1.0, 1.4, 1.0, 0.9, 0.8, 0.8, 1.2, 0.9],
            Tab::Cables if self.cores_of.is_some() => vec![0.6, 1.0, 1.4, 1.4, 0.6],
            Tab::Cables => {
                let mut v = vec![1.0, 1.0, 0.6, 1.2, 1.4, 0.6, 1.0, 1.4, 1.4];
                v.extend(std::iter::repeat_n(1.0, self.table.headers.len().saturating_sub(9)));
                v
            }
            Tab::Terminals => vec![0.7, 1.6, 0.9, 0.6, 0.5, 0.9, 0.7, 0.6, 1.6, 0.9],
        };
        let sum: f32 = weights.iter().sum();
        weights.iter().map(|w| total * w / sum).collect()
    }
}

/// `text` cut with `…` to about `width` points of monospace 12.
fn cut(text: &str, width: f32) -> String {
    let n = ((width - 4.0) / 7.2).max(1.0) as usize;
    if text.chars().count() <= n {
        return text.to_string();
    }
    let mut s: String = text.chars().take(n.saturating_sub(1)).collect();
    s.push('\u{2026}');
    s
}

/// Print Book's scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeKind {
    #[default]
    All,
    Folder,
    Pages,
}

/// The Print Book dialog (design 024 §4.1).
#[derive(Debug, Clone, Default)]
pub struct PrintBook {
    /// Brief 038: the shell's slot it fills (no title bar, not movable), or `None` to float.
    pub host: Option<egui::Rect>,
    /// Brief 038: its workspace is not shown, so it is not drawn.
    pub hidden: bool,
    pub scope: ScopeKind,
    /// The folders to choose from: (id hex, path), and the chosen one.
    pub folders: Vec<(String, String)>,
    pub folder: usize,
    pub pages: String,
    pub path: String,
    pub mono: bool,
    pub lineweights: bool,
    /// Sheets the scope prints, numbers that match no page, and `?` values on generated pages.
    pub sheets: usize,
    pub unknown: Vec<String>,
    pub flagged: usize,
    /// Brief 033: what Print Book will note (a continuation number prints its whole page; the
    /// drawing changed since the newest issue).
    pub notes: Vec<String>,
    pub error: Option<String>,
}

impl PrintBook {
    /// The `printbook` parameters.
    pub fn params(&self) -> serde_json::Value {
        let scope = match self.scope {
            ScopeKind::All => serde_json::json!("all"),
            ScopeKind::Folder => serde_json::json!({"folder": self.folders.get(self.folder).map(|f| f.0.clone())}),
            ScopeKind::Pages => serde_json::json!({"pages": self.pages}),
        };
        serde_json::json!({
            "scope": scope, "path": self.path, "colour": if self.mono { "mono" } else { "screen" }, "lineweights": self.lineweights,
        })
    }

    /// The dialog. `Some(true)`: Print; `Some(false)`: Cancel.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<bool> {
        let mut out = None;
        let mut open = true;
        let t = Tokens::get();
        egui::Window::new("PRINT BOOK")
            .id(egui::Id::unique("ecad.printbook"))
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .open(&mut open)
            .hosted(self.host)
            .show(ctx, |ui| {
                egui::Grid::new("ecad.printbook.grid").num_columns(2).show(ui, |ui| {
                    ui.label("Pages");
                    ui.vertical(|ui| {
                        let all = if self.scope == ScopeKind::All { format!("All pages ({} sheets)", self.sheets) } else { "All pages".into() };
                        ui.radio_value(&mut self.scope, ScopeKind::All, all)
                            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioButton, true, "ecad.printbook.scope.all"));
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(!self.folders.is_empty(), |ui| {
                                ui.radio_value(&mut self.scope, ScopeKind::Folder, "Folder")
                                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioButton, true, "ecad.printbook.scope.folder"));
                                let name = self.folders.get(self.folder).map(|f| f.1.clone()).unwrap_or_default();
                                egui::ComboBox::from_id_salt("ecad.printbook.folder").selected_text(name).show_ui(ui, |ui| {
                                    for (i, (_, path)) in self.folders.iter().enumerate() {
                                        ui.selectable_value(&mut self.folder, i, path);
                                    }
                                });
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.scope, ScopeKind::Pages, "Pages")
                                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioButton, true, "ecad.printbook.scope.pages"));
                            let r = ui.add(egui::TextEdit::singleline(&mut self.pages).desired_width(200.0)).accessible_name("ecad.printbook.pages");
                            if r.changed() {
                                self.scope = ScopeKind::Pages;
                            }
                        });
                        if self.scope == ScopeKind::Pages && !self.unknown.is_empty() {
                            for n in &self.unknown {
                                ui.label(RichText::new(format!("No page {n}; it is left out.")).small().underline().color(WARNING));
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("File");
                    ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(300.0)).accessible_name("ecad.printbook.path");
                    ui.end_row();
                    ui.label("Colour");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("ecad.printbook.colour")
                            .selected_text(if self.mono { "Black on white" } else { "As on screen" })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.mono, true, "Black on white");
                                ui.selectable_value(&mut self.mono, false, "As on screen");
                            });
                        ui.checkbox(&mut self.lineweights, "Line weights");
                    });
                    ui.end_row();
                });
                for n in &self.notes {
                    ui.label(RichText::new(n).small().color(WARNING)).accessible_name("ecad.printbook.note");
                }
                if self.flagged > 0 {
                    let n = if self.flagged == 1 { "1 value".to_string() } else { format!("{} values", self.flagged) };
                    ui.label(RichText::new(format!("{n} marked ? on generated pages.")).small().color(t.text_dim));
                }
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).small().color(Color32::from_rgb(0xFF, 0x40, 0x40)));
                }
                let none = self.sheets == 0;
                if none {
                    ui.label(RichText::new("No page matches.").small().color(WARNING));
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!none && !self.path.trim().is_empty(), egui::Button::new("Print"))
                        .accessible_name("ecad.printbook.print")
                        .clicked()
                    {
                        out = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        out = Some(false);
                    }
                });
            });
        if !open {
            out = Some(false);
        }
        out
    }
}

/// The Accessory menu's items (design 030 §2.4): (words, `accessory` params) for strip `c`.
pub fn accessory_menu(c: ecad_model::ComponentId) -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;
    let s = c.hex();
    vec![
        ("At start: end stop", json!({"strip": s, "at": "start", "add": ["end_stop"]})),
        ("At start: strip label", json!({"strip": s, "at": "start", "add": ["strip_label"]})),
        ("At end: end plate", json!({"strip": s, "at": "end", "add": ["end_plate"]})),
        ("At end: end stop", json!({"strip": s, "at": "end", "add": ["end_stop"]})),
        ("Marker on every position", json!({"strip": s, "at": "all", "on": true, "items": ["marker"]})),
        ("Remove every marker", json!({"strip": s, "at": "all", "on": true, "items": null})),
        ("Clear the ends", json!({"strip": s, "at": ["start", "end"], "items": null})),
    ]
}
