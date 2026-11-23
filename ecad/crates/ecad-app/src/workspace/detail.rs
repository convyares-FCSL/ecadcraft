//! The Project workspace's right panel ("Project workspace": "the right panel shows only
//! the selected item: its properties and its own checks";–2.06). It reads the
//! selected row of the tab shown, from the view's own copy (no project read), and lays it out as
//! follows: the item's name and where it is, a go-to, its checks, then folding sections of
//! label and value. Grey text is read from the drawing; a boxed value is set here. A Devices row
//! keeps the shared property editor (the same `setprop` the page runs). What is not built yet (the part
//! catalogue, the cable manager's ends, the report generator's settings) says so, greyed.
//!
//! **Update map.** Nothing of its own: it shows what the tab's view holds this frame.

use ecad_model::{DeviceId, PageId, Pt, RepresentationId};
use ecad_widgets::panel;
use ecad_widgets::prim::{self, Tone};
use ecad_widgets::tokens::{space, ui_type};
use eframe::egui::{self, Rect, vec2};

use crate::shell::ListsView;
use crate::shell::app::App;

/// The selected row of the tab shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selected {
    Device(DeviceId),
    ConnectionPoint(crate::terminal_strips::plan::TermKey),
    Card(usize),
    /// A channel row of Project ▸ PLC.
    Channel(crate::plc::list::IoKey),
    Wire(usize),
    Cable(usize),
    Report(usize),
}

/// What the panel says with nothing selected.
pub const NOTHING: &str = "Select a row to see it here.";
/// The greyed parts buttons' reason.
pub(crate) const CATALOGUE_COMES: &str = "The part catalogue comes later.";
pub(crate) const CABLE_MANAGER_COMES: &str = "Comes with the cable manager.";
/// The Rating section of a cable with nothing to rate yet.
pub(crate) const RATE_NOTE: &str = "Set a cable type to rate this cable.";
/// The Details pane's tabs and their accessible name.
pub const CABLE_TABS: &str = "ecad.project.detail.cable.tabs";
/// A cross-section's unit.
pub(crate) const MM2: &str = "mm\u{b2}";

fn first<K: Copy + Ord>(t: &ecad_widgets::table::Table<K>) -> Option<K> {
    t.selection.cursor().or_else(|| t.selection.keys().iter().next().copied())
}

/// The selected row of the tab shown, if any.
pub(crate) fn selected_key(t: &App) -> Option<Selected> {
    let r = t.reports.as_ref();
    match t.shell.lists {
        ListsView::Devices => t.panel.devices.selected.map(Selected::Device),
        ListsView::TerminalBlocks => r.and_then(|r| first(&r.connection_points.table)).map(Selected::ConnectionPoint),
        ListsView::Plc => r.and_then(|r| {
            // A channel row selected shows the channel; else the selected tile's card.
            let row = first(&r.io.table).filter(|k| r.io.source.row(*k).is_some());
            row.map(Selected::Channel).or_else(|| r.io.selected.map(Selected::Card))
        }),
        ListsView::Wires => r.and_then(|r| r.wires.selected()).map(Selected::Wire),
        ListsView::Cables => r.and_then(|r| r.cables.selected()).map(Selected::Cable),
        ListsView::Reports => r.and_then(|r| first(&r.list.table)).map(Selected::Report),
        ListsView::Checks => None,
    }
}

/// Where the head's go-to button goes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Go {
    Symbols(Vec<RepresentationId>),
    Place(PageId, Pt),
}

/// One labelled value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Prop {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    /// Where it comes from (`from page`, `auto`), at the right.
    pub(crate) hint: String,
    /// Read from the drawing (grey, no box) or set here (boxed).
    pub(crate) set: bool,
    /// Not built yet: greyed with this reason.
    pub(crate) later: Option<&'static str>,
    /// Its unit (`mm²`), whether it is the key identity value (bold), its colour swatch, and
    /// whether it is one of a list (a chevron).
    pub(crate) unit: &'static str,
    pub(crate) bold: bool,
    pub(crate) swatch: Option<(ecad_model::palette::Color, Option<ecad_model::palette::Color>)>,
    pub(crate) choice: bool,
}

impl Prop {
    pub(crate) fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }
    pub(crate) fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub(crate) fn choice(mut self) -> Self {
        self.choice = true;
        self
    }
    pub(crate) fn swatch(mut self, code: &str) -> Self {
        self.swatch = swatch(code);
        self
    }
}

