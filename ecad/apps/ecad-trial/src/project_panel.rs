//! The **Project** panel (brief 016, design 016 §2–§3, §7): the project's pages in page order on
//! the far left, one click to make a page the active page, and the page actions (new, rename,
//! reorder, preview) through the existing page commands. Since the trial's top banner was removed
//! (owner, 2026-11-08) its header also holds the project's status, file actions and the
//! experiment note.
//!
//! The panel only reads: it keeps a copy of the rows ([`ecad_cadcraft::PageRow`]) and rebuilds it
//! when the project's change count moves (one integer compared per frame; a rebuild reads
//! `page_order` and page titles only). The list is virtualised, so a frame draws only the visible
//! rows whatever the page count. Everything it does is returned as [`Action`]s for the trial to
//! run through the command path.
//!
//! Excluded: deleting pages (no command exists), lists of cables, components or references.

use cadcraft_ui_egui::icons::{self, Icon};
use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::PageRow;
use ecad_model::PageId;
use eframe::egui::{self, Color32, Key, Modifiers, Rect, RichText, Sense, Stroke, pos2, vec2};

/// Row height (design 016 §2.2).
pub const ROW_H: f32 = 20.0;
/// Width of the collapsed strip.
pub const STRIP_W: f32 = 22.0;
/// Accessible labels of the header buttons (design 016 §8 names them `ecad.project.*`).
pub const NEW_PAGE: &str = "New page";
pub const MOVE_UP: &str = "Move page up";
pub const MOVE_DOWN: &str = "Move page down";
pub const PREVIEW: &str = "Preview page";
pub const COLLAPSE: &str = "Collapse the Project panel";
pub const EXPAND: &str = "Project";
/// Accessible label of the rename field.
pub const RENAME: &str = "Page title";
/// Said where writing is disabled.
pub const READ_ONLY: &str = "The project is read-only.";
/// Said by Preview with no row picked.
pub const PICK_FIRST: &str = "Click a page first.";
/// Said when the active page itself is asked for a preview.
pub const ALREADY_SHOWN: &str = "This page is already shown.";

/// The accessible label of row `number` with `title`: `Page 2: Field`.
pub fn row_label(number: &str, title: &str) -> String {
    format!("Page {number}: {title}")
}

/// What the user asked the panel for.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Make this page the active page (`openpage`).
    Open(PageId),
    /// `newpage` titled with its number, then rename it in place.
    New,
    /// `renamepage` with this title (trimmed by the command).
    Rename(PageId, String),
    /// `movepage` to this 1-based place.
    Move(PageId, usize),
    /// Open (or close, when it is shown) the preview of this page.
    Preview(PageId),
    /// Header file actions, moved from the trial's banner.
    NewProject,
    OpenProject,
    Save,
    SaveAs,
    CloseProject,
    /// Devices tab: show this symbol's page and select it.
    GotoSymbol(ecad_model::RepresentationId),
    /// Devices tab: `setprop` of a device property (brief 018's rules; `merge` takes another
    /// device's tag by making this device part of it).
    SetDeviceProperty {
        component: ecad_model::ComponentId,
        key: &'static str,
        value: String,
        merge: bool,
    },
    /// Rename this page in the name box (the row's right-click menu).
    RenameInBox(PageId),
    /// Checks tab (brief 022): show this row's page, centre on it, select it, ring it.
    GotoCheck(Box<ecad_cadcraft::checks::Check>),
    /// Checks tab: `junction {at}` on this page.
    AddJunction(PageId, ecad_model::Pt),
    /// Checks tab: `renumberwires {scope: "unnumbered"}`.
    NumberThem,
}

/// Accessible label of the Developer section's header ([`crate::developer`]).
pub const DEVELOPER: &str = crate::developer::HEADER;

/// The panel's two tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Pages,
    Devices,
    /// Brief 022.
    Checks,
}

/// Accessible labels of the tabs.
pub const PAGES_TAB: &str = "Pages";
pub const DEVICES_TAB: &str = "Devices";

/// A rename in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    pub page: PageId,
    pub text: String,
    /// The command's refusal, shown under the field.
    pub error: Option<String>,
    /// The title the field opened with (selected when it opens).
    old: String,
}

/// A row being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    page: PageId,
    from: usize,
    /// The gap the row would land in (0 = before the first row), while over the list.
    gap: Option<usize>,
}

