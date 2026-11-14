//! The Project workspace's right panel (UI rev 3, "Project workspace": "the right panel shows only
//! the selected item: its properties and its own checks"; boards 2.01–2.06). It reads the
//! selected row of the tab shown, from the view's own copy (no project read), and lays it out as
//! the boards do: the item's name and where it is, a go-to, its checks, then folding sections of
//! label and value. Grey text is read from the drawing; a boxed value is set here. A Devices row
//! keeps 039's property editor (the same `setprop` the page runs). What is not built yet (the part
//! catalogue, the cable manager's ends, the report generator's settings) says so, greyed.
//!
//! **Update map.** Nothing of its own: it shows what the tab's view holds this frame.

use ecad_model::{ComponentId, PageId, Pt, RepresentationId};
use ecad_ui::prim::{self, Tone};
use ecad_ui::tokens::{line, radius, space, ui_type};
use eframe::egui::{self, Rect, Stroke, pos2, vec2};

use crate::Trial;
use crate::shell::ListsView;

/// The selected row of the tab shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selected {
    Device(ComponentId),
    Terminal(super::terminals::TermKey),
    Card(usize),
    Wire(usize),
    Cable(usize),
    Report(usize),
}

/// What the panel says with nothing selected.
pub const NOTHING: &str = "Select a row to see it here.";
/// The greyed parts buttons' reason.
pub const CATALOGUE_COMES: &str = "The part catalogue comes later.";
pub const CABLE_MANAGER_COMES: &str = "Comes with the cable manager.";

fn first<K: Copy + Ord>(t: &ecad_ui::table::Table<K>) -> Option<K> {
    t.selection.cursor().or_else(|| t.selection.keys().iter().next().copied())
}