/// A wire colour code's swatch in its insulation colours (the paper sheet's, on either shell:
/// black insulation stays black); none for a code that is not a colour.
pub(crate) fn swatch(code: &str) -> Option<(ecad_model::palette::Color, Option<ecad_model::palette::Color>)> {
    let sheet = &ecad_model::palette::sheet::PAPER;
    ecad_model::objects::wire::Colours::parse(code).and_then(|c| ecad_electrical::cables::words::wire_colours(&c, sheet)).map(|c| (c.solid, c.dashes))
}

pub(crate) fn read(label: &'static str, value: impl Into<String>) -> Prop {
    Prop { label, value: value.into(), hint: String::new(), set: false, later: None, unit: "", bold: false, swatch: None, choice: false }
}

pub(crate) fn set(label: &'static str, value: impl Into<String>, hint: impl Into<String>) -> Prop {
    Prop { label, value: value.into(), hint: hint.into(), set: true, later: None, unit: "", bold: false, swatch: None, choice: false }
}

pub(crate) fn later(label: &'static str, why: &'static str) -> Prop {
    Prop {
        label,
        value: crate::reports::report::DASH.into(),
        hint: String::new(),
        set: false,
        later: Some(why),
        unit: "",
        bold: false,
        swatch: None,
        choice: false,
    }
}

/// A folding section.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Section {
    pub(crate) title: &'static str,
    pub(crate) hint: String,
    pub(crate) props: Vec<Prop>,
    /// Plain lines (a part set's lines, the report's columns).
    pub(crate) lines: Vec<(String, String)>,
    /// Greyed buttons with their reason.
    pub(crate) later: Vec<(&'static str, &'static str)>,
    pub(crate) note: Option<&'static str>,
    /// The Rating section's result (drawn by [`crate::wires::rating`]).
    pub(crate) rating: Option<crate::wires::rating::Panel>,
}

/// The panel's contents.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Detail {
    pub(crate) title: String,
    pub(crate) provisional: bool,
    pub(crate) subtitle: String,
    pub(crate) go: Option<(String, Go)>,
    /// Its checks (an empty list: "No checks on …").
    pub(crate) checks: Vec<(String, Tone)>,
    /// The open cores' fixes, as buttons under the checks.
    pub(crate) core_fixes: Vec<ecad_electrical::checks::Fix>,
    /// The Devices row's property editor goes after the head.
    pub(crate) editor: bool,
    /// The cable editor goes after the head, with the cable's drawn ends.
    pub(crate) cable_editor: Option<(ecad_model::CableId, [String; 2])>,
    pub(crate) sections: Vec<Section>,
}

pub(crate) fn nonempty(s: &str) -> String {
    if s.trim().is_empty() || s == "?" { crate::reports::report::DASH.into() } else { s.to_string() }
}

fn detail(t: &App) -> Option<Detail> {
    match selected_key(t)? {
        Selected::Device(c) => crate::devices::detail::device(t, c),
        Selected::ConnectionPoint(k) => crate::terminal_strips::detail::connection_point(t, k),
        // A card is edited by `card_editor` (see `show`).
        Selected::Card(_) => None,
        Selected::Channel(k) => crate::plc::detail::channel(t, k),
        Selected::Wire(k) => crate::wires::detail::wire(t, k),
        Selected::Cable(k) => crate::cables::detail::cable(t, k),
        Selected::Report(k) => crate::reports::detail::report(t, k),
    }
}

/// One value row: the label, the value (boxed when set here), its hint at the right.
fn prop_row(ui: &mut egui::Ui, p: &Prop) {
    let kind = match (p.later.is_some(), p.set) {
        (true, _) => panel::ValueKind::Later,
        (false, true) => panel::ValueKind::Set,
        (false, false) => panel::ValueKind::Read,
    };
    let decor = ecad_widgets::control::Decor { hint: &p.hint, unit: p.unit, bold: p.bold, swatch: p.swatch };
    let resp = panel::value_row(ui, p.label, &p.value, decor, kind, p.choice);
    if let Some(why) = p.later {
        resp.on_hover_text(why);
    }
}

/// A section's head: its title and fold mark, its hint at the right. Returns whether it is open.
fn section_head(ui: &mut egui::Ui, id: egui::Id, title: &str, hint: &str) -> bool {
    let open_id = id.with("open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
    let resp = panel::section_header(ui, open, title, hint);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, title));
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(open_id, open));
    }
    if open {
        ui.add_space(panel::BODY_GAP);
    }
    open
}

