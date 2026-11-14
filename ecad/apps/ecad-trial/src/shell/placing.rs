//! Placing a device (UI rev 3, "Placing a device"; boards 1.11 and 1.12): the picker at the
//! canvas's top left while Place Symbol runs.
//!
//! - **Open** while the tool asks for a symbol (Device ▸ Symbol, or the chip's Change): the
//!   Symbol | Connector switch, a search, the matching symbols as tiles, a note pointing to the
//!   Library, and, once a symbol is chosen, what it can be placed as (a new device with the next
//!   free tag, or another symbol of a device that takes it; [`crate::search::choices`]).
//! - **Folded** to a chip once a symbol is chosen and the tool asks for the point: the symbol,
//!   "as -K02", Change (P) and search. P or the chip opens it again.
//!
//! Choosing runs what Find symbol always ran ([`Trial::start_pick`], [`Trial::start_choice`]);
//! Connector starts Device ▸ Connector, whose questions the connector maker shows
//! ([`super::connector`]).
//!
//! Update map: the tiles come from the built-in table (no project read); the second step is read
//! from the project when the symbol or the project's change count moved.

use ecad_cadcraft::symbols;
use ecad_ui::prim;
use ecad_ui::tokens::{Icon, line, radius, space, ui_type};
use eframe::egui::{self, Rect, pos2, vec2};

use super::bar;
use crate::Trial;
use crate::search::{Choice, Pick};

/// Accessible labels.
pub const PICKER: &str = "Symbol picker";
pub const CHIP: &str = "Placing";
pub const CHANGE: &str = "Change symbol";
pub const SEARCH: &str = "Search symbols";
pub const FIELD: &str = "Find a symbol";
pub const BROWSE: &str = "Browse the library";
pub const SYMBOL_TAB: &str = "Symbol";
pub const CONNECTOR_TAB: &str = "Connector";

/// The panel's width (board 1.12), its tiles per row and most tiles shown.
pub const WIDTH: f32 = 396.0;
const COLUMNS: usize = 3;
const MAX_TILES: usize = 9;
const TILE_H: f32 = 104.0;

/// The picker's state between frames.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// The symbol chosen for the running Place Symbol, when the window knows it.
    pub symbol: Option<&'static str>,
    /// The person opened the picker while the tool asks for a point (Change, P, the chip).
    pub open: bool,
    /// The panel's search text, and whether the field takes the keyboard next frame.
    pub query: String,
    focus: bool,
    /// The second step for (symbol, change count).
    choices: Option<((&'static str, u64), Vec<Choice>)>,
    /// How many times the second step was read from the project (tests).
    pub reads: u64,
    /// What was typed at the command line last frame while the tool asked for a symbol: the
    /// answer it took when the prompt moved on.
    typed: Option<String>,
}

/// The built-in symbol a typed answer to "Enter symbol" names: its key, or the one symbol the
/// text finds (as the tool takes one match). `None` for a device tag, a project symbol or an
/// answer that matches several.
pub fn typed_symbol(text: &str) -> Option<&'static str> {
    let t = text.trim();
    if let Some(s) = symbols::by_key(t) {
        return Some(s.key);
    }
    match symbols::search(t).as_slice() {
        [one] => Some(one.key),
        _ => None,
    }
}

/// What the picker shows this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Nothing,
    Open,
    Chip,
}

/// The tag a Place Symbol prompt names ("… (new device -K02, rotation 0)" or "… (joins -K01 as
/// 13/14, …)"), if it names one.
pub fn tag_in(prompt: &str) -> Option<String> {
    let after = ["new device ", "joins "].iter().find_map(|w| prompt.split_once(w).map(|(_, rest)| rest))?;
    let tag: String = after.chars().take_while(|c| !c.is_whitespace() && *c != ',' && *c != ')').collect();
    (!tag.is_empty()).then_some(tag)
}

impl Trial {
    /// What the picker shows: open while Place Symbol asks for a symbol or the person opened it,
    /// folded while it asks for anything else, nothing when it is not running.
    pub fn picker_shown(&self) -> Shown {
        let placing = self.cad.session.running.as_ref().is_some_and(|r| r.id.eq_ignore_ascii_case("place"));
        if !placing || self.shell.state.workspace != super::Workspace::Draw || self.shell.state.cad_chrome {
            return Shown::Nothing;
        }
        let asks_symbol =
            self.cad.session.current_prompt().is_some_and(|p| p.message.starts_with("Enter symbol") && !p.message.contains(" to add to "));
        if asks_symbol || self.shell.tools.placing.open { Shown::Open } else { Shown::Chip }
    }

