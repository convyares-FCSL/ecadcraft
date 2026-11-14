//! The right panel (spec §2): details only, the inspector. UI rev 3 (boards 1.05 to 1.07): the
//! right side mirrors the left. A permanent 56 px rail at the right edge has one labelled button
//! per tab, Device, Layout and Checks, each with a ‹ mark (› while open); Checks carries a dot
//! when there are problems. Pressing one opens the panel beside the rail on that tab, and pressing
//! it again closes it (the strip's panel buttons' rule); `]` or the header's collapse button
//! closes it too. The rail's buttons are the tabs: the panel has no tab selector.
//!
//! The panel's header names what is selected: the device's tag, its description, which symbol
//! and where it is, a Row button (its row in Project ▸ Devices) and collapse. Below it:
//! - **Device** ([`device`]): a one-line status of the device's own checks, then Identity, Parts,
//!   Where it is drawn, Connections, and Notes and attributes. With nothing selected, the page's
//!   properties; a wire, a cable… its properties.
//! - **Layout** ([`layout_tab`]): Placement, Text on the sheet, the contact table, Line and colour;
//!   for anything else the defaults that apply to it (brief 042 step 4): each value from the
//!   registry with where it comes from; a badge opens Settings ▸ Standards at that key. A value set
//!   at this object's level is a field (a list for a choice), with Reset when it is overridden
//!   here; a value read from further out is grey text that goes to where it is set.
//! - **Checks** ([`checks_tab`]): This page or Project, each check with its fix and a go-to.
//!
//! While Place Symbol runs (board 1.11), Device and Layout describe the device about to be placed
//! instead ([`placing_view`]): its next tag, its symbol and what the click changes.
//!
//! Every titled section folds to a one-line summary ([`widgets::section`]). Grey text is read from
//! the drawing; an input box is something you set here. The rows come from the ECAD properties
//! copy ([`crate::sheets_ui::PropsShared`], design 019; edits go through its commands as before)
//! and from this inspector's own copy. The Text panel opens over it ([`super::text_panel`]).
//!
//! **Update map** (the inspector's rows): rebuilt on a new selection or page, or after a change
//! that touched the selected object, its pages, its nets, the page or the project
//! (`props_changes_since`); every other commit leaves them.

mod checks_tab;
mod details;
mod device;
mod fields;
mod layout_tab;
mod placing_view;
mod widgets;

pub use checks_tab::{ELSEWHERE, GO_TO, SCOPE};
pub use details::{ConnRow, DeviceDetails, DrawnRow};
pub use device::{CONNECTION_ROW, DRAWN_ROW, OWN_CHECKS};
pub use layout_tab::{CONTACT_TABLE, MIRROR, ROTATION};
pub use placing_view::{CHANGES, NOT_PLACED, Placing};
pub use widgets::section_name;

use std::collections::{BTreeMap, BTreeSet};

use ecad_cadcraft::default_cmds::{self, Target};
use ecad_cadcraft::defaults::{self, Entry, Kind, Level, Scope, Source};
use ecad_cadcraft::project::Mapped;
use ecad_cadcraft::text_style::TextKind;
use ecad_model::{ObjectRef, PageId, RepresentationId, Subject};
use ecad_ui::tokens::{Icon, size, space, ui_type};
use eframe::egui::{self, Rect, RichText, pos2, vec2};

use super::Slots;
use super::state::RightTab;
use crate::Trial;

/// Accessible labels.
pub const COLLAPSE: &str = "Collapse the details panel";
pub const EXPAND: &str = "Details";
pub const TITLE: &str = "PROPERTIES";
/// The right panel's title in the Library (brief 048).
pub const LIBRARY_TITLE: &str = "DETAILS";
/// The header's Row button: the device's row in Project ▸ Devices.
pub const ROW: &str = "Row";
/// The header's words (accessible names).
pub const HEAD_TITLE: &str = "ecad.inspector.head.title";
pub const HEAD_SUB: &str = "ecad.inspector.head.sub";
/// The header's height (tag, two lines of words).
const HEAD_H: f32 = 64.0;

/// The rail button's accessible label for `tab`.
pub fn rail_label(tab: RightTab) -> String {
    format!("{} tab", tab.label())
}

impl Trial {
    /// Expand the inspector on `tab`.
    pub fn open_inspector(&mut self, tab: RightTab) {
        self.shell.state.right_tab = tab;
        self.shell.state.right_collapsed = false;
    }

    /// A rail button: open the inspector on `tab`, or close it when it is already open there (the
    /// strip's panel buttons' rule, mirrored).
    pub fn toggle_inspector(&mut self, tab: RightTab) {
        if !self.shell.state.right_collapsed && self.shell.state.right_tab == tab {
            self.shell.state.right_collapsed = true;
        } else {
            self.open_inspector(tab);
        }
    }

    /// The header's Row button: Project ▸ Devices with the device's row selected.
    pub fn show_device_row(&mut self, c: ecad_model::ComponentId) {
        self.panel.devices.selected = Some(c);
        self.shell.lists = super::ListsView::Devices;
        self.set_workspace(super::Workspace::Lists);
    }
}

/// What a tab body draws from (split borrows of the trial).
pub struct Cx<'a> {
    pub sh: &'a mut crate::sheets_ui::PropsShared,
    pub insp: &'a mut Inspector,
    pub checks: &'a [crate::checks_panel::Row],
    pub writable: bool,
    /// A check's fix may run (writable, and no old revision shown).
    pub fixes: bool,
    /// The panel's full width (for the rules between sections).
    pub full: egui::Rangef,
    /// What a check card asked for (the Checks tab's actions).
    pub panel_out: Vec<crate::project_panel::Action>,
}

