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

use ecad_cadcraft::device_list::DeviceEntry;
use ecad_cadcraft::devices::PROPERTIES;
use ecad_model::ComponentId;
use eframe::egui::{self, RichText, Stroke, vec2};

use crate::lists::devices::{self as dl, DevicesSource};
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

    /// The tab's contents: Renumber, the table, the selected device's properties. Returns the
    /// actions asked for.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        use ecad_ui::table::TableEvent;
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
        self.table.query = self.filter.clone();
        let events = ui
            .allocate_ui(vec2(ui.available_width(), list_h), |ui| {
                ui.set_min_height(list_h);
                self.table.show(ui, &self.source)
            })
            .inner;
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
        if let Some(c) = self.selected {
            self.editor(ui, c, writable, &mut out);
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