    /// Open the picker while placing (P, Change, the chip).
    pub fn open_picker(&mut self) {
        self.shell.tools.placing.open = true;
        self.shell.tools.placing.focus = true;
    }

    /// Draw the picker or its chip at the canvas's top left (Draw, while Place Symbol runs).
    pub(crate) fn placing_frame(&mut self, ui: &mut egui::Ui) {
        let shown = self.picker_shown();
        self.shell.tools.panel_right = None;
        if shown == Shown::Nothing {
            if self.cad.session.running.is_none() {
                self.shell.tools.placing = State::default();
            }
            return;
        }
        let Some(canvas) = self.shell.page_rect else { return };
        let at = canvas.min + vec2(space::M, space::M);
        // Which symbol is being placed: while the tool asks for one, the default it offers (a
        // click takes it); once it moved on, what was typed if anything was (a tile or Find
        // symbol set it already, through `start_pick`).
        let asks_symbol = self.cad.session.current_prompt().filter(|p| p.message.starts_with("Enter symbol"));
        let buffer = self.cad.cmd.buffer.trim().to_string();
        let st = &mut self.shell.tools.placing;
        match asks_symbol {
            Some(p) => {
                st.symbol = p.default.as_deref().and_then(symbols::by_key).map(|s| s.key);
                st.typed = (!buffer.is_empty()).then_some(buffer);
            }
            None => {
                if let Some(t) = st.typed.take() {
                    st.symbol = typed_symbol(&t);
                }
            }
        }
        // P opens the picker, unless the prompt takes the letter as text (a tag such as -P02).
        let p_key = ui.ctx().input(|i| i.key_pressed(egui::Key::P) && i.modifiers.is_none())
            && ui.ctx().memory(|m| m.focused().is_none())
            && !super::keys::prompt_takes(self, 'P');
        match shown {
            Shown::Chip => {
                if self.chip(ui, at) || p_key {
                    self.open_picker();
                }
            }
            Shown::Open => self.picker(ui, at, canvas),
            Shown::Nothing => {}
        }
    }

