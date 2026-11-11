//! The **Checks** tab of the Project panel (brief 022, design 022 §8): open ends, loose wires,
//! false-looking connections, nets with one connection point, unconnected device points and
//! number or potential conflicts, each row with where it is and a go-to.
//!
//! The rows are a copy of [`ecad_cadcraft::checks::Checks`], which the project builds whole on the
//! tab's first show and then keeps from each transaction's ops; the copy is taken again only when
//! that list's revision moved (or the project or the Show filter changed). Live: no refresh button.
//! The rows are virtualised, so a frame lays out only the visible ones.

use std::collections::BTreeSet;

use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::checks::{Check, Fix, Kind, Severity};
use eframe::egui::{self, Color32, RichText, Sense, pos2, vec2};

use crate::project_panel::{Action, ROW_H};

/// Accessible label of the tab.
pub const CHECKS_TAB: &str = "Checks";
/// What an empty list says.
pub const NO_PROBLEMS: &str = "No problems found.";
/// The fix buttons.
pub const ADD_JUNCTION: &str = "Add junction";
pub const NUMBER_THEM: &str = "Number them";
/// Brief 026: opens Finalise numbering (the Renumber dialog), never a direct write.
pub const FINALISE: &str = "Finalise";

/// One copied row, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub check: Check,
    /// `/2.1`.
    pub locator: String,
}

/// The accessible label of a row: `Warning: Open end, /3.2, -W01 3`.
pub fn row_label(r: &Row) -> String {
    let sev = match r.check.severity() {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
        Severity::Info => "Info",
    };
    format!("{sev}: {}, {}, {}", r.check.what(), r.locator, r.check.which)
}

/// `text` cut with an ellipsis to fit `width` points.
fn fit(p: &egui::Painter, text: &str, font: &egui::FontId, width: f32) -> String {
    let w = |s: &str| p.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x;
    if w(text) <= width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        n -= 1;
        let cut: String = chars.iter().take(n).collect::<String>() + "\u{2026}";
        if w(&cut) <= width {
            return cut;
        }
    }
    String::new()
}

/// The tab's state.
#[derive(Debug, Default)]
pub struct ChecksPanel {
    rows: Vec<Row>,
    /// (project serial, list revision, filter) the copy was taken at.
    seen: Option<(u64, u64, BTreeSet<Kind>)>,
    /// Kinds unticked in Show (kept for the session).
    pub hidden: BTreeSet<Kind>,
    /// The row gone to last (highlighted).
    pub selected: Option<usize>,
    /// After a click in the list, ↑/↓ move between rows and Enter goes to.
    pub armed: bool,
    /// Counts of the shown rows: errors, warnings.
    counts: (usize, usize),
    /// How many times the copy was taken (tests: only when the list changed).
    pub copies: u64,
    /// Rows laid out on the last frame (tests: virtualised).
    pub rows_drawn: usize,
    /// The project cannot be written: no fix buttons.
    pub read_only: bool,
}

impl ChecksPanel {
    /// The rows shown now.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The title of the tab: `Checks (5)`.
    pub fn title(&self) -> String {
        if self.rows.is_empty() { CHECKS_TAB.to_string() } else { format!("{CHECKS_TAB} ({})", self.rows.len()) }
    }

    /// Forget the copy: the next [`ChecksPanel::sync`] takes it again.
    pub fn invalidate(&mut self) {
        self.seen = None;
    }

    /// Bring the copy up to date with the project's kept list (built on the first call).
    pub fn sync(&mut self, serial: u64, p: &mut ecad_cadcraft::Project) {
        let hidden = self.hidden.clone();
        let revision = p.checks().revision;
        if self.seen.as_ref().is_some_and(|(s, r, h)| *s == serial && *r == revision && *h == hidden) {
            return;
        }
        // Built now if it was not; read without changing anything from here on.
        let p: &ecad_cadcraft::Project = p;
        let doc = p.ledger.document();
        let Some(list) = p.checks.as_ref() else { return };
        let rows: Vec<Row> =
            list.sorted(doc).into_iter().filter(|c| !hidden.contains(&c.kind)).map(|c| Row { locator: c.locator(doc), check: c }).collect();
        self.counts = (
            rows.iter().filter(|r| r.check.severity() == Severity::Error).count(),
            rows.iter().filter(|r| r.check.severity() == Severity::Warning).count(),
        );
        self.rows = rows;
        if self.selected.is_some_and(|i| i >= self.rows.len()) {
            self.selected = None;
        }
        self.seen = Some((serial, revision, hidden));
        self.copies += 1;
    }

    fn colour(t: &Tokens, s: Severity) -> Color32 {
        match s {
            Severity::Error => Color32::from_rgb(0xFF, 0x00, 0x00),
            Severity::Warning => Color32::from_rgb(0xFF, 0xFF, 0x00),
            Severity::Info => t.text_dim,
        }
    }

