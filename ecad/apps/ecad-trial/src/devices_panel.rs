//! The **Devices** tab of the Project panel (owner request, 2026-11-08, after brief 018): every
//! device with its tag and description, its function, location, symbols, pages and part; a click
//! on a row shows its first symbol's page and selects all its symbols on that page; the selected
//! device's properties are edited in place through `setprop` (brief 018's rules, its merge offer
//! included).
//!
//! Brief 039: the rows are the one table's Devices column set ([`crate::lists::devices`]): tag and
//! description edit in place (the same `setprop`), the list is patched with the devices a change
//! touched, and selecting symbols on the page selects their rows. This tab is its host until the
//! shell (U2) places it.
//!
//! Excluded: cables, terminals, creating or deleting devices.

use std::collections::BTreeSet;

use ecad_cadcraft::device_list::DeviceEntry;
use ecad_cadcraft::devices::PROPERTIES;
use ecad_model::ComponentId;
use eframe::egui::{self, Rect, RichText, Stroke, pos2, vec2};

use crate::lists::devices::{self as dl, DevicesSource, NOT_PLACED};
use crate::project_panel::Action;

/// Accessible label of the filter field.
pub const FILTER: &str = "Filter Devices";
/// The merge offer's button.
pub const MERGE: &str = "Make it the same device";
/// Brief 026: the clash box's first button, and the header's Renumber.
pub const SWAP: &str = "Swap";
pub const RENUMBER: &str = "Renumber…";

/// The accessible label of a device row: `Device -K01: Contactor / relay coil`.
pub fn device_label(e: &DeviceEntry) -> String {
    format!("Device {}: {}", e.tag.as_deref().unwrap_or("(no tag)"), e.description)
}

/// A property being edited: (device, property key, text).
type Editing = (ComponentId, &'static str, String);

/// Board 1.03's texts.
pub const FIND: &str = "Find a device or tag";
/// Under "Not on a page yet": placing a listed device has no command yet, so the rows offer no
/// drag (the board's "Drag one onto the sheet" waits for that command).
pub const PLACE_LATER: &str = "Placing from this list is not available yet.";
pub const FOOTER: &str = "Click goes to it. Edit many at once in Project.";
pub const PROJECT: &str = "Project →";
const NO_LOCATION: &str = "No location";
/// A device's function is read from the device only (no page or box gives one yet), so the group
/// says where the value is missing.
const NO_FUNCTION: &str = "Function not set on the device";
/// A list row's height, and how many devices a group shows before "n more".
pub const DEVICE_ROW_H: f32 = 27.0;
const GROUP_CAP: usize = 8;

/// How the devices are grouped (board 1.03).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Grouping {
    None,
    #[default]
    Location,
    Function,
    Type,
}

impl Grouping {
    pub const ALL: [Grouping; 4] = [Grouping::None, Grouping::Location, Grouping::Function, Grouping::Type];

    pub fn label(self) -> &'static str {
        match self {
            Grouping::None => "None",
            Grouping::Location => "Location",
            Grouping::Function => "Function",
            Grouping::Type => "Type",
        }
    }

    pub fn index(self) -> usize {
        Grouping::ALL.iter().position(|g| *g == self).unwrap_or(0)
    }
}

/// One drawn line of the list.
#[derive(Debug, Clone, PartialEq)]
enum Line {
    Group {
        key: String,
        name: String,
        count: usize,
        unplaced: bool,
    },
    /// A device, by its index in the list's entries.
    Device {
        at: usize,
    },
    More {
        key: String,
        n: usize,
    },
    Hint,
}

/// What the lines were laid out from.
type LaidOut = (Option<(u64, u64)>, String, Grouping, BTreeSet<String>, BTreeSet<String>);

/// The clash box's ways for a taken tag (brief 026, design §7.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clash {
    /// The holder is a device, so Swap and Make it the same device are offered.
    pub same_kind: bool,
    /// The next free tag, for **Use -K08**.
    pub next: Option<String>,
}

