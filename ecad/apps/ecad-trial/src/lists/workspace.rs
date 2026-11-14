//! UI rev 3's Project workspace (was Lists; `docs/discussions/ui-2026-11/rev3-build-spec.md`,
//! "Project workspace", boards 2.01–2.06): the tabs in the tool strip's order (Devices,
//! Terminals, PLC | Wires, Cables | Reports), each a full-width table under one head (the tabs
//! with their counts at the left; filter, Import, Export and "Open beside the page" at the right)
//! and over one foot (what the colours mean, and the tab's counts). The right panel shows only the
//! selected item ([`super::detail`]).
//!
//! UI only: every view is an existing one (039's tables, 051's PLC, 024's reports) laid out to the
//! boards. What is not built yet is a placeholder that says so: Import (no importer), the cable
//! drawing, ferrules, lengths and glands (the cable manager), and the report generator's settings.
//!
//! **Update map.** Each tab's view keeps its own (named in its module); the head's counts are
//! taken with Reports' tab counts, only when the report models' revision changed. The workspace
//! itself reads nothing from the project.

use ecad_ui::prim;
use ecad_ui::tokens::{line, radius, space, ui_type};
use eframe::egui::{self, Rect, Role, Sense, Stroke, StrokeKind, WidgetInfo, pos2, vec2};

use crate::Trial;
use crate::shell::{ListsView, in_rect};

/// The head's height (the tab strip and the actions), px.
pub const HEAD: f32 = 48.0;
/// The foot's height, px.
pub const FOOT: f32 = 32.0;
/// A tab's height in the strip, px.
pub const TAB_H: f32 = 30.0;

/// The tabs, in the tool strip's order; a divider after PLC and after Cables (board 2.01).
pub const TABS: [ListsView; 6] = [ListsView::Devices, ListsView::Terminals, ListsView::Plc, ListsView::Wires, ListsView::Cables, ListsView::Reports];

/// The head's buttons.
pub const IMPORT: &str = "Import";
pub const IMPORT_LIST: &str = "Import a list";
pub const IMPORT_IO: &str = "Import an I/O list";
pub const EXPORT: &str = "Export";
pub const OPEN_BESIDE: &str = "Open beside the page";
pub const CLOSE_BESIDE: &str = "Close the page beside";
pub const RENUMBER: &str = "Renumber\u{2026}";
pub const REGENERATE: &str = "Regenerate all";
pub const NEW_REPORT: &str = "+ New report";
/// Why Import is greyed (no importer is built yet).
pub const IMPORT_COMES: &str = "Importing a list comes later.";
/// Why the report generator's buttons are greyed (brief 2.9 is not built yet).
pub const REPORTS_COME: &str = "The report generator comes later.";

/// The foot's legend words.
pub const LEGEND_INPUT: &str = "you set it here";
pub const LEGEND_READ: &str = "read from the drawing, go to its page to change it";

/// The head, the body and the foot of `main`.
pub fn layout(main: Rect) -> (Rect, Rect, Rect) {
    let head = Rect::from_min_size(main.min, vec2(main.width(), HEAD));
    let foot = Rect::from_min_max(pos2(main.left(), (main.bottom() - FOOT).max(head.bottom())), main.max);
    let body = Rect::from_min_max(pos2(main.left(), head.bottom()), pos2(main.right(), foot.top()));
    (head, body, foot)
}

/// Where the Reports tab hosts today's Reports window: its preview, under the list of reports.
pub fn reports_preview(main: Rect) -> Rect {
    let (_, body, _) = layout(main);
    let top = (body.top() + super::reports::LIST_H).min(body.bottom());
    Rect::from_min_max(pos2(body.left(), top), body.max)
}