    /// The folded chip: the symbol, "as -K02", Change (P) and search. True when clicked.
    fn chip(&mut self, ui: &mut egui::Ui, at: egui::Pos2) -> bool {
        let s = ecad_ui::SHELL;
        let sym = self.shell.tools.placing.symbol.and_then(symbols::by_key);
        let tag = self.cad.session.current_prompt().and_then(|p| tag_in(&p.message));
        let name = sym.map_or("Symbol", |x| x.name);
        let as_words = match &tag {
            Some(t) => format!("as {t} · IEC built-in"),
            None => "IEC built-in".to_string(),
        };
        let mut clicked = false;
        let area = egui::Area::new(egui::Id::unique("ecad.shell.placing.chip")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
            popover_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = space::M;
                    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 30.0), egui::Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CHIP));
                    clicked |= resp.clicked();
                    thumb(ui, r, sym);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            egui::RichText::new(name).font(ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS)).color(ecad_ui::color(s.text)),
                        );
                        ui.label(egui::RichText::new(&as_words).font(ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL)).color(ecad_ui::color(s.text_2)));
                    });
                    ui.separator();
                    let change = ui.add(prim::Button::new("Change").kind(prim::Kind::Quiet).small());
                    change.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CHANGE));
                    clicked |= change.clicked();
                    prim::keycap(ui, "P");
                    clicked |= ui.add(prim::IconButton::new(Icon::Search, SEARCH)).clicked();
                });
            });
        });
        self.shell.tools.panel_right = Some(area.response.rect.right());
        self.shell.tools.chip_text = format!("{name} {as_words}");
        clicked
    }

    /// The open picker (board 1.12).
    fn picker(&mut self, ui: &mut egui::Ui, at: egui::Pos2, canvas: Rect) {
        let s = ecad_ui::SHELL;
        let q = self.shell.tools.placing.query.clone();
        let found = symbols::search(q.trim());
        let total = symbols::STARTERS.len();
        let chosen_key = self.shell.tools.placing.symbol;
        let choices = chosen_key.map(|k| self.picker_choices(k)).unwrap_or_default();
        let mut pick = None;
        let mut choice = None;
        let mut connector = false;
        let mut browse = false;
        let max_h = (canvas.height() - 2.0 * space::M).max(200.0);
        let focus = std::mem::take(&mut self.shell.tools.placing.focus);
        let query = &mut self.shell.tools.placing.query;
        let area = egui::Area::new(egui::Id::unique("ecad.shell.placing.picker")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
            popover_frame().show(ui, |ui| {
                ui.set_width(WIDTH - 2.0 * space::L);
                ui.set_max_width(WIDTH - 2.0 * space::L);
                ui.set_max_height(max_h);
                let r = ui.label("");
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Window, true, PICKER));
                connector = tabs(ui, true);
                ui.add_space(space::S);
                // The search.
                ui.horizontal(|ui| {
                    let field = egui::TextEdit::singleline(query)
                        .id_salt("ecad.shell.placing.field")
                        .hint_text("Find a symbol, tag or family")
                        .char_limit(symbols::MAX_QUERY)
                        .desired_width(WIDTH - 2.0 * space::L - 72.0);
                    let r = ui.add(field);
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, FIELD));
                    if focus {
                        r.request_focus();
                    }
                    prim::read_value(ui, &format!("{} of {total}", found.len()));
                });
                ui.add_space(space::S);
                // The tiles.
                let w = (WIDTH - 2.0 * space::L - space::S * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
                for row in found.chunks(COLUMNS).take(MAX_TILES / COLUMNS) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = space::S;
                        for sym in row {
                            if tile(ui, sym, w, chosen_key == Some(sym.key)).clicked() {
                                pick = Some(sym.key);
                            }
                        }
                    });
                }
                ui.add_space(space::S);
                ui.horizontal(|ui| {
                    ui.allocate_ui(vec2(WIDTH - 2.0 * space::L - 140.0, 32.0), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("This panel places by search. For categories, filters and previews:")
                                    .font(ecad_ui::fonts::font(ui.ctx(), ui_type::SMALL))
                                    .color(ecad_ui::color(s.text_2)),
                            )
                            .wrap(),
                        )
                    });
                    browse = ui.add(prim::Button::new(BROWSE).small()).clicked();
                });
                // The second step: what the chosen symbol is placed as.
                if let Some(sym) = chosen_key.and_then(symbols::by_key)
                    && !choices.is_empty()
                {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(sym.name).font(ecad_ui::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS)).color(ecad_ui::color(s.text)),
                        );
                        prim::read_value(ui, &format!("tag letter {} · {}", sym.prefix(), sym.labels().join(", ")));
                    });
                    for (i, c) in choices.iter().enumerate() {
                        if ui
                            .radio(
                                i == 0 && c.device.is_none(),
                                egui::RichText::new(c.text.trim()).font(ecad_ui::fonts::font(ui.ctx(), ui_type::BODY)),
                            )
                            .clicked()
                        {
                            choice = Some(c.clone());
                        }
                    }
                }
            });
        });
        self.shell.tools.panel_right = Some(area.response.rect.right());
        if connector && let Some(a) = ecad_cadcraft::actions::find("place.connector") {
            self.shell.tools.placing = State::default();
            self.start_tool(a);
            return;
        }
        if browse {
            self.set_workspace(super::Workspace::Library);
            return;
        }
        if let Some(k) = pick {
            self.shell.tools.placing.open = false;
            self.shell.tools.placing.symbol = Some(k);
            let _ = self.start_pick(Pick::Symbol(k));
        }
        if let Some(c) = choice {
            self.shell.tools.placing.open = false;
            let _ = self.start_choice(&c);
        }
    }

    /// The second step for symbol `key`, read again only when the project changed.
    fn picker_choices(&mut self, key: &'static str) -> Vec<Choice> {
        let Some(p) = self.project.clone() else { return Vec::new() };
        let count = self.change_count();
        if let Some(((k, n), v)) = &self.shell.tools.placing.choices
            && *k == key
            && *n == count
        {
            return v.clone();
        }
        let v = p.read(&mut self.cad.session, |p, _| crate::search::choices(p, &Pick::Symbol(key))).unwrap_or_default();
        self.shell.tools.placing.reads += 1;
        self.shell.tools.placing.choices = Some(((key, count), v.clone()));
        v
    }
}

