//! The Project workspace's right panel (UI rev 3, "Project workspace": "the right panel shows only
//! the selected item: its properties and its own checks"; boards 2.01–2.06). It reads the
//! selected row of the tab shown, from the view's own copy (no project read), and lays it out as
//! the boards do: the item's name and where it is, a go-to, its checks, then folding sections of
//! label and value. Grey text is read from the drawing; a boxed value is set here. A Devices row
//! keeps 039's property editor (the same `setprop` the page runs). What is not built yet (the part
//! catalogue, the cable manager's ends, the report generator's settings) says so, greyed.
//!
//! **Update map.** Nothing of its own: it shows what the tab's view holds this frame.

use ecad_model::{DeviceId, PageId, Pt, RepresentationId};
use ecad_widgets::panel;
use ecad_widgets::prim::{self, Tone};
use ecad_widgets::tokens::{space, ui_type};
use eframe::egui::{self, Rect, vec2};

use crate::App;
use crate::shell::ListsView;

/// The selected row of the tab shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selected {
    Device(DeviceId),
    Terminal(crate::terminal_strips::plan::TermKey),
    Card(usize),
    /// Brief 056 (design 056 §5): a channel row of Project ▸ PLC.
    Channel(crate::plc::list::IoKey),
    Wire(usize),
    Cable(usize),
    Report(usize),
}

/// What the panel says with nothing selected.
pub const NOTHING: &str = "Select a row to see it here.";
/// The greyed parts buttons' reason.
pub const CATALOGUE_COMES: &str = "The part catalogue comes later.";
pub const CABLE_MANAGER_COMES: &str = "Comes with the cable manager.";
/// Brief 059 (owner): the Rating section of a cable with nothing to rate yet.
pub const RATE_NOTE: &str = "Set a cable type to rate this cable.";
/// Brief 059 (board 2.06): the Details pane's tabs and their accessible name.
pub const CABLE_TABS: &str = "ecad.project.detail.cable.tabs";
/// A cross-section's unit.
const MM2: &str = "mm\u{b2}";

fn first<K: Copy + Ord>(t: &ecad_widgets::table::Table<K>) -> Option<K> {
    t.selection.cursor().or_else(|| t.selection.keys().iter().next().copied())
}