/// The header's words for what is selected.
fn head_words(insp: &Inspector, sh: &crate::sheets_ui::PropsShared) -> (String, String, Option<ecad_model::ComponentId>) {
    if let Some(d) = &insp.device {
        let sub: Vec<&str> = [d.description.as_str(), d.symbol_words.as_str(), d.place.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
        return (d.tag.clone(), sub.join(" \u{b7} "), d.component);
    }
    if let Some(p) = &insp.page_words {
        return (format!("Page {}", p.0), p.1.clone(), None);
    }
    match sh.view.sections.first() {
        Some(s) => (s.title.trim_start_matches("ECAD \u{b7} ").to_string(), String::new(), None),
        None => (TITLE.to_string(), String::new(), None),
    }
}

/// Draw the right panel (or its rail, or the Text panel over it).
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    let s = ecad_ui::SHELL;
    // The rail, always shown at the right edge (the strip's mirror).
    super::fill(ui, slots.rail, s.panel);
    ui.painter().vline(slots.rail.left() + 0.5, slots.rail.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    rail(t, ui, slots.rail, !slots.right_rail);
    if slots.right_rail {
        return;
    }
    let rect = slots.right;
    super::fill(ui, rect, s.panel);
    ui.painter().vline(rect.left() + 0.5, rect.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    if t.shell.state.text_panel {
        super::text_panel::show(t, ui, rect);
        return;
    }
    // Brief 048: the Library's right panel holds a symbol's details (design 048 §3.1, §8.1).
    if t.shell.state.workspace == super::Workspace::Library {
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), size::PANEL_HEADER));
        let collapse = super::in_rect(ui, head.shrink2(vec2(space::L, space::XS)), "ecad.shell.right.head", |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new(LIBRARY_TITLE).font(ecad_ui::fonts::font(ui.ctx(), ui_type::TITLE)).color(ecad_ui::color(s.text)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(ecad_ui::prim::IconButton::new(Icon::Collapse, COLLAPSE)).on_hover_text("Collapse (])").clicked()
                })
                .inner
            })
            .inner
        });
        if collapse {
            t.shell.state.right_collapsed = true;
        }
        let body = Rect::from_min_max(pos2(rect.left() + space::L, head.bottom()), pos2(rect.right() - space::M, rect.bottom()));
        super::in_rect(ui, body, crate::library::details::DETAILS, |ui| {
            egui::ScrollArea::vertical().id_salt("library.details.scroll").auto_shrink([false, false]).show(ui, |ui| t.library_right(ui));
        });
        return;
    }
    // UI rev 3, Project workspace (boards 2.01–2.06): the selected row with its own head, its
    // properties and its own checks (`lists::detail`); the Checks tab stays the check list.
    if t.shell.state.workspace == super::Workspace::Lists && t.shell.state.right_tab != RightTab::Checks {
        // The inspector keeps following the page's selection meanwhile.
        if t.project_open() {
            sync(t);
        }
        crate::lists::detail::show(t, ui, rect);
        return;
    }
    if t.project_open() {
        sync(t);
    }
    let writable = t.project_open() && !t.header().read_only;
    // A check's fix writes the live project: never while an old revision is shown read-only
    // (the guard the drawer's Checks list had, as `shell/mod.rs` and the wheel keep it).
    let fixes = {
        let h = t.header();
        h.name.is_some() && !h.read_only && t.panel.revision.is_none()
    };
    // Board 1.11: the device about to be placed, on Device and Layout.
    let placing = if t.shell.state.right_tab == RightTab::Checks { None } else { placing_view::of(t) };
    let standard = match placing {
        Some(_) => {
            let words = t.standard_words();
            format!("{} built-in", words.trim_start_matches("Standard: ").trim_end_matches(" (built in)"))
        }
        None => String::new(),
    };
    let props = t.props.clone();
    let Ok(mut sh) = props.lock() else { return };
    sh.frames += 1;
    // The header: what is selected, its Row button and collapse.
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), HEAD_H));
    let (title, sub, device) = match &placing {
        Some(p) => {
            let (title, sub) = p.head();
            (title, sub, None)
        }
        None => head_words(&t.settings.inspector, &sh),
    };
    let accent = placing.is_some();
    let (collapse, row) =
        super::in_rect(ui, head.shrink2(vec2(space::L, space::S)), "ecad.shell.right.head", |ui| header(ui, &title, &sub, device.is_some(), accent));
    ui.painter().hline(rect.x_range(), head.bottom() - 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let body = Rect::from_min_max(pos2(rect.left() + space::L, head.bottom()), pos2(rect.right() - space::L, rect.bottom()));
    let full = rect.x_range();
    let mut acts = Vec::new();
    let mut panel_acts = Vec::new();
    match t.shell.state.right_tab {
        RightTab::Checks => {
            if t.project_open() && t.lists_shows_checks() {
                // Project ▸ Check list shows the same rows: one place at a time.
                super::in_rect(ui, body, "ecad.shell.right.checks", |ui| ecad_ui::prim::read_value(ui, super::CHECKS_IN_LISTS));
            } else if t.project_open() {
                let page = t.panel.active;
                let insp = &mut t.settings.inspector;
                let panel = &mut t.panel.checks;
                panel_acts = super::in_rect(ui, body, "ecad.shell.right.checks", |ui| {
                    checks_tab::show(ui, panel, &mut insp.checks_project, page, fixes, body)
                });
            }
        }
        tab => {
            let mut cx =
                Cx { sh: &mut sh, insp: &mut t.settings.inspector, checks: t.panel.checks.rows(), writable, fixes, full, panel_out: Vec::new() };
            super::in_rect(ui, body, "ecad.shell.right.body", |ui| {
                egui::ScrollArea::vertical().id_salt(("ecad.shell.right.scroll", tab.label())).auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(p) = &placing {
                        placing_view::show(ui, &mut cx, p, &standard);
                    } else if tab == RightTab::Layout {
                        layout_tab::show(ui, &mut cx, &mut acts);
                    } else {
                        device::show(ui, &mut cx, &mut acts);
                    }
                    ui.add_space(space::XL);
                });
            });
            panel_acts = std::mem::take(&mut cx.panel_out);
        }
    }
    drop(sh);
    if collapse {
        t.shell.state.right_collapsed = true;
    }
    if row && let Some(c) = device {
        t.show_device_row(c);
    }
    for a in acts {
        t.inspector_action(a);
    }
    for a in panel_acts {
        t.panel_action(a);
    }
}

