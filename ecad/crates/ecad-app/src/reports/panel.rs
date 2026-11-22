//! Brief 024's windows (design 024 §4.1, §6.2): **Reports** (BOM, wires, cables, each exported to
//! CSV) and **Print Book**. Both read the project and record nothing. Brief 028 adds the
//! **Terminals** tab (the terminal plan, per strip or all) and a cable's core list on the Cables
//! tab (double-click a cable).
//!
//! The Reports window shows a copy of a table of the project's kept report models
//! ([`ecad_electrical::reports::kept::ReportModels`], built on first show and then kept from each change's
//! ops); the copy is taken again only when the models' revision, the tab or the grouping changed.
//! Live while open: no refresh button. Rows are virtualised.

use crate::shell::Hosted;
use ecad_widgets::control::{Select, TextEntry, Toggle};
use ecad_widgets::prim::{self, Button, Checkbox, Kind, Radio, SearchField, Tone};
use ecad_widgets::tokens::{size, space};
use eframe::egui::{self, Sense, pos2, vec2};
use {ecad_electrical::reports::BomView, ecad_electrical::reports::Group, ecad_electrical::reports::Table};

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

/// Row height (design 024 §6.2).
const ROW: f32 = 20.0;

/// What the window asks the app for.
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
    seen: Option<(u64, u64, Tab, Group, Option<ecad_model::DeviceId>, Option<ecad_model::CableId>)>,
    /// The (project, models revision) the tab counts were taken at.
    counted: Option<(u64, u64, Group)>,
    /// The strips revision the strip picker was taken at.
    strips_seen: Option<(u64, u64)>,
    /// Brief 028: the strip the Terminals tab shows (`None`: all), and the strips to pick from.
    pub strip: Option<ecad_model::DeviceId>,
    pub strips: Vec<(ecad_model::DeviceId, String)>,
    /// Brief 030: the Accessories check box is off (gap rows hidden; view only).
    pub hide_accessories: bool,
    /// Brief 059 (design 059 §5.8, Q3): Include not-drawn cables is off for this run (view only;
    /// on by default).
    pub hide_not_drawn: bool,
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
    /// The copy's cells marked `?`, counted when it is taken (Project ▸ Reports' right panel).
    pub flagged: usize,
    pub rows_drawn: usize,
    /// The project folder and name, for the export path's default.
    pub stem: String,
    /// Brief 039: the Terminals tab as the one table's terminal manager.
    pub terminals: crate::terminal_strips::plan::TerminalStripPlanView,
    /// Brief 045c and 051: Lists ▸ PLC, kept in step while it is shown.
    pub io: crate::plc::list::IoView,
    pub io_shown: bool,
    /// UI rev 3 (boards 2.04, 2.05): Project ▸ Wires and Cables, kept in step while shown.
    pub wires: crate::wires::list::WiresView,
    pub cables: crate::cables::list::CablesView,
    pub wires_shown: bool,
    pub cables_shown: bool,
    /// UI rev 3 (board 2.06): Project ▸ Reports' list of reports; the window is its preview,
    /// shown without its own tab row.
    pub list: crate::reports::list::ReportsList,
    pub preview: bool,
    /// UI rev 3: the Project tabs' counts (terminals, PLC channels, wires, cables), from kept
    /// counts, taken with the Reports tab counts.
    pub tab_counts: [usize; 4],
    /// Brief 047 (design 047 §6.1): the project's BOM report settings (the Columns menu reads
    /// them; ticking writes `setdefault`), whether the project can be written (Columns and the
    /// add-item button are greyed when not), each BOM row's item when it lists one item only, and
    /// the BOM as CSV (Qty a number, a Unit column), row for row with the shown table.
    pub view: BomView,
    pub writable: bool,
    pub items: Vec<Option<ecad_model::LoosePartId>>,
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
    pub fn sync(&mut self, serial: u64, p: &mut ecad_engine::Project) {
        // Brief 045c: Lists ▸ I/O follows the PLC model's own update map while it is shown.
        if self.io_shown {
            p.outputs();
            if let Some(o) = p.kept_reports() {
                self.io.sync(serial, p.ledger.document(), o, self.writable);
            }
        }
        // Brief 057: the rating results the Wires and Cables tabs show (kept by the Checks list).
        if self.wires_shown || self.cables_shown {
            let kept = &p.checks().rating;
            if self.wires_shown {
                self.wires.sync_rating(serial, kept);
            }
            if self.cables_shown {
                self.cables.sync_rating(serial, kept);
            }
        }
        // Brief 057 (board 2.06): the selected cable's core colours.
        if self.cables_shown {
            let count = p.change_count();
            self.cables.sync_colours(p.ledger.document(), count);
            // Brief 059: the Details pane's Checks tab.
            self.cables.sync_checks(p, count);
        }
        let reference_style = p.ledger.document().settings.reference_style();
        let style = reference_style as u64;
        let o = p.outputs();
        if self.wires_shown {
            self.wires.sync(serial, o);
        }
        if self.cables_shown {
            self.cables.sync(serial, o);
        }
        // The counts on the tabs, from kept counts (no rows made).
        if self.counted != Some((serial, o.build_id ^ o.revision, self.group)) {
            self.counts = [o.bom_count(self.group), o.wire_count(), o.cable_count(), o.strips.len()];
            let terminals = o.strips.sorted().iter().map(|s| s.rows.len()).sum();
            let cards: Vec<_> = o.plc.cards.values().collect();
            let channels = usize::try_from(ecad_electrical::plc::overview::totals(&cards).0).unwrap_or(usize::MAX);
            self.tab_counts = [terminals, channels, o.wire_count(), o.cable_count()];
            self.counted = Some((serial, o.build_id ^ o.revision, self.group));
        }
        // The Terminals tab follows only the strips it shows (review of PR #55, finding 2; design
        // 028 §11.1): an edit that changes none leaves its copy; the others follow the models.
        let revision = match self.tab {
            Tab::Terminals => o.build_id.wrapping_mul(1 << 40).wrapping_add(o.strips.changed_at(self.strip).wrapping_mul(8)).wrapping_add(style),
            _ => o.revision,
        };
        let revision = revision.wrapping_add(u64::from(self.hide_accessories)).wrapping_add(u64::from(self.hide_not_drawn) << 1);
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
                o.bom(self.group).iter().map(|r| if r.entries.len() == 1 { r.entries.first().and_then(|e| e.0.loose()) } else { None }).collect();
            self.csv = Some(o.bom_table_with(self.group, o.view, true));
        }
        self.table = match (self.tab, self.cores_of) {
            (Tab::Bom, _) => o.bom_table(self.group),
            (Tab::Wires, _) => o.wire_table(),
            (Tab::Cables, Some(k)) => o.core_table(k),
            (Tab::Cables, None) => o.cable_table_with(!self.hide_not_drawn),
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
        self.flagged = self.table.flagged();
        self.filter();
    }

    /// Forget the copy.
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.counted = None;
        self.strips_seen = None;
        self.terminals.invalidate();
        self.wires.invalidate();
        self.cables.invalidate();
    }

    /// The result of the last [`Out::Run`]: a terminal plan edit's refusal shows under its cell.
    pub fn ran(&mut self, refusal: Option<String>) {
        // Only the view whose edit is running holds it.
        self.io.ran(refusal.clone());
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
                let o = ecad_electrical::locations::natural_cmp(&get(a), &get(b));
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
            ui.menu_button("Columns \u{25be}", |ui| {
                let mut article = self.view.article;
                if Checkbox::new(&mut article, "Article number").show(ui).accessible_name(COLUMN_ARTICLE).changed() {
                    let v = ecad_model::naming::words::flag_word(article);
                    *out = Some(Out::Run(format!("setdefault {}", serde_json::json!({"key": "bom.column.article", "level": "project", "value": v}))));
                    ui.close();
                }
                if self.group == Group::Location {
                    let mut nested = self.view.nested;
                    if Checkbox::new(&mut nested, "Lines under devices").show(ui).accessible_name(COLUMN_NESTED).changed() {
                        let v = if nested { "nested" } else { "flat" };
                        *out = Some(Out::Run(format!("setdefault {}", serde_json::json!({"key": "bom.lines", "level": "project", "value": v}))));
                        ui.close();
                    }
                }
            })
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, COLUMNS));
            let add = ui.add(Button::new("+ Item").small()).accessible_name(ADD_ITEM);
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
        let s = ecad_widgets::SHELL;
        egui::Window::new("REPORTS")
            .id(egui::Id::unique("ecad.reports"))
            .default_size(vec2(size::DIALOG_M * 1.6, size::DIALOG_M * 0.85))
            .resizable(true)
            .collapsible(false)
            .open(&mut open)
            .hosted(self.host)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // UI rev 3: as Project ▸ Reports' preview the list above picks the tab.
                    for (i, tab) in TABS.into_iter().enumerate().filter(|_| !self.preview) {
                        let n = self.counts.get(i).copied().unwrap_or(0);
                        let text = format!("{} ({n})", tab.label());
                        let b = prim::Chip::new(&text).selected(self.tab == tab).show(ui).response;
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
                        let strips = self.strips.clone();
                        let names: Vec<&str> = std::iter::once("All").chain(strips.iter().map(|(_, t)| t.as_str())).collect();
                        let mut k = self.strip.and_then(|c| strips.iter().position(|(x, _)| *x == c)).map_or(0, |i| i + 1);
                        if Select::new(&mut k, &names, "ecad.reports.strip").width(size::LABEL_COLUMN * 1.5).show(ui).changed() {
                            self.strip = k.checked_sub(1).and_then(|i| strips.get(i)).map(|(c, _)| *c);
                        }
                        // Brief 030 (design 030 §2.4): accessories at the strip's ends and markers,
                        // for the strip picked (or the only one).
                        let only = match self.strips.as_slice() {
                            [(c, _)] => Some(*c),
                            _ => None,
                        };
                        let one = self.strip.or(only);
                        let mut acc = !self.hide_accessories;
                        if Checkbox::new(&mut acc, "Accessories").show(ui).accessible_name("ecad.reports.terminals.acc").changed() {
                            self.hide_accessories = !acc;
                        }
                        ui.add_enabled_ui(one.is_some(), |ui| {
                            ui.menu_button("Accessory \u{25be}", |ui| {
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
                    // Brief 059 (Q3): Include not-drawn cables, for this run.
                    if self.tab == Tab::Cables && self.cores_of.is_none() {
                        let mut on = !self.hide_not_drawn;
                        prim::body(ui, "Include not-drawn cables", true);
                        if Toggle::new(&mut on, "ecad.reports.cables.include_not_drawn").show(ui).changed() {
                            self.hide_not_drawn = !on;
                        }
                    }
                    if self.tab == Tab::Cables
                        && self.cores_of.is_some()
                        && ui.add(Button::new("\u{2190} All cables").small()).accessible_name("ecad.reports.cables.back").clicked()
                    {
                        self.cores_of = None;
                    }
                    if self.tab == Tab::Bom {
                        prim::body(ui, "Group", true);
                        let mut k = usize::from(self.group != Group::Location);
                        if Select::new(&mut k, &["By location", "By part"], "ecad.reports.group").width(size::LABEL_COLUMN * 1.25).show(ui).changed()
                        {
                            self.group = if k == 0 { Group::Location } else { Group::Part };
                        }
                        self.bom_tools(ui, &mut out);
                    }
                    let find = SearchField::new(&mut self.find, "ecad.reports.find").placeholder("Find").width(size::LABEL_COLUMN * 1.5).show(ui);
                    if find.changed() {
                        self.filter();
                    }
                    if Checkbox::new(&mut self.only_flagged, "Only ?").show(ui).accessible_name("ecad.reports.flagged").changed() {
                        self.filter();
                    }
                    let flagged = self.table.flagged();
                    if flagged > 0 {
                        prim::note(ui, &format!("{flagged} ?"), Tone::Warning);
                    }
                    if ui.add(Button::new("Export CSV\u{2026}").small()).accessible_name("ecad.reports.export").clicked() {
                        self.export = Some(format!("{}-{}.csv", self.stem, self.tab.file()));
                    }
                });
                if let Some(path) = &mut self.export {
                    let mut go = false;
                    let mut cancel = false;
                    ui.horizontal(|ui| {
                        prim::body(ui, "Save as", true);
                        let r = TextEntry::new(path, "ecad.reports.export.path").width(size::DIALOG_M * 0.75).show(ui);
                        go = ui.add(Button::new("Save").kind(Kind::Primary).small()).clicked()
                            || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        cancel = ui.add(Button::new("Cancel").small()).clicked();
                    });
                    if go {
                        out = Some(Out::Export(path.clone()));
                    }
                    if cancel {
                        self.export = None;
                    }
                }
                if let Some(n) = &self.note {
                    prim::note(ui, n, Tone::Neutral);
                }
                ui.separator();
                // Brief 039: the terminal manager is the one table, mirrored (design 036 §9).
                if self.tab == Tab::Terminals {
                    for a in self.terminals.show(ui, true) {
                        out = Some(match a {
                            crate::workspace::ListAction::Run(line) => Out::Run(line),
                            crate::workspace::ListAction::GoTo(pg, at) => Out::GotoAt(pg, at),
                            _ => continue,
                        });
                    }
                    self.rows_drawn = self.terminals.table.rows_drawn;
                    return;
                }
                if self.table.rows.is_empty() {
                    prim::body(ui, self.tab.empty(), true);
                    self.rows_drawn = 0;
                    return;
                }
                let widths = self.widths(ui);
                let font = egui::TextStyle::Monospace.resolve(ui.style());
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
                    ui.painter().text(
                        pos2(x + 2.0, hr.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("{h}{arrow}"),
                        font.clone(),
                        ecad_widgets::color(s.text_2),
                    );
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
                                p.rect_filled(rect, 0.0, ecad_widgets::color(s.hover));
                            }
                            let mut x = rect.left();
                            let mut tip: Vec<String> = Vec::new();
                            for (col, c) in row.iter().enumerate() {
                                let w = widths.get(col).copied().unwrap_or(80.0);
                                let colour = ecad_widgets::color(if c.flag.is_some() { s.warning } else { s.text });
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
                                    let d = ui.add_enabled_ui(writable, |ui| ui.add(Button::new("Delete item").kind(Kind::Danger).small())).inner;
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
        egui::Window::new("PRINT BOOK")
            .id(egui::Id::unique("ecad.printbook"))
            .collapsible(false)
            .resizable(false)
            .default_width(size::DIALOG_S)
            .open(&mut open)
            .hosted(self.host)
            .show(ctx, |ui| {
                prim::PropertyGrid::new().show(ui, |g| {
                    let all = if self.scope == ScopeKind::All { format!("All pages ({} sheets)", self.sheets) } else { "All pages".into() };
                    g.row("Pages", |ui| {
                        if Radio::new(self.scope == ScopeKind::All, &all).show(ui).accessible_name("ecad.printbook.scope.all").clicked() {
                            self.scope = ScopeKind::All;
                        }
                    });
                    g.row("", |ui| {
                        ui.add_enabled_ui(!self.folders.is_empty(), |ui| {
                            if Radio::new(self.scope == ScopeKind::Folder, "Folder").show(ui).accessible_name("ecad.printbook.scope.folder").clicked()
                            {
                                self.scope = ScopeKind::Folder;
                            }
                            let paths: Vec<&str> = self.folders.iter().map(|f| f.1.as_str()).collect();
                            let mut k = self.folder;
                            if Select::new(&mut k, &paths, "ecad.printbook.folder").show(ui).changed() && k < self.folders.len() {
                                self.folder = k;
                                self.scope = ScopeKind::Folder;
                            }
                        });
                    });
                    g.row("", |ui| {
                        if Radio::new(self.scope == ScopeKind::Pages, "Pages").show(ui).accessible_name("ecad.printbook.scope.pages").clicked() {
                            self.scope = ScopeKind::Pages;
                        }
                        if TextEntry::new(&mut self.pages, "ecad.printbook.pages").placeholder("1-4, 7").show(ui).changed() {
                            self.scope = ScopeKind::Pages;
                        }
                    });
                    if self.scope == ScopeKind::Pages && !self.unknown.is_empty() {
                        for n in &self.unknown {
                            g.row("", |ui| prim::note(ui, &format!("No page {n}; it is left out."), Tone::Warning));
                        }
                    }
                    g.row("File", |ui| TextEntry::new(&mut self.path, "ecad.printbook.path").show(ui));
                    g.row("Colour", |ui| {
                        let mut k = usize::from(!self.mono);
                        if prim::Segmented::new(&mut k, &["Black on white", "As on screen"]).name("ecad.printbook.colour").show(ui).clicked.is_some()
                        {
                            self.mono = k == 0;
                        }
                    });
                    g.row("", |ui| Checkbox::new(&mut self.lineweights, "Line weights").show(ui));
                });
                ui.add_space(space::XS);
                for n in &self.notes {
                    prim::note(ui, n, Tone::Warning).accessible_name("ecad.printbook.note");
                }
                if self.flagged > 0 {
                    let n = if self.flagged == 1 { "1 value".to_string() } else { format!("{} values", self.flagged) };
                    prim::note(ui, &format!("{n} marked ? on generated pages."), Tone::Neutral);
                }
                if let Some(e) = &self.error {
                    prim::note(ui, e, Tone::Error);
                }
                let none = self.sheets == 0;
                if none {
                    prim::note(ui, "No page matches.", Tone::Warning);
                }
                let blocked = if none {
                    Some("No page matches.")
                } else if self.path.trim().is_empty() {
                    Some("Choose a file first.")
                } else {
                    None
                };
                ui.add_space(space::S);
                ui.allocate_ui_with_layout(vec2(ui.available_width(), size::CONTROL), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ecad_widgets::dialog::action(ui, "Print", Kind::Primary, "ecad.printbook.print", blocked).clicked() {
                        out = Some(true);
                    }
                    if ecad_widgets::dialog::action(ui, "Cancel", Kind::Secondary, "ecad.printbook.cancel", None).clicked() {
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
pub fn accessory_menu(c: ecad_model::DeviceId) -> Vec<(&'static str, serde_json::Value)> {
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
