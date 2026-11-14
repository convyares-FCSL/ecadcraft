//! Settings ▸ Project info (UI rev 3, board 5.01) and Settings ▸ Pages and sheets: the Project
//! info and Project settings popouts (design 019 §7.1, split by the owner; brief 042), laid out as
//! rev 3's cards. Their copy is the trial's `project_props`, filled from `projectprops {}` when the
//! section is shown; Save (OK) writes the changed fields as before (one transaction per line) and
//! closes Settings, Cancel writes nothing.
//!
//! Project info shows the fields that exist today (title, drawing number, customer, status, the
//! default function and location, and the free properties). People and approval, each function's
//! and location's description and count, where each title block field is filled from, the
//! customer logo, revision letters and the project's language and units are placeholders: they
//! have no data yet.

use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Rect, pos2, vec2};

use super::{card, dashed_rect, later_box, later_row, row};
use crate::Trial;
use crate::sheets_ui::{Out, Part, Popout};
use crate::shell::bar;

/// Pages and sheets' two tabs.
pub const TABS: [&str; 2] = ["Sheets and numbering", "Text styles"];
/// Said where a project is needed.
pub const NO_PROJECT: &str = "No project is open.";
/// Save and Cancel (042's popout names).
pub const OK: &str = "ecad.projectprops.ok";
pub const CANCEL: &str = "ecad.projectprops.cancel";
/// The title block preview (board 5.01).
pub const TITLE_BLOCK: &str = "settings.project.title_block";

fn grey(text: &str) -> egui::RichText {
    egui::RichText::new(text).color(ecad_ui::color(ecad_ui::SHELL.text_2))
}

