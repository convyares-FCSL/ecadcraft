//! The intent line (brief 041 step 4, spec §3 "CAD command line: removed"): a 28 px strip at the
//! bottom of the page that replaces CADCraft's command line, at rest and while a tool prompts.
//! Output only: typing still goes to the running prompt, as before.
//!
//! - While a tool runs: its prompt, its keywords as chips that answer it, what is being typed,
//!   and the electrical intent at the cursor where it is known (`interact::intent`: "joins A1 of
//!   -K01").
//! - A refusal (spec §5): what was refused, why, and **Show it**; a line a running tool echoed
//!   (its refusal of a click) shows the same way.
//! - At rest: how to reach things (no project read).
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
/// What the line says at rest.
pub const AT_REST: &str = "Right-click an object for what you can do with it · V W P C D B T start tools · Ctrl+K searches";

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

    /// Draw the intent line at the bottom of the shown page (Draw only), with the developer
    /// switch off.
    pub(crate) fn intent_frame(&mut self, ui: &mut egui::Ui) {
        self.track_tool_messages();
        let rect = match (self.shell.page_rect, self.shell.state.workspace) {
            (Some(r), super::Workspace::Draw) if !self.shell.state.cad_chrome => Rect::from_min_max(egui::pos2(r.left(), r.bottom() - HEIGHT), r.max),
            _ => {
                self.shell.tools.intent_text.clear();
                return;
            }
        };
        let (parts, keywords, sentence) = self.intent_parts(ui.ctx());
        super::fill(ui, rect, ecad_ui::SHELL.panel);
        ui.painter().hline(rect.x_range(), rect.top(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(ecad_ui::SHELL.line)));
        let refusal = self.shell.tools.intent.refusal.clone();
        let note = self.shell.tools.intent.note.clone();
        let mut keyword = None;
        let (mut show_it, mut dismiss) = (false, false);
        let mut text = String::new();
        super::in_rect(ui, rect.shrink2(egui::vec2(space::M, 2.0)), LINE, |ui| {
            ui.horizontal_centered(|ui| {
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
                    let t = note.clone().unwrap_or_else(|| AT_REST.to_string());
                    prim::read_value(ui, &t);
                    text = t;
                    return;
                }
                for p in &parts {
                    prim::read_value(ui, p);
                }
                for k in &keywords {
                    if prim::Chip::new(k).show(ui).response.clicked() {
                        keyword = Some(k.clone());
                    }
                }
                if let Some(s) = &sentence {
                    prim::pill(ui, s, Tone::Ok);
                }
                text = [parts.join(" "), keywords.join(" "), sentence.clone().unwrap_or_default()].join(" ");
            });
        });
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
