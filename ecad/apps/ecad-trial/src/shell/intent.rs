//! The intent line (brief 041 step 4, spec §3 "CAD command line: removed"): a 28 px strip that
//! replaces CADCraft's command line while a tool prompts. UI rev 3: it sits at the top of the
//! canvas, so it never covers the title block, and shows only when it has something to say
//! (nothing permanent sits over the sheet). Output only: typing still goes to the running prompt,
//! as before.
//!
//! - While a tool runs: its prompt, its keywords as chips that answer it, what is being typed,
//!   and the electrical intent at the cursor where it is known (`interact::intent`: "joins A1 of
//!   -K01").
//! - A refusal (spec §5): what was refused, why, and **Show it**; a line a running tool echoed
//!   (its refusal of a click) shows the same way.
//! - At rest: nothing (rev 3; 041 showed how to reach things here).
//!
//! Update map: the sentence is recomputed only when the tool, the cursor's pixel or the project's
//! change count moved, and only while a tool runs; at rest nothing reads the project
//! ([`Intent::reads`] counts reads).

use cadcraft_engine::Input;
use ecad_ui::prim::{self, Tone};
use ecad_ui::tokens::space;
use eframe::egui::{self, Rect};

use super::wheel::Refusal;
use crate::Trial;

/// The strip's height.
pub const HEIGHT: f32 = 28.0;
/// Accessible labels.
pub const LINE: &str = "Intent line";
pub const SHOW_IT: &str = "Show it";
pub const DISMISS: &str = "Dismiss";

/// The intent line's state between frames.
#[derive(Debug, Clone, Default)]
pub struct Intent {
    /// The sentence for (tool, cursor pixel, change count).
    cache: Option<(Key, Option<String>)>,
    /// How many times the sentence was worked out from the project (tests: 0 at rest).
    pub reads: u64,
    pub refusal: Option<Refusal>,
    /// A plain note from the last action (Show net's count).
    pub note: Option<String>,
    /// The command history's length last frame, and the tool then running.
    log_len: usize,
    log_tool: Option<String>,
}

type Key = (String, i64, i64, u64);

impl Trial {
    /// The intent line's text this frame, as one string (tests).
    pub fn intent_text(&self) -> String {
        self.shell.tools.intent_text.clone()
    }