/// Settings ▸ Project info.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    if !t.project_open() {
        ecad_ui::prim::read_value(ui, NO_PROJECT);
        return;
    }
    if t.project_props.as_ref().is_none_or(|d| d.popout != Popout::Info) {
        t.fill_project_props(Popout::Info);
    }
    let name = t.header().name.unwrap_or_default();
    let page = t.panel.active.and_then(|a| t.panel.rows().iter().find(|r| r.id == a)).map(|r| (r.number.clone(), r.title.clone()));
    let pages = t.panel.rows().len();
    let sheet = match t.shell.state.sheet {
        crate::canvas::Sheet::Paper => "Paper",
        crate::canvas::Sheet::Dark => "Dark",
    };
    let Some(d) = &mut t.project_props else { return };
    let ro = d.read_only;
    let mut out = None;
    let full = ui.max_rect();
    let foot_h = 44.0;
    let body = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - foot_h));
    let gap = space::XL;
    let left_w = ((body.width() - gap) * 0.5).floor();
    let left = Rect::from_min_max(body.min, pos2(body.left() + left_w, body.bottom()));
    let right = Rect::from_min_max(pos2(left.right() + gap, body.top()), body.max);
    super::in_rect(ui, left, "settings.project.left", |ui| {
        egui::ScrollArea::vertical().id_salt("settings.project.left.scroll").auto_shrink([false, false]).show(ui, |ui| {
            card(
                ui,
                "Project",
                |_| {},
                |ui| {
                    row(ui, "Name", |ui| ui.label(grey(&name)));
                    for (k, label) in [("title", "Title"), ("drawing_number", "Drawing no."), ("customer", "Customer"), ("status", "Status")] {
                        let v = d.fields.entry(k.to_string()).or_default();
                        row(ui, label, |ui| {
                            let r = ui.add_enabled(!ro, egui::TextEdit::singleline(v).desired_width(ui.available_width()));
                            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, !ro, format!("ecad.projectprops.{k}")));
                        });
                    }
                    later_row(ui, "Site", "Comes with project details");
                    later_row(ui, "Customer reference", "Optional, prints on the cover sheet");
                    if !d.others.is_empty() || !ro {
                        ui.add_space(space::S);
                        ecad_ui::prim::read_value(ui, "Other properties");
                        let mut remove = None;
                        for (i, (k, v)) in d.others.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add_enabled(!ro, egui::TextEdit::singleline(k).desired_width(110.0));
                                ui.add_enabled(!ro, egui::TextEdit::singleline(v).desired_width((ui.available_width() - 30.0).max(60.0)));
                                if !ro && ui.small_button("×").on_hover_text("Remove").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            d.others.remove(i);
                        }
                        if !ro && ui.add(ecad_ui::prim::Button::new("+ Property").small()).clicked() {
                            d.others.push((String::new(), String::new()));
                        }
                    }
                },
            );
            ui.add_space(space::L);
            card(
                ui,
                "People and approval",
                |_| {},
                |ui| {
                    later_row(ui, "Drawn by", "Comes with people and approval");
                    later_row(ui, "Checked by", "Name, at release");
                    later_row(ui, "Approved by", "Name, at release");
                    later_row(ui, "Release needs", "Nobody · Checked · Checked and approved");
                },
            );
            ui.add_space(space::L);
            card(
                ui,
                "Functions and locations",
                |_| {},
                |ui| {
                    for (k, label) in [("function", "Function, ="), ("location", "Location, +")] {
                        let v = d.fields.entry(k.to_string()).or_default();
                        row(ui, label, |ui| {
                            let r = ui.add_enabled(!ro, egui::TextEdit::singleline(v).desired_width(ui.available_width()));
                            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, !ro, format!("ecad.projectprops.{k}")));
                        });
                    }
                    ui.label(egui::RichText::new("The defaults for new pages.").small().color(ecad_ui::color(ecad_ui::SHELL.text_3)));
                    ui.add_space(space::S);
                    later_box(ui, "Each function and location with its description and device count comes later", 32.0);
                },
            );
        });
    });
    let title = d.fields.get("title").cloned().unwrap_or_default();
    let number = d.fields.get("drawing_number").cloned().unwrap_or_default();
    let function = d.fields.get("function").cloned().unwrap_or_default();
    let location = d.fields.get("location").cloned().unwrap_or_default();
    let revision = d.revision.clone();
    let mut revisions = false;
    super::in_rect(ui, right, "settings.project.right", |ui| {
        egui::ScrollArea::vertical().id_salt("settings.project.right.scroll").auto_shrink([false, false]).show(ui, |ui| {
            card(
                ui,
                "Title block",
                |_| {},
                |ui| {
                    let shown = if title.is_empty() { name.clone() } else { title.clone() };
                    let (n, page_title) = page.clone().unwrap_or_default();
                    let of = if n.is_empty() { String::new() } else { format!("{n}  of {pages}") };
                    title_block(
                        ui,
                        &[("Project", &shown), ("Page title", &page_title), ("Page", &of)],
                        &[("Drawing no.", &number), ("Function", &function), ("Location", &location), ("Drawn", ""), ("Revision", "")],
                    );
                    ui.add_space(space::M);
                    later_row(ui, "Filled from", "Where each field comes from: comes later");
                    later_row(ui, "Customer logo", "Drop an image: comes later");
                },
            );
            ui.add_space(space::L);
            card(
                ui,
                "Revisions",
                |ui| {
                    revisions = ui.add(ecad_ui::prim::Button::new("Revisions…").small()).clicked();
                },
                |ui| {
                    row(ui, "Current", |ui| ui.label(grey(revision.trim_start_matches("Current: "))));
                    later_row(ui, "Revision letters", "A, B, C · 01, 02, 03 · P1, P2 then A");
                    ui.label(
                        egui::RichText::new("The pending mark is set in Pages and sheets.").small().color(ecad_ui::color(ecad_ui::SHELL.text_3)),
                    );
                },
            );
            ui.add_space(space::L);
            card(
                ui,
                "Defaults for this project",
                |_| {},
                |ui| {
                    row(ui, "Sheet", |ui| ui.label(grey("Set in Pages and sheets")));
                    row(ui, "Sheet colour", |ui| ui.label(grey(sheet)));
                    later_row(ui, "Language", "English");
                    later_row(ui, "Units", "mm");
                },
            );
        });
    });
    // The foot: Save and Cancel, as the popout's OK and Cancel.
    let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - foot_h), full.max);
    super::in_rect(ui, foot, "settings.project.foot", |ui| {
        ui.add_space(space::S);
        ui.horizontal(|ui| {
            if let Some(e) = &d.error {
                ui.label(egui::RichText::new(e).small().color(ecad_ui::color(ecad_ui::SHELL.error)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let c = ui.add(ecad_ui::prim::Button::new("Cancel").small());
                c.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CANCEL));
                if c.clicked() {
                    out = Some(Out::Close);
                }
                if !ro {
                    let line = d.line();
                    let ok = ui.add_enabled(line.is_ok(), ecad_ui::prim::Button::new("Save").kind(ecad_ui::prim::Kind::Primary).small());
                    ok.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, line.is_ok(), OK));
                    if ok.clicked() {
                        out = Some(match line {
                            Ok(Some(l)) => Out::Run(l),
                            _ => Out::Close,
                        });
                    }
                }
            });
        });
    });
    if revisions {
        t.open_revisions();
    }
    if let Some(o) = out {
        t.project_props_out(o);
    }
}

