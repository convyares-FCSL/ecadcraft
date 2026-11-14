//! The Checks tab (UI rev 3, board 1.07): This page or Project; each check as a card with its fix
//! button and a go-to arrow; on This page, the checks on other pages follow under "Elsewhere in
//! the project". The rows are the Checks copy ([`crate::checks_panel::ChecksPanel`]), kept from
//! each change; the bottom bar's counter opens this tab. The full table is Project ▸ Check list.

use ecad_cadcraft::checks::Severity;
use ecad_model::PageId;
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Rect, Sense, pos2, vec2};

use super::widgets as w;
use crate::checks_panel::{ChecksPanel, Row};
use crate::project_panel::Action;

/// Accessible names.
pub const SCOPE: &str = "ecad.inspector.checks.scope";
pub const ELSEWHERE: &str = "ecad.inspector.checks.elsewhere";
pub const GO_TO: &str = "Go to the check";
/// At most this many cards, and lines under "Elsewhere in the project" (the rest are in Project
/// ▸ Check list).
const MAX_CARDS: usize = 100;
const MAX_ELSEWHERE: usize = 50;
/// The footer's words.
pub const FOOTER: &str =
    "Checks run as you draw. Selecting a check marks it on the page; the counter in the bottom bar opens this tab from anywhere.";

fn words(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `1 error · 1 warning`.
pub fn count_words(errors: usize, warnings: usize) -> String {
    format!("{} \u{b7} {}", words(errors, "error", "errors"), words(warnings, "warning", "warnings"))
}

/// `1 error · 1 warning`, `4 notes`: the kinds there are, or `No problems`.
fn counts_of(rows: &[&Row]) -> String {
    let n = |s: Severity| rows.iter().filter(|r| r.check.severity() == s).count();
    let parts: Vec<String> = [(Severity::Error, "error", "errors"), (Severity::Warning, "warning", "warnings"), (Severity::Info, "note", "notes")]
        .iter()
        .filter(|(s, _, _)| n(*s) > 0)
        .map(|(s, one, many)| words(n(*s), one, many))
        .collect();
    if parts.is_empty() { "No problems".to_string() } else { parts.join(" \u{b7} ") }
}

/// The card's sentence: what, which and where.
fn sentence(r: &Row) -> String {
    let what = r.check.what();
    let mut s = what.clone();
    if !r.check.which.is_empty() && !what.contains(&r.check.which) {
        s.push_str(": ");
        s.push_str(&r.check.which);
    }
    if !r.locator.is_empty() {
        s.push_str(", ");
        s.push_str(&r.locator);
    }
    s.push('.');
    s
}

/// One check as a card: its mark, its sentence (a click goes to it), its fix buttons (the first
/// primary) and a go-to arrow.
pub fn card(ui: &mut egui::Ui, r: &Row, writable: bool, out: &mut Vec<Action>) {
    let width = ui.available_width();
    ui.add_space(space::M);
    let top = ui.cursor().top();
    w::severity_mark(ui.painter(), pos2(ui.cursor().left() + 8.0, top + 9.0), r.check.severity());
    let text_w = width - 26.0;
    let resp = ui
        .horizontal(|ui| {
            ui.add_space(26.0);
            ui.vertical(|ui| {
                ui.set_max_width(text_w);
                ui.add(egui::Label::new(w::rich(ui, sentence(r), ui_type::BODY, w::text())).wrap().sense(Sense::click()))
            })
            .inner
        })
        .inner;
    let label = crate::checks_panel::row_label(r);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    if resp.on_hover_text("Show it on the page").clicked() {
        out.push(Action::GotoCheck(Box::new(r.check.clone())));
    }
    ui.add_space(space::S);
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        if let Some(f) = r.check.fix {
            for (k, name) in crate::lists::checks::fix_names(f).into_iter().enumerate() {
                let why = (!writable).then_some("The project is read-only.");
                if w::button(ui, name, k == 0, why).clicked()
                    && let Some(a) = ChecksPanel::fix(r, f, k)
                {
                    out.push(a);
                }
            }
        }
        // The go-to arrow.
        let (rect, go) = ui.allocate_exact_size(vec2(28.0, 24.0), Sense::click());
        go.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, GO_TO));
        if go.hovered() {
            ui.painter().rect_filled(rect, ecad_ui::style::corner(ecad_ui::tokens::radius::ROW), ecad_ui::color(ecad_ui::SHELL.hover));
        }
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "\u{2192}", w::font(ui, ui_type::TITLE), w::text_2());
        if go.on_hover_text("Go to it").clicked() {
            out.push(Action::GotoCheck(Box::new(r.check.clone())));
        }
    });
    ui.add_space(space::S);
}

