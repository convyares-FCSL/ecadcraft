//! The **Devices** tab of the Project panel (owner request, 2026-11-08, after brief 018): every
//! device grouped by its tag prefix, each row its tag and description, expandable to its symbols
//! with page and column; a click on a symbol shows its page and selects it; the selected device's
//! properties are edited in place through `setprop` (brief 018's rules, its merge offer included).
//!
//! The list is a copy ([`ecad_cadcraft::device_list::DeviceEntry`]) rebuilt only when the
//! project's change count or the filter changed; the rows are virtualised, so a frame lays out only
//! the visible ones.
//!
//! Excluded: cables, terminals, creating or deleting devices.

use std::collections::BTreeSet;

use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::device_list::DeviceEntry;
use ecad_cadcraft::devices::PROPERTIES;
use ecad_model::{ComponentId, RepresentationId};
use eframe::egui::{self, RichText, Sense, Stroke, pos2, vec2};

use crate::project_panel::{Action, ROW_H};

/// Accessible label of the filter field.
pub const FILTER: &str = "Filter devices";
/// The merge offer's button.
pub const MERGE: &str = "Make it the same device";

/// The accessible label of a device row: `Device -K01: Contactor / relay coil`.
pub fn device_label(e: &DeviceEntry) -> String {
    format!("Device {}: {}", e.tag.as_deref().unwrap_or("(no tag)"), e.description)
}

/// The accessible label of a symbol row: `-K01 NO contact, page 2, column 3`.
pub fn symbol_label(e: &DeviceEntry, s: &ecad_cadcraft::device_list::SymbolEntry) -> String {
    let col = s.column.map(|c| format!(", column {c}")).unwrap_or_default();
    format!("{} {}, page {}{col}", e.tag.as_deref().unwrap_or("(no tag)"), s.name, s.page_number)
}

/// One laid-out row.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    Group(String, usize),
    Device(usize),
    Symbol(usize, usize),
}