/// The selected row of the tab shown, if any.
pub fn selected_key(t: &App) -> Option<Selected> {
    let r = t.reports.as_ref();
    match t.shell.lists {
        ListsView::Devices => t.panel.devices.selected.map(Selected::Device),
        ListsView::Terminals => r.and_then(|r| first(&r.terminals.table)).map(Selected::Terminal),
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
enum Go {
    Symbols(Vec<RepresentationId>),
    Place(PageId, Pt),
}

/// One labelled value.
#[derive(Debug, Clone, PartialEq)]
struct Prop {
    label: &'static str,
    value: String,
    /// Where it comes from (`from page`, `auto`), at the right.
    hint: String,
    /// Read from the drawing (grey, no box) or set here (boxed).
    set: bool,
    /// Not built yet: greyed with this reason.
    later: Option<&'static str>,
    /// Its unit (`mm²`), whether it is the key identity value (bold), its colour swatch, and
    /// whether it is one of a list (a chevron).
    unit: &'static str,
    bold: bool,
    swatch: Option<(ecad_widgets::tokens::Color, Option<ecad_widgets::tokens::Color>)>,
    choice: bool,
}

impl Prop {
    fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }
    fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    fn choice(mut self) -> Self {
        self.choice = true;
        self
    }
    fn swatch(mut self, code: &str) -> Self {
        self.swatch = swatch(code);
        self
    }
}

/// A wire colour code's swatch in its insulation colours (the paper sheet's, on either shell:
/// black insulation stays black); none for a code that is not a colour.
fn swatch(code: &str) -> Option<(ecad_widgets::tokens::Color, Option<ecad_widgets::tokens::Color>)> {
    let sheet = &ecad_drawing_style::sheet::PAPER;
    ecad_model::objects::wire::Colours::parse(code).and_then(|c| ecad_engine::cables::draw::wire_colours(&c, sheet)).map(|c| (c.solid, c.dashes))
}

fn read(label: &'static str, value: impl Into<String>) -> Prop {
    Prop { label, value: value.into(), hint: String::new(), set: false, later: None, unit: "", bold: false, swatch: None, choice: false }
}

fn set(label: &'static str, value: impl Into<String>, hint: impl Into<String>) -> Prop {
    Prop { label, value: value.into(), hint: hint.into(), set: true, later: None, unit: "", bold: false, swatch: None, choice: false }
}

fn later(label: &'static str, why: &'static str) -> Prop {
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
struct Section {
    title: &'static str,
    hint: String,
    props: Vec<Prop>,
    /// Plain lines (a part set's lines, the report's columns).
    lines: Vec<(String, String)>,
    /// Greyed buttons with their reason.
    later: Vec<(&'static str, &'static str)>,
    note: Option<&'static str>,
    /// Brief 057: the Rating section's result (drawn by [`crate::wires::rating`]).
    rating: Option<crate::wires::rating::Panel>,
}

/// The panel's contents.
#[derive(Debug, Clone, PartialEq, Default)]
struct Detail {
    title: String,
    provisional: bool,
    subtitle: String,
    go: Option<(String, Go)>,
    /// Its checks (an empty list: "No checks on …").
    checks: Vec<(String, Tone)>,
    /// Brief 057 (owner): the open cores' fixes, as buttons under the checks.
    core_fixes: Vec<ecad_electrical::checks::Fix>,
    /// The Devices row's property editor goes after the head.
    editor: bool,
    /// Brief 059: the cable editor goes after the head, with the cable's drawn ends.
    cable_editor: Option<(ecad_model::CableId, [String; 2])>,
    sections: Vec<Section>,
}

fn nonempty(s: &str) -> String {
    if s.trim().is_empty() || s == "?" { crate::reports::report::DASH.into() } else { s.to_string() }
}

fn device(t: &App, c: DeviceId) -> Option<Detail> {
    let r = t.panel.devices.source.row(c)?;
    let e = &r.entry;
    let prop = |i: usize| e.properties.get(i).cloned().flatten().unwrap_or_default();
    let tag = e.tag.clone().unwrap_or_default();
    let place = e.symbols.first().map(|s| s.place.clone());
    let symbol = e.symbols.first().map(|s| s.name.clone()).unwrap_or_else(|| crate::reports::report::DASH.into());
    let subtitle = match &place {
        Some(p) => format!("{} \u{b7} drawn at {p}", nonempty(&e.description)),
        None => format!("{} \u{b7} not on a page yet", nonempty(&e.description)),
    };
    let go = (!e.symbols.is_empty()).then(|| ("Page".to_string(), Go::Symbols(e.symbols.iter().map(|s| s.rep).collect())));
    let mut checks = Vec::new();
    if e.tag.as_deref().is_some_and(ecad_model::naming::tag::is_provisional) {
        checks.push(("The tag is provisional.".to_string(), Tone::Provisional));
    }
    if e.symbols.is_empty() {
        checks.push(("Not on a page yet.".to_string(), Tone::Warning));
    }
    if prop(3).is_empty() {
        checks.push(("No part yet: it has no order code.".to_string(), Tone::Warning));
    }
    let maker = prop(2);
    let part = prop(3);
    let main = if part.is_empty() { "No parts yet".to_string() } else { part.clone() };
    let mut lines = vec![(main, if maker.is_empty() { "main part".to_string() } else { format!("{maker} \u{b7} main part") })];
    if r.parts > 1 {
        lines.push((
            format!("{} more line{}", r.parts - 1, if r.parts == 2 { "" } else { "s" }),
            "listed in Project \u{25b8} Reports \u{25b8} Parts list".into(),
        ));
    }
    let drawn = Section {
        title: "Where it is drawn",
        hint: String::new(),
        lines: e.symbols.iter().map(|s| (s.name.clone(), s.place.clone())).collect(),
        ..Section::default()
    };
    Some(Detail {
        title: tag,
        provisional: e.tag.as_deref().is_some_and(ecad_model::naming::tag::is_provisional),
        subtitle,
        go,
        checks,
        editor: true,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Symbol",
                hint: "read from the drawing".into(),
                props: vec![read("Symbol", symbol), read("Location", nonempty(&r.location))],
                ..Section::default()
            },
            Section {
                title: "Parts",
                hint: format!("{} line{}", r.parts, if r.parts == 1 { "" } else { "s" }),
                lines,
                later: vec![("+ Accessory", CATALOGUE_COMES), ("From catalogue", CATALOGUE_COMES), ("Save as a kit", CATALOGUE_COMES)],
                note: Some("The main part is first and gives the device its ratings. A line with no order code raises a check."),
                ..Section::default()
            },
            drawn,
        ],
    })
}

