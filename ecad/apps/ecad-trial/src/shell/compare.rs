//! Compare revisions (UI rev 3, board 1.16), the layout only: a header over the canvas naming the
//! revisions compared, with Close compare, and the list of changes under the canvas with the
//! board's columns. The changes, their numbered clouds and the selected change's command come with
//! Compare itself (rev 3 build order, step 7); until then the list says so, and nothing here
//! reads the history.
//!
//! It opens from History ([`Trial::open_compare`]) and shows in Draw only.

use ecad_ui::prim::Button;
use ecad_ui::tokens::{line, space, ui_type};
use eframe::egui::{self, Rect, pos2, vec2};

use super::{Slots, bar};
use crate::Trial;

/// The header's Close compare button.
pub const CLOSE: &str = "Close compare";
/// The list's columns (board 1.16) and their widths, px; the change takes the rest.
pub const COLUMNS: [(&str, f32); 4] = [("Cloud", 56.0), ("Zone", 70.0), ("Change since", 0.0), ("By", 96.0)];
/// What the list says until Compare is built.
pub const NOT_YET: &str = "The changes since the last revision, with their clouds on the page, will list here when Compare comes.";

impl Trial {
    /// Open Compare revisions over the canvas (Draw).
    pub fn open_compare(&mut self) {
        self.shell.state.workspace = super::Workspace::Draw;
        self.shell.state.compare = true;
    }

    /// Close Compare revisions.
    pub fn close_compare(&mut self) {
        self.shell.state.compare = false;
    }
}

/// `Rev C → Now, D*`: the last issued revision and the pending one, from the bottom bar's facts.
fn title(t: &Trial) -> (String, String) {
    match &t.shell.well.rev {
        Some((last, pending, dirty)) => {
            let from = if last.is_empty() { "the start".to_string() } else { format!("Rev {last}") };
            let star = if *dirty { "*" } else { "" };
            (from, format!("Now, {pending}{star}"))
        }
        None => ("Rev –".to_string(), "Now".to_string()),
    }
}

/// Draw the header and the list, while Compare is open.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    let Some((head, list)) = slots.compare else { return };
    let s = ecad_ui::SHELL;
    let hair = egui::Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line));
    let (from, to) = title(t);
    // Header: `Rev C → Now, D*`, then Close compare at the right.
    super::fill(ui, head, s.panel);
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, hair);
    let y = head.center().y;
    let mut x = bar::text_at(ui, head.left() + space::M, y, &from, ui_type::BODY, ecad_ui::color(s.text));
    x = bar::text_at(ui, x, y, "  →  ", ui_type::BODY, ecad_ui::color(s.text_3));
    x = bar::text_at(ui, x, y, &to, ui_type::BODY, ecad_ui::color(s.text));
    bar::text_at(ui, x, y, "  ·  the changes are listed once Compare comes", ui_type::SMALL, ecad_ui::color(s.text_3));
    let close_w = bar::text_width(ui, CLOSE, ui_type::BODY) + 2.0 * space::M;
    let close_r = Rect::from_center_size(pos2(head.right() - space::M - close_w / 2.0, y), vec2(close_w, 28.0));
    let close = super::in_rect(ui, close_r, "ecad.compare.close", |ui| ui.add(Button::new(CLOSE))).clicked();

    // The list: its column heads, then the placeholder line.
    super::fill(ui, list, s.panel);
    ui.painter().hline(list.x_range(), list.top() + 0.5, hair);
    let row_h = 28.0;
    let heads = Rect::from_min_size(list.min, vec2(list.width(), row_h));
    let fixed: f32 = COLUMNS.iter().map(|c| c.1).sum();
    let mut cx = heads.left() + space::M;
    for (name, w) in COLUMNS {
        let w = if w == 0.0 { (heads.width() - 2.0 * space::M - fixed).max(0.0) } else { w };
        let text = if name == "Change since" { format!("{name} {from}") } else { name.to_string() };
        bar::text_at(ui, cx, heads.center().y, &text, ui_type::SMALL, ecad_ui::color(s.text_3));
        cx += w;
    }
    ui.painter().hline(heads.x_range(), heads.bottom() - 0.5, hair);
    let r = ui.interact(heads, egui::Id::unique("ecad.compare.list"), egui::Sense::hover());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, NOT_YET));
    bar::text_at(ui, list.left() + space::M, heads.bottom() + row_h / 2.0, NOT_YET, ui_type::BODY, ecad_ui::color(s.text_2));

    if close {
        t.close_compare();
    }
}