/// One folding section (its head, then its rows); returns what it asked for.
fn draw_section(ui: &mut egui::Ui, sec: &Section, writable: bool) -> Vec<Out> {
    let s = ecad_widgets::SHELL;
    let mut out = Vec::new();
    let id = egui::Id::unique(("ecad.project.detail", sec.title));
    if section_head(ui, id, sec.title, &sec.hint) {
        if let Some(rp) = &sec.rating {
            let rp = crate::wires::rating::Panel { writable, ..rp.clone() };
            out.extend(crate::wires::rating::show(ui, &rp).into_iter().map(|(c, v)| Out::Run(c, v)));
        }
        for p in &sec.props {
            prop_row(ui, p);
        }
        for (a, b) in &sec.lines {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(a).size(ui_type::BODY.size).color(ecad_widgets::color(s.text)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(b).size(ui_type::SMALL.size).color(ecad_widgets::color(s.text_2)));
                });
            });
        }
        if !sec.later.is_empty() {
            ui.horizontal(|ui| {
                for (label, why) in &sec.later {
                    ui.add(prim::Button::new(label).small().disabled(why));
                }
            });
        }
        if let Some(n) = sec.note {
            ui.add(egui::Label::new(egui::RichText::new(n).size(ui_type::SMALL.size).color(ecad_widgets::color(s.text_3))).wrap());
        }
    }
    out
}

