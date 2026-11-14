//! Board 1.11 (UI rev 3, "Placing a device"): while Place Symbol runs, the inspector describes the
//! device about to be placed: its symbol, its next tag (from the tool's prompt), the page's
//! Function and Location it will take, and what the click changes. Read only: the picker and the
//! tool do the placing ([`crate::shell::placing`]); nothing here writes or reads the project.

use ecad_cadcraft::symbols;
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Sense, Stroke, pos2, vec2};

use super::widgets as w;
use crate::Trial;
use crate::shell::placing::{Shown, tag_in};

/// Accessible names.
pub const CHANGES: &str = "ecad.inspector.placing.changes";
pub const NOT_PLACED: &str = "not placed yet";

/// What the inspector knows about the device being placed.
#[derive(Debug, Clone)]
pub struct Placing {
    /// `-K02` (a new device) or `-K01` (another symbol of it); `None` until the tool names it.
    pub tag: Option<String>,
    /// The symbol joins an existing device.
    pub joins: bool,
    pub symbol: Option<&'static symbols::Starter>,
}

impl Placing {
    /// The header: the tag (or the symbol's name) and `Contactor coil · not placed yet`.
    pub fn head(&self) -> (String, String) {
        let name = self.symbol.map_or("Symbol", |s| s.name);
        let title = self.tag.clone().unwrap_or_else(|| name.to_string());
        let sub = match (&self.tag, self.joins) {
            (Some(t), true) => format!("{name} \u{b7} joins {t} \u{b7} {NOT_PLACED}"),
            _ => format!("{name} \u{b7} {NOT_PLACED}"),
        };
        (title, sub)
    }

    /// What the click changes, as (mark, words): `ok`, `free` or `warning`.
    pub fn changes(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        let tag = self.tag.clone().unwrap_or_else(|| "the device".into());
        if self.joins {
            out.push(("ok", format!("Adds this symbol to {tag} where you click.")));
        } else {
            out.push(("ok", format!("Adds device {tag} where you click.")));
        }
        let labels = self.symbol.map(|s| s.labels()).unwrap_or_default();
        if !labels.is_empty() {
            let list = match labels.as_slice() {
                [one] => one.clone(),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
                [] => String::new(),
            };
            let verb = if labels.len() == 1 { "joins" } else { "join" };
            out.push(("free", format!("{list} {verb} a wire end they land on; otherwise they stay free connection points until you wire them.")));
        }
        if !self.joins {
            out.push(("warning", format!("Checks counts {tag} among the devices without a part number until you choose one.")));
        }
        out
    }
}

/// The device being placed, while Place Symbol runs and the picker or its chip shows.
pub fn of(t: &Trial) -> Option<Placing> {
    if t.picker_shown() == Shown::Nothing {
        return None;
    }
    let prompt = t.cad.session.current_prompt().map(|p| p.message.clone()).unwrap_or_default();
    let symbol = t.shell.tools.placing.symbol.and_then(symbols::by_key);
    Some(Placing { tag: tag_in(&prompt), joins: prompt.contains("joins "), symbol })
}

/// The body: Identity, Parts and What this click changes.
pub fn show(ui: &mut egui::Ui, cx: &mut super::Cx<'_>, p: &Placing, standard: &str) {
    let full = cx.full;
    // The page's Function and Location, which the new device takes.
    let page = cx.sh.view.sections.iter().find(|s| s.kind == "page");
    let from_page = |key: &str| {
        page.and_then(|s| s.rows.iter().find(|r| r.key == key)).map(|r| (r.value.clone(), if r.inherited { "from project" } else { "from page" }))
    };
    let (function, location) = (from_page("function"), from_page("location"));
    let mut folded = std::mem::take(&mut cx.insp.folded);
    let name = p.symbol.map_or("Symbol", |s| s.name);
    w::section(ui, &mut folded, full, "placing-identity", "Identity", p.tag.as_deref().unwrap_or(""), false, |ui| {
        let tag_hint = if p.joins { "joins" } else { "next free" };
        boxed(ui, "Tag", p.tag.as_deref().unwrap_or("?"), tag_hint, "ecad.inspector.placing.tag");
        for (label, v, key) in [("Function", &function, "function"), ("Location", &location, "location")] {
            match v {
                Some((value, from)) if !value.is_empty() => boxed(ui, label, value, from, &format!("ecad.inspector.placing.{key}")),
                _ => boxed(ui, label, "", "none", &format!("ecad.inspector.placing.{key}")),
            }
        }
        w::row(ui, "Description", |ui| {
            let width = ui.available_width();
            let (rect, r) = ui.allocate_exact_size(vec2(width, w::FIELD_H), Sense::hover());
            dashed(ui.painter(), rect.shrink(0.5), w::text_3());
            ui.painter().text(
                pos2(rect.left() + space::M, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Type one now or later",
                w::font(ui, ui_type::BODY),
                w::text_3(),
            );
            r.on_hover_text("Set it in the Device tab once it is placed");
        });
        // One label (the symbol and its standard), so it never reads as the picker's tile.
        w::row(ui, "Symbol", |ui| {
            let mut job = egui::text::LayoutJob::default();
            let fmt = |f, c| egui::TextFormat { font_id: f, color: c, ..Default::default() };
            job.append(name, 0.0, fmt(w::font(ui, ui_type::BODY), w::text()));
            job.append(&format!("  \u{b7} {standard}"), 0.0, fmt(w::font(ui, ui_type::SMALL), w::text_3()));
            ui.label(job);
        });
    });
    w::section(ui, &mut folded, full, "placing-parts", "Parts", "none yet", false, |ui| {
        w::button(ui, "From catalogue", false, Some(super::device::NOT_YET));
        ui.add(egui::Label::new(w::rich(ui, "Contacts offered follow the part set you choose.", ui_type::SMALL, w::text_3())).wrap());
    });
    w::section(ui, &mut folded, full, "placing-changes", "What this click changes", "", false, |ui| {
        ui.scope(|ui| {
            for (mark, words) in p.changes() {
                change_line(ui, mark, &words);
            }
        })
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, CHANGES));
    });
    cx.insp.folded = folded;
}