/// A property being edited: (device, property key, text).
type Editing = (ComponentId, &'static str, String);

/// The tab's state.
#[derive(Debug, Default)]
pub struct DevicesPanel {
    pub filter: String,
    entries: Vec<DeviceEntry>,
    rows: Vec<Row>,
    /// (project serial, change count, filter) the copy was built at.
    seen: Option<(u64, u64, String)>,
    pub expanded: BTreeSet<ComponentId>,
    pub selected: Option<ComponentId>,
    editing: Option<Editing>,
    /// A refusal of the last property edit, and, for a tag another device holds, the edit to
    /// offer as a merge.
    pub error: Option<(String, Option<(ComponentId, String)>)>,
    /// Rebuilds of the copy (tests: only on a change).
    pub rebuilds: u64,
    /// Rows laid out on the last frame (tests: virtualised).
    pub rows_drawn: usize,
}

impl DevicesPanel {
    /// The devices listed now.
    pub fn entries(&self) -> &[DeviceEntry] {
        &self.entries
    }

    /// Bring the copy up to date (only when the project, its change count or the filter changed).
    pub fn sync(&mut self, serial: u64, count: u64, build: impl FnOnce(&str) -> Option<Vec<DeviceEntry>>) {
        let key = (serial, count, self.filter.clone());
        if self.seen.as_ref() == Some(&key) {
            return;
        }
        if let Some(e) = build(&self.filter) {
            self.entries = e;
            self.seen = Some(key);
            self.rebuilds += 1;
            self.flatten();
        }
    }

    /// Lay the copy out as rows: a group header per prefix, its devices, and the symbols of the
    /// expanded ones.
    fn flatten(&mut self) {
        let mut rows = Vec::new();
        let mut last: Option<&str> = None;
        for (i, e) in self.entries.iter().enumerate() {
            if last != Some(e.group.as_str()) {
                let n = self.entries.iter().skip(i).take_while(|x| x.group == e.group).count();
                rows.push(Row::Group(e.group.clone(), n));
                last = Some(e.group.as_str());
            }
            rows.push(Row::Device(i));
            if self.expanded.contains(&e.id) {
                rows.extend((0..e.symbols.len()).map(|j| Row::Symbol(i, j)));
            }
        }
        self.rows = rows;
    }

    /// Expand or fold device `c`'s symbols.
    pub fn toggle(&mut self, c: ComponentId) {
        if !self.expanded.remove(&c) {
            self.expanded.insert(c);
        }
        self.flatten();
    }

    /// The tab's contents. Returns the actions asked for.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        let mut out = Vec::new();
        let t = Tokens::get();
        ui.add(
            egui::TextEdit::singleline(&mut self.filter).hint_text("Filter: tag or text").desired_width(f32::INFINITY),
        )
        .on_hover_text("A tag such as k1 finds -K01, -K10…; other text matches the description, maker or part number")
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, FILTER));
        let count = self.entries.len();
        ui.label(
            RichText::new(format!("{count} device{}", if count == 1 { "" } else { "s" })).small().color(t.text_dim),
        );
        // The selected device's properties, at the bottom of the tab.
        let editor_h = if self.selected.is_some() { 6.0 * 24.0 + 40.0 } else { 0.0 }
            + if self.error.is_some() { 60.0 } else { 0.0 };
        let list_h = (ui.available_height() - editor_h).max(ROW_H * 3.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut drawn = 0;
        let mut toggled = None;
        egui::ScrollArea::vertical()
            .id_salt("ecad.devices.list")
            .max_height(list_h)
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, self.rows.len(), |ui, range| {
                for i in range {
                    let Some(row) = self.rows.get(i).cloned() else { continue };
                    drawn += 1;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    let p = ui.painter_at(rect);
                    let font = egui::FontId::monospace(12.0);
                    match row {
                        Row::Group(g, n) => {
                            p.text(
                                pos2(rect.left() + 4.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                format!("{g}  ({n})"),
                                egui::FontId::proportional(11.0),
                                t.text_dim,
                            );
                        }
                        Row::Device(d) => {
                            let Some(e) = self.entries.get(d) else { continue };
                            let selected = self.selected == Some(e.id);
                            if selected {
                                p.rect_filled(rect, 0.0, t.tab_active);
                                p.rect_filled(
                                    egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                                    0.0,
                                    t.accent,
                                );
                            } else if resp.hovered() {
                                p.rect_filled(rect, 0.0, t.chrome);
                            }
                            let open = self.expanded.contains(&e.id);
                            let arrow = if e.symbols.is_empty() {
                                " "
                            } else if open {
                                "▾"
                            } else {
                                "▸"
                            };
                            let tag = e.tag.as_deref().unwrap_or("(no tag)");
                            let text = format!("{arrow} {tag:<7} {}", e.description);
                            let colour = if selected { t.text } else { t.text_dim };
                            p.text(
                                pos2(rect.left() + 8.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                text,
                                font,
                                colour,
                            );
                            let label = device_label(e);
                            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &label));
                            if resp.clicked() {
                                // A click on the arrow folds or unfolds; anywhere else selects the
                                // device and unfolds its symbols.
                                if resp.interact_pointer_pos().is_some_and(|q| q.x < rect.left() + 20.0) {
                                    toggled = Some(e.id);
                                } else {
                                    if self.selected != Some(e.id) {
                                        self.editing = None;
                                        self.error = None;
                                    }
                                    self.selected = Some(e.id);
                                    if !open {
                                        toggled = Some(e.id);
                                    }
                                }
                            }
                        }
                        Row::Symbol(d, s) => {
                            let Some(e) = self.entries.get(d) else { continue };
                            let Some(sym) = e.symbols.get(s) else { continue };
                            if resp.hovered() {
                                p.rect_filled(rect, 0.0, t.chrome);
                            }
                            let col = sym.column.map(|c| format!(".{c}")).unwrap_or_default();
                            let text = format!("    /{}{col}  {}", sym.page_number, sym.name);
                            p.text(
                                pos2(rect.left() + 8.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                text,
                                font,
                                t.text_dim,
                            );
                            let label = symbol_label(e, sym);
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                            let rep: RepresentationId = sym.rep;
                            if resp.on_hover_text("Show this symbol's page and select it").clicked() {
                                self.selected = Some(e.id);
                                out.push(Action::GotoSymbol(rep));
                            }
                        }
                    }
                }
            });
        self.rows_drawn = drawn;
        if let Some(c) = toggled {
            self.toggle(c);
        }
        if let Some(c) = self.selected {
            self.editor(ui, c, writable, &mut out);
        }
        out
    }

    /// The selected device's six properties, each a field; Enter or leaving a changed field runs
    /// `setprop`, Esc puts it back.
    fn editor(&mut self, ui: &mut egui::Ui, c: ComponentId, writable: bool, out: &mut Vec<Action>) {
        let Some(e) = self.entries.iter().find(|e| e.id == c).cloned() else {
            self.selected = None;
            return;
        };
        let t = Tokens::get();
        ui.add_space(4.0);
        ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, t.border));
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(
            RichText::new(format!("DEVICE {}", e.tag.as_deref().unwrap_or("(no tag)")))
                .small()
                .strong()
                .color(t.text_dim),
        );
        if let Some((msg, offer)) = self.error.clone() {
            ui.add(egui::Label::new(RichText::new(msg).small().color(ui.visuals().warn_fg_color)).wrap());
            if let Some((target, tag)) = offer
                && ui
                    .small_button(MERGE)
                    .on_hover_text("Make this device part of the device that has that tag (one undo step)")
                    .clicked()
            {
                self.error = None;
                out.push(Action::SetDeviceProperty { component: target, key: "tag", value: tag, merge: true });
            }
        }
        egui::Grid::new("ecad.devices.props").num_columns(2).spacing(vec2(6.0, 2.0)).show(ui, |ui| {
            for (i, (key, name)) in PROPERTIES.iter().enumerate() {
                let stored = e.properties.get(i).cloned().flatten().unwrap_or_default();
                ui.label(RichText::new(*name).small());
                let mine = self.editing.as_ref().filter(|(d, k, _)| *d == c && k == key);
                let mut text = mine.map_or_else(|| stored.clone(), |(_, _, x)| x.clone());
                let field = ui.add_enabled(
                    writable,
                    egui::TextEdit::singleline(&mut text)
                        .id(egui::Id::unique(("ecad.devices.prop", *key)))
                        .desired_width(f32::INFINITY),
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
                        out.push(Action::SetDeviceProperty { component: c, key, value: edited, merge: false });
                    }
                }
                ui.end_row();
            }
        });
    }

    /// The trial reports a refused property edit; a tag held by another device offers the merge.
    pub fn refused(&mut self, c: ComponentId, key: &str, value: &str, message: String) {
        let offer = (key == "tag" && message.contains("already used by")).then(|| (c, value.to_string()));
        // The command's way forward names its JSON parameter; here the button is the way.
        let message = match message.find(" Set `merge` to true") {
            Some(i) if offer.is_some() => message.get(..i).unwrap_or(&message).to_string(),
            _ => message,
        };
        self.error = Some((message, offer));
    }
}