/// The header: the tag (or what is selected) and its words; the Row button and collapse at the
/// right; `accent` draws the title in the accent colour (a device not placed yet). Returns
/// (collapse, row) pressed.
fn header(ui: &mut egui::Ui, title: &str, sub: &str, row: bool, accent: bool) -> (bool, bool) {
    let mut out = (false, false);
    ui.horizontal_top(|ui| {
        let right_w = if row { 96.0 } else { 36.0 };
        let text_w = (ui.available_width() - right_w).max(60.0);
        ui.vertical(|ui| {
            ui.set_width(text_w);
            let ink = if accent { ecad_ui::color(ecad_ui::SHELL.accent_text) } else { widgets::text() };
            ui.add(egui::Label::new(widgets::rich(ui, title, ui_type::TITLE, ink)).truncate())
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, HEAD_TITLE));
            if !sub.is_empty() {
                ui.add(egui::Label::new(widgets::rich(ui, sub, ui_type::SMALL, widgets::text_2())).wrap())
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, HEAD_SUB));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            out.0 = ui.add(ecad_ui::prim::IconButton::new(Icon::Collapse, COLLAPSE)).on_hover_text("Collapse (])").clicked();
            if row {
                let b = ui.add(ecad_ui::prim::Button::new(ROW).small()).on_hover_text("Open its row in Project \u{25b8} Devices");
                out.1 = b.clicked();
            }
        });
    });
    out
}

/// The collapsed rail: one labelled button per tab, each with a ‹ mark; Checks carries a dot
/// when there are problems. A press expands the panel on that tab.
fn rail(t: &mut Trial, ui: &mut egui::Ui, rect: Rect, open: bool) {
    let s = ecad_ui::SHELL;
    let problems = t.checks_count().0 > 0;
    let x = rect.center().x - size::TOOL_BUTTON_W / 2.0;
    let mut y = rect.top() + space::S;
    let mut pick = None;
    let label_style = ecad_ui::tokens::ui_type::TypeStyle { size: size::TOOL_LABEL, line: 12.0, weight: 400 };
    for tab in RightTab::ALL {
        let r = Rect::from_min_size(pos2(x, y), vec2(size::TOOL_BUTTON_W, size::TOOL_BUTTON));
        y += size::TOOL_BUTTON + 2.0;
        // Lit while the inspector is open on it, as the strip's open panel button.
        let on = open && tab == t.shell.state.right_tab;
        let resp = super::bar::hit(ui, r, &rail_label(tab), on, false, true);
        let ink = if resp.hovered() || on { ecad_ui::color(s.text) } else { ecad_ui::color(s.text_2) };
        let icon = match tab {
            RightTab::Device => Icon::Place,
            RightTab::Layout => Icon::Rotate,
            RightTab::Checks => Icon::Checks,
        };
        let icon_rect = Rect::from_center_size(pos2(r.center().x, r.top() + 6.0 + size::TOOL_ICON / 2.0), vec2(size::TOOL_ICON, size::TOOL_ICON));
        ecad_ui::icons::paint(ui.painter(), icon_rect, icon, if on { ecad_ui::color(s.accent_text) } else { ink });
        ui.painter().text(pos2(r.center().x, r.bottom() - 9.0), egui::Align2::CENTER_CENTER, tab.label(), super::bar::font(ui, label_style), ink);
        let mark = Rect::from_center_size(pos2(r.left() + 4.0, icon_rect.center().y), vec2(8.0, 8.0));
        // ‹ opens towards the canvas; › while open (the strip's marks, mirrored).
        let chevron = if on { Icon::ChevronRight } else { Icon::ChevronLeft };
        ecad_ui::icons::paint(ui.painter(), mark, chevron, ecad_ui::color(s.text_3));
        if tab == RightTab::Checks && problems {
            ui.painter().circle_filled(pos2(icon_rect.right() + 2.0, icon_rect.top() + 1.0), 3.5, ecad_ui::color(s.error));
        }
        if resp.clicked() {
            pick = Some(tab);
        }
    }
    if let Some(tab) = pick {
        t.toggle_inspector(tab);
    }
}

/// Accessible names of the inspector's rows: `ecad.inspector.<key>` (the value),
/// `ecad.origin.<key>` (its badge), `ecad.inspector.reset.<key>`.
pub const DEFAULTS_TITLE: &str = "ecad.inspector";
pub const PLACEMENTS: &str = "ecad.inspector.placements";
/// Brief 047: the part set section; its rows are `ecad.inspector.parts.row.<n>` (0 the main part).
pub const PARTS: &str = "ecad.inspector.parts";

/// What the inspector shows defaults for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum What {
    Page(PageId),
    Symbol(ecad_model::ComponentId, RepresentationId),
    Conductor(ecad_model::ConductorId),
}