    /// Draw the intent line at the top of the shown page (Draw only), with the developer switch
    /// off, while a tool prompts or a refusal or note waits.
    pub(crate) fn intent_frame(&mut self, ui: &mut egui::Ui) {
        self.track_tool_messages();
        let rect = match (self.shell.page_rect, self.shell.state.workspace) {
            (Some(r), super::Workspace::Draw) if !self.shell.state.cad_chrome => Rect::from_min_size(r.min, egui::vec2(r.width(), HEIGHT)),
            _ => {
                self.shell.tools.intent_text.clear();
                return;
            }
        };
        let (parts, keywords, sentence) = self.intent_parts(ui.ctx());
        if parts.is_empty() && self.shell.tools.intent.refusal.is_none() && self.shell.tools.intent.note.is_none() {
            self.shell.tools.intent_text.clear();
            return;
        }
        let refusal = self.shell.tools.intent.refusal.clone();
        let note = self.shell.tools.intent.note.clone();
        let (icon, title) = self.intent_title();
        let mut keyword = None;
        let (mut show_it, mut dismiss) = (false, false);
        let mut text = String::new();
        // UI rev 3 (boards 1.11 to 1.13): a floating line centred at the top of the canvas, as
        // wide as what it says, so it never covers the title block.
        let s = ecad_ui::SHELL;
        let frame = egui::Frame::new()
            .fill(ecad_ui::color(s.panel))
            .stroke(egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line_strong)))
            .corner_radius(ecad_ui::style::corner(ecad_ui::tokens::radius::POPOVER))
            .shadow(ecad_ui::style::shadow())
            .inner_margin(egui::Margin::symmetric(space::L as i8, space::S as i8));
        // Beside the picker, its chip or the connector maker when one shows (boards 1.11, 1.12).
        let left = self.shell.tools.panel_right.map_or(rect.left() + space::XXL, |r| r + space::XL);
        let max_w = (rect.right() - space::XXL - left).max(200.0);
        let last_w = ui.ctx().memory(|m| m.area_rect(egui::Id::unique("ecad.shell.intent"))).map_or(max_w, |r| r.width());
        let x = (rect.center().x - last_w / 2.0).max(left);
        egui::Area::new(egui::Id::unique("ecad.shell.intent")).order(egui::Order::Foreground).fixed_pos(egui::pos2(x, rect.top() + space::L)).show(
            ui.ctx(),
            |ui| {
                frame.show(ui, |ui| {
                    ui.set_max_width(max_w);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = space::M;
                        let r = ui.label("");
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, LINE));
                        if let Some(rf) = &refusal {
                            prim::pill(ui, &rf.what, if rf.refused { Tone::Error } else { Tone::Neutral });
                            prim::read_value(ui, &rf.why);
                            text = format!("{}: {}", rf.what, rf.why);
                            show_it = ui.add(prim::Button::new(SHOW_IT).small()).clicked();
                            dismiss = ui.add(prim::Button::new(DISMISS).kind(prim::Kind::Quiet).small()).clicked();
                            return;
                        }
                        if parts.is_empty() {
                            let t = note.clone().unwrap_or_default();
                            prim::read_value(ui, &t);
                            text = t;
                            return;
                        }
                        // The tool's icon and name, a divider, then what the click will do.
                        let (ir, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ecad_ui::icons::paint(ui.painter(), ir, icon, ecad_ui::color(s.accent_text));
                        ui.label(
                            egui::RichText::new(&title)
                                .font(ecad_ui::fonts::font(ui.ctx(), ecad_ui::tokens::ui_type::BODY_EMPHASIS))
                                .color(ecad_ui::color(s.text)),
                        );
                        ui.separator();
                        for p in parts.iter().skip(1) {
                            prim::read_value(ui, p);
                        }
                        if let Some(sn) = &sentence {
                            ui.label(
                                egui::RichText::new(sn)
                                    .font(ecad_ui::fonts::font(ui.ctx(), ecad_ui::tokens::ui_type::BODY))
                                    .color(ecad_ui::color(s.accent_text)),
                            );
                        }
                        for k in &keywords {
                            if prim::Chip::new(k).show(ui).response.clicked() {
                                keyword = Some(k.clone());
                            }
                        }
                        prim::keycap(ui, "Esc");
                        prim::read_value(ui, "finish");
                        text = [parts.join(" "), keywords.join(" "), sentence.clone().unwrap_or_default()].join(" ");
                    });
                });
            },
        );
        self.shell.tools.intent_text = text;
        if let Some(k) = keyword {
            let _ = self.cad.session.input(Input::Keyword(k));
            self.after_logic();
        }
        if dismiss {
            self.shell.tools.intent.refusal = None;
        }
        if show_it && let Some(rf) = refusal {
            self.show_refusal(&rf);
        }
    }

    /// The running tool's icon and name for the intent line: the symbol being placed, or the
    /// tool's label in the action table, or the command's own name.
    fn intent_title(&self) -> (ecad_ui::tokens::Icon, String) {
        let tool = self.cad.session.running.as_ref().map(|r| r.id.to_lowercase()).unwrap_or_default();
        if tool == "place"
            && let Some(sym) = self.shell.tools.placing.symbol.and_then(ecad_cadcraft::symbols::by_key)
        {
            return (ecad_ui::tokens::Icon::Pin, sym.name.to_string());
        }
        let a = ecad_cadcraft::actions::ACTIONS.iter().find(|a| a.group.is_some() && a.command.eq_ignore_ascii_case(&tool));
        match a {
            Some(a) => (a.icon, a.label.to_string()),
            None => {
                let mut t = tool.clone();
                if let Some(c) = t.get_mut(..1) {
                    c.make_ascii_uppercase();
                }
                (ecad_ui::tokens::Icon::Select, t)
            }
        }
    }

    /// The prompt, the typed text, the keywords and the sentence (empty parts at rest).
    fn intent_parts(&mut self, ctx: &egui::Context) -> (Vec<String>, Vec<String>, Option<String>) {
        let Some(prompt) = self.cad.session.current_prompt() else {
            self.shell.tools.intent.cache = None;
            return (Vec::new(), Vec::new(), None);
        };
        let tool = self.cad.session.running.as_ref().map(|r| r.id.clone()).unwrap_or_default();
        let mut parts = vec![format!("{}:", tool.to_uppercase()), prompt.message.trim().trim_end_matches(':').to_string()];
        let typed = self.cad.cmd.buffer.trim().to_string();
        if !typed.is_empty() {
            parts.push(format!("› {typed}"));
        }
        let sentence = self.sentence(ctx, &tool);
        (parts, prompt.keywords.clone(), sentence)
    }

    /// The electrical intent at the cursor, cached per (tool, cursor pixel, change count).
    fn sentence(&mut self, ctx: &egui::Context, tool: &str) -> Option<String> {
        if !matches!(tool, "wire" | "connect") {
            return None;
        }
        let pos = ctx.pointer_hover_pos()?;
        if !self.over_shown_page(ctx, pos) {
            return None;
        }
        let count = self.change_count();
        let key: Key = (tool.to_string(), pos.x.round() as i64, pos.y.round() as i64, count);
        if let Some((k, s)) = &self.shell.tools.intent.cache
            && *k == key
        {
            return s.clone();
        }
        let cursor = self.cad.canvas.xf.map_or(self.cad.session.cursor, |xf| xf.to_world(pos));
        let s = ecad_cadcraft::interact::intent::at_cursor(&self.cad.session, tool, cursor);
        self.shell.tools.intent.reads += 1;
        self.shell.tools.intent.cache = Some((key, s.clone()));
        s
    }

    /// A line the running tool echoed (its refusal of a click) becomes the line's refusal.
    fn track_tool_messages(&mut self) {
        let len = self.cad.session.log.len();
        let tool = self.cad.session.running.as_ref().map(|r| r.id.clone());
        let i = &mut self.shell.tools.intent;
        if let (Some(t), Some(prev)) = (&tool, &i.log_tool)
            && t == prev
            && len > i.log_len
            && let Some(line) = self.cad.session.log.iter().skip(i.log_len).rev().find(|l| !l.starts_with("Command:") && !l.trim().is_empty())
        {
            i.refusal = Some(Refusal {
                what: t.to_uppercase(),
                why: line.trim().to_string(),
                page: self.panel.active,
                at: Some([self.cad.session.cursor.x, self.cad.session.cursor.y]),
                refused: false,
            });
        }
        if tool.is_some() && tool != i.log_tool {
            i.refusal = None;
            i.note = None;
        }
        i.log_len = len;
        i.log_tool = tool;
    }

    /// **Show it**: the refused place, centred on the page it is on.
    fn show_refusal(&mut self, rf: &Refusal) {
        if let Some(pg) = rf.page
            && self.panel.active != Some(pg)
        {
            let _ = self.show_page(pg);
        }
        if let (Some(at), Ok(st)) = (rf.at, self.cad.session.state_mut()) {
            let mut v = st.view();
            v.center = cadcraft_engine::geom::Vec2::new(at[0], at[1]);
            let _ = st.set_view(v);
        }
    }
}