/// The tab's state.
#[derive(Debug)]
pub struct DevicesPanel {
    /// The search text (the table's search field).
    pub filter: String,
    /// The list of every device, kept up to date by `ecad-cadcraft`.
    pub list: ecad_cadcraft::device_list::DeviceList,
    /// Brief 039: the rows as a table source, and the table.
    pub source: DevicesSource,
    pub table: ecad_ui::table::Table<ComponentId>,
    /// The (project serial, change count) the table was brought up to.
    seen: Option<(u64, u64)>,
    /// The page selection last followed (handles), so the table follows only a new one.
    followed: Vec<cadcraft_engine::doc::Handle>,
    pub selected: Option<ComponentId>,
    editing: Option<Editing>,
    /// Board 1.03: the grouping, folded groups, groups showing every device, the lines drawn and
    /// what they were laid out from, and a note (a drop on the sheet).
    pub grouping: Grouping,
    pub folded: BTreeSet<String>,
    pub unfolded: BTreeSet<String>,
    lines: Vec<Line>,
    laid_out: Option<LaidOut>,
    pub note: Option<String>,
    /// A refusal of the last property edit, and, for a tag another device holds, the edit to
    /// offer as a merge.
    pub error: Option<(String, Option<(ComponentId, String)>)>,
    /// Brief 026: what the clash box offers with that refusal.
    pub clash: Option<Clash>,
    /// Rows laid out on the last frame (tests: virtualised).
    pub rows_drawn: usize,
}

impl Default for DevicesPanel {
    fn default() -> Self {
        let mut table = ecad_ui::table::Table::new("ecad.devices", "Devices");
        table.empty = "No devices yet.";
        Self {
            filter: String::new(),
            list: Default::default(),
            source: Default::default(),
            table,
            seen: None,
            followed: Vec::new(),
            selected: None,
            editing: None,
            grouping: Grouping::default(),
            folded: BTreeSet::new(),
            unfolded: BTreeSet::new(),
            lines: Vec::new(),
            laid_out: None,
            note: None,
            error: None,
            clash: None,
            rows_drawn: 0,
        }
    }
}

impl DevicesPanel {
    /// The devices listed now (those the filter matches).
    pub fn entries(&self) -> Vec<DeviceEntry> {
        self.list.entries.iter().filter(|e| ecad_cadcraft::device_list::matches(e, &self.filter)).cloned().collect()
    }

    /// Bring the list and the table up to date: the list through
    /// [`ecad_cadcraft::device_list::DeviceList::sync`] (built whole once, then only the devices a
    /// change touched), the table by the update map of [`crate::lists::devices`].
    pub fn sync(&mut self, serial: u64, p: &ecad_cadcraft::Project) {
        let count = p.change_count();
        if self.seen == Some((serial, count)) && self.table.built() {
            return;
        }
        let since = match self.seen {
            Some((s, n)) if s == serial && self.table.built() => Some(n),
            _ => None,
        };
        let (changed, touched) = dl::changes(p, since);
        self.list.sync(serial, p);
        let doc = p.ledger.document();
        match &changed {
            ecad_ui::table::Changed::All => self.source.build(doc, &self.list.entries),
            ecad_ui::table::Changed::Keys(_) => self.source.patch(doc, &self.list.entries, &touched),
            ecad_ui::table::Changed::Nothing => {}
        }
        self.table.update(&self.source, changed);
        if self.selected.is_some_and(|c| !self.source.contains_key(c)) {
            self.selected = None;
        }
        self.seen = Some((serial, count));
    }

    /// The page's selection changed: select the devices it holds symbols of, and scroll to the
    /// first (row ↔ object link). Reads only the selected handles.
    pub fn follow(&mut self, p: &ecad_cadcraft::Project, handles: &[cadcraft_engine::doc::Handle]) {
        if self.followed == handles {
            return;
        }
        self.followed = handles.to_vec();
        let doc = p.ledger.document();
        let Some(map) = p.active_page().and_then(|pg| p.pages.get(&pg)).map(|v| &v.map) else { return };
        let mut devices: Vec<ComponentId> = Vec::new();
        for h in handles.iter().take(ecad_cadcraft::props::MAX_LOOKED_AT) {
            let rep = match map.get(*h) {
                Some(ecad_cadcraft::project::Mapped::Item(id)) => doc.representation_of_item(id).map(|(_, r)| r.subject),
                Some(ecad_cadcraft::project::Mapped::Tag(rep) | ecad_cadcraft::project::Mapped::Marker(rep, _)) => {
                    doc.representations.get(&rep).map(|r| r.subject)
                }
                _ => None,
            };
            if let Some(ecad_model::Subject::Component(c)) = rep
                && self.source.contains_key(c)
                && !devices.contains(&c)
            {
                devices.push(c);
            }
        }
        if devices.is_empty() {
            return;
        }
        self.selected = devices.first().copied();
        self.table.reveal(&devices);
    }