/// One default as the inspector shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub key: &'static str,
    pub label: String,
    pub value: String,
    pub from: Source,
    /// The level and object this row sets, when the value is set here.
    pub here: Option<(Level, Target)>,
    /// The words a choice entry takes.
    pub choices: &'static [&'static str],
    /// Brief 050a: a style choice's styles, (ID, name), for its value list; empty otherwise.
    pub styles: Vec<(String, String)>,
    /// Brief 050a (design 050 §8): the object's own choice names no style ("Not found: drawn as
    /// Tag").
    pub dangling: Option<String>,
}

impl Row {
    /// Overridden at this object's level: Reset is offered.
    pub fn overridden_here(&self) -> bool {
        match self.here {
            Some((Level::Page, _)) => self.from == Source::Page,
            Some((Level::Object, Target::Symbol(_))) => self.from == Source::Symbol,
            Some((Level::Object, _)) => self.from == Source::Object,
            _ => false,
        }
    }
}

/// Brief 044 (design 044 §9.4): one row of a connector's Halves group, or the selected pin's Pin
/// and Mate rows. Accessible name `ecad.props.connector.<key>`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectorRow {
    /// `pins`, `half.s.description`…, `pin`, `mate`.
    pub key: String,
    /// A sub-heading (a half's tag) shown before this row, if any.
    pub heading: Option<String>,
    pub label: &'static str,
    pub value: String,
    /// 042's origin words: `set here`, `from standard`, `from project`, `from box`…
    pub origin: &'static str,
    /// The command and its parameters that set this row (the value goes in `value_key`); `None`
    /// for a read row.
    pub set: Option<(&'static str, serde_json::Value, &'static str)>,
    /// A read row that goes to these symbols when clicked (the mate).
    pub goto: Vec<RepresentationId>,
}

/// The connector rows for symbol `rep` of `c` (design 044 §9.4): Pins, each drawn half's
/// Description, Maker, Part number and Location, and for a pin symbol its Pin and Mate.
fn connector_rows(doc: &ecad_model::Document, c: ecad_model::ComponentId, rep: RepresentationId) -> Vec<ConnectorRow> {
    use ecad_cadcraft::connectors as cn;
    use ecad_cadcraft::symbols::Side;
    if !cn::is_connector(doc, c) {
        return Vec::new();
    }
    let comp = json_id(c);
    let d = cn::drawn(doc, c);
    let mut out = vec![ConnectorRow {
        key: "pins".into(),
        heading: None,
        label: "Pins",
        value: cn::pin_count(doc, c, &d).to_string(),
        origin: if cn::pins_prop(doc, c).is_some() { "set here" } else { "from symbols" },
        set: Some(("setprop", serde_json::json!({"component": comp, "key": cn::PINS_KEY}), "value")),
        goto: Vec::new(),
    }];
    let mut cache = ecad_cadcraft::locations::BoxCache::default();
    for h in cn::half_rows(doc, c, &mut cache) {
        let Some(letter) = h.side.letter() else { continue };
        let own = |f: &str| cn::half_prop(doc, c, h.side, f).is_some();
        let row = |field: &'static str, label: &'static str, value: Option<String>, origin: &'static str, heading: Option<String>| ConnectorRow {
            key: format!("half.{letter}.{field}"),
            heading,
            label,
            value: value.unwrap_or_else(|| "?".into()),
            origin,
            set: Some(("setprop", serde_json::json!({"component": comp, "key": format!("half.{letter}.{field}")}), "value")),
            goto: Vec::new(),
        };
        let desc_origin = if own("description") {
            "set here"
        } else if doc.properties.keys().any(|k| k.starts_with("connector.") && k.ends_with(".description")) {
            "from project"
        } else {
            "from standard"
        };
        let loc_origin = match h.location.source {
            _ if own("location") => "set here",
            ecad_cadcraft::locations::Source::Device => "from device",
            ecad_cadcraft::locations::Source::Box { .. } => "from box",
            ecad_cadcraft::locations::Source::Page(_) => "from page",
            ecad_cadcraft::locations::Source::Project => "from project",
            ecad_cadcraft::locations::Source::None => "none",
        };
        out.push(row("description", "Description", h.description.clone(), desc_origin, Some(h.tag.clone())));
        out.push(row("maker", "Maker", h.maker.clone(), if own("maker") { "set here" } else { "none" }, None));
        out.push(row("part_number", "Part number", h.part_number.clone(), if own("part_number") { "set here" } else { "none" }, None));
        out.push(row("location", "Location", h.location.value.clone(), loc_origin, None));
    }
    // A pin symbol: its pin, and where its mate is.
    let Some(r) = doc.representations.get(&rep) else { return out };
    if cn::rep_half(doc, r).is_some_and(cn::is_pin_starter) {
        let t = r.connection_points.keys().next().copied();
        let name = t.and_then(|t| doc.terminals.get(&t)).and_then(|x| x.label.clone()).unwrap_or_default();
        out.push(ConnectorRow {
            key: "pin".into(),
            heading: None,
            label: "Pin",
            value: name.clone(),
            origin: "set here",
            set: Some(("pinlabel", serde_json::json!({"device": comp, "pin": name}), "to")),
            goto: Vec::new(),
        });
        let side = t.and_then(|t| cn::side_of(doc, t));
        if let Some(side @ (Side::Socket | Side::Plug)) = side {
            let mate = doc.terminals_of(c).find(|(_, x)| x.label.as_deref() == Some(name.as_str()) && x.key.as_deref() == side.other().key());
            let (value, goto) = match mate {
                Some((m, _)) => {
                    let reps: Vec<RepresentationId> = doc.representations_showing(m).map(|(id, _)| id).collect();
                    let at = reps.first().map(|id| cn::where_words(doc, *id)).unwrap_or_default();
                    (format!("{} on {at}", cn::pin_name(doc, m).unwrap_or_default()), reps)
                }
                None => (format!("{} has no mate", cn::half_tag(doc, c, side)), Vec::new()),
            };
            out.push(ConnectorRow { key: "mate".into(), heading: None, label: "Mate", value, origin: "derived", set: None, goto });
        }
    }
    out
}