fn side_text(cells: &[ecad_electrical::terminal_strips::SideCell]) -> String {
    let v: Vec<String> = cells
        .iter()
        .map(|c| {
            let target = c.target.clone().unwrap_or_else(|| "open end".into());
            match &c.wire {
                Some(w) => format!("{target} \u{b7} wire {w}"),
                None => target,
            }
        })
        .collect();
    if v.is_empty() { "not wired".into() } else { v.join(", ") }
}

fn terminal(t: &App, k: crate::terminal_strips::plan::TermKey) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let row = r.terminals.source.row(k)?;
    let crate::terminal_strips::plan::PlanRow::Term { tag, row, place, .. } = row else {
        return None;
    };
    let strip = r.strips.iter().find(|(c, _)| *c == k.0).map_or_else(|| tag.clone(), |(_, t)| t.clone());
    let mut checks = Vec::new();
    if row.inside.iter().chain(&row.outside).any(|c| c.target.is_none()) {
        checks.push(("An end is open.".to_string(), Tone::Error));
    }
    let cable = row.outside.iter().find_map(|c| c.cable.as_ref().map(|(_, t)| t.clone().unwrap_or_default())).unwrap_or_default();
    let core = row.outside.iter().find_map(|c| c.core.clone()).unwrap_or_default();
    let mut outside = side_text(&row.outside);
    if !cable.is_empty() {
        outside = format!("{outside} \u{b7} {cable} {core}");
    }
    Some(Detail {
        title: format!("{tag}:{}", row.shown),
        provisional: false,
        subtitle: format!("{} \u{b7} {strip}", row.ty.name),
        go: row.go.map(|(pg, at)| (place.clone(), Go::Place(pg, at))),
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Identity",
                props: vec![
                    read("Strip", strip),
                    set("Position", row.label.clone(), "set in the table"),
                    read("Level", if row.level_name.is_empty() { row.level.to_string() } else { row.level_name.clone() }),
                    read("State", row.state()),
                ],
                ..Section::default()
            },
            Section { title: "Part", hint: "from part set".into(), props: vec![read("Type", row.ty.name)], ..Section::default() },
            Section {
                title: "Connections",
                hint: "read from the drawing".into(),
                props: vec![
                    read("Inside", side_text(&row.inside)),
                    read("Outside", outside),
                    read("Link", row.bridge_group.map_or_else(|| "none".to_string(), |g| format!("bridge {g}"))),
                ],
                ..Section::default()
            },
            Section { title: "Where it is drawn", lines: vec![("Schematic".into(), place.clone())], ..Section::default() },
        ],
    })
}