/// A line under "Elsewhere in the project": a dot, the words, and where at the right.
fn elsewhere_line(ui: &mut egui::Ui, r: &Row) -> bool {
    let s = ecad_ui::SHELL;
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let label = crate::checks_panel::row_label(r);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, ecad_ui::style::corner(ecad_ui::tokens::radius::ROW), ecad_ui::color(s.hover));
    }
    let colour = match r.check.severity() {
        Severity::Error => s.error,
        Severity::Warning => s.warning,
        Severity::Info => s.text_3,
    };
    p.circle_filled(pos2(rect.left() + 6.0, rect.center().y), 4.0, ecad_ui::color(colour));
    let mut words = r.check.what();
    if !r.check.which.is_empty() && !words.contains(&r.check.which) {
        words = format!("{} \u{b7} {words}", r.check.which);
    }
    let loc = p.layout_no_wrap(r.locator.clone(), w::font(ui, ui_type::BODY), w::text_2());
    let room = ((rect.width() - 26.0 - loc.size().x - space::M) / 7.0).max(8.0) as usize;
    p.text(pos2(rect.left() + 18.0, rect.center().y), egui::Align2::LEFT_CENTER, w::ellipsis(&words, room), w::font(ui, ui_type::BODY), w::text());
    p.galley(pos2(rect.right() - loc.size().x, rect.center().y - loc.size().y / 2.0), loc, w::text_2());
    resp.on_hover_text("Go to it").clicked()
}

/// The tab: the scope switch and counts, the cards, Elsewhere, and the footer.
pub fn show(ui: &mut egui::Ui, panel: &mut ChecksPanel, project_scope: &mut bool, page: Option<PageId>, writable: bool, body: Rect) -> Vec<Action> {
    let mut out = Vec::new();
    let full = egui::Rangef::new(body.left() - space::L, body.right() + space::M);
    let footer_h = 88.0;
    let list = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - footer_h));
    let rows = panel.rows();
    // A check about the whole project (no page) belongs on every page.
    let on_page = |r: &&Row| r.check.page.is_none() || r.check.page == page;
    let here: Vec<&Row> = rows.iter().filter(|r| *project_scope || on_page(r)).collect();
    let counts = counts_of(&here);
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        ui.add_space(space::M);
        ui.horizontal(|ui| {
            ui.allocate_ui(vec2(168.0, w::FIELD_H), |ui| {
                let sel = Some(usize::from(*project_scope));
                if let Some(i) = w::segmented(ui, &["This page", "Project"], sel, true, SCOPE, "") {
                    *project_scope = i == 1;
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(w::rich(ui, &counts, ui_type::SMALL, w::text_3()));
            });
        });
        ui.add_space(space::M);
        w::rule(ui, full);
        egui::ScrollArea::vertical().id_salt("ecad.inspector.checks.scroll").auto_shrink([false, false]).show(ui, |ui| {
            if here.is_empty() {
                ui.add_space(space::M);
                let words = if *project_scope { crate::checks_panel::NO_PROBLEMS } else { "No problems on this page." };
                ui.label(w::rich(ui, words, ui_type::BODY, w::text_2()));
                ui.add_space(space::M);
            }
            for r in here.iter().take(MAX_CARDS) {
                card(ui, r, writable && !panel.read_only, &mut out);
                w::rule(ui, full);
            }
            if here.len() > MAX_CARDS {
                ui.label(w::rich(ui, format!("{} more in Project \u{25b8} Check list.", here.len() - MAX_CARDS), ui_type::SMALL, w::text_3()));
            }
            if !*project_scope {
                let elsewhere: Vec<&Row> = rows.iter().filter(|r| !on_page(r)).collect();
                if !elsewhere.is_empty() {
                    ui.add_space(space::L);
                    ui.label(w::rich(ui, "Elsewhere in the project", ui_type::SMALL, w::text_3()))
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, ELSEWHERE));
                    ui.add_space(space::XS);
                    for r in elsewhere.iter().take(MAX_ELSEWHERE) {
                        if elsewhere_line(ui, r) {
                            out.push(Action::GotoCheck(Box::new(r.check.clone())));
                        }
                    }
                    if elsewhere.len() > MAX_ELSEWHERE {
                        ui.label(w::rich(ui, format!("{} more", elsewhere.len() - MAX_ELSEWHERE), ui_type::SMALL, w::text_3()));
                    }
                }
            }
        });
    });
    // The footer: what checks do, and which kinds are shown.
    let foot = Rect::from_min_max(pos2(body.left(), body.bottom() - footer_h), body.max);
    ui.painter().hline(full, foot.top() + 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, w::line()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(foot.shrink2(vec2(0.0, space::S))), |ui| {
        ui.add(egui::Label::new(w::rich(ui, FOOTER, ui_type::SMALL, w::text_3())).wrap());
        ui.horizontal(|ui| {
            let label = if panel.hidden.is_empty() { "All kinds" } else { "Some kinds" };
            ui.label(w::rich(ui, "Show:", ui_type::SMALL, w::text_3()));
            ui.menu_button(w::rich(ui, format!("{label} \u{25be}"), ui_type::SMALL, w::text_2()), |ui| {
                for k in ecad_cadcraft::checks::Kind::ALL {
                    let mut on = !panel.hidden.contains(&k);
                    if ui.checkbox(&mut on, k.words()).changed() {
                        if on {
                            panel.hidden.remove(&k);
                        } else {
                            panel.hidden.insert(k);
                        }
                    }
                }
            });
        });
    });
    out
}