fn json_id(c: ecad_model::ComponentId) -> String {
    c.hex()
}

/// What the inspector asks the trial for.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectorAction {
    /// Open Settings ▸ Standards at this key.
    Standards(&'static str),
    /// Run this `setdefault` line (the row's key, for its refusal).
    Run(&'static str, String),
    /// Select every placement of the device.
    Placements(Vec<RepresentationId>),
    /// Brief 044: run this command line (a connector row's edit).
    RunLine(String),
    /// Brief 048: bring this copy to its symbol's newest version (the review shows when it clashes).
    UpdateSymbol(RepresentationId),
    /// Brief 048c: the drawer row of this copy against the newest version, no save.
    Compare(RepresentationId),
}

/// The inspector's rows and what they were built from.
#[derive(Debug, Default)]
pub struct Inspector {
    pub rows: Vec<Row>,
    what: Option<What>,
    deps: BTreeSet<ObjectRef>,
    /// (project serial, change count, page, selection).
    seen: Option<(u64, u64, Option<PageId>, Vec<cadcraft_engine::doc::Handle>)>,
    /// The device's placements (its symbols), for "Select all placements".
    placements: Vec<RepresentationId>,
    /// Brief 044: a connector's Halves group and the selected pin's rows.
    pub connector: Vec<ConnectorRow>,
    connector_edits: BTreeMap<String, String>,
    /// The values being typed, by key.
    edits: BTreeMap<&'static str, String>,
    /// A refusal per key.
    pub errors: BTreeMap<&'static str, String>,
    /// Full builds of the rows (tests: the update map).
    pub rebuilds: u64,
    /// Brief 047 (design 047 §6.2): the selected device's part set, read only; empty when it has
    /// no part lines.
    pub parts: Vec<ecad_cadcraft::bom::PartSetRow>,
    /// Brief 048: "update available" for the selected symbol.
    pub symbol_update: Option<SymbolUpdate>,
    /// UI rev 3: what the Device and Layout tabs show about one selected device.
    pub device: Option<DeviceDetails>,
    /// With nothing selected, the page's number and title (the header).
    pub page_words: Option<(String, String)>,
    /// The folded sections' ids (`!id` for one that starts folded and was opened).
    pub folded: BTreeSet<String>,
    /// The Checks tab shows the whole project (else this page).
    pub checks_project: bool,
}

/// The kinds of a symbol's own texts.
const SYMBOL_KINDS: [TextKind; 4] = [TextKind::Tag, TextKind::PointLabel, TextKind::Description, TextKind::PartNumber];

/// A value as the inspector shows it: a show word in words (spec §3), else as the Standards table.
fn shown(e: &Entry, r: &defaults::Resolved<'_>) -> String {
    if e.key.ends_with(".show") {
        return match r.word() {
            "off" => "Hidden".into(),
            "first" => "Shown on the first of each run".into(),
            _ => "Shown on the page".into(),
        };
    }
    crate::lists::standards::value_text(e, r)
}

/// A style choice's row (brief 050a): the chosen style's name, the catalogue's styles, and the
/// warning of an own choice naming no style.
fn style_row(
    doc: &ecad_model::Document, e: &Entry, cat: &str, scope: Scope, r: &defaults::Resolved<'_>,
) -> (String, Vec<(String, String)>, Option<String>) {
    use ecad_cadcraft::styles;
    // Brief 050b (design 050 §7.5): a wire's category with where it comes from.
    if e.key == ecad_model::wiring::WIRE_CATEGORY
        && let Some(c) = scope.conductor
    {
        let looks = styles::wire::WireLooks::new(doc);
        let (id, from) = looks.category(doc, c);
        let list = styles::catalogue(cat).map(|x| styles::rows(doc, x).into_iter().map(|r| (r.id.text(), r.name)).collect()).unwrap_or_default();
        let name = id.as_ref().map(|i| styles::name(doc, cat, i));
        let value = match (name, from) {
            (Some(n), styles::wire::CategoryFrom::Potential(p)) => format!("{n} (from potential {p})"),
            (Some(n), styles::wire::CategoryFrom::Project) => format!("{n} (project default)"),
            (Some(n), styles::wire::CategoryFrom::Standard) => format!("{n} (standard default)"),
            (Some(n), _) => n,
            (None, _) => "None".into(),
        };
        let own = doc.conductors.get(&c).and_then(|x| x.properties.get(e.key));
        let dangling = own.filter(|v| e.parse_in(doc, v).is_none()).map(|_| "Not found: drawn without a category".to_string());
        let _ = r;
        return (value, list, dangling);
    }
    let id = styles::chosen(doc, e, scope);
    let list = styles::catalogue(cat).map(|c| styles::rows(doc, c).into_iter().map(|x| (x.id.text(), x.name)).collect()).unwrap_or_default();
    let own = match scope.component {
        Some(c) => doc.components.get(&c).and_then(|x| x.properties.get(e.key)),
        None => scope.page.and_then(|p| doc.pages.get(&p)).and_then(|x| x.properties.get(e.key)),
    };
    let dangling = own.filter(|v| e.parse_in(doc, v).is_none()).map(|_| format!("Not found: drawn as {}", styles::name(doc, cat, &id)));
    let _ = r;
    (styles::name(doc, cat, &id), list, dangling)
}

/// What one build of the inspector's copy makes.
#[derive(Debug, Default)]
struct Built {
    rows: Vec<Row>,
    deps: BTreeSet<ObjectRef>,
    placements: Vec<RepresentationId>,
    connector: Vec<ConnectorRow>,
    parts: Vec<ecad_cadcraft::bom::PartSetRow>,
    update: Option<SymbolUpdate>,
    device: Option<DeviceDetails>,
    page_words: Option<(String, String)>,
}

/// Brief 048 (design 048 §8.4): the selected symbol shows an older version of its project
/// symbol than the newest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolUpdate {
    pub rep: RepresentationId,
    pub shown: u64,
    pub newest: u64,
}