/// What the header shows about the project, read by the trial each frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    /// `None`: no project is open.
    pub name: Option<String>,
    /// The banner's old status line.
    pub status: String,
    /// Read-only and unconfirmed-save warnings.
    pub warnings: Vec<String>,
    pub read_only: bool,
    /// The experiment note, as one small line.
    pub note: &'static str,
    /// Snapshots can be exported (`--snapshots`).
    pub snapshots: bool,
}

/// The panel's state.
#[derive(Debug, Default)]
pub struct ProjectPanel {
    pub collapsed: bool,
    /// Pages or Devices.
    pub tab: Tab,
    /// The Devices tab.
    pub devices: crate::devices_panel::DevicesPanel,
    /// The Checks tab (brief 022).
    pub checks: crate::checks_panel::ChecksPanel,
    /// The copy of the project's pages.
    rows: Vec<PageRow>,
    /// The project change count the copy was taken at, and which project (its pointer value).
    seen: Option<(u64, u64)>,
    /// The active page, read each frame.
    pub active: Option<PageId>,
    /// The page previewed now, for its row's icon.
    pub previewed: Option<PageId>,
    /// The keyboard cursor (a row index).
    pub cursor: Option<usize>,
    /// After a click in the panel, it takes Up/Down/Enter/F2/Space/Alt+arrows/Esc (design §3.3).
    pub armed: bool,
    pub rename: Option<Rename>,
    drag: Option<Drag>,
    /// Scroll the row with this index into view on the next frame.
    reveal: Option<usize>,
    /// How many times the copy was rebuilt (tests: only when the project changed).
    pub rebuilds: u64,
    /// How many rows the last frame laid out (tests: the list is virtualised).
    pub rows_drawn: usize,
    /// A message under the header for one action (for example "Click a page first.").
    pub note: Option<String>,
}

impl ProjectPanel {
    /// The rows of the copy.
    pub fn rows(&self) -> &[PageRow] {
        &self.rows
    }

    /// Bring the copy up to date: `count` is the project's change count, `project` names the
    /// project, `rows` reads the rows (called only when either differs from the copy's).
    pub fn sync(&mut self, project: u64, count: u64, rows: impl FnOnce() -> Option<Vec<PageRow>>) {
        if self.seen == Some((project, count)) {
            return;
        }
        if let Some(r) = rows() {
            self.rows = r;
            self.seen = Some((project, count));
            self.rebuilds += 1;
            if let Some(c) = self.cursor
                && c >= self.rows.len()
            {
                self.cursor = self.rows.len().checked_sub(1);
            }
            if self.rename.as_ref().is_some_and(|r| !self.rows.iter().any(|x| x.id == r.page)) {
                self.rename = None;
            }
        }
    }

    /// Forget the project (it was closed).
    pub fn clear(&mut self) {
        *self = ProjectPanel { collapsed: self.collapsed, tab: self.tab, ..ProjectPanel::default() };
    }

    /// The row being dragged and the gap it would land in (0 = before the first row), if any.
    pub fn dragging(&self) -> Option<(PageId, Option<usize>)> {
        self.drag.map(|d| (d.page, d.gap))
    }

    /// The row index of `page`.
    pub fn index_of(&self, page: PageId) -> Option<usize> {
        self.rows.iter().position(|r| r.id == page)
    }

    /// Open the rename field on `page` with its title selected.
    pub fn start_rename(&mut self, page: PageId) {
        if let Some(i) = self.index_of(page) {
            let title = self.rows.get(i).map(|r| r.title.clone()).unwrap_or_default();
            self.rename = Some(Rename { page, text: title.clone(), error: None, old: title });
            self.cursor = Some(i);
            self.reveal = Some(i);
        }
    }

    /// Scroll `page`'s row into view on the next frame.
    pub fn reveal(&mut self, page: PageId) {
        self.reveal = self.index_of(page);
    }

    /// The row the header's Move and Preview act on: the cursor row, else the active row.
    fn target(&self) -> Option<usize> {
        self.cursor.or_else(|| self.active.and_then(|a| self.index_of(a)))
    }