/// What the panel asks the app for.
#[derive(Debug, Clone, PartialEq)]
enum Out {
    Go(Go),
    Panel(crate::pages::project_panel::Action),
    /// A `rating` call from the Rating section.
    Run(&'static str, serde_json::Value),
    /// The cable editor's ask, on its cable.
    Cable(ecad_model::CableId, crate::cables::editor::Ask),
}

/// Draw the right panel for the Project workspace in `rect`.
pub fn show(t: &mut App, ui: &mut egui::Ui, rect: Rect) {
    // A card selected in Project ▸ PLC is edited, as.
    if let Some(Selected::Card(i)) = selected_key(t) {
        crate::plc::card_editor::show(t, ui, rect, i);
        return;
    }
    let s = ecad_widgets::SHELL;
    let Some(d) = detail(t) else {
        crate::shell::in_rect(ui, rect.shrink(space::L), "ecad.project.detail.none", |ui| {
            ui.label(egui::RichText::new(NOTHING).size(ui_type::BODY.size).color(ecad_widgets::color(s.text_3)));
        });
        return;
    };
    // Not while a revision is open: its edits would write to the live project behind it.
    let writable = t.project_writable();
    let mut out = Vec::new();
    let devices = &mut t.panel.devices;
    // The cable's Cable / Checks tabs, and its Checks rows.
    let (mut cable_tab, cable_checks) = t.reports.as_ref().map_or((0, Vec::new()), |r| (r.cables.detail_tab, r.cables.cable_checks.clone()));
    let tab_before = cable_tab;
    let mut cable_edit = t.reports.as_mut().map(|r| &mut r.cables.edit);
    let full = rect.x_range();
    let rule = |ui: &mut egui::Ui| panel::rule(ui, full);
    crate::shell::in_rect(ui, rect.shrink2(vec2(space::L, space::M)), "ecad.project.detail", |ui| {
        egui::ScrollArea::vertical().id_salt("ecad.project.detail.scroll").auto_shrink([false, false]).show(ui, |ui| {
            // The head: the name, where it is, the go-to.
            ui.horizontal(|ui| {
                let title_ink = if d.provisional { s.provisional } else { s.text };
                ui.label(
                    egui::RichText::new(&d.title).font(ecad_widgets::fonts::font(ui.ctx(), ui_type::OBJECT)).color(ecad_widgets::color(title_ink)),
                );
                if let Some((words, go)) = &d.go {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(prim::Button::new(&format!("\u{2192} {words}")).small()).clicked() {
                            out.push(Out::Go(go.clone()));
                        }
                    });
                }
            });
            ui.label(egui::RichText::new(&d.subtitle).size(ui_type::SMALL.size).color(ecad_widgets::color(s.text_2)));
            ui.add_space(space::S);
            // Cable | Checks n.
            if d.cable_editor.is_some() {
                let checks_label = if cable_checks.is_empty() { "Checks".to_string() } else { format!("Checks \u{25cf} {}", cable_checks.len()) };
                let labels = ["Cable", checks_label.as_str()];
                prim::Segmented::new(&mut cable_tab, &labels).name(CABLE_TABS).show(ui);
                ui.add_space(space::S);
                if cable_tab == 1 {
                    if cable_checks.is_empty() {
                        let words = format!("No checks on {}", d.title);
                        panel::status_pill(ui, Tone::Ok, &words, &format!("\u{2713} {words}"));
                    }
                    for (c, place) in &cable_checks {
                        let tone = match c.severity() {
                            ecad_electrical::checks::Severity::Error => Tone::Error,
                            ecad_electrical::checks::Severity::Warning => Tone::Warning,
                            ecad_electrical::checks::Severity::Info => Tone::Neutral,
                        };
                        let words = c.what();
                        panel::status_pill(ui, tone, &words, &words);
                        if c.page.is_some()
                            && ecad_widgets::control::link(ui, &format!("\u{2192} {}", if place.is_empty() { "Go to" } else { place })).clicked()
                        {
                            out.push(Out::Panel(crate::pages::project_panel::Action::GotoCheck(Box::new(c.clone()))));
                        }
                    }
                    return;
                }
            }
            // Its own checks.
            if d.checks.is_empty() {
                let words = format!("No checks on {}", d.title);
                panel::status_pill(ui, Tone::Ok, &words, &format!("\u{2713} {words}"));
            } else {
                for (c, tone) in &d.checks {
                    panel::status_pill(ui, *tone, c, c);
                }
            }
            // Go to the open end, for each open core (the core table's Not
            // used toggle is right there, so no tick button here).
            for f in &d.core_fixes {
                if ui.add(prim::Button::new(crate::checks::panel::GO_TO_OPEN_END).small()).clicked() {
                    out.push(Out::Panel(crate::pages::project_panel::Action::CoreFix(*f, 1)));
                }
            }
            ui.add_space(space::S);
            rule(ui);
            // The cable editor, with the Rating section between Part
            // and Ends.
            if let (Some((cable, ends)), Some(ed)) = (&d.cable_editor, cable_edit.take()) {
                let mut mid = Vec::new();
                let mut middle = |ui: &mut egui::Ui| {
                    for sec in d.sections.iter().filter(|s| !s.title.is_empty()) {
                        mid.extend(draw_section(ui, sec, writable));
                        panel::rule(ui, full);
                    }
                };
                let asks = crate::cables::editor::show(ui, ed, ends.clone(), writable, &|ui: &mut egui::Ui| panel::rule(ui, full), &mut middle);
                out.extend(mid);
                out.extend(asks.into_iter().map(|a| Out::Cable(*cable, a)));
                rule(ui);
            }
            if d.editor {
                let id = egui::Id::unique("ecad.project.detail.properties");
                if section_head(ui, id, "Identity", "set here") {
                    out.extend(devices.show_editor(ui, writable).into_iter().map(Out::Panel));
                }
                rule(ui);
            }
            // A cable's sections (the Rating section) were drawn inside the editor, after Part.
            if d.cable_editor.is_none() {
                for sec in d.sections.iter().filter(|s| !s.title.is_empty()) {
                    out.extend(draw_section(ui, sec, writable));
                    rule(ui);
                }
            }
        });
    });
    if cable_tab != tab_before
        && let Some(r) = t.reports.as_mut()
    {
        r.cables.detail_tab = cable_tab;
    }
    for o in out {
        match o {
            Out::Go(Go::Symbols(reps)) => {
                let _ = t.goto_symbols(&reps);
            }
            Out::Go(Go::Place(pg, at)) => t.goto_place(pg, at),
            Out::Panel(a) => t.panel_action(a),
            Out::Run(cmd, params) => {
                let _ = t.run_value(cmd, params);
            }
            Out::Cable(k, a) => t.cable_ask(k, a),
        }
    }
}

/// The Rating section, after Made as or Part; its folded line is the
/// result. A wire or cable with no rating data shows none, so a project that does not rate is
/// not nagged.
pub(crate) fn rating_section(_t: &App, r: Option<&ecad_electrical::circuits::circuit::Rating>, methods: &[&'static str]) -> Section {
    let Some(r) = r else { return Section::default() };
    let methods = if methods.is_empty() { ecad_electrical::devices::ratings::dataset::METHODS.to_vec() } else { methods.to_vec() };
    Section {
        title: "Rating",
        hint: r.summary(),
        rating: Some(crate::wires::rating::Panel { rating: r.clone(), methods, writable: false }),
        ..Section::default()
    }
}