/// The frame of the chip and the picker: the board's floating panel.
pub(crate) fn popover_frame() -> egui::Frame {
    let s = ecad_ui::SHELL;
    egui::Frame::new()
        .fill(ecad_ui::color(s.panel))
        .stroke(egui::Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line_strong)))
        .corner_radius(ecad_ui::style::corner(radius::POPOVER))
        .shadow(ecad_ui::style::shadow())
        .inner_margin(egui::Margin::same(space::L as i8))
}

/// The Symbol | Connector switch with P beside it (Symbol is shown while placing a symbol, and
/// Connector while the connector maker runs). True when the other side was clicked.
pub(crate) fn tabs(ui: &mut egui::Ui, symbol: bool) -> bool {
    let mut other = false;
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 32.0) / 2.0;
        for (label, on) in [(SYMBOL_TAB, symbol), (CONNECTOR_TAB, !symbol)] {
            let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), egui::Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, label));
            let s = ecad_ui::SHELL;
            let fill = if on { s.raised } else { s.bg };
            ui.painter().rect_filled(r, ecad_ui::style::corner(radius::BUTTON), ecad_ui::color(fill));
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                label,
                bar::font(ui, ui_type::BODY),
                ecad_ui::color(if on { s.text } else { s.text_2 }),
            );
            other |= resp.clicked() && !on;
        }
        prim::keycap(ui, "P");
    });
    other
}

/// A symbol drawn in `r` on a light tile (the sheet's paper), or a plain tile.
fn thumb(ui: &egui::Ui, r: Rect, sym: Option<&symbols::Starter>) {
    let paper = ecad_ui::color(ecad_ui::tokens::sheet::PAPER.sheet);
    ui.painter().rect_filled(r, ecad_ui::style::corner(radius::CHIP), paper);
    if let Some(d) = sym.and_then(|s| s.definition().ok()) {
        crate::library::browser::paint_symbol(ui.painter(), r.shrink(space::XS), d, ecad_ui::color(ecad_ui::tokens::sheet::PAPER.ink), None);
    }
}

/// One tile: the symbol, its name and where it comes from.
fn tile(ui: &mut egui::Ui, sym: &symbols::Starter, w: f32, on: bool) -> egui::Response {
    let s = ecad_ui::SHELL;
    let (r, resp) = ui.allocate_exact_size(vec2(w, TILE_H), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, sym.name));
    let fill = if on {
        s.accent_soft
    } else if resp.hovered() {
        s.hover
    } else {
        s.raised
    };
    let stroke = if on { s.accent } else { s.line };
    ui.painter().rect(
        r,
        ecad_ui::style::corner(radius::TILE),
        ecad_ui::color(fill),
        egui::Stroke::new(1.0, ecad_ui::color(stroke)),
        egui::StrokeKind::Inside,
    );
    thumb(ui, Rect::from_min_size(r.min + vec2(space::XS, space::XS), vec2(w - 2.0 * space::XS, 56.0)), Some(sym));
    let text = ui.painter().layout(sym.name.to_string(), bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text), w - 2.0 * space::XS);
    let y = r.top() + 62.0;
    ui.painter().galley(pos2(r.center().x - text.size().x / 2.0, y), text.clone(), ecad_ui::color(s.text));
    ui.painter().text(
        pos2(r.center().x, (y + text.size().y + 8.0).min(r.bottom() - 7.0)),
        egui::Align2::CENTER_CENTER,
        "IEC built-in",
        bar::font(ui, ui_type::CAPTION),
        ecad_ui::color(s.text_3),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chip_reads_the_tag_the_prompt_names() {
        assert_eq!(tag_in("Specify insertion point (new device -K02, rotation 0)").as_deref(), Some("-K02"));
        assert_eq!(tag_in("Specify insertion point (joins -K01 as 13/14, rotation 90)").as_deref(), Some("-K01"));
        assert_eq!(tag_in("Specify insertion point (rotation 0)"), None);
    }

    #[test]
    fn a_typed_symbol_is_its_key_or_its_one_match() {
        assert_eq!(typed_symbol(" lamp "), Some("lamp"));
        assert_eq!(typed_symbol("-K01"), None, "a device tag names no symbol");
    }
}