/// The update row's element name.
pub const SYMBOL_UPDATE: &str = "ecad.props.symbol.update";

fn rows_for(doc: &ecad_model::Document, what: What) -> Built {
    let mut deps = BTreeSet::from([ObjectRef::Project]);
    let (scope, entries, here): (Scope, Vec<&'static Entry>, Option<(Level, Target)>) = match what {
        What::Page(pg) => {
            deps.insert(ObjectRef::Page(pg));
            (Scope::page(pg), defaults::all().filter(|e| e.allows(Level::Page)).collect(), Some((Level::Page, Target::Page(pg))))
        }
        What::Symbol(c, rep) => {
            deps.insert(ObjectRef::Component(c));
            let mut es: Vec<&'static Entry> = Vec::new();
            // Brief 050a (design 050 §7.4): a Style row per text kind, chosen for the device.
            for k in SYMBOL_KINDS {
                es.push(k.style_entry());
                es.extend(k.show_entry());
            }
            // A location box shows its Box style.
            if ecad_cadcraft::locations::is_box(doc, c) {
                es.push(&defaults::styles::LOCATION);
            }
            let starter = doc
                .components
                .get(&c)
                .and_then(|c| c.definition)
                .and_then(|d| doc.definitions.get(&d))
                .and_then(ecad_cadcraft::symbols::of_definition);
            es.extend(starter.and_then(|s| defaults::symbols::prefix_entry(s.key)));
            (Scope::symbol(doc, rep), es, Some((Level::Object, Target::Symbol(rep))))
        }
        What::Conductor(w) => {
            deps.insert(ObjectRef::Conductor(w));
            let es = defaults::all()
                .filter(|e| e.key == defaults::text::WIRE_LABEL_HEIGHT.key || e.group == "Wire labels" || e.key.starts_with("wire."))
                .collect();
            (Scope { conductor: Some(w), ..Scope::project() }, es, Some((Level::Object, Target::Conductor(w))))
        }
    };
    let rows = entries
        .into_iter()
        .map(|e| {
            let r = defaults::resolve(doc, e, scope);
            // A style choice is set on the device (its component), not on one symbol.
            let target = match (e.kind, here) {
                (Kind::Style(_), Some((Level::Object, Target::Symbol(_)))) => match what {
                    What::Symbol(c, _) => Some((Level::Object, Target::Component(c))),
                    _ => here,
                },
                _ => here,
            };
            let (value, styles, dangling) = match e.kind {
                Kind::Style(cat) => style_row(doc, e, cat, scope, &r),
                _ => (shown(e, &r), Vec::new(), None),
            };
            Row {
                key: e.key,
                label: e.label.to_string(),
                value,
                from: r.from,
                here: target.filter(|(l, _)| e.allows(*l)),
                choices: match e.kind {
                    Kind::Choice(w) => w,
                    _ => &[],
                },
                styles,
                dangling,
            }
        })
        .collect();
    let placements = match what {
        What::Symbol(c, _) => doc.representations_of(Subject::Component(c)).map(|(id, _)| id).collect(),
        _ => Vec::new(),
    };
    let connector = match what {
        What::Symbol(c, rep) => connector_rows(doc, c, rep),
        _ => Vec::new(),
    };
    // Brief 047: the part set reads the device's own properties (already a dependency).
    let parts = match what {
        What::Symbol(c, _) => ecad_cadcraft::bom::part_set(doc, c),
        _ => Vec::new(),
    };
    let update = match what {
        What::Symbol(_, rep) => ecad_cadcraft::symbol_cmds::update_available(doc, rep).map(|(_, shown, newest)| SymbolUpdate { rep, shown, newest }),
        _ => None,
    };
    // UI rev 3: the Device and Layout tabs' copy of one device (a location box keeps the plain
    // view: its own section).
    let device = match what {
        What::Symbol(c, rep) if !ecad_cadcraft::locations::is_box(doc, c) => Some(details::build(doc, c, rep, &SYMBOL_KINDS, &mut deps)),
        _ => None,
    };
    let page_words = match what {
        What::Page(pg) => Some(details::page_words(doc, pg)),
        _ => None,
    };
    Built { rows, deps, placements, connector, parts, update, device, page_words }
}

