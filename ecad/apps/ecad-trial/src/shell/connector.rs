//! The connector maker (UI rev 3, "Connector maker"; board 1.13): the picker's Connector side,
//! while Device ▸ Connector runs. It lays brief 044's questions out as sections that fold to a
//! one-line summary; the section the tool is asking opens, and its chips answer the prompt (the
//! same keywords the intent line offers). Nothing here adds behaviour: every answer goes to the
//! running `connector` form.
//!
//! The model is today's (brief 044): one device (`-X10`) with two halves (`-X10.S`, `-X10.P`).
//! Rev 3's sections that have no answer in that form yet (more families and variants, pin
//! descriptions, the shield contact, hood and housing, Save as a type) are shown greyed in place.

use std::collections::BTreeMap;

use cadcraft_engine::Input;
use ecad_ui::prim;
use ecad_ui::tokens::{Icon, space, ui_type};
use eframe::egui::{self, vec2};

use crate::Trial;

/// Accessible label of the maker.
pub const MAKER: &str = "Connector maker";
/// Why a greyed option is greyed.
pub const LATER: &str = "Not in the connector library yet.";

/// The maker's sections, in order, and the prompts of the `connector` form each one answers.
pub const SECTIONS: [(&str, &[&str]); 5] = [
    ("Which halves", &["Halves"]),
    ("Type", &["Type"]),
    ("Pins", &["Pins", "Pin names", "Pin names, comma-separated"]),
    ("Hood and housing", &[]),
    ("Draw it", &["Draw"]),
];

/// Families the board lists, with their variants; a variant the form knows answers its Type
/// prompt with that keyword, the others are greyed.
const FAMILIES: [(&str, &[(&str, Option<&str>)]); 6] = [
    ("Plain", &[("Plain", Some("Plain"))]),
    ("M12", &[("M12", Some("M12")), ("A-coded", None), ("B-coded", None), ("D-coded", None), ("X-coded", None), ("T, L power", None)]),
    ("M8", &[("3-pin", None), ("4-pin", None), ("8-pin", None)]),
    ("D-sub", &[("D-sub", Some("Dsub")), ("37 W", None), ("HD 15", None), ("HD 26", None)]),
    (
        "Heavy-duty",
        &[("Han 3A", None), ("Han 4A", None), ("Han 6B", None), ("Han 10B", None), ("Han 16B", None), ("Han 24B", None), ("Modular", None)],
    ),
    ("Other", &[("RJ45", Some("RJ45")), ("USB-A", Some("USBA")), ("USB-B", Some("USBB")), ("USB-C", None), ("7/8\"", None), ("Custom", None)]),
];

/// Which halves, in the board's words, and the form's keyword.
const HALVES: [(&str, Option<&str>); 4] =
    [("Socket only", Some("Socket")), ("Plug only", Some("Plug")), ("Plug and socket", Some("Both")), ("Socket in a panel", None)];

/// The maker's state between frames: what each prompt was answered with.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// Prompt label → the answer given (a keyword, or the default taken with Enter).
    pub answers: BTreeMap<String, String>,
    /// The prompt shown last frame, its default and its keywords.
    last: Option<(String, Option<String>)>,
    keywords: Vec<String>,
    /// A keyword sent from the maker or the intent line (recorded as that prompt's answer).
    pub sent: Option<(String, String)>,
    /// What was typed at the command line last frame, and at which prompt.
    typed: Option<(String, String)>,
}

/// The Halves row for tag `t` and the Halves answer (`None`: not answered yet).
pub fn halves_words(t: &str, halves: Option<&str>) -> String {
    match halves {
        Some("Socket") => format!("{t}.S socket"),
        Some("Plug") => format!("{t}.P plug"),
        Some(_) => format!("{t}.S socket, {t}.P plug"),
        None => "set under Which halves".into(),
    }
}

/// The section prompt `label` belongs to.
pub fn section_of(label: &str) -> Option<usize> {
    SECTIONS.iter().position(|(_, prompts)| prompts.contains(&label))
}

impl Trial {
    /// Whether the connector maker shows: Device ▸ Connector runs in Draw.
    pub fn maker_shown(&self) -> bool {
        self.shell.state.workspace == super::Workspace::Draw
            && !self.shell.state.cad_chrome
            && self.cad.session.running.as_ref().is_some_and(|r| r.id.eq_ignore_ascii_case("connector"))
    }