/// Brief 056 (design 056 §5): a channel row: its signal, where it is drawn and its test.
fn channel(t: &App, k: crate::plc::list::IoKey) -> Option<Detail> {
    use ecad_electrical::plc::signals;
    let r = t.reports.as_ref()?;
    let row = r.io.source.row(k)?;
    let addr = row.address.clone().unwrap_or_default();
    let mut subtitle = vec![addr.clone(), row.ty.to_string()];
    if let Some(tag) = &row.tag {
        subtitle.push(tag.clone());
    }
    let go = row.page.zip(row.at).map(|(pg, at)| (row.place.clone().unwrap_or_else(|| "Page".into()), Go::Place(pg, at)));
    let checks = Vec::new();
    let safe = match row.safe_shown() {
        Some((s, true)) => set("Safe", signals::safe_words(s), "default"),
        Some((s, false)) => set("Safe", signals::safe_words(s), "set here"),
        None => read("Safe", crate::reports::report::DASH),
    };
    let (drives, through) = signals::drives(&row.targets);
    let mut drawn: Vec<(String, String)> =
        drives.iter().map(|d| (format!("{}:{}{}", d.device, d.point, if through { " (passes through)" } else { "" }), d.place.clone())).collect();
    if drawn.is_empty() {
        drawn.push(("Drives".into(), crate::reports::report::DASH.into()));
    }
    Some(Detail {
        title: format!("{} channel {}", row.card_tag, row.channel),
        provisional: false,
        subtitle: subtitle.join(" \u{b7} "),
        go,
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Signal",
                hint: "set in the table".into(),
                props: vec![
                    set("PLC tag", nonempty(row.tag.as_deref().unwrap_or_default()), ""),
                    set("Description", nonempty(row.text.as_deref().unwrap_or_default()), ""),
                    safe,
                    set("Address", nonempty(&ecad_electrical::plc::iolist::address_words(row)), if row.typed { "typed" } else { "automatic" }),
                    set("Pins", nonempty(&ecad_electrical::plc::iolist::pins_words(&row.pins)), ""),
                ],
                ..Section::default()
            },
            Section {
                title: "Drawn",
                hint: "read from the drawing".into(),
                props: vec![
                    read("Place", nonempty(row.place.as_deref().unwrap_or_default())),
                    read("Wire", nonempty(row.wire.as_deref().unwrap_or_default())),
                ],
                lines: drawn,
                ..Section::default()
            },
            Section { title: "Test", lines: vec![("No tests yet.".into(), String::new())], ..Section::default() },
        ],
    })
}

fn wire(t: &App, k: usize) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let v = &r.wires;
    let rows = &v.source.rows;
    let number = rows.text(k, "Wire number");
    let (from, to, page) = (rows.text(k, "From"), rows.text(k, "To"), rows.text(k, "Page"));
    let checks = rows.rows.get(k).map(|r| r.iter().filter_map(|c| c.flag.clone()).map(|f| (f, Tone::Warning)).collect()).unwrap_or_default();
    // Board 2.05's hints: a number is given by the numbering ("auto") unless it is provisional; a
    // colour or size is the category's ("from category") unless the wire sets its own ("set
    // here"), as the wire report records it.
    let number_hint = match (v.source.provisional(k), number.trim().is_empty()) {
        (true, _) => "provisional",
        (false, false) => "auto",
        (false, true) => "",
    };
    let has_category = !rows.text(k, "Category").trim().is_empty();
    let (colour_own, size_own) = v.own.get(k).copied().unwrap_or_default();
    let source = |own: bool, value: &str| match (own, has_category && !value.trim().is_empty()) {
        (true, _) => "set here",
        (false, true) => "from category",
        (false, false) => "",
    };
    Some(Detail {
        title: number.clone(),
        provisional: v.source.provisional(k),
        subtitle: format!("Wire \u{b7} {from} to {to} \u{b7} {page}"),
        go: v.gos.get(k).copied().flatten().map(|(pg, at)| (page.clone(), Go::Place(pg, at))),
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Identity",
                props: vec![
                    set("Number", nonempty(&number), number_hint).bold(),
                    set("Potential", nonempty(&rows.text(k, "Potential")), "from the drawing"),
                    read("From", nonempty(&from)),
                    read("To", nonempty(&to)),
                ],
                ..Section::default()
            },
            Section {
                title: "Made as",
                hint: "from the category".into(),
                props: vec![
                    set("Category", nonempty(&rows.text(k, "Category")), "").choice(),
                    set("Colour", nonempty(&rows.text(k, "Colour")), source(colour_own, &rows.text(k, "Colour"))).swatch(&rows.text(k, "Colour")),
                    set("Size", nonempty(&rows.text(k, "Cross-section")), source(size_own, &rows.text(k, "Cross-section"))).unit(MM2),
                    read("Cable", nonempty(&rows.text(k, "Cable"))),
                ],
                ..Section::default()
            },
            rating_section(t, v.source.rating(k), &v.source.rating.methods),
            Section {
                title: "Ends",
                props: vec![later("Ferrules", CABLE_MANAGER_COMES), later("Marker", CABLE_MANAGER_COMES), later("Length", CABLE_MANAGER_COMES)],
                ..Section::default()
            },
        ],
    })
}