/// The title block preview: two rows of cells, label above value, on paper (board 5.01). A cell
/// with no value yet shows a dashed box.
fn title_block(ui: &mut egui::Ui, top: &[(&str, &str)], bottom: &[(&str, &str)]) {
    let sheet = crate::canvas::Sheet::Paper.tokens();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 172.0), egui::Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, TITLE_BLOCK));
    let p = ui.painter();
    p.rect_filled(r, 0.0, ecad_ui::color(sheet.sheet));
    let ink = ecad_ui::color(sheet.ink);
    let dim = ecad_ui::color(sheet.ink_2);
    let st = egui::Stroke::new(1.0, ink);
    p.rect_stroke(r, 0.0, st, egui::StrokeKind::Inside);
    let mid = r.top() + r.height() * 0.5;
    p.hline(r.x_range(), mid, st);
    let label_font = bar::font(ui, ui_type::SMALL);
    let cells = |y0: f32, y1: f32, items: &[(&str, &str)], weights: &[f32], big: f32| {
        let total: f32 = weights.iter().sum();
        let mut x = r.left();
        for (i, ((label, value), w)) in items.iter().zip(weights).enumerate() {
            let cw = r.width() * w / total;
            if i > 0 {
                p.vline(x, y0..=y1, st);
            }
            p.text(pos2(x + space::M, y0 + space::M), egui::Align2::LEFT_TOP, *label, label_font.clone(), dim);
            let cell = Rect::from_min_max(pos2(x, y0), pos2(x + cw, y1));
            if value.is_empty() {
                dashed_rect(p, Rect::from_min_size(pos2(x + space::M, y1 - space::M - 22.0), vec2((cw - 2.0 * space::M).max(10.0), 20.0)), dim);
            } else {
                p.with_clip_rect(cell.shrink(2.0)).text(
                    pos2(x + space::M, y1 - space::M),
                    egui::Align2::LEFT_BOTTOM,
                    *value,
                    egui::FontId::proportional(big),
                    ink,
                );
            }
            x += cw;
        }
    };
    cells(r.top(), mid, top, &[3.0, 3.6, 2.4], 20.0);
    cells(mid, r.bottom(), bottom, &[3.0, 1.5, 1.9, 1.3, 1.3], 18.0);
}

/// Settings ▸ Pages and sheets: the Project settings popout (sheet, references, tags, revisions,
/// numbering, wires) on its first tab, 031's text styles on the second.
pub fn pages(t: &mut Trial, ui: &mut egui::Ui) {
    let text = t.settings.section == super::Section::TextStyles;
    let mut i = usize::from(text);
    ecad_ui::prim::segmented(ui, &mut i, &TABS);
    if i != usize::from(text) {
        t.settings.section = if i == 1 { super::Section::TextStyles } else { super::Section::Pages };
    }
    ui.add_space(space::M);
    ui.set_max_width(900.0);
    if text {
        ecad_ui::prim::read_value(ui, "Each kind of text: its height, weight and font, and whether it is shown. Also listed in Standards.");
        ui.add_space(space::S);
    }
    body(t, ui, Popout::Settings, if text { Part::Text } else { Part::Settings });
}

/// The copy of `popout` drawn as `part`, filled first if there is none of that popout.
pub fn body(t: &mut Trial, ui: &mut egui::Ui, popout: Popout, part: Part) {
    if !t.project_open() {
        ecad_ui::prim::read_value(ui, NO_PROJECT);
        return;
    }
    if t.project_props.as_ref().is_none_or(|d| d.popout != popout) {
        t.fill_project_props(popout);
    }
    let Some(d) = &mut t.project_props else { return };
    if let Some(out) = d.show_in(ui, part) {
        t.project_props_out(out);
    }
}

impl Trial {
    /// Fill the Project info or Project settings copy from `projectprops {}` (reads only).
    pub fn fill_project_props(&mut self, popout: Popout) {
        let read_only = self.header().read_only;
        match self.run_value("projectprops", serde_json::json!({})) {
            Ok(v) => {
                let mut d = crate::sheets_ui::ProjectProps::from_json(&v, read_only, popout);
                if popout == Popout::Settings {
                    // Brief 031: the Text section reads `textstyle {}`; brief 032 §4.4: the wire
                    // label rows read `wiresettings {}` (reads only).
                    d.text = self.run_value("textstyle", serde_json::json!({})).ok().map(|t| crate::text_ui::TextSection::from_json(&t));
                    d.labels = self.run_value("wiresettings", serde_json::json!({})).ok().map(|w| crate::sheets_ui::WireLabels::from_json(&w));
                }
                self.project_props = Some(d);
            }
            Err(e) => self.cad.session.echo(e),
        }
    }

    /// What the Project or Text styles section asked for: OK runs its lines (`projectprops`,
    /// then `textstyle` and `wiresettings`; the first refusal stops and is shown) and closes
    /// Settings; Cancel closes it.
    pub fn project_props_out(&mut self, out: Out) {
        match out {
            Out::Run(lines) => match lines.lines().find_map(|l| self.run_line(l)) {
                None => self.close_settings(),
                Some(e) => {
                    if let Some(d) = &mut self.project_props {
                        d.error = Some(e);
                    }
                }
            },
            Out::Close => self.close_settings(),
            Out::OpenRevisions => self.open_revisions(),
            Out::Open(_) | Out::SwitchToMain | Out::MergeIntoMain | Out::Release(_) | Out::OpenFolder(_) => {}
        }
    }
}