    /// The go-to of row `i`.
    fn goto(&self, i: usize) -> Option<Action> {
        let r = self.rows.get(i)?;
        Some(Action::GotoCheck(Box::new(r.check.clone())))
    }

    /// The tab's contents. Returns the actions asked for.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        let mut out = Vec::new();
        let t = Tokens::get();
        ui.horizontal(|ui| {
            let label = if self.hidden.is_empty() { "All kinds" } else { "Some kinds" };
            ui.label(RichText::new("Show:").small());
            ui.menu_button(RichText::new(format!("{label} \u{25be}")).small(), |ui| {
                for k in Kind::ALL {
                    let mut on = !self.hidden.contains(&k);
                    if ui.checkbox(&mut on, k.words()).changed() {
                        if on {
                            self.hidden.remove(&k);
                        } else {
                            self.hidden.insert(k);
                        }
                    }
                }
            });
            let (e, w) = self.counts;
            let words = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
            ui.label(RichText::new(format!("{} \u{b7} {}", words(e, "error", "errors"), words(w, "warning", "warnings"))).small().color(t.text_dim));
        });
        if self.rows.is_empty() {
            ui.label(RichText::new(NO_PROBLEMS).color(t.text_dim));
            self.rows_drawn = 0;
            return out;
        }
        if self.armed {
            let (up, down, enter) =
                ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::Enter)));
            let n = self.rows.len();
            if up || down {
                let cur = self.selected.unwrap_or(0);
                let next = if up { cur.saturating_sub(1) } else { (cur + 1).min(n.saturating_sub(1)) };
                self.selected = Some(next);
            }
            if enter && let Some(a) = self.selected.and_then(|i| self.goto(i)) {
                out.push(a);
            }
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut drawn = 0;
        let mut clicked: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt("ecad.checks.list").auto_shrink([false, false]).show_rows(ui, ROW_H, self.rows.len(), |ui, range| {
            for i in range {
                let Some(r) = self.rows.get(i).cloned() else { continue };
                drawn += 1;
                let fix = r.check.fix.filter(|_| writable && !self.read_only);
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                let p = ui.painter_at(rect);
                let selected = self.selected == Some(i);
                if selected {
                    p.rect_filled(rect, 0.0, t.tab_active);
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, t.chrome);
                }
                let sev = r.check.severity();
                let font = egui::FontId::monospace(12.0);
                p.text(pos2(rect.left() + 4.0, rect.center().y), egui::Align2::LEFT_CENTER, sev.glyph(), font.clone(), Self::colour(&t, sev));
                let summary = matches!(r.check.kind, Kind::Unnumbered | Kind::ProvisionalWires);
                let what = if summary { r.check.which.clone() } else { r.check.what() };
                let colour = if selected { t.text } else { t.text_dim };
                // Where and which, on the right; a fix button beside them when it has one. The
                // description takes the room left, cut with an ellipsis.
                let right = if fix.is_some() { rect.right() - 92.0 } else { rect.right() - 4.0 };
                let tail = if summary { String::new() } else { format!("{} {}", r.locator, r.check.which) };
                let tail_g = p.layout_no_wrap(tail, font.clone(), t.text_dim);
                let tail_w = tail_g.size().x;
                let room = (right - tail_w - 6.0 - (rect.left() + 18.0)).max(0.0);
                let what = fit(&p, &what, &font, room);
                p.text(pos2(rect.left() + 18.0, rect.center().y), egui::Align2::LEFT_CENTER, what, font.clone(), colour);
                let top = rect.center().y - tail_g.size().y / 2.0;
                p.galley(pos2(right - tail_w, top), tail_g, t.text_dim);
                let label = row_label(&r);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &label));
                if let Some(f) = fix {
                    let b = egui::Rect::from_min_size(pos2(rect.right() - 88.0, rect.top() + 2.0), vec2(84.0, ROW_H - 4.0));
                    let name = match f {
                        Fix::AddJunction(_) => ADD_JUNCTION,
                        Fix::NumberThem => NUMBER_THEM,
                        Fix::Finalise => FINALISE,
                    };
                    if ui.put(b, egui::Button::new(RichText::new(name).small())).clicked() {
                        match (f, r.check.page) {
                            (Fix::AddJunction(at), Some(page)) => out.push(Action::AddJunction(page, at)),
                            (Fix::NumberThem, _) => out.push(Action::NumberThem),
                            (Fix::Finalise, _) => out.push(Action::Finalise),
                            _ => {}
                        }
                        continue;
                    }
                }
                if resp.on_hover_text("Show it: its page, centred, with a ring on it").clicked() {
                    clicked = Some(i);
                }
            }
        });
        self.rows_drawn = drawn;
        if let Some(i) = clicked {
            self.selected = Some(i);
            self.armed = true;
            if let Some(a) = self.goto(i) {
                out.push(a);
            }
        }
        out
    }
}