    /// The tab is shown again: the next [`DevicesPanel::sync`] builds the list and table whole.
    pub fn invalidate(&mut self) {
        self.list.invalidate();
        self.table.reset();
        self.seen = None;
    }

    /// The panel's contents (UI rev 3, board 1.03): search, the grouping, "Not on a page yet"
    /// first, then the devices in their groups, and the footer pointing to Project for editing many
    /// at once. A click on a device shows its first symbol's page and selects its symbols there.
    pub fn show_panel(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        let mut out = Vec::new();
        let s = ecad_ui::SHELL;
        let c = ecad_ui::color;
        let r = ui.max_rect();
        ecad_ui::prim::Field::new(&mut self.filter, ecad_ui::prim::FieldKind::Search, FILTER).placeholder(FIND).show(ui);
        ui.add_space(6.0);
        let mut g = self.grouping.index();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (i, grouping) in Grouping::ALL.iter().enumerate() {
                let on = g == i;
                let w = crate::shell::bar::text_width(ui, grouping.label(), ecad_ui::tokens::ui_type::BODY) + 16.0;
                let (rect, _) = ui.allocate_exact_size(vec2(w, 24.0), egui::Sense::hover());
                if crate::shell::bar::text_button(ui, rect, grouping.label(), &format!("Group by {}", grouping.label()), on, false).clicked() {
                    g = i;
                }
            }
        });
        self.grouping = Grouping::ALL.get(g).copied().unwrap_or_default();
        ui.add_space(6.0);
        // The clash box of a refused tag edit (brief 026), above the list.
        self.clash_box(ui, &mut out);
        if let Some(n) = &self.note {
            ui.add(egui::Label::new(RichText::new(n).small().color(c(s.text_2))).wrap());
        }
        self.relayout();
        let footer_h = 44.0;
        let list_h = (ui.available_height() - footer_h).max(DEVICE_ROW_H);
        let lines = std::mem::take(&mut self.lines);
        let mut toggle: Option<String> = None;
        let mut more: Option<String> = None;
        let mut goto: Option<ComponentId> = None;
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut drawn = 0;
        egui::ScrollArea::vertical().id_salt("ecad.devices.list").max_height(list_h).auto_shrink([false, false]).show_rows(
            ui,
            DEVICE_ROW_H,
            lines.len(),
            |ui, range| {
                // Rows are as wide as the panel (the content never widens it).
                ui.set_max_width(r.width());
                for line in lines.get(range).unwrap_or_default() {
                    drawn += 1;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), DEVICE_ROW_H), egui::Sense::click_and_drag());
                    let p = ui.painter_at(rect);
                    let y = rect.center().y;
                    let body = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::BODY);
                    let bold = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::BODY_EMPHASIS);
                    match line {
                        Line::Group { key, name, count, unplaced } => {
                            let folded = self.folded.contains(key);
                            let chev = Rect::from_center_size(pos2(rect.left() + 8.0, y), vec2(10.0, 10.0));
                            let icon = if folded { ecad_ui::tokens::Icon::ChevronRight } else { ecad_ui::tokens::Icon::ChevronDown };
                            ecad_ui::icons::paint(&p, chev, icon, c(s.text_3));
                            let ink = if *unplaced { c(s.accent_text) } else { c(s.text_2) };
                            p.text(pos2(rect.left() + 20.0, y), egui::Align2::LEFT_CENTER, name, bold, ink);
                            let n = if *unplaced { c(s.accent_text) } else { c(s.text_3) };
                            p.text(pos2(rect.right() - 6.0, y), egui::Align2::RIGHT_CENTER, count.to_string(), body, n);
                            if resp.hovered() {
                                p.rect_filled(rect, 0.0, c(s.hover).gamma_multiply(0.4));
                            }
                            let label = format!("Group {name}: {count}");
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                            if resp.clicked() {
                                toggle = Some(key.clone());
                            }
                        }
                        Line::Device { at } => {
                            let Some(e) = self.list.entries.get(*at) else { continue };
                            let picked = self.selected == Some(e.id);
                            if picked {
                                p.rect_filled(rect, 0.0, c(s.accent_soft));
                                p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, c(s.accent));
                            } else if resp.hovered() {
                                p.rect_filled(rect, 0.0, c(s.hover));
                            }
                            let x = rect.left() + 20.0;
                            let tag = e.tag.as_deref().unwrap_or("(no tag)");
                            let tag_ink = c(s.text);
                            p.text(pos2(x, y), egui::Align2::LEFT_CENTER, tag, bold.clone(), tag_ink);
                            let place = e.symbols.first().map(|sym| sym.place.clone()).unwrap_or_default();
                            let pr = p.text(pos2(rect.right() - 6.0, y), egui::Align2::RIGHT_CENTER, &place, body.clone(), c(s.text_3));
                            let dx = x + 52.0;
                            let room = (pr.left() - dx - 6.0).max(0.0);
                            let desc = crate::project_panel::cut_text(ui, &e.description, &body, room);
                            p.text(pos2(dx, y), egui::Align2::LEFT_CENTER, desc, body, c(s.text_2));
                            let label = device_label(e);
                            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, picked, &label));
                            if resp.clicked() {
                                goto = Some(e.id);
                            }
                        }
                        Line::More { key, n } => {
                            let label = format!("{n} more");
                            p.text(pos2(rect.left() + 72.0, y), egui::Align2::LEFT_CENTER, &label, body, c(s.text_3));
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                            if resp.clicked() {
                                more = Some(key.clone());
                            }
                        }
                        Line::Hint => {
                            p.text(
                                pos2(rect.left() + 20.0, y),
                                egui::Align2::LEFT_CENTER,
                                PLACE_LATER,
                                crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::SMALL),
                                c(s.text_3),
                            );
                        }
                    }
                }
            },
        );
        self.rows_drawn = drawn;
        self.lines = lines;
        if let Some(k) = toggle
            && !self.folded.remove(&k)
        {
            self.folded.insert(k);
        }
        if let Some(k) = more {
            self.unfolded.insert(k);
        }
        if let Some(id) = goto {
            if self.selected != Some(id) {
                self.editing = None;
                self.error = None;
            }
            self.selected = Some(id);
            let reps: Vec<_> = self.list.entries.iter().find(|e| e.id == id).map(|e| e.symbols.iter().map(|x| x.rep).collect()).unwrap_or_default();
            if !reps.is_empty() {
                out.push(Action::GotoSymbols(reps));
            }
        }
        // The footer.
        let foot = Rect::from_min_max(pos2(r.left() - 8.0, (r.bottom() - footer_h).max(r.top())), pos2(r.right() + 8.0, r.bottom()));
        ui.painter().hline(foot.x_range(), foot.top() + 0.5, Stroke::new(1.0, c(s.line)));
        let inner = foot.shrink2(vec2(8.0, 6.0));
        crate::shell::in_rect(ui, inner, "ecad.devices.footer", |ui| {
            ui.horizontal_centered(|ui| {
                let _ = writable;
                let w = (ui.available_width() - 84.0).max(40.0);
                ui.add_sized(vec2(w, 30.0), egui::Label::new(RichText::new(FOOTER).small().color(c(s.text_3))).wrap());
                if ui.add(ecad_ui::prim::Button::new(PROJECT).small()).clicked() {
                    out.push(Action::ProjectDevices);
                }
            });
        });
        out
    }

    /// Lay the lines out again when the list, the search or the grouping changed (not per frame).
    fn relayout(&mut self) {
        let key = (self.seen, self.filter.clone(), self.grouping, self.folded.clone(), self.unfolded.clone());
        if self.laid_out.as_ref() == Some(&key) {
            return;
        }
        let shown: Vec<usize> =
            self.list.entries.iter().enumerate().filter(|(_, e)| ecad_cadcraft::device_list::matches(e, &self.filter)).map(|(i, _)| i).collect();
        let (unplaced, placed): (Vec<usize>, Vec<usize>) =
            shown.into_iter().partition(|i| self.list.entries.get(*i).is_some_and(|e| e.symbols.is_empty()));
        let mut lines = Vec::new();
        if !unplaced.is_empty() {
            let k = NOT_PLACED.to_string();
            lines.push(Line::Group { key: k.clone(), name: NOT_PLACED.to_string(), count: unplaced.len(), unplaced: true });
            if !self.folded.contains(&k) {
                lines.extend(unplaced.iter().map(|at| Line::Device { at: *at }));
                lines.push(Line::Hint);
            }
        }
        let prop = |e: &DeviceEntry, i: usize| e.properties.get(i).cloned().flatten().filter(|v| !v.trim().is_empty());
        let mut groups: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();
        for at in placed {
            let Some(e) = self.list.entries.get(at) else { continue };
            let name = match self.grouping {
                Grouping::None => String::new(),
                // Where the device is (its own value, else its box's, page's or the project's), as
                // the Device list shows it, without the "(from box on 105)" note.
                Grouping::Location => self
                    .source
                    .row(e.id)
                    .map(|r| r.location.split(" (").next().unwrap_or_default().trim().to_string())
                    .filter(|l| !l.is_empty() && l != "?")
                    .unwrap_or_else(|| NO_LOCATION.to_string()),
                Grouping::Function => prop(e, 5).unwrap_or_else(|| NO_FUNCTION.to_string()),
                Grouping::Type => e.group.clone(),
            };
            groups.entry(name).or_default().push(at);
        }
        for (name, ats) in groups {
            let flat = self.grouping == Grouping::None;
            if !flat {
                lines.push(Line::Group { key: name.clone(), name: name.clone(), count: ats.len(), unplaced: false });
                if self.folded.contains(&name) {
                    continue;
                }
            }
            let cap = if flat || self.unfolded.contains(&name) { ats.len() } else { GROUP_CAP };
            lines.extend(ats.iter().take(cap).map(|at| Line::Device { at: *at }));
            if ats.len() > cap {
                lines.push(Line::More { key: name.clone(), n: ats.len() - cap });
            }
        }
        self.lines = lines;
        self.laid_out = Some(key);
    }

    /// The refusal of a tag edit with its ways (brief 026, design §7.1): Swap first, then the same
    /// device, then the next free tag.
    fn clash_box(&mut self, ui: &mut egui::Ui, out: &mut Vec<Action>) {
        let Some((msg, offer)) = self.error.clone() else { return };
        let (text, accent) = (ecad_ui::color(ecad_ui::SHELL.text), ecad_ui::color(ecad_ui::SHELL.accent));
        ui.add(egui::Label::new(RichText::new(&msg).small().color(ui.visuals().warn_fg_color)).wrap());
        if let Some((target, tag)) = offer {
            let (same_kind, next) = self.clash.as_ref().map_or((false, None), |k| (k.same_kind, k.next.clone()));
            ui.horizontal_wrapped(|ui| {
                if same_kind {
                    let swap = egui::Button::new(RichText::new(SWAP).small().color(text)).fill(accent);
                    if ui.add(swap).accessible_name("ecad.tagclash.swap").on_hover_text("Exchange the two tags (one undo step)").clicked() {
                        self.error = None;
                        out.push(Action::SetDeviceProperty { component: target, key: "tag", value: tag.clone(), conflict: Some("swap") });
                    }
                    if ui
                        .small_button(MERGE)
                        .accessible_name("ecad.tagclash.merge")
                        .on_hover_text("Make this device part of the device that has that tag (one undo step)")
                        .clicked()
                    {
                        self.error = None;
                        out.push(Action::SetDeviceProperty { component: target, key: "tag", value: tag.clone(), conflict: Some("merge") });
                    }
                }
                let label = next.map_or_else(|| "Use the next free".to_string(), |n| format!("Use {n}"));
                if ui.small_button(label).accessible_name("ecad.tagclash.next").clicked() {
                    self.error = None;
                    out.push(Action::SetDeviceProperty { component: target, key: "tag", value: tag.clone(), conflict: Some("next") });
                }
            });
        }
        ui.add_space(4.0);
    }

    /// The tab's contents: Renumber, the table, the selected device's properties. Returns the
    /// actions asked for.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        let mut out = Vec::new();
        ui.horizontal(|ui| {
            // Brief 026 (design §9): Renumber in the header.
            if ui.add_enabled(writable, egui::Button::new(RichText::new(RENUMBER).small())).accessible_name("ecad.devices.renumber").clicked() {
                out.push(Action::Renumber);
            }
        });
        // The selected device's properties, at the bottom of the tab.
        let editor_h = if self.selected.is_some() { 6.0 * 24.0 + 40.0 } else { 0.0 } + if self.error.is_some() { 60.0 } else { 0.0 };
        let list_h = (ui.available_height() - editor_h).max(ecad_ui::tokens::size::ROW_TABLE * 4.0);
        let acts = ui
            .allocate_ui(vec2(ui.available_width(), list_h), |ui| {
                ui.set_min_height(list_h);
                self.show_table(ui, writable, true)
            })
            .inner;
        out.extend(acts);
        if let Some(c) = self.selected {
            self.editor(ui, c, writable, &mut out);
        }
        out
    }

    /// The selected device's properties (UI rev 3: Project ▸ Devices shows them in the right
    /// panel); nothing when no device is selected.
    pub fn show_editor(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        let mut out = Vec::new();
        if let Some(c) = self.selected {
            self.editor(ui, c, writable, &mut out);
        }
        out
    }

    /// The table alone, with or without its toolbar (UI rev 3: Project ▸ Devices draws the
    /// filter in the workspace's head). Returns the actions asked for.
    pub fn show_table(&mut self, ui: &mut egui::Ui, writable: bool, toolbar: bool) -> Vec<Action> {
        use ecad_ui::table::TableEvent;
        let mut out = Vec::new();
        self.table.toolbar = toolbar;
        self.table.query = self.filter.clone();
        let events = self.table.show(ui, &self.source);
        self.filter = self.table.query.clone();
        self.rows_drawn = self.table.rows_drawn;
        // The device's symbols, first first; empty for a device with none.
        let symbols =
            |s: &DevicesSource, c: ComponentId| s.row(c).map(|r| r.entry.symbols.iter().map(|x| x.rep).collect::<Vec<_>>()).unwrap_or_default();
        for e in events {
            match e {
                TableEvent::Selected(keys) => {
                    let first = keys.first().copied();
                    if self.selected != first {
                        self.editing = None;
                        self.error = None;
                    }
                    self.selected = first;
                    // One row: its symbols on the page (the first one's page, all of them on it
                    // selected).
                    if let [c] = keys.as_slice() {
                        let reps = symbols(&self.source, *c);
                        if !reps.is_empty() {
                            out.push(Action::GotoSymbols(reps));
                        }
                    }
                }
                TableEvent::GoTo { key, .. } | TableEvent::Open(key) => {
                    self.selected = Some(key);
                    let reps = symbols(&self.source, key);
                    if !reps.is_empty() {
                        out.push(Action::GotoSymbols(reps));
                    }
                }
                TableEvent::Commit { key, col, value } => {
                    if let Some(k) = dl::property(col).filter(|_| writable) {
                        self.selected = Some(key);
                        out.push(Action::SetDeviceProperty { component: key, key: k, value, conflict: None });
                    }
                }
                TableEvent::Export(csv) => crate::lists::copy_csv(ui, csv),
                TableEvent::Button { .. } => {}
            }
        }
        out
    }

    /// The selected device's six properties, each a field; Enter or leaving a changed field runs
    /// `setprop`, Esc puts it back.
    fn editor(&mut self, ui: &mut egui::Ui, c: ComponentId, writable: bool, out: &mut Vec<Action>) {
        let Some(e) = self.list.entries.iter().find(|e| e.id == c).cloned() else {
            self.selected = None;
            return;
        };
        let (line, dim, text, accent) = (
            ecad_ui::color(ecad_ui::SHELL.line),
            ecad_ui::color(ecad_ui::SHELL.text_2),
            ecad_ui::color(ecad_ui::SHELL.text),
            ecad_ui::color(ecad_ui::SHELL.accent),
        );
        ui.add_space(4.0);
        ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, line));
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(RichText::new(format!("DEVICE {}", e.tag.as_deref().unwrap_or("(no tag)"))).small().strong().color(dim));
        if let Some((msg, offer)) = self.error.clone() {
            ui.add(egui::Label::new(RichText::new(&msg).small().color(ui.visuals().warn_fg_color)).wrap());
            // Brief 026 (design §7.1): Swap first, then the same device, then the next free tag.
            if let Some((target, tag)) = offer {
                let (same_kind, next) = self.clash.as_ref().map_or((false, None), |k| (k.same_kind, k.next.clone()));
                ui.horizontal(|ui| {
                    if same_kind {
                        let swap = egui::Button::new(RichText::new(SWAP).small().color(text)).fill(accent);
                        if ui.add(swap).accessible_name("ecad.tagclash.swap").on_hover_text("Exchange the two tags (one undo step)").clicked() {
                            self.error = None;
                            out.push(Action::SetDeviceProperty { component: target, key: "tag", value: tag.clone(), conflict: Some("swap") });
                        }
                        if ui
                            .small_button(MERGE)
                            .accessible_name("ecad.tagclash.merge")
                            .on_hover_text("Make this device part of the device that has that tag (one undo step)")
                            .clicked()
                        {
                            self.error = None;
                            out.push(Action::SetDeviceProperty { component: target, key: "tag", value: tag.clone(), conflict: Some("merge") });
                        }
                    }
                    let label = next.map_or_else(|| "Use the next free".to_string(), |n| format!("Use {n}"));
                    if ui.small_button(label).accessible_name("ecad.tagclash.next").clicked() {
                        self.error = None;
                        out.push(Action::SetDeviceProperty { component: target, key: "tag", value: tag.clone(), conflict: Some("next") });
                    }
                });
            }
        }
        // Brief 026 (design §4): the Fixed box beside the tag; greyed for a provisional tag.
        let provisional = e.tag.as_deref().is_some_and(ecad_model::tag::is_provisional);
        let mut fixed = e.fixed;
        let can = writable && e.tag.is_some() && !provisional;
        let r = ui.add_enabled(can, egui::Checkbox::new(&mut fixed, RichText::new("Fixed").small())).accessible_name("ecad.props.tagfixed");
        let r = if provisional { r.on_disabled_hover_text("A provisional tag can't be fixed; give it a number first.") } else { r };
        if r.changed() {
            out.push(Action::SetDeviceProperty {
                component: c,
                key: "tag_fixed",
                value: if fixed { "yes".into() } else { String::new() },
                conflict: None,
            });
        }
        egui::Grid::new("ecad.devices.props").num_columns(2).spacing(vec2(6.0, 2.0)).show(ui, |ui| {
            for (i, (key, name)) in PROPERTIES.iter().enumerate() {
                let stored = e.properties.get(i).cloned().flatten().unwrap_or_default();
                ui.label(RichText::new(*name).small());
                let mine = self.editing.as_ref().filter(|(d, k, _)| *d == c && k == key);
                let mut text = mine.map_or_else(|| stored.clone(), |(_, _, x)| x.clone());
                let field = ui.add_enabled(
                    writable,
                    egui::TextEdit::singleline(&mut text).id(egui::Id::unique(("ecad.devices.prop", *key))).desired_width(f32::INFINITY),
                );
                let field = field.accessible_name(format!("{name} field"));
                if field.changed() {
                    self.editing = Some((c, key, text.clone()));
                }
                if field.lost_focus() && self.editing.as_ref().is_some_and(|(d, k, _)| *d == c && k == key) {
                    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    // The key that ended the edit is the field's, not CADCraft's command line's.
                    ui.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                    });
                    let edited = self.editing.take().map(|(_, _, x)| x).unwrap_or_default();
                    if !esc && edited.trim() != stored.trim() {
                        let _ = enter;
                        out.push(Action::SetDeviceProperty { component: c, key, value: edited, conflict: None });
                    }
                }
                ui.end_row();
            }
        });
    }

    /// The trial reports a refused property edit; a tag held by another device offers the merge.
    pub fn refused(&mut self, c: ComponentId, key: &str, value: &str, message: String) {
        let offer = (key == "tag" && message.contains(" is used by ")).then(|| (c, value.to_string()));
        // Brief 026: the ways are read from the refusal before it is shortened.
        self.clash = offer.as_ref().map(|_| Clash {
            same_kind: message.contains(r#""swap""#),
            next: message.split("(use ").nth(1).and_then(|r| r.split(')').next()).map(str::to_string),
        });
        // The command's ways forward name its JSON parameter; here the buttons are the ways, so
        // only the first sentence is shown.
        let message = match message.find(" Give conflict:").or_else(|| message.find(" It is a ")) {
            Some(i) if offer.is_some() => message.get(..i).unwrap_or(&message).to_string(),
            _ => message,
        };
        // Brief 039: the refusal shows under the table's cell too, and the cell reverts.
        if let Some(col) = dl::column_of(key) {
            self.table.refuse(c, col, message.clone());
        }
        self.error = Some((message, offer));
    }
}