    /// Draw the maker at the canvas's top left while Device ▸ Connector runs.
    pub(crate) fn maker_frame(&mut self, ui: &mut egui::Ui) {
        if !self.maker_shown() {
            if self.cad.session.running.is_none() {
                self.shell.tools.maker = State::default();
            }
            return;
        }
        let Some(canvas) = self.shell.page_rect else { return };
        let prompt = self.cad.session.current_prompt();
        let label = prompt.as_ref().map(|p| p.message.trim().to_string()).unwrap_or_default();
        let default = prompt.as_ref().and_then(|p| p.default.clone());
        let keywords: Vec<String> = prompt.as_ref().map(|p| p.keywords.clone()).unwrap_or_default();
        // A prompt that moved on was answered: with the keyword sent from the maker or the
        // intent line, else with what was typed at the command line, else (Enter on an empty
        // line) with its default.
        let buffer = self.cad.cmd.buffer.trim().to_string();
        let st = &mut self.shell.tools.maker;
        if let Some((prev, prev_default)) = st.last.clone()
            && prev != label
        {
            let sent = st.sent.take().filter(|(l, _)| *l == prev).map(|(_, a)| a);
            let typed = st.typed.take().filter(|(l, _)| *l == prev).map(|(_, a)| a);
            if let Some(a) = sent.or(typed).or(prev_default) {
                st.answers.insert(prev, keyword_of(&a, &st.keywords));
            }
        }
        st.typed = (!buffer.is_empty()).then(|| (label.clone(), buffer));
        st.keywords = keywords.clone();
        st.last = Some((label.clone(), default.clone()));
        let answers = st.answers.clone();
        let open = section_of(&label);
        let tag =
            answers.get("Connector").filter(|a| !a.eq_ignore_ascii_case("new")).cloned().or_else(|| default.clone().filter(|_| label == "Connector"));
        let mut send: Option<String> = None;
        let mut symbol = false;
        let at = canvas.min + vec2(space::M, space::M);
        let max_h = (canvas.height() - 2.0 * space::M).max(240.0);
        let area =
            egui::Area::new(egui::Id::unique("ecad.shell.connector.maker")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
                super::placing::popover_frame().show(ui, |ui| {
                    ui.set_width(super::placing::WIDTH - 2.0 * space::L);
                    ui.set_max_width(super::placing::WIDTH - 2.0 * space::L);
                    ui.set_max_height(max_h);
                    let r = ui.label("");
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Window, true, MAKER));
                    symbol = super::placing::tabs(ui, false);
                    ui.add_space(space::S);
                    for (i, (title, prompts)) in SECTIONS.iter().enumerate() {
                        let summary = summary(i, prompts, &answers, open == Some(i));
                        header(ui, title, &summary, open == Some(i));
                        if open != Some(i) {
                            continue;
                        }
                        ui.indent(("ecad.connector.section", i), |ui| match i {
                            0 => chips(ui, &HALVES, &keywords, default.as_deref(), &mut send),
                            1 => {
                                for (family, variants) in FAMILIES {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.add_sized(
                                            vec2(72.0, 20.0),
                                            egui::Label::new(
                                                egui::RichText::new(family)
                                                    .font(ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL))
                                                    .color(ecad_ui::color(ecad_ui::SHELL.text_2)),
                                            ),
                                        );
                                        chips(ui, variants, &keywords, default.as_deref(), &mut send);
                                    });
                                }
                                prim::read_value(ui, "or search the library for another type");
                            }
                            2 => {
                                ui.horizontal_wrapped(|ui| {
                                    for k in &keywords {
                                        if prim::Chip::new(k).selected(default.as_deref() == Some(k.as_str())).show(ui).response.clicked() {
                                            send = Some(k.clone());
                                        }
                                    }
                                });
                                ui.add(prim::Button::new("Shield contact on the housing").small().disabled(LATER));
                                prim::read_value(ui, "Pin descriptions come later; names are set here.");
                            }
                            _ => {
                                ui.horizontal_wrapped(|ui| {
                                    for k in &keywords {
                                        let words = if k == "Row" {
                                            "All pins in a row"
                                        } else if k == "Pins" {
                                            "Pin by pin across wires"
                                        } else {
                                            k.as_str()
                                        };
                                        if prim::Chip::new(words).selected(default.as_deref() == Some(k.as_str())).show(ui).response.clicked() {
                                            send = Some(k.clone());
                                        }
                                    }
                                })
                                .inner
                            }
                        });
                    }
                    ui.separator();
                    // Tag, description and the two halves (brief 044: one device, two halves).
                    let t = tag.clone().unwrap_or_else(|| "-X?".into());
                    row(ui, "Tag", &t);
                    // The form's first question: a new connector (its next free tag) or one to add to.
                    if label == "Connector" {
                        ui.horizontal(|ui| {
                            for k in &keywords {
                                if prim::Chip::new(&format!("{k} ({t})")).selected(true).show(ui).response.clicked() {
                                    send = Some(k.clone());
                                }
                            }
                            prim::read_value(ui, "or type a connector's tag to add to it");
                        });
                    }
                    row(ui, "Description", "Set it in the inspector after placing");
                    row(ui, "Halves", &halves_words(&t, answers.get("Halves").map(String::as_str)));
                    ui.horizontal(|ui| {
                        ui.allocate_ui(vec2(super::placing::WIDTH - 2.0 * space::L - 120.0, 32.0), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("One device with two halves; each half keeps its own location and parts.")
                                        .font(ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL))
                                        .color(ecad_ui::color(ecad_ui::SHELL.text_2)),
                                )
                                .wrap(),
                            )
                        });
                        ui.add(prim::Button::new("Save as a type").small().disabled(LATER));
                    });
                });
            });
        self.shell.tools.panel_right = Some(area.response.rect.right());
        if symbol && let Some(a) = ecad_cadcraft::actions::find("place.symbol") {
            self.start_tool(a);
            return;
        }
        if let Some(k) = send {
            self.shell.tools.maker.sent = Some((label, k.clone()));
            if let Err(e) = self.cad.session.input(Input::Keyword(k)) {
                self.cad.session.echo(e.to_string());
            }
            self.after_logic();
        }
    }
}