/// Brief 057 (design 057 §2): the Rating section, after Made as or Part; its folded line is the
/// result. A wire or cable with no rating data shows none, so a project that does not rate is
/// not nagged.
fn rating_section(_t: &App, r: Option<&ecad_electrical::circuits::circuit::Rating>, methods: &[&'static str]) -> Section {
    let Some(r) = r else { return Section::default() };
    let methods = if methods.is_empty() { ecad_electrical::devices::ratings::dataset::METHODS.to_vec() } else { methods.to_vec() };
    Section {
        title: "Rating",
        hint: r.summary(),
        rating: Some(crate::wires::rating::Panel { rating: r.clone(), methods, writable: false }),
        ..Section::default()
    }
}

fn cable(t: &App, k: usize) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let v = &r.cables;
    let rows = &v.source.rows;
    let tag = rows.text(k, "Tag");
    let (from, to) = (rows.text(k, "From"), rows.text(k, "To"));
    // Brief 059: a cable not drawn has no drawing to flag (no type, no ends yet): its one row is
    // "not in the drawing yet".
    let mut checks: Vec<(String, Tone)> = if v.source.drawn(k) {
        rows.rows.get(k).map(|r| r.iter().filter_map(|c| c.flag.clone()).map(|f| (f, Tone::Error)).collect()).unwrap_or_default()
    } else {
        Vec::new()
    };
    // Brief 057 (owner rule): each open core, as an error, with its two fixes.
    let mut core_fixes = Vec::new();
    for (i, label) in v.source.open.get(k).into_iter().flatten().enumerate() {
        checks.push((format!("Core {label} is open at an end: connect it, or tick Not used."), Tone::Error));
        if let Some(cable) = v.ids.get(k) {
            core_fixes.push(ecad_electrical::checks::Fix::CoreOpen(*cable, u16::try_from(i).unwrap_or(u16::MAX)));
        }
    }
    let spare = rows.text(k, "Spare");
    if spare.parse::<u32>().is_ok_and(|n| n > 0) {
        checks.push((format!("{spare} spare core{}.", if spare == "1" { "" } else { "s" }), Tone::Neutral));
    }
    // Brief 059 (design 059 §6): a cable not drawn says so (Info), with no core rows.
    let drawn = v.source.drawn(k);
    if !drawn {
        checks.push((format!("{tag} is not in the drawing yet."), Tone::Neutral));
    }
    let subtitle = if drawn {
        format!("{} to {}", nonempty(&rows.text(k, "From location")), nonempty(&rows.text(k, "To location")))
    } else {
        "not drawn".to_string()
    };
    Some(Detail {
        title: tag.clone(),
        provisional: false,
        subtitle,
        go: v.gos.get(k).copied().flatten().map(|(pg, at)| ("Page".to_string(), Go::Place(pg, at))),
        checks,
        editor: false,
        cable_editor: v.ids.get(k).map(|c| (*c, [from, to])),
        core_fixes,
        // Brief 059: Identity, Part and Ends are the cable editor's; the Rating section stays 057's,
        // and shows a note when the cable has nothing to rate yet (owner, review of 059).
        sections: vec![match v.source.rating(k) {
            Some(r) => rating_section(t, Some(r), &v.source.rating.methods),
            None => Section { title: "Rating", hint: "not rated".into(), note: Some(RATE_NOTE), ..Section::default() },
        }],
    })
}