    /// The collapsed strip: one click expands the panel.
    pub fn strip(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get();
        let rect = ui.max_rect();
        let resp = ui.interact(rect, egui::Id::unique("ecad.project.expand"), Sense::click());
        ui.painter().rect_filled(rect, 0.0, t.chrome);
        let icon = Rect::from_min_size(pos2(rect.left() + 2.0, rect.top() + 4.0), vec2(18.0, 18.0));
        icons::paint(ui.painter(), icon, Icon::ChevronRight, false);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, EXPAND));
        if resp.on_hover_text(EXPAND).clicked() {
            self.collapsed = false;
        }
    }

    /// The panel's contents. Returns the actions asked for this frame.
    pub fn show(&mut self, ui: &mut egui::Ui, header: &Header) -> Vec<Action> {
        let mut out = Vec::new();
        let t = Tokens::get();
        let panel_rect = ui.max_rect();
        let writable = header.name.is_some() && !header.read_only;
        self.keyboard(ui, writable, &mut out);

        // Header: PROJECT and the icon buttons.
        ui.horizontal(|ui| {
            ui.label(RichText::new("PROJECT").small().strong().color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, "ecad.project.collapse", Icon::ChevronLeft, COLLAPSE, true).clicked() {
                    self.collapsed = true;
                }
                if header.name.is_none() {
                    return;
                }
                let target = self.target();
                let preview_tip = if target.is_some() { PREVIEW } else { PICK_FIRST };
                if icon_button(ui, "ecad.project.preview", Icon::Preview, preview_tip, target.is_some()).clicked()
                    && let Some(r) = target.and_then(|i| self.rows.get(i))
                {
                    self.armed = true;
                    out.push(Action::Preview(r.id));
                }
                let last = self.rows.len().saturating_sub(1);
                let down_ok = writable && target.is_some_and(|i| i < last);
                let tip = |ok_tip: &'static str| if writable { ok_tip } else { READ_ONLY };
                if icon_button(ui, "ecad.project.down", Icon::ChevronDown, tip(MOVE_DOWN), down_ok).clicked()
                    && let Some(i) = target
                {
                    self.move_row(i, i + 1, &mut out);
                }
                let up_ok = writable && target.is_some_and(|i| i > 0);
                if icon_button(ui, "ecad.project.up", Icon::ChevronUp, tip(MOVE_UP), up_ok).clicked()
                    && let Some(i) = target
                {
                    self.move_row(i, i - 1, &mut out);
                }
                if icon_button(ui, "ecad.project.new", Icon::Plus, tip(NEW_PAGE), writable).clicked() {
                    self.armed = true;
                    out.push(Action::New);
                }
            });
        });
        self.status(ui, header, &mut out);
        ui.separator();

        if header.name.is_none() {
            ui.label(RichText::new("No project open.").color(t.text_dim));
            self.rows_drawn = 0;
            return out;
        }

        // Pages | Devices.
        ui.horizontal(|ui| {
            let checks = self.checks.title();
            for (tab, name) in [(Tab::Pages, PAGES_TAB), (Tab::Devices, DEVICES_TAB), (Tab::Checks, checks.as_str())] {
                let resp = ui.selectable_label(self.tab == tab, RichText::new(name).small());
                let resp = if tab == Tab::Checks {
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            self.tab == tab,
                            crate::checks_panel::CHECKS_TAB,
                        )
                    });
                    resp
                } else {
                    resp
                };
                if resp.clicked() {
                    if tab == Tab::Devices && self.tab != Tab::Devices {
                        // Shown again after being hidden: rebuilt once, then kept up to date.
                        self.devices.invalidate();
                    }
                    if tab == Tab::Checks && self.tab != Tab::Checks {
                        self.checks.invalidate();
                    }
                    self.tab = tab;
                    self.armed = false;
                }
            }
        });
        if self.tab == Tab::Devices {
            out.extend(self.devices.show(ui, writable));
            self.rows_drawn = 0;
            return out;
        }
        if self.tab == Tab::Checks {
            out.extend(self.checks.show(ui, writable));
            self.rows_drawn = 0;
            return out;
        }

        // The rows, virtualised: only the visible ones are laid out.
        let n = self.rows.len();
        let reveal = self.reveal.take();
        let mut area = egui::ScrollArea::vertical().id_salt("ecad.project.list").auto_shrink([false, false]);
        if let Some(i) = reveal {
            // Keep the revealed row inside the visible band (show_rows scrolls by offset).
            let h = ui.available_height().max(ROW_H);
            let top = i as f32 * ROW_H;
            let state = egui::scroll_area::State::load(ui.ctx(), ui.make_persistent_id("ecad.project.list"));
            let now = state.map_or(0.0, |s| s.clamped_offset().y);
            let want = if top < now {
                top
            } else if top + ROW_H > now + h {
                top + ROW_H - h
            } else {
                now
            };
            area = area.vertical_scroll_offset(want.max(0.0));
        }
        // Rows are exactly ROW_H apart (the reveal and drag maths count on it).
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut drawn = 0;
        let drag_before = self.drag;
        let list = area.show_rows(ui, ROW_H, n, |ui, range| {
            let mut first_top: Option<f32> = None;
            for i in range.clone() {
                let Some(row) = self.rows.get(i).cloned() else { continue };
                drawn += 1;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
                first_top.get_or_insert(rect.top() - (i - range.start) as f32 * ROW_H);
                self.row(ui, rect, resp, i, &row, writable, &mut out);
            }
            // The insertion line of a drag in progress.
            if let (Some(d), Some(top)) = (self.drag, first_top)
                && let Some(g) = d.gap
            {
                let y = top + g as f32 * ROW_H;
                let r = ui.max_rect();
                ui.painter().hline(r.x_range(), y, Stroke::new(2.0, t.accent));
            }
            first_top
        });
        self.rows_drawn = drawn;

        // Dragging: where the row would land, auto-scroll near the edges, drop or cancel.
        if let Some(mut d) = self.drag {
            let pointer = ui.ctx().pointer_latest_pos();
            let inside = pointer.filter(|p| list.inner_rect.contains(*p));
            d.gap = match (inside, list.inner) {
                (Some(p), Some(top)) => Some((((p.y - top) / ROW_H).round().max(0.0) as usize).min(n)),
                _ => None,
            };
            if let Some(p) = inside {
                let edge = 24.0;
                if p.y < list.inner_rect.top() + edge {
                    ui.scroll_with_delta(vec2(0.0, ROW_H));
                } else if p.y > list.inner_rect.bottom() - edge {
                    ui.scroll_with_delta(vec2(0.0, -ROW_H));
                }
                ui.ctx().request_repaint();
            }
            let released = ui.input(|i| i.pointer.any_released());
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            if esc {
                self.drag = None;
            } else if released {
                self.drag = None;
                if let Some(g) = d.gap {
                    let to = if g > d.from { g - 1 } else { g };
                    if to != d.from && to < n {
                        out.push(Action::Move(d.page, to + 1));
                    }
                }
            } else if drag_before.is_some() {
                self.drag = Some(d);
            }
        }

        // A click elsewhere (the drawing) disarms the panel.
        if ui.input(|i| i.pointer.any_pressed())
            && ui.ctx().pointer_interact_pos().is_some_and(|p| !panel_rect.contains(p))
        {
            self.armed = false;
        }
        out
    }

    /// The status, file actions and experiment note moved from the banner.
    fn status(&mut self, ui: &mut egui::Ui, header: &Header, out: &mut Vec<Action>) {
        let t = Tokens::get();
        let warn = ui.visuals().warn_fg_color;
        match &header.name {
            Some(_) => {
                ui.add(egui::Label::new(RichText::new(&header.status).small()).wrap());
                for w in &header.warnings {
                    ui.add(egui::Label::new(RichText::new(w).small().color(warn)).wrap());
                }
            }
            None => {
                ui.label(RichText::new("No ECAD project is open.").small());
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.small_button("New").on_hover_text("New project").clicked() {
                out.push(Action::NewProject);
            }
            if ui.small_button("Open…").on_hover_text("Open project…").clicked() {
                out.push(Action::OpenProject);
            }
            if header.name.is_some() {
                if ui.small_button("Save").clicked() {
                    out.push(Action::Save);
                }
                if ui.small_button("Save As…").clicked() {
                    out.push(Action::SaveAs);
                }
                if ui.small_button("Close").on_hover_text("Close the project (asks to save changes)").clicked() {
                    out.push(Action::CloseProject);
                }
            }
        });
        if let Some(n) = &self.note {
            ui.label(RichText::new(n).small().color(t.text_dim));
        }
        ui.add(egui::Label::new(RichText::new(header.note).small().color(warn)).truncate())
            .on_hover_text(crate::BANNER);
    }

    /// One row: number, title, states and its clicks.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        resp: egui::Response,
        i: usize,
        row: &PageRow,
        writable: bool,
        out: &mut Vec<Action>,
    ) {
        let t = Tokens::get();
        let active = self.active == Some(row.id);
        let label = row_label(&row.number, &row.title);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, &label));
        let p = ui.painter_at(rect);
        let dragging = self.drag.is_some_and(|d| d.page == row.id);
        if active {
            p.rect_filled(rect, 0.0, t.tab_active);
            p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, t.accent);
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, t.chrome);
        }
        if self.armed && self.cursor == Some(i) {
            p.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        }
        let fade = |c: Color32| if dragging { c.gamma_multiply(0.5) } else { c };
        let colour = fade(if active { t.text } else { t.text_dim });
        let font = egui::FontId::monospace(12.0);
        let number = format!("{:>4}  ", row.number);
        let x0 = rect.left() + 4.0;
        let num = p.text(pos2(x0, rect.center().y), egui::Align2::LEFT_CENTER, &number, font.clone(), colour);
        let previewed = self.previewed == Some(row.id);
        let right = rect.right() - if previewed { 18.0 } else { 4.0 };
        if previewed {
            let r = Rect::from_center_size(pos2(rect.right() - 10.0, rect.center().y), vec2(12.0, 12.0));
            icons::paint(&p, r, Icon::Preview, true);
        }

        let renaming = self.rename.as_ref().is_some_and(|r| r.page == row.id);
        if renaming {
            let field = Rect::from_min_max(pos2(num.right(), rect.top() + 1.0), pos2(right, rect.bottom() - 1.0));
            self.rename_field(ui, field, out);
            return;
        }
        let mut title = row.title.clone();
        if row.unavailable {
            title.push_str(" (unavailable)");
        }
        let width = (right - num.right()).max(0.0);
        let shown = cut(ui, &title, &font, width);
        let was_cut = shown != title;
        p.text(pos2(num.right(), rect.center().y), egui::Align2::LEFT_CENTER, shown, font, colour);

        // A title cut with `…` shows in full on hover.
        let resp = if was_cut { resp.on_hover_text(format!("Page {}: {}", row.number, row.title)) } else { resp };
        if resp.drag_started() && writable {
            self.drag = Some(Drag { page: row.id, from: i, gap: None });
        }
        // A click just before a double-click makes egui count three: either renames.
        if resp.double_clicked() || resp.triple_clicked() {
            if writable {
                self.start_rename(row.id);
            }
        } else if resp.clicked() {
            self.armed = true;
            self.cursor = Some(i);
            if !active {
                out.push(Action::Open(row.id));
            }
        }
        if resp.secondary_clicked() {
            self.armed = true;
            self.cursor = Some(i);
        }
        resp.context_menu(|ui| {
            if ui.add_enabled(writable, egui::Button::new("New page…")).clicked() {
                out.push(Action::New);
                ui.close();
            }
            if ui.button("Open").clicked() {
                out.push(Action::Open(row.id));
                ui.close();
            }
            if ui.button("Preview").clicked() {
                out.push(Action::Preview(row.id));
                ui.close();
            }
            if ui.add_enabled(writable, egui::Button::new("Rename…")).clicked() {
                out.push(Action::RenameInBox(row.id));
                ui.close();
            }
            let n = self.rows.len();
            if ui.add_enabled(writable && i > 0, egui::Button::new("Move up").shortcut_text("Alt+Up")).clicked() {
                self.move_row(i, i - 1, out);
                ui.close();
            }
            if ui.add_enabled(writable && i + 1 < n, egui::Button::new("Move down").shortcut_text("Alt+Down")).clicked()
            {
                self.move_row(i, i + 1, out);
                ui.close();
            }
        });
    }

    /// Ask to move row `from` to index `to` (0-based); the cursor follows it.
    fn move_row(&mut self, from: usize, to: usize, out: &mut Vec<Action>) {
        if let Some(r) = self.rows.get(from)
            && to < self.rows.len()
            && to != from
        {
            out.push(Action::Move(r.id, to + 1));
            self.cursor = Some(to);
            self.reveal = Some(to);
            self.armed = true;
        }
    }

    /// The rename field over a row's title: Enter or a click elsewhere renames, Esc cancels.
    fn rename_field(&mut self, ui: &mut egui::Ui, rect: Rect, out: &mut Vec<Action>) {
        let Some(r) = &mut self.rename else { return };
        let id = egui::Id::unique("ecad.project.rename");
        let opening = egui::TextEdit::load_state(ui.ctx(), id).is_none();
        let error = r.error.is_some();
        let edit = egui::TextEdit::singleline(&mut r.text)
            .id(id)
            .font(egui::FontId::monospace(12.0))
            .margin(egui::Margin::symmetric(2, 0))
            .frame(egui::Frame::NONE.stroke(Stroke::new(1.0, if error { ORPHAN_RED } else { Tokens::get().accent })));
        let resp = ui.put(rect, edit).accessible_name(RENAME);
        if opening {
            resp.request_focus();
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                let all = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(r.old.chars().count()),
                );
                state.cursor.set_char_range(Some(all));
                state.store(ui.ctx(), id);
            }
        }
        if let Some(e) = &r.error {
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                id.with("error"),
                egui::PopupAnchor::Position(rect.left_bottom()),
            )
            .show(|ui| ui.label(RichText::new(e).small().color(ORPHAN_RED)));
        }
        if resp.lost_focus() {
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            // The key that ended the edit is the field's: CADCraft's command line, laid out after
            // the panel, must not take it too (Enter there repeats the last command).
            ui.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Enter);
                i.consume_key(Modifiers::NONE, Key::Escape);
            });
            let page = r.page;
            let text = r.text.trim().to_string();
            let same = text == r.old;
            ui.ctx().data_mut(|d| d.remove::<egui::text_edit::TextEditState>(id));
            if esc || same {
                self.rename = None;
            } else {
                // The trial runs `renamepage`; a refusal reopens the field with the message.
                out.push(Action::Rename(page, text));
            }
        }
    }

    /// The keys the armed panel takes before CADCraft's command line sees them (design §3.3).
    fn keyboard(&mut self, ui: &mut egui::Ui, writable: bool, out: &mut Vec<Action>) {
        if !self.armed || self.rename.is_some() || self.rows.is_empty() {
            return;
        }
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let n = self.rows.len();
        let page_rows = ((ui.available_height() / ROW_H).floor() as usize).max(1);
        let cur = self.target().unwrap_or(0).min(n - 1);
        let take = |k: Key, m: Modifiers| ui.ctx().input_mut(|i| i.consume_key(m, k));
        let mut moved = None;
        if take(Key::ArrowUp, Modifiers::ALT) {
            if writable && cur > 0 {
                self.move_row(cur, cur - 1, out);
            }
        } else if take(Key::ArrowDown, Modifiers::ALT) {
            if writable && cur + 1 < n {
                self.move_row(cur, cur + 1, out);
            }
        } else if take(Key::ArrowUp, Modifiers::NONE) {
            moved = Some(cur.saturating_sub(1));
        } else if take(Key::ArrowDown, Modifiers::NONE) {
            moved = Some((cur + 1).min(n - 1));
        } else if take(Key::Home, Modifiers::NONE) {
            moved = Some(0);
        } else if take(Key::End, Modifiers::NONE) {
            moved = Some(n - 1);
        } else if take(Key::PageUp, Modifiers::NONE) {
            moved = Some(cur.saturating_sub(page_rows));
        } else if take(Key::PageDown, Modifiers::NONE) {
            moved = Some((cur + page_rows).min(n - 1));
        } else if take(Key::Enter, Modifiers::NONE) {
            if let Some(r) = self.rows.get(cur) {
                out.push(Action::Open(r.id));
            }
        } else if take(Key::F2, Modifiers::NONE) {
            if let Some(r) = self.rows.get(cur).map(|r| r.id)
                && writable
            {
                self.start_rename(r);
            }
        } else if take(Key::Escape, Modifiers::NONE) {
            self.armed = false;
        } else if take(Key::Space, Modifiers::NONE) {
            // Space also arrives as text, which CADCraft's command line takes as Enter.
            ui.ctx().input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(t) if t == " ")));
            if let Some(r) = self.rows.get(cur) {
                out.push(Action::Preview(r.id));
            }
        } else if ui.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Key { pressed: true, .. }))
        }) {
            // Any other key goes to the command line and disarms the panel.
            self.armed = false;
        }
        if let Some(m) = moved {
            self.cursor = Some(m);
            self.reveal = Some(m);
        }
    }
}

/// `ecad.orphan` red (design tokens).
const ORPHAN_RED: Color32 = Color32::from_rgb(0xff, 0x00, 0x00);

/// An 18 × 18 icon button, as CADCraft's file tabs draw them, with an accessible label.
fn icon_button(ui: &mut egui::Ui, id: &str, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, egui::Id::unique(id), sense);
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, Tokens::get().control_hover);
    }
    icons::paint(ui.painter(), rect.shrink(1.0), icon, !enabled);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    resp.on_hover_text(label)
}

/// `text` cut with `…` to fit `width` in `font`.
fn cut(ui: &egui::Ui, text: &str, font: &egui::FontId, width: f32) -> String {
    let fits =
        |s: &str| ui.fonts_mut(|f| f.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x) <= width;
    if fits(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let s: String = chars.iter().take(mid).chain(std::iter::once(&'…')).collect();
        if fits(&s) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars.iter().take(lo).chain(std::iter::once(&'…')).collect()
}