/// A section's folded summary: its answers, or what the open prompt offers.
fn summary(i: usize, prompts: &[&str], answers: &BTreeMap<String, String>, open: bool) -> String {
    if i == 3 {
        return "not in the library yet".into();
    }
    let given: Vec<String> = prompts.iter().filter_map(|p| answers.get(*p)).map(|a| words(a)).collect();
    if !given.is_empty() {
        return given.join(" · ");
    }
    if open { "choose below".into() } else { "next".into() }
}

/// A typed answer as the prompt's keyword when it names one (`m12` is `M12`), else as typed.
fn keyword_of(answer: &str, keywords: &[String]) -> String {
    keywords.iter().find(|k| k.eq_ignore_ascii_case(answer)).cloned().unwrap_or_else(|| answer.to_string())
}

/// A form keyword in the board's words.
fn words(k: &str) -> String {
    match k {
        "Both" => "Plug and socket".into(),
        "Socket" => "Socket only".into(),
        "Plug" => "Plug only".into(),
        "Dsub" => "D-sub".into(),
        "USBA" => "USB-A".into(),
        "USBB" => "USB-B".into(),
        "Row" => "all pins in a row".into(),
        "Pins" => "pin by pin".into(),
        other => other.to_string(),
    }
}

/// A section's header: a fold mark, its title and its summary at the right.
fn header(ui: &mut egui::Ui, title: &str, summary: &str, open: bool) {
    let s = ecad_ui::SHELL;
    ui.add_space(space::XS);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
        ecad_ui::icons::paint(ui.painter(), r, if open { Icon::ChevronDown } else { Icon::ChevronRight }, ecad_ui::color(s.text_3));
        let h = ui.label(egui::RichText::new(title).font(ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS)).color(ecad_ui::color(s.text)));
        h.widget_info(|| egui::WidgetInfo::selected(egui::Role::Heading, true, open, title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            prim::read_value(ui, summary);
        });
    });
    ui.add_space(space::XS);
}

/// Chips for `options`: one whose keyword the prompt offers answers it; the others are greyed.
fn chips(ui: &mut egui::Ui, options: &[(&str, Option<&str>)], keywords: &[String], default: Option<&str>, send: &mut Option<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(space::XS, space::XS);
        for (text, kw) in options {
            match kw.filter(|k| keywords.iter().any(|x| x.eq_ignore_ascii_case(k))) {
                Some(k) => {
                    if prim::Chip::new(text).selected(default.is_some_and(|d| d.eq_ignore_ascii_case(k))).show(ui).response.clicked() {
                        *send = Some(k.to_string());
                    }
                }
                None => {
                    ui.add(prim::Button::new(text).small().disabled(LATER));
                }
            }
        }
    });
}

/// A read-only row: label and value.
fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            vec2(80.0, 22.0),
            egui::Label::new(
                egui::RichText::new(label).font(ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL)).color(ecad_ui::color(ecad_ui::SHELL.text_2)),
            ),
        );
        prim::read_value(ui, value);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_prompt_of_the_form_has_a_section() {
        for p in ["Halves", "Type", "Pins", "Pin names", "Draw"] {
            assert!(section_of(p).is_some(), "{p}");
        }
        assert_eq!(section_of("Connector"), None, "the tag sits under the sections");
    }

    #[test]
    fn the_halves_row_names_only_the_halves_chosen() {
        assert_eq!(halves_words("-X10", Some("Socket")), "-X10.S socket");
        assert_eq!(halves_words("-X10", Some("Plug")), "-X10.P plug");
        assert_eq!(halves_words("-X10", Some("Both")), "-X10.S socket, -X10.P plug");
        assert_eq!(halves_words("-X10", None), "set under Which halves");
        assert_eq!(keyword_of("m12", &["Plain".into(), "M12".into()]), "M12");
    }

    #[test]
    fn a_folded_section_says_what_was_chosen() {
        let mut a = BTreeMap::new();
        a.insert("Type".to_string(), "Dsub".to_string());
        assert_eq!(summary(1, &["Type"], &a, false), "D-sub");
        assert_eq!(summary(0, &["Halves"], &a, true), "choose below");
        assert_eq!(summary(4, &["Draw"], &a, false), "next");
    }
}