/// The selected row of the tab shown, if any.
pub fn selected_key(t: &Trial) -> Option<Selected> {
    let r = t.reports.as_ref();
    match t.shell.lists {
        ListsView::Devices => t.panel.devices.selected.map(Selected::Device),
        ListsView::Terminals => r.and_then(|r| first(&r.terminals.table)).map(Selected::Terminal),
        ListsView::Plc => r.and_then(|r| r.io.selected).map(Selected::Card),
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
    Page(PageId, Option<ComponentId>),
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
}

fn read(label: &'static str, value: impl Into<String>) -> Prop {
    Prop { label, value: value.into(), hint: String::new(), set: false, later: None }
}

fn set(label: &'static str, value: impl Into<String>, hint: impl Into<String>) -> Prop {
    Prop { label, value: value.into(), hint: hint.into(), set: true, later: None }
}

fn later(label: &'static str, why: &'static str) -> Prop {
    Prop { label, value: super::report::DASH.into(), hint: String::new(), set: false, later: Some(why) }
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
    /// The Devices row's property editor goes after the head.
    editor: bool,
    sections: Vec<Section>,
}

fn nonempty(s: &str) -> String {
    if s.trim().is_empty() || s == "?" { super::report::DASH.into() } else { s.to_string() }
}

fn device(t: &Trial, c: ComponentId) -> Option<Detail> {
    let r = t.panel.devices.source.row(c)?;
    let e = &r.entry;
    let prop = |i: usize| e.properties.get(i).cloned().flatten().unwrap_or_default();
    let tag = e.tag.clone().unwrap_or_default();
    let place = e.symbols.first().map(|s| s.place.clone());
    let symbol = e.symbols.first().map(|s| s.name.clone()).unwrap_or_else(|| super::report::DASH.into());
    let subtitle = match &place {
        Some(p) => format!("{} \u{b7} drawn at {p}", nonempty(&e.description)),
        None => format!("{} \u{b7} not on a page yet", nonempty(&e.description)),
    };
    let go = (!e.symbols.is_empty()).then(|| ("Page".to_string(), Go::Symbols(e.symbols.iter().map(|s| s.rep).collect())));
    let mut checks = Vec::new();
    if e.tag.as_deref().is_some_and(ecad_model::tag::is_provisional) {
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
        provisional: e.tag.as_deref().is_some_and(ecad_model::tag::is_provisional),
        subtitle,
        go,
        checks,
        editor: true,
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

fn side_text(cells: &[ecad_cadcraft::strips::SideCell]) -> String {
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

fn terminal(t: &Trial, k: super::terminals::TermKey) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let row = r.terminals.source.row(k)?;
    let super::terminals::PlanRow::Term { tag, row, place, .. } = row else {
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

fn card(t: &Trial, i: usize) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let tile = r.io.tiles.get(i)?;
    let mut checks = Vec::new();
    if tile.checks > 0 {
        checks.push((format!("{} check{} on its channels.", tile.checks, if tile.checks == 1 { "" } else { "s" }), Tone::Error));
    }
    if !tile.drawn {
        checks.push(("Not drawn yet.".to_string(), Tone::Warning));
    }
    let used = tile.squares.iter().filter(|s| **s != ecad_cadcraft::plc::overview::Square::Spare).count();
    let go = tile.pages.first().map(|p| ("Page".to_string(), Go::Page(*p, tile.card())));
    Some(Detail {
        title: tile.tag.clone(),
        provisional: false,
        subtitle: format!("{} \u{b7} {}", tile.what, tile.head),
        go,
        checks,
        editor: false,
        sections: vec![
            Section { title: "Identity", props: vec![read("Card", tile.what.clone()), read("Slot", tile.head.clone())], ..Section::default() },
            Section {
                title: "Addresses",
                props: vec![read("Range", tile.range.clone()), read("In use", format!("{used} of {}", tile.squares.len()))],
                ..Section::default()
            },
            Section {
                title: "Detailed page",
                lines: tile.place.iter().map(|p| ("First drawn at".to_string(), p.clone())).collect(),
                hint: format!("{} page{}", tile.pages.len(), if tile.pages.len() == 1 { "" } else { "s" }),
                note: Some("A card can be drawn across several pages. Each channel remembers which."),
                ..Section::default()
            },
        ],
    })
}

fn wire(t: &Trial, k: usize) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let v = &r.wires;
    let rows = &v.source.rows;
    let number = rows.text(k, "Wire number");
    let (from, to, page) = (rows.text(k, "From"), rows.text(k, "To"), rows.text(k, "Page"));
    let checks = rows.rows.get(k).map(|r| r.iter().filter_map(|c| c.flag.clone()).map(|f| (f, Tone::Warning)).collect()).unwrap_or_default();
    Some(Detail {
        title: number.clone(),
        provisional: v.source.provisional(k),
        subtitle: format!("Wire \u{b7} {from} to {to} \u{b7} {page}"),
        go: v.gos.get(k).copied().flatten().map(|(pg, at)| (page.clone(), Go::Place(pg, at))),
        checks,
        editor: false,
        sections: vec![
            Section {
                title: "Identity",
                props: vec![
                    read("Number", nonempty(&number)),
                    read("Potential", nonempty(&rows.text(k, "Potential"))),
                    read("From", nonempty(&from)),
                    read("To", nonempty(&to)),
                ],
                ..Section::default()
            },
            Section {
                title: "Made as",
                hint: "from the category".into(),
                props: vec![
                    read("Category", nonempty(&rows.text(k, "Category"))),
                    read("Colour", nonempty(&rows.text(k, "Colour"))),
                    read("Size", nonempty(&rows.text(k, "Cross-section"))),
                    read("Cable", nonempty(&rows.text(k, "Cable"))),
                ],
                ..Section::default()
            },
            Section {
                title: "Ends",
                props: vec![later("Ferrules", CABLE_MANAGER_COMES), later("Marker", CABLE_MANAGER_COMES), later("Length", CABLE_MANAGER_COMES)],
                ..Section::default()
            },
        ],
    })
}

fn cable(t: &Trial, k: usize) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let v = &r.cables;
    let rows = &v.source.rows;
    let tag = rows.text(k, "Tag");
    let (from, to) = (rows.text(k, "From"), rows.text(k, "To"));
    let mut checks: Vec<(String, Tone)> =
        rows.rows.get(k).map(|r| r.iter().filter_map(|c| c.flag.clone()).map(|f| (f, Tone::Error)).collect()).unwrap_or_default();
    let spare = rows.text(k, "Spare");
    if spare.parse::<u32>().is_ok_and(|n| n > 0) {
        checks.push((format!("{spare} spare core{}.", if spare == "1" { "" } else { "s" }), Tone::Neutral));
    }
    let length = rows.text(k, "Length");
    Some(Detail {
        title: tag.clone(),
        provisional: false,
        subtitle: format!("{} to {}", nonempty(&rows.text(k, "From location")), nonempty(&rows.text(k, "To location"))),
        go: v.gos.get(k).copied().flatten().map(|(pg, at)| ("Page".to_string(), Go::Place(pg, at))),
        checks,
        editor: false,
        sections: vec![
            Section {
                title: "Identity",
                props: vec![
                    read("Tag", nonempty(&tag)),
                    read("Runs", format!("{} \u{2192} {}", nonempty(&rows.text(k, "From location")), nonempty(&rows.text(k, "To location")))),
                ],
                ..Section::default()
            },
            Section {
                title: "Part",
                hint: "sets the cores".into(),
                props: vec![
                    read("Type", nonempty(&rows.text(k, "Type"))),
                    read("Cores", format!("{} \u{b7} {} used", nonempty(&rows.text(k, "Cores")), nonempty(&rows.text(k, "Used")))),
                    read("Maker", nonempty(&rows.text(k, "Maker"))),
                    read("Length", nonempty(&length)),
                ],
                ..Section::default()
            },
            Section {
                title: "Ends",
                props: vec![
                    read("Cabinet", nonempty(&from)),
                    read("Field", nonempty(&to)),
                    later("Glands", CABLE_MANAGER_COMES),
                    later("Shield", CABLE_MANAGER_COMES),
                ],
                ..Section::default()
            },
        ],
    })
}

fn report(t: &Trial, k: usize) -> Option<Detail> {
    let rep = super::reports::REPORTS.get(k)?;
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
        sections: vec![
            Section {
                title: "Contents",
                props: vec![read("Reads", rep.reads), read("Group by", rep.grouped), later("Merge", super::workspace::REPORTS_COME)],
                ..Section::default()
            },
            Section {
                title: "Columns",
                hint: "as the preview shows them".into(),
                lines: columns,
                later: vec![("Choose columns", super::workspace::REPORTS_COME)],
                ..Section::default()
            },
            Section {
                title: "Output",
                props: vec![
                    later("In the book", super::workspace::REPORTS_COME),
                    later("Layout", super::workspace::REPORTS_COME),
                    later("Updates", super::workspace::REPORTS_COME),
                ],
                note: Some("Export CSV from the preview."),
                ..Section::default()
            },
        ],
    })
}