/// A value read from the tool or the page, in a box with where it comes from at the right.
fn boxed(ui: &mut egui::Ui, label: &str, value: &str, hint: &str, name: &str) {
    w::row(ui, label, |ui| {
        let width = ui.available_width();
        let rect = w::field_box(ui, width);
        let r = ui.interact(rect, egui::Id::unique(name), Sense::hover());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, name));
        let p = ui.painter();
        p.text(pos2(rect.left() + space::M, rect.center().y), egui::Align2::LEFT_CENTER, value, w::font(ui, ui_type::BODY), w::text_2());
        if !hint.is_empty() {
            let g = p.layout_no_wrap(hint.to_string(), w::font(ui, ui_type::SMALL), w::text_3());
            let pill =
                egui::Rect::from_min_size(pos2(rect.right() - space::S - g.size().x - 12.0, rect.center().y - 10.0), vec2(g.size().x + 12.0, 20.0));
            p.rect_stroke(
                pill,
                egui::CornerRadius::same(10),
                Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(ecad_ui::SHELL.line_strong)),
                egui::StrokeKind::Inside,
            );
            p.galley(pill.center() - g.size() / 2.0, g, w::text_3());
        }
    });
}

/// A dashed outline (a value not given yet).
fn dashed(p: &egui::Painter, r: egui::Rect, colour: egui::Color32) {
    let s = Stroke::new(1.0, colour);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for pair in pts.windows(2) {
        if let [a, b] = pair {
            p.extend(egui::Shape::dashed_line(&[*a, *b], s, 4.0, 3.0));
        }
    }
}

/// One line of What this click changes: a tick, a ring (stays free) or a warning, then the words.
fn change_line(ui: &mut egui::Ui, mark: &str, words: &str) {
    let s = ecad_ui::SHELL;
    ui.add_space(space::XS);
    let top = ui.cursor().top();
    let at = pos2(ui.cursor().left() + 8.0, top + 9.0);
    let p = ui.painter();
    match mark {
        "ok" => {
            let c = ecad_ui::color(s.ok);
            p.line_segment([at + vec2(-4.5, 0.0), at + vec2(-1.5, 3.0)], Stroke::new(1.6, c));
            p.line_segment([at + vec2(-1.5, 3.0), at + vec2(4.5, -3.5)], Stroke::new(1.6, c));
        }
        "free" => {
            p.circle_stroke(at, 4.5, Stroke::new(1.6, ecad_ui::color(s.connect)));
        }
        _ => w::severity_mark(p, at, ecad_cadcraft::checks::Severity::Warning),
    }
    let width = ui.available_width() - 26.0;
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        ui.vertical(|ui| {
            ui.set_max_width(width);
            ui.add(egui::Label::new(w::rich(ui, words, ui_type::BODY, w::text())).wrap());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_coil_says_what_the_click_adds_and_leaves() {
        let p = Placing { tag: Some("-K02".into()), joins: false, symbol: symbols::by_key("coil") };
        let (title, sub) = p.head();
        assert_eq!(title, "-K02");
        assert!(sub.ends_with(NOT_PLACED), "{sub}");
        let c = p.changes();
        assert_eq!(c.first().map(|x| x.0), Some("ok"));
        assert!(c[0].1.contains("Adds device -K02"), "{c:?}");
        assert!(c.iter().any(|(m, w)| *m == "free" && w.contains("A1 and A2")), "{c:?}");
        assert!(c.iter().any(|(m, _)| *m == "warning"), "a new device has no part number yet");
    }

    #[test]
    fn a_symbol_joining_a_device_adds_no_device() {
        let p = Placing { tag: Some("-K01".into()), joins: true, symbol: symbols::by_key("no") };
        let c = p.changes();
        assert!(c[0].1.contains("to -K01"), "{c:?}");
        assert!(c.iter().all(|(m, _)| *m != "warning"), "{c:?}");
        assert!(p.head().1.contains("joins -K01"));
    }
}