fn report(t: &App, k: usize) -> Option<Detail> {
    let rep = crate::reports::list::REPORTS.get(k)?;
    let r = t.reports.as_ref()?;
    let columns: Vec<(String, String)> =
        if rep.tab == Some(r.tab) { r.table().headers.iter().map(|h| ((*h).to_string(), "shown".to_string())).collect() } else { Vec::new() };
    // Counted when the copy is taken, not each frame.
    let flagged = if rep.tab == Some(r.tab) { r.flagged } else { 0 };
    let checks =
        if flagged > 0 { vec![(format!("{flagged} cell{} marked ?", if flagged == 1 { "" } else { "s" }), Tone::Warning)] } else { Vec::new() };
    Some(Detail {
        title: rep.name.to_string(),
        provisional: false,
        subtitle: "Report \u{b7} generated on demand".into(),
        go: None,
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Contents",
                props: vec![read("Reads", rep.reads), read("Group by", rep.grouped), later("Merge", crate::workspace::frame::REPORTS_COME)],
                ..Section::default()
            },
            Section {
                title: "Columns",
                hint: "as the preview shows them".into(),
                lines: columns,
                later: vec![("Choose columns", crate::workspace::frame::REPORTS_COME)],
                ..Section::default()
            },
            Section {
                title: "Output",
                props: vec![
                    later("In the book", crate::workspace::frame::REPORTS_COME),
                    later("Layout", crate::workspace::frame::REPORTS_COME),
                    later("Updates", crate::workspace::frame::REPORTS_COME),
                ],
                note: Some("Export CSV from the preview."),
                ..Section::default()
            },
        ],
    })
}

fn detail(t: &App) -> Option<Detail> {
    match selected_key(t)? {
        Selected::Device(c) => device(t, c),
        Selected::Terminal(k) => terminal(t, k),
        // A card is edited by `card_editor` (see `show`).
        Selected::Card(_) => None,
        Selected::Channel(k) => channel(t, k),
        Selected::Wire(k) => wire(t, k),
        Selected::Cable(k) => cable(t, k),
        Selected::Report(k) => report(t, k),
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
    /// Brief 057: a `rating` call from the Rating section.
    Run(&'static str, serde_json::Value),
    /// Brief 059: the cable editor's ask, on its cable.
    Cable(ecad_model::CableId, crate::cables::editor::Ask),
}

/// Draw the right panel for the Project workspace in `rect`.
pub fn show(t: &mut App, ui: &mut egui::Ui, rect: Rect) {
    // Brief 056 (owner, 2026-11-14): a card selected in Project ▸ PLC is edited, as board 2.03.
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
    // Brief 059 (board 2.06): the cable's Cable / Checks tabs, and its Checks rows.
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
                ui.label(egui::RichText::new(&d.title).font(ecad_widgets::fonts::font(ui.ctx(), ui_type::OBJECT)).color(ecad_widgets::color(title_ink)));
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
            // Brief 059 (board 2.06): Cable | Checks n.
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
            // Brief 057 (owner): Go to the open end, for each open core (the core table's Not
            // used toggle is right there, so no tick button here).
            for f in &d.core_fixes {
                if ui.add(prim::Button::new(crate::checks::panel::GO_TO_OPEN_END).small()).clicked() {
                    out.push(Out::Panel(crate::pages::project_panel::Action::CoreFix(*f, 1)));
                }
            }
            ui.add_space(space::S);
            rule(ui);
            // Brief 059: the cable editor (design 059 §5.2), with 057's Rating section between Part
            // and Ends, as board 2.06 orders them.
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
            // A cable's sections (057's Rating) were drawn inside the editor, after Part.
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