/// The tab strip: each tab's name and count, the selected one raised; returns the tab clicked.
fn tab_strip(ui: &mut egui::Ui, shown: ListsView, counts: &[(ListsView, Option<usize>)]) -> Option<ListsView> {
    let s = ecad_ui::SHELL;
    let mut clicked = None;
    let font = ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS);
    // Lay the tabs out first, to draw the frame round them.
    let pad = space::L;
    let mut items = Vec::new();
    for v in TABS {
        let n = counts.iter().find(|(x, _)| *x == v).and_then(|(_, n)| *n);
        let text = match n {
            Some(n) => format!("{} {n}", v.label()),
            None => v.label().to_string(),
        };
        let g = ui.painter().layout_no_wrap(text, font.clone(), ecad_ui::color(s.text));
        items.push((v, g));
    }
    let gap = space::XS;
    let dividers = 2.0 * (space::M + line::HAIRLINE_PX);
    let w: f32 = items.iter().map(|(_, g)| g.size().x + 2.0 * pad).sum::<f32>() + gap * (TABS.len() as f32 - 1.0) + dividers + 2.0 * space::XS;
    let (frame, _) = ui.allocate_exact_size(vec2(w, TAB_H + 2.0 * space::XS), Sense::hover());
    let p = ui.painter().clone();
    p.rect_stroke(frame, ecad_ui::style::corner(radius::BUTTON), Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)), StrokeKind::Inside);
    let mut x = frame.left() + space::XS;
    for (v, g) in items {
        let r = Rect::from_min_size(pos2(x, frame.top() + space::XS), vec2(g.size().x + 2.0 * pad, TAB_H));
        let resp = ui.interact(r, egui::Id::unique(("ecad.project.tab", v.label())), Sense::click());
        let on = v == shown;
        resp.widget_info(|| WidgetInfo::selected(Role::Tab, true, on, v.label()));
        let ground = if on {
            Some(s.raised)
        } else if resp.hovered() {
            Some(s.hover)
        } else {
            None
        };
        if let Some(c) = ground {
            p.rect_filled(r, ecad_ui::style::corner(radius::CHIP), ecad_ui::color(c));
        }
        let ink = if on { s.text } else { s.text_2 };
        p.galley(pos2(r.left() + pad, r.center().y - g.size().y / 2.0), g, ecad_ui::color(ink));
        if resp.clicked() {
            clicked = Some(v);
        }
        x = r.right() + gap;
        if matches!(v, ListsView::Plc | ListsView::Cables) {
            let dx = x + space::M / 2.0 - gap / 2.0;
            p.vline(dx, frame.y_range().shrink(space::M), Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line_strong)));
            x += space::M + line::HAIRLINE_PX;
        }
    }
    clicked
}

/// A greyed placeholder button: what it will do, and why it does nothing yet.
fn placeholder(ui: &mut egui::Ui, label: &str, why: &str) {
    ui.add(prim::Button::new(label).small().disabled(why));
}

/// A quiet button.
fn button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(prim::Button::new(label).small()).clicked()
}