fn detail(t: &Trial) -> Option<Detail> {
    match selected_key(t)? {
        Selected::Device(c) => device(t, c),
        Selected::Terminal(k) => terminal(t, k),
        Selected::Card(i) => card(t, i),
        Selected::Wire(k) => wire(t, k),
        Selected::Cable(k) => cable(t, k),
        Selected::Report(k) => report(t, k),
    }
}

/// One value row: the label, the value (boxed when set here), its hint at the right.
fn prop_row(ui: &mut egui::Ui, p: &Prop) {
    let s = ecad_ui::SHELL;
    let h = ecad_ui::tokens::size::CONTROL;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h + space::XS), egui::Sense::hover());
    let painter = ui.painter();
    let label_w = 96.0;
    let small = ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL);
    let body = ecad_ui::fonts::font(ui.ctx(), ui_type::BODY);
    painter.text(pos2(rect.left(), rect.center().y), egui::Align2::LEFT_CENTER, p.label, small.clone(), ecad_ui::color(s.text_2));
    let value = Rect::from_min_max(pos2(rect.left() + label_w, rect.top() + space::XXS), pos2(rect.right(), rect.bottom() - space::XXS));
    if p.set {
        painter.rect_filled(value, egui::CornerRadius::same(radius::FIELD as u8), ecad_ui::color(s.raised));
    }
    let ink = if p.later.is_some() {
        s.text_3
    } else if p.set {
        s.text
    } else {
        s.text_2
    };
    let hint_w = if p.hint.is_empty() {
        0.0
    } else {
        let g = painter.layout_no_wrap(p.hint.clone(), small.clone(), ecad_ui::color(s.text_3));
        let w = g.size().x;
        painter.galley(pos2(value.right() - space::M - w, value.center().y - g.size().y / 2.0), g, ecad_ui::color(s.text_3));
        w + space::M
    };
    let room = (value.width() - 2.0 * space::M - hint_w).max(0.0);
    let g = ecad_ui::table::cells::fitted(ui, &p.value, ui_type::BODY, ecad_ui::color(ink), room);
    let _ = body;
    ui.painter().galley(pos2(value.left() + space::M, value.center().y - g.size().y / 2.0), g, ecad_ui::color(ink));
    let label = format!("{}: {}", p.label, p.value);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
    if let Some(why) = p.later {
        resp.on_hover_text(why);
    }
}