/// Bring the inspector's rows up to date (its update map).
fn sync(t: &mut Trial) {
    let Some(p) = t.project.clone().filter(|p| p.is_attached(&t.cad.session)) else {
        let i = &mut t.settings.inspector;
        (i.rows, i.what, i.seen, i.device, i.page_words) = (Vec::new(), None, None, None, None);
        return;
    };
    let owned = t.cad.session.state().is_ok_and(|st| t.cad.session.ext.owner_of(st.uid).is_some());
    let selection = if owned { t.cad.session.selection() } else { Vec::new() };
    let serial = t.project_serial;
    let i = &mut t.settings.inspector;
    let seen = i.seen.clone();
    let deps = i.deps.clone();
    let built = p.read(&mut t.cad.session, |p, _| {
        let page = p.active_page();
        let count = p.change_count();
        let same = seen.as_ref().is_some_and(|(s, _, pg, sel)| *s == serial && *pg == page && *sel == selection);
        let stale = !same
            || match seen.as_ref().map(|s| s.1) {
                Some(n) if n == count => false,
                Some(n) => p.props_changes_since(n).is_none_or(|t| t.iter().any(|o| deps.contains(o))),
                None => true,
            };
        if !stale {
            return (count, None);
        }
        let doc = p.ledger.document();
        let what = page.and_then(|pg| what_of(p, pg, &selection));
        (count, Some((page, what, what.map(|w| rows_for(doc, w)))))
    });
    let Ok((count, fresh)) = built else { return };
    match fresh {
        None => {
            if let Some(s) = &mut i.seen {
                s.1 = count;
            }
        }
        Some((page, what, rows)) => {
            if what != i.what {
                i.edits.clear();
                i.errors.clear();
            }
            i.what = what;
            let b = rows.unwrap_or_default();
            (i.rows, i.deps, i.placements, i.connector, i.parts, i.symbol_update) = (b.rows, b.deps, b.placements, b.connector, b.parts, b.update);
            (i.device, i.page_words) = (b.device, b.page_words);
            i.connector_edits.clear();
            i.rebuilds += 1;
            i.seen = Some((serial, count, page, selection));
        }
    }
}

/// What the selection on `page` is, for the inspector: nothing selected is the page; one symbol
/// (or its tag text) is that symbol; one wire is that wire; anything else shows no defaults.
fn what_of(p: &ecad_cadcraft::Project, page: PageId, selection: &[cadcraft_engine::doc::Handle]) -> Option<What> {
    if selection.is_empty() {
        return Some(What::Page(page));
    }
    let doc = p.ledger.document();
    let map = &p.pages.get(&page)?.map;
    let mut found: Option<What> = None;
    for h in selection.iter().take(256) {
        let w = match map.get(*h)? {
            Mapped::Item(item) => match doc.representation_of_item(item) {
                Some((rid, r)) => match r.subject {
                    Subject::Component(c) => What::Symbol(c, rid),
                    Subject::Conductor(w) => What::Conductor(w),
                    Subject::Cable(_) => return None,
                },
                None => return None,
            },
            Mapped::Tag(r) | Mapped::Marker(r, _) | Mapped::Xref(r) => match doc.representations.get(&r).map(|x| x.subject) {
                Some(Subject::Component(c)) => What::Symbol(c, r),
                _ => return None,
            },
            Mapped::Label(w) => What::Conductor(w),
            _ => return None,
        };
        match found {
            None => found = Some(w),
            Some(f) if f == w => {}
            Some(_) => return None,
        }
    }
    found
}

impl Inspector {
    /// Draw the defaults rows that `keep` keeps (brief 042: each with its origin badge, a field
    /// when set here, Reset when overridden here, grey text read from further out).
    pub(super) fn show_rows(&mut self, ui: &mut egui::Ui, writable: bool, out: &mut Vec<InspectorAction>, keep: impl Fn(&Row) -> bool) {
        if !self.rows.iter().any(&keep) {
            return;
        }
        let weak = ui.visuals().weak_text_color();
        for row in self.rows.iter().filter(|r| keep(r)) {
            ui.add_space(space::XS);
            // Line one: the label, then on the right the origin badge and Reset (two lines fit
            // the panel's width; one line did not).
            ui.horizontal(|ui| {
                ui.label(RichText::new(&row.label).small().color(weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if writable
                        && row.overridden_here()
                        && let Some((level, target)) = row.here
                    {
                        let r = ui.small_button("Reset");
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("ecad.inspector.reset.{}", row.key)));
                        if r.clicked() {
                            out.push(InspectorAction::Run(row.key, default_cmds::line(row.key, level, target, None)));
                        }
                    }
                    let badge =
                        ecad_ui::prim::badge(ui, crate::lists::standards::source_word(row.from), crate::lists::standards::source_tone(row.from));
                    let b = ui.interact(badge.rect, egui::Id::unique(("ecad.origin", row.key)), egui::Sense::click());
                    b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("ecad.origin.{}", row.key)));
                    if b.on_hover_text("Where this value comes from: open Standards").clicked() {
                        out.push(InspectorAction::Standards(row.key));
                    }
                });
            });
            // Line two: the value, set here (a field or a list) or read (grey, goes to Standards).
            let name = format!("ecad.inspector.{}", row.key);
            let width = ui.available_width();
            match row.here.filter(|_| writable) {
                // Brief 050a (design 050 §7.4): a style choice's value list of the catalogue's
                // styles, by name; choosing runs `setdefault … level object` with the ID.
                Some((level, target)) if !row.styles.is_empty() => {
                    let name = format!("ecad.inspector.style.{}", row.key);
                    let mut pick = None;
                    let combo = egui::ComboBox::from_id_salt(&name).selected_text(&row.value).width(width.min(240.0)).show_ui(ui, |ui| {
                        for (id, n) in &row.styles {
                            if ui.selectable_label(*n == row.value, n).clicked() {
                                pick = Some(id.clone());
                            }
                        }
                    });
                    combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &name));
                    if let Some(id) = pick {
                        // Brief 050b: a wire's category runs `category` (it may set the potential).
                        let line = match target {
                            Target::Conductor(c) if row.key == ecad_model::wiring::WIRE_CATEGORY => {
                                format!("category {}", serde_json::json!({"conductor": c.hex(), "value": id}))
                            }
                            _ => default_cmds::line(row.key, level, target, Some(&id)),
                        };
                        out.push(InspectorAction::Run(row.key, line));
                    }
                }
                Some((level, target)) if !row.choices.is_empty() => {
                    let mut pick = None;
                    let combo = egui::ComboBox::from_id_salt(&name).selected_text(&row.value).width(width.min(240.0)).show_ui(ui, |ui| {
                        for w in row.choices {
                            if ui.selectable_label(false, *w).clicked() {
                                pick = Some(*w);
                            }
                        }
                    });
                    combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &name));
                    if let Some(w) = pick {
                        out.push(InspectorAction::Run(row.key, default_cmds::line(row.key, level, target, Some(w))));
                    }
                }
                Some((level, target)) => {
                    let buf = self.edits.entry(row.key).or_insert_with(|| row.value.trim_end_matches(" mm").to_string());
                    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(width.min(240.0)));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, &name));
                    if r.lost_focus() && buf.trim() != row.value.trim_end_matches(" mm") {
                        out.push(InspectorAction::Run(row.key, default_cmds::line(row.key, level, target, Some(buf.trim()))));
                    }
                }
                None => {
                    let r = ui.add(egui::Label::new(RichText::new(&row.value).color(weak)).sense(egui::Sense::click()));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, &name));
                    if r.on_hover_text("Set in Settings ▸ Standards").clicked() {
                        out.push(InspectorAction::Standards(row.key));
                    }
                }
            }
            if let Some(d) = &row.dangling {
                ui.label(RichText::new(d).small().color(ui.visuals().warn_fg_color));
            }
            if let Some(e) = self.errors.get(row.key) {
                ui.label(RichText::new(e).small().color(ui.visuals().error_fg_color));
            }
        }
    }

    /// "Select all N placements" when the device has more than one symbol.
    pub(super) fn show_placements(&self, ui: &mut egui::Ui, out: &mut Vec<InspectorAction>) {
        if self.placements.len() > 1 {
            let n = self.placements.len();
            let r = widgets::button(ui, &format!("Select all {n} placements"), false, None);
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, PLACEMENTS));
            if r.clicked() {
                out.push(InspectorAction::Placements(self.placements.clone()));
            }
        }
    }
}