/// The foot: the legend at the left (an input box is set here; grey text is read), the tab's
/// counts at the right, a warning in its tone.
fn foot(ui: &mut egui::Ui, legend: bool, read: &str, counts: &[(String, prim::Tone)]) {
    let s = ecad_ui::SHELL;
    ui.horizontal_centered(|ui| {
        ui.add_space(space::L);
        if legend {
            let (r, _) = ui.allocate_exact_size(vec2(44.0, 20.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, ecad_ui::style::corner(radius::CHIP), ecad_ui::color(s.raised));
            p.rect_stroke(r, ecad_ui::style::corner(radius::CHIP), Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line_strong)), StrokeKind::Inside);
            p.text(r.center(), egui::Align2::CENTER_CENTER, "input", ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL), ecad_ui::color(s.text));
            ui.label(egui::RichText::new(LEGEND_INPUT).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_2)));
            ui.add_space(space::L);
        }
        ui.label(egui::RichText::new("grey text").size(ui_type::SMALL.size).color(ecad_ui::color(s.text_3)));
        ui.label(egui::RichText::new(read).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_2)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(space::L);
            for (i, (text, tone)) in counts.iter().enumerate().rev() {
                let ink = if *tone == prim::Tone::Neutral { s.text_2 } else { tone.text() };
                let sep = if i == 0 { "" } else { " \u{b7} " };
                ui.label(egui::RichText::new(format!("{sep}{text}")).size(ui_type::SMALL.size).color(ecad_ui::color(ink)));
            }
        });
    });
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Trial {
    /// Whether the project can be written now (fixes and edits are offered).
    pub(crate) fn project_writable(&mut self) -> bool {
        let h = self.header();
        h.name.is_some() && !h.read_only && self.panel.revision.is_none()
    }

    /// Show `v` in the Project workspace, keeping the view it reads in step.
    pub fn show_project_tab(&mut self, v: ListsView) {
        self.shell.lists = v;
    }

    /// The tabs' counts for the strip (`None`: not known yet).
    fn project_counts(&self) -> Vec<(ListsView, Option<usize>)> {
        let devices = Some(self.panel.devices.list.entries.len());
        let t = self.reports.as_ref().map(|r| r.tab_counts);
        vec![
            (ListsView::Devices, devices),
            (ListsView::Terminals, t.map(|c| c[0])),
            (ListsView::Plc, t.map(|c| c[1])),
            (ListsView::Wires, t.map(|c| c[2])),
            (ListsView::Cables, t.map(|c| c[3])),
            (ListsView::Reports, Some(super::reports::REPORTS.len())),
        ]
    }

    /// The Project workspace in `main` (UI rev 3, boards 2.01–2.06).
    pub(crate) fn project_view(&mut self, ui: &mut egui::Ui, main: Rect) {
        let s = ecad_ui::SHELL;
        let (head, body, foot_r) = layout(main);
        ui.painter().rect_filled(main, 0.0, ecad_ui::color(s.bg));
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)));
        ui.painter().hline(foot_r.x_range(), foot_r.top() + 0.5, Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)));
        let open = self.project_open();
        if open {
            // The views below read Reports' kept models.
            self.open_reports();
        }
        let tab = self.shell.lists;
        if let Some(r) = self.reports.as_mut() {
            r.io_shown = tab == ListsView::Plc;
            r.wires_shown = tab == ListsView::Wires;
            r.cables_shown = tab == ListsView::Cables;
            if tab == ListsView::Terminals && (r.tab != crate::outputs_ui::Tab::Terminals || r.strip.is_some()) {
                r.tab = crate::outputs_ui::Tab::Terminals;
                r.strip = None;
                r.invalidate();
            }
        }
        let counts = self.project_counts();
        let companion = self.shell.state.companion.is_some();
        let clicked = in_rect(ui, head.shrink2(vec2(space::L, 0.0)), "ecad.project.head", |ui| {
            ui.horizontal_centered(|ui| {
                let clicked = tab_strip(ui, tab, &counts);
                let acts = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.head_actions(ui, tab, companion)).inner;
                (clicked, acts)
            })
            .inner
        });
        let (picked, head_acts) = clicked;
        if let Some(v) = picked {
            self.show_project_tab(v);
        }
        for a in head_acts {
            self.head_action(ui.ctx(), a);
        }
        if !open {
            return;
        }
        let writable = self.project_writable();
        let tab = self.shell.lists;
        let body_in = body.shrink2(vec2(space::XS, 0.0));
        match tab {
            ListsView::Devices => {
                let acts = in_rect(ui, body_in, "ecad.project.devices", |ui| self.panel.devices.show_table(ui, writable, false));
                for a in acts {
                    self.panel_action(a);
                }
            }
            ListsView::Checks => {
                let acts = in_rect(ui, body_in, "ecad.project.checks", |ui| self.panel.checks.show(ui, writable));
                for a in acts {
                    self.panel_action(a);
                }
            }
            ListsView::Terminals => {
                let Some(r) = self.reports.as_mut() else { return };
                r.terminals.table.toolbar = false;
                let acts = in_rect(ui, body_in, "ecad.project.terminals", |ui| r.terminals.show(ui, writable));
                self.list_actions(acts);
            }
            ListsView::Plc => {
                let Some(r) = self.reports.as_mut() else { return };
                r.io.table.toolbar = false;
                let acts =
                    in_rect(ui, body_in, "ecad.project.plc", |ui| r.io.show(ui).into_iter().filter_map(|a| r.io.handle(a)).collect::<Vec<_>>());
                self.list_actions(acts);
            }
            ListsView::Wires => {
                let Some(r) = self.reports.as_mut() else { return };
                let go = in_rect(ui, body_in, "ecad.project.wires", |ui| r.wires.show(ui));
                if let Some((pg, at)) = go {
                    self.goto_place(pg, at);
                }
            }
            ListsView::Cables => {
                let go = in_rect(ui, body_in, "ecad.project.cables", |ui| super::cables::show_tab(self, ui));
                if let Some((pg, at)) = go {
                    self.goto_place(pg, at);
                }
            }
            ListsView::Reports => {
                in_rect(ui, body_in, "ecad.project.reports", |ui| super::reports::show(self, ui));
            }
        }
        // Selecting a row opens the right panel on it (boards 2.01–2.06 always show it).
        if self.project_selection_changed() {
            self.shell.state.right_collapsed = false;
            if self.shell.state.right_tab == crate::shell::RightTab::Layout {
                self.shell.state.right_tab = crate::shell::RightTab::Device;
            }
        }
        let (legend, read, counts) = self.project_foot(tab);
        in_rect(ui, foot_r, "ecad.project.foot", |ui| foot(ui, legend, read, &counts));
    }

    /// Whether the selected row changed since the last frame (the right panel then opens on it).
    fn project_selection_changed(&mut self) -> bool {
        let now = super::detail::selected_key(self);
        let changed = now.is_some() && now != self.shell.project_selected;
        self.shell.project_selected = now;
        changed
    }

    /// The head's actions at the right (laid out right to left).
    fn head_actions(&mut self, ui: &mut egui::Ui, tab: ListsView, companion: bool) -> Vec<HeadAction> {
        let mut out = Vec::new();
        if tab == ListsView::Reports {
            placeholder(ui, NEW_REPORT, REPORTS_COME);
            placeholder(ui, REGENERATE, REPORTS_COME);
            return out;
        }
        if tab != ListsView::Wires {
            let beside = if companion { CLOSE_BESIDE } else { OPEN_BESIDE };
            if button(ui, beside) {
                out.push(HeadAction::Beside);
            }
        }
        if button(ui, EXPORT) {
            out.push(HeadAction::Export);
        }
        match tab {
            ListsView::Wires => placeholder(ui, RENUMBER, "Renumbering wires comes later; renumber on the page."),
            ListsView::Devices => placeholder(ui, IMPORT_LIST, IMPORT_COMES),
            ListsView::Plc => placeholder(ui, IMPORT_IO, IMPORT_COMES),
            _ => placeholder(ui, IMPORT, IMPORT_COMES),
        }
        ui.add_space(space::S);
        // The filter, bound to the tab's table.
        let (placeholder_text, w) = match tab {
            ListsView::Devices => ("Filter devices", 200.0),
            ListsView::Terminals => ("Filter terminals", 220.0),
            ListsView::Plc => ("Filter channels", 200.0),
            ListsView::Wires => ("Filter wires", 200.0),
            ListsView::Cables => ("Filter cables", 220.0),
            _ => ("Filter", 200.0),
        };
        match tab {
            ListsView::Devices => {
                let d = &mut self.panel.devices;
                prim::Field::new(&mut d.filter, prim::FieldKind::Search, crate::devices_panel::FILTER)
                    .placeholder(placeholder_text)
                    .width(w)
                    .show(ui);
                // Kept from 039: the chips and the grouping (the board has no room for them, so
                // they sit beside the filter).
                d.table.group_menu(ui, &d.source);
                d.table.chip_row(ui, &d.source);
            }
            ListsView::Checks => {}
            _ => {
                if let Some(r) = self.reports.as_mut() {
                    match tab {
                        ListsView::Terminals => {
                            r.terminals.table.filter_field(ui, placeholder_text, w);
                        }
                        ListsView::Plc => {
                            r.io.table.filter_field(ui, placeholder_text, w);
                        }
                        ListsView::Wires => {
                            r.wires.table.filter_field(ui, placeholder_text, w);
                            ui.add_space(space::S);
                            let mut g = r.wires.group;
                            prim::segmented(ui, &mut g, &super::wires::GROUPS);
                            if g != r.wires.group {
                                r.wires.set_group(g);
                            }
                        }
                        ListsView::Cables => {
                            r.cables.table.filter_field(ui, placeholder_text, w);
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }

    fn head_action(&mut self, ctx: &egui::Context, a: HeadAction) {
        match a {
            HeadAction::Beside => {
                self.shell.state.companion = match self.shell.state.companion {
                    Some(_) => None,
                    None => Some(crate::shell::state::Companion::default()),
                };
            }
            HeadAction::Export => {
                if let Some((csv, n)) = self.project_csv() {
                    ctx.copy_text(csv);
                    self.cad.session.echo(format!("Copied {} as CSV.", plural(n, "row", "rows")));
                }
            }
        }
    }

    /// The shown tab's rows as CSV, and how many.
    fn project_csv(&self) -> Option<(String, usize)> {
        let r = self.reports.as_ref();
        let n = |s: &str| s.lines().count().saturating_sub(1);
        let csv = match self.shell.lists {
            ListsView::Devices => {
                let d = &self.panel.devices;
                d.table.csv(&d.source)
            }
            ListsView::Checks => return None,
            ListsView::Terminals => r.map(|r| r.terminals.table.csv(&r.terminals.source))?,
            ListsView::Plc => r.map(|r| r.io.table.csv(&r.io.source))?,
            ListsView::Wires => r.map(|r| r.wires.table.csv(&r.wires.source))?,
            ListsView::Cables => r.map(|r| r.cables.table.csv(&r.cables.source))?,
            ListsView::Reports => return None,
        };
        let k = n(&csv);
        Some((csv, k))
    }

    /// What a list view asks for (Terminals and PLC).
    fn list_actions(&mut self, acts: Vec<super::ListAction>) {
        for a in acts {
            match a {
                super::ListAction::Panel(a) => self.panel_action(a),
                super::ListAction::Run(line) => {
                    let refusal = self.run_line(&line);
                    if let Some(r) = self.reports.as_mut() {
                        r.ran(refusal);
                    }
                }
                super::ListAction::GoTo(page, at) => self.goto_place(page, at),
                other => self.plc_action(other),
            }
        }
    }

    /// Brief 051: what Project ▸ PLC asks the trial for (design 051 §3.4, §3.5).
    pub fn plc_action(&mut self, a: super::ListAction) {
        use super::ListAction;
        match a {
            ListAction::Say(t) => self.cad.session.echo(t),
            ListAction::GoToCard(page, card) => {
                // The card's first drawing on that page, else the page.
                let at = match (self.project.clone(), card) {
                    (Some(p), Some(c)) => {
                        p.read(&mut self.cad.session, |p, _| ecad_cadcraft::plc::overview::at_on(p.ledger.document(), c, page)).ok().flatten()
                    }
                    _ => None,
                };
                match at {
                    Some(at) => self.goto_place(page, at),
                    None => {
                        let _ = self.show_page(page);
                    }
                }
            }
            ListAction::ChecksOf(c) => {
                // UI rev 3: the Check list has no Project tab; the card's checks show in the right
                // panel's Checks, filtered to it.
                let tag = self
                    .project
                    .clone()
                    .and_then(|p| p.read(&mut self.cad.session, |p, _| ecad_cadcraft::devices::tag_words(p.ledger.document(), c)).ok());
                if let Some(t) = tag {
                    self.panel.checks.table.query = t;
                }
                self.open_inspector(crate::shell::RightTab::Checks);
            }
            ListAction::PlaceCard(slot) => {
                if self.cad.session.running.is_some() {
                    self.cad.session.cancel();
                }
                match self.cad.session.start("place") {
                    Ok(()) => self.cad.session.echo(format!("Search \"card\" and pick a card body: it goes in the next free slot ({slot}).")),
                    Err(e) => self.cad.session.echo(e.to_string()),
                }
                self.focus_drawing = true;
                self.after_logic();
            }
            _ => {}
        }
    }

    /// The foot of `tab`: whether the input legend shows, what grey text means, and the counts.
    fn project_foot(&self, tab: ListsView) -> (bool, &'static str, Vec<(String, prim::Tone)>) {
        use prim::Tone;
        let r = self.reports.as_ref();
        match tab {
            ListsView::Devices => {
                let d = &self.panel.devices;
                let n = d.list.entries.len();
                let no_part = d.list.entries.iter().filter(|e| e.properties.get(3).is_none_or(|p| p.as_deref().is_none_or(str::is_empty))).count();
                let mut v = vec![(plural(n, "device", "devices"), Tone::Neutral)];
                if no_part > 0 {
                    v.push((format!("{no_part} without a part"), Tone::Warning));
                }
                (true, "comes from the part set, the symbol or the page", v)
            }
            ListsView::Terminals => {
                let n = r.map_or(0, |r| r.tab_counts[0]);
                let strips = r.map_or(0, |r| r.strips.len());
                (true, LEGEND_READ, vec![(format!("{} on {}", plural(n, "terminal", "terminals"), plural(strips, "strip", "strips")), Tone::Neutral)])
            }
            ListsView::Plc => {
                let words = r.map(|r| r.io.totals.clone()).unwrap_or_default();
                (true, "read from the drawing", vec![(words, Tone::Neutral)])
            }
            ListsView::Wires => {
                let (n, prov, flagged) = r.map_or((0, 0, 0), |r| r.wires.counts());
                let mut v = vec![(plural(n, "wire", "wires"), Tone::Neutral)];
                if prov > 0 {
                    v.push((plural(prov, "provisional number", "provisional numbers"), Tone::Provisional));
                }
                if flagged > 0 {
                    v.push((format!("{flagged} with a check"), Tone::Warning));
                }
                (false, LEGEND_READ, v)
            }
            ListsView::Cables => {
                let (n, flagged, no_length) = r.map_or((0, 0, 0), |r| r.cables.counts());
                let mut v = vec![(plural(n, "cable", "cables"), Tone::Neutral)];
                if flagged > 0 {
                    v.push((format!("{flagged} with a check"), Tone::Error));
                }
                if no_length > 0 {
                    v.push((format!("{no_length} without a length"), Tone::Warning));
                }
                (false, LEGEND_READ, v)
            }
            ListsView::Reports | ListsView::Checks => (false, "read from the drawing", Vec::new()),
        }
    }
}

/// What the head asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadAction {
    Beside,
    Export,
}

/// The tabs' labels, for tests.
pub fn tab_labels() -> Vec<&'static str> {
    TABS.iter().map(|v| v.label()).collect()
}