/// A section's head: its title and fold mark, its hint at the right. Returns whether it is open.
fn section_head(ui: &mut egui::Ui, id: egui::Id, title: &str, hint: &str) -> bool {
    let s = ecad_ui::SHELL;
    let open_id = id.with("open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, title));
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(open_id, open));
    }
    let icon = Rect::from_center_size(pos2(rect.left() + 6.0, rect.center().y), vec2(12.0, 12.0));
    ecad_ui::icons::paint(ui.painter(), icon, if open { ecad_ui::Icon::ChevronDown } else { ecad_ui::Icon::ChevronRight }, ecad_ui::color(s.text_2));
    let font = ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS);
    ui.painter().text(pos2(icon.right() + space::S, rect.center().y), egui::Align2::LEFT_CENTER, title, font, ecad_ui::color(s.text));
    if !hint.is_empty() {
        let small = ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL);
        ui.painter().text(pos2(rect.right(), rect.center().y), egui::Align2::RIGHT_CENTER, hint, small, ecad_ui::color(s.text_3));
    }
    open
}

fn rule(ui: &mut egui::Ui) {
    let s = ecad_ui::SHELL;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), space::M), egui::Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)));
}

/// What the panel asks the trial for.
#[derive(Debug, Clone, PartialEq)]
enum Out {
    Go(Go),
    Panel(crate::project_panel::Action),
}

/// Draw the right panel for the Project workspace in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    let Some(d) = detail(t) else {
        crate::shell::in_rect(ui, rect.shrink(space::L), "ecad.project.detail.none", |ui| {
            ui.label(egui::RichText::new(NOTHING).size(ui_type::BODY.size).color(ecad_ui::color(s.text_3)));
        });
        return;
    };
    // Not while a revision is open: its edits would write to the live project behind it.
    let writable = t.project_writable();
    let mut out = Vec::new();
    let devices = &mut t.panel.devices;
    crate::shell::in_rect(ui, rect.shrink2(vec2(space::L, space::M)), "ecad.project.detail", |ui| {
        egui::ScrollArea::vertical().id_salt("ecad.project.detail.scroll").auto_shrink([false, false]).show(ui, |ui| {
            // The head: the name, where it is, the go-to.
            ui.horizontal(|ui| {
                let title_ink = if d.provisional { s.provisional } else { s.text };
                ui.label(egui::RichText::new(&d.title).font(ecad_ui::fonts::font(ui.ctx(), ui_type::OBJECT)).color(ecad_ui::color(title_ink)));
                if let Some((words, go)) = &d.go {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(prim::Button::new(&format!("\u{2192} {words}")).small()).clicked() {
                            out.push(Out::Go(go.clone()));
                        }
                    });
                }
            });
            ui.label(egui::RichText::new(&d.subtitle).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_2)));
            ui.add_space(space::S);
            // Its own checks.
            if d.checks.is_empty() {
                prim::pill(ui, &format!("\u{2713} No checks on {}", d.title), Tone::Ok);
            } else {
                for (c, tone) in &d.checks {
                    prim::pill(ui, c, *tone);
                }
            }
            ui.add_space(space::S);
            rule(ui);
            if d.editor {
                let id = egui::Id::unique("ecad.project.detail.properties");
                if section_head(ui, id, "Identity", "set here") {
                    out.extend(devices.show_editor(ui, writable).into_iter().map(Out::Panel));
                }
                rule(ui);
            }
            for sec in &d.sections {
                let id = egui::Id::unique(("ecad.project.detail", sec.title));
                if section_head(ui, id, sec.title, &sec.hint) {
                    for p in &sec.props {
                        prop_row(ui, p);
                    }
                    for (a, b) in &sec.lines {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(a).size(ui_type::BODY.size).color(ecad_ui::color(s.text)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(b).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_2)));
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
                        ui.add(egui::Label::new(egui::RichText::new(n).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_3))).wrap());
                    }
                }
                rule(ui);
            }
        });
    });
    for o in out {
        match o {
            Out::Go(Go::Symbols(reps)) => {
                let _ = t.goto_symbols(&reps);
            }
            Out::Go(Go::Place(pg, at)) => t.goto_place(pg, at),
            Out::Go(Go::Page(pg, card)) => t.plc_action(super::ListAction::GoToCard(pg, card)),
            Out::Panel(a) => t.panel_action(a),
        }
    }
}