impl Inspector {
    /// Brief 044 (design 044 §9.4): the Halves group, and Pin and Mate for a pin symbol.
    fn show_connector(&mut self, ui: &mut egui::Ui, writable: bool, out: &mut Vec<InspectorAction>) {
        if self.connector.is_empty() {
            return;
        }
        ui.add_space(space::M);
        ecad_ui::prim::read_value(ui, "HALVES").widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.props.connector"));
        let weak = ui.visuals().weak_text_color();
        for row in &self.connector {
            if let Some(h) = &row.heading {
                ui.add_space(space::XS);
                ui.label(RichText::new(h).strong());
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(row.label).small().color(weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ecad_ui::prim::badge(ui, row.origin, ecad_ui::prim::Tone::Neutral);
                });
            });
            let name = format!("ecad.props.connector.{}", row.key);
            let width = ui.available_width().min(240.0);
            match (&row.set, writable) {
                (Some((cmd, params, value_key)), true) => {
                    let start = if row.value == "?" { String::new() } else { row.value.clone() };
                    let buf = self.connector_edits.entry(row.key.clone()).or_insert(start);
                    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(width));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, &name));
                    if r.lost_focus() && buf.trim() != row.value.trim() {
                        let mut p = params.clone();
                        p[*value_key] = if buf.trim().is_empty() { serde_json::Value::Null } else { serde_json::json!(buf.trim()) };
                        out.push(InspectorAction::RunLine(format!("{cmd} {p}")));
                    }
                }
                _ => {
                    let r = ui.add(egui::Label::new(RichText::new(&row.value).color(weak)).sense(egui::Sense::click()));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, &name));
                    if r.clicked() && !row.goto.is_empty() {
                        out.push(InspectorAction::Placements(row.goto.clone()));
                    }
                }
            }
        }
    }

    /// Brief 048 (design 048 §8.4): `Symbol: v2 · update available` with Update, which brings this
    /// copy to the newest version (refused with the reason when it would clash).
    fn show_symbol_update(&self, ui: &mut egui::Ui, writable: bool, out: &mut Vec<InspectorAction>) {
        let Some(u) = &self.symbol_update else { return };
        ui.add_space(space::M);
        ui.horizontal(|ui| {
            let text = format!("Symbol: v{} · update available (v{})", u.shown, u.newest);
            ui.label(RichText::new(text).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, SYMBOL_UPDATE));
            if writable {
                let r = ui.small_button("Update");
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{SYMBOL_UPDATE}.button")));
                if r.clicked() {
                    out.push(InspectorAction::UpdateSymbol(u.rep));
                }
            }
            // Brief 048c (design 048 §8.4): the drawer row of this copy alone, no save.
            let c = ui.small_button("Compare");
            c.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{SYMBOL_UPDATE}.compare")));
            if c.clicked() {
                out.push(InspectorAction::Compare(u.rep));
            }
        });
    }
}

impl Trial {
    /// Act on what the inspector asked for.
    pub fn inspector_action(&mut self, a: InspectorAction) {
        match a {
            InspectorAction::Standards(key) => self.open_standards(Some(key)),
            InspectorAction::Run(key, line) => {
                let r = self.run_line(&line);
                let i = &mut self.settings.inspector;
                i.edits.remove(key);
                match r {
                    Some(e) => {
                        i.errors.insert(key, e);
                    }
                    None => {
                        i.errors.remove(key);
                    }
                }
            }
            InspectorAction::RunLine(line) => {
                if let Some(e) = self.run_line(&line) {
                    self.cad.session.echo(e);
                }
            }
            InspectorAction::UpdateSymbol(rep) => {
                if let Err(e) = self.update_symbol(rep) {
                    self.cad.session.echo(e);
                }
            }
            InspectorAction::Compare(rep) => self.compare_symbol(rep),
            InspectorAction::Placements(reps) => {
                if let Err(e) = self.goto_symbols(&reps) {
                    self.cad.session.echo(e);
                }
            }
        }
    }
}
