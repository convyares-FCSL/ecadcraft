//! The one table (brief 039, design `ui-architecture.md` §6): every manager is a column set and a
//! data source on this table, never a panel of its own.
//!
//! - [`source`]: [`TableSource`], [`Column`], [`Cell`], [`Changed`].
//! - [`view_model`]: the visible order (filter → group → sort), patched in place.
//! - [`selection`], [`tint`], [`edit`]: multi-select, the change tint, inline edit.
//! - [`cells`]: drawing a cell; [`export`]: CSV.
//!
//! The table draws and reports [`TableEvent`]s; it never runs a command. Its owner turns a commit
//! into the same command the page runs (`ecad-trial`'s `lists`), and reports changes back through
//! [`Table::update`] with the keys its update map names. Rows are virtualised
//! (`egui_extras::TableBuilder`, heterogeneous rows): a frame lays out only the visible ones.
//!
//! Excluded: anything about projects or commands; the proposal row state (M2); XLSX (M3).

pub mod cells;
pub mod edit;
pub mod export;
pub mod selection;
pub mod source;
pub mod strip;
pub mod tint;
pub mod view_model;

pub use edit::{CellEdit, Refusal};
pub use selection::Selection;
pub use source::{Align, Cell, Changed, Column, ColumnId, ColumnKind, Editor, Lane, LaneCell, TableSource, Width};
pub use tint::{TINT_SECONDS, TintMap};
pub use view_model::{Dir, Row, ViewModel, natural_cmp};

use ecad_tokens::{Icon, line, size, space, ui_type};
use egui::{Id, Key, Modifiers, Rect, Response, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};

use crate::prim::{self, Chip, Field, FieldKind, Kind};
use crate::{color, icons};

/// What a table reports to its owner.
#[derive(Clone, Debug, PartialEq)]
pub enum TableEvent<K> {
    /// The user changed the selection (in the shown order): select these objects on the page.
    Selected(Vec<K>),
    /// Go to where a read value is set (a read cell with a target, or Enter on one).
    GoTo { key: K, col: ColumnId },
    /// An inline edit was committed: run the column's command with `value`.
    Commit { key: K, col: ColumnId, value: String },
    /// A row was double-clicked outside a set cell, or Enter pressed on a cell with no target.
    Open(K),
    /// Export CSV was pressed: the shown rows as CSV.
    Export(String),
    /// Button `index` of a [`Cell::Buttons`] cell was pressed.
    Button { key: K, col: ColumnId, index: usize },
}

/// One table's state: the view, the selection, tints and the edit. The values stay in the source.
#[derive(Debug)]
pub struct Table<K: Ord> {
    id: Id,
    /// Its accessible name ("Devices"): the search field is "Filter Devices".
    pub name: &'static str,
    pub view: ViewModel<K>,
    pub selection: Selection<K>,
    pub tint: TintMap<K>,
    pub edit: Option<CellEdit<K>>,
    pub refusal: Option<Refusal<K>>,
    /// The column the keyboard is on.
    pub cursor_col: Option<ColumnId>,
    /// The search text (the field in the toolbar edits it).
    pub query: String,
    /// Show the toolbar (search, chips, group, export).
    pub toolbar: bool,
    /// What an empty table says.
    pub empty: &'static str,
    scroll_to: Option<K>,
    built: bool,
    /// egui's time at the last frame drawn: changes reported between frames are tinted from it.
    now: f64,
    /// Rows laid out on the last frame (tests: virtualised).
    pub rows_drawn: usize,
}

impl<K: Copy + Ord + std::hash::Hash + std::fmt::Debug> Table<K> {
    /// A table; `salt` keeps its id apart from other tables'.
    pub fn new(salt: &'static str, name: &'static str) -> Self {
        Self {
            id: Id::unique(("ecad.table", salt)),
            name,
            view: ViewModel::default(),
            selection: Selection::default(),
            tint: TintMap::default(),
            edit: None,
            refusal: None,
            cursor_col: None,
            query: String::new(),
            toolbar: true,
            empty: "Nothing to list.",
            scroll_to: None,
            built: false,
            now: 0.0,
            rows_drawn: 0,
        }
    }

    pub fn id(&self) -> Id {
        self.id
    }

    /// Whether the view has been built since it was created or [`Table::reset`].
    pub fn built(&self) -> bool {
        self.built
    }

    /// Forget the view: the next [`Table::update`] builds it whole (shown again, reload).
    pub fn reset(&mut self) {
        self.built = false;
    }

    /// The update map's entry point: rebuild on [`Changed::All`] or before the first build, patch
    /// the keys of [`Changed::Keys`] (tinting the cells whose look changed), nothing for
    /// [`Changed::Nothing`]. The tint starts at the last frame's time.
    pub fn update<S: TableSource<Key = K>>(&mut self, src: &S, changed: Changed<K>) {
        let now = self.now;
        self.update_at(src, changed, now);
    }

    /// [`Table::update`] with the tint starting at `now` (egui's time, seconds).
    pub fn update_at<S: TableSource<Key = K>>(&mut self, src: &S, changed: Changed<K>, now: f64) {
        match changed {
            _ if !self.built => {
                self.view.rebuild(src);
                self.tint.clear();
                self.built = true;
            }
            Changed::All => {
                self.view.rebuild(src);
                self.tint.clear();
            }
            Changed::Keys(keys) => {
                let cells = self.view.patch(src, &keys);
                self.tint.mark(cells, now);
            }
            Changed::Nothing => return,
        }
        let view = &self.view;
        self.selection.retain(|k| view.has(k));
        if self.edit.as_ref().is_some_and(|e| !view.has(e.key)) {
            self.edit = None;
        }
    }

    /// Select `keys` (they were selected on the page) and scroll to the first shown one. Reports
    /// nothing back.
    pub fn reveal(&mut self, keys: &[K]) {
        let shown: Vec<K> = keys.iter().copied().filter(|k| self.view.has(*k)).collect();
        if shown.iter().copied().collect::<std::collections::BTreeSet<_>>() == *self.selection.keys() {
            return;
        }
        self.selection.set(shown.iter().copied());
        self.scroll_to = shown.first().copied();
    }

    /// The owner refused a commit: show `message` under the cell; the cell shows the source's
    /// value again.
    pub fn refuse(&mut self, key: K, col: ColumnId, message: impl Into<String>) {
        self.refusal = Some(Refusal { key, col, message: message.into() });
    }

    /// Start editing `key`'s cell in `col` when it is a set cell. Returns whether it started.
    pub fn start_edit<S: TableSource<Key = K>>(&mut self, src: &S, key: K, col: ColumnId) -> bool {
        let set = src.columns().iter().any(|c| c.id == col && c.is_set());
        match src.cell(key, col) {
            Cell::Input(v) if set => {
                self.edit = Some(CellEdit::new(key, col, v));
                self.refusal = None;
                self.cursor_col = Some(col);
                true
            }
            _ => false,
        }
    }

    /// The next set cell after (`key`, `col`) in the shown order: later in the row, then the
    /// following rows.
    pub fn next_set_cell<S: TableSource<Key = K>>(&self, src: &S, key: K, col: ColumnId) -> Option<(K, ColumnId)> {
        let shown = self.view.visible_keys();
        let set: Vec<ColumnId> = src.columns().iter().filter(|c| c.is_set()).map(|c| c.id).collect();
        let start = shown.iter().position(|k| *k == key)?;
        let cols = src.columns();
        let after = cols.iter().position(|c| c.id == col).unwrap_or(0);
        for (n, k) in shown.iter().enumerate().skip(start) {
            for c in cols.iter().filter(|c| set.contains(&c.id)) {
                let pos = cols.iter().position(|x| x.id == c.id).unwrap_or(0);
                if n == start && pos <= after {
                    continue;
                }
                if matches!(src.cell(*k, c.id), Cell::Input(_)) {
                    return Some((*k, c.id));
                }
            }
        }
        None
    }

    /// The keyboard while the table has focus and no cell is being edited.
    fn keys<S: TableSource<Key = K>>(&mut self, ui: &Ui, src: &S, out: &mut Vec<TableEvent<K>>) {
        let (up, down, enter, f2, tab, all, shift, esc) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::F2),
                i.key_pressed(Key::Tab),
                i.modifiers.command && i.key_pressed(Key::A),
                i.modifiers.shift,
                i.key_pressed(Key::Escape),
            )
        });
        // The shown keys are listed only when a key needs them, never every frame.
        let shown = if up || down || all { self.view.visible_keys() } else { Vec::new() };
        if up || down {
            self.refusal = None;
            if let Some(k) = self.selection.step(&shown, if up { -1 } else { 1 }, shift) {
                self.scroll_to = Some(k);
                out.push(TableEvent::Selected(self.selection.in_order(&shown)));
            }
        }
        if all {
            self.selection.all(&shown);
            out.push(TableEvent::Selected(self.selection.in_order(&shown)));
        }
        if esc {
            self.refusal = None;
        }
        let Some(k) = self.selection.cursor() else { return };
        let col = self.cursor_col.or_else(|| src.columns().first().map(|c| c.id));
        if tab {
            let from = col.unwrap_or(ColumnId(0));
            if let Some((nk, nc)) = self.next_set_cell(src, k, from) {
                self.selection.click(nk);
                self.cursor_col = Some(nc);
                self.scroll_to = Some(nk);
            }
        }
        if (enter || f2)
            && let Some(col) = col
        {
            if self.start_edit(src, k, col) {
                return;
            }
            if !enter {
                return;
            }
            match src.cell(k, col) {
                Cell::Read { go: true, .. } => out.push(TableEvent::GoTo { key: k, col }),
                _ => out.push(TableEvent::Open(k)),
            }
        }
    }

    /// The toolbar: search, chips, group, export, the row count.
    fn show_toolbar<S: TableSource<Key = K>>(&mut self, ui: &mut Ui, src: &S, out: &mut Vec<TableEvent<K>>) {
        ui.horizontal(|ui| {
            let label = format!("Filter {}", self.name);
            Field::new(&mut self.query, FieldKind::Search, &label).placeholder("Filter").width(220.0).show(ui);
            for (i, chip) in src.chips().iter().enumerate() {
                let on = self.view.chip_on(i);
                if Chip::new(chip).selected(on).show(ui).response.clicked() {
                    self.view.set_chip(src, i, !on);
                }
            }
            let groupable: Vec<&Column> = src.columns().iter().filter(|c| c.groupable).collect();
            if !groupable.is_empty() {
                let now = self.view.group_by().and_then(|g| groupable.iter().find(|c| c.id == g)).map_or("None", |c| c.title);
                let mut pick = None;
                ui.menu_button(format!("Group: {now}"), |ui| {
                    if ui.button("None").clicked() {
                        pick = Some(None);
                    }
                    for c in &groupable {
                        if ui.button(c.title).clicked() {
                            pick = Some(Some(c.id));
                        }
                    }
                })
                .response
                .widget_info(|| WidgetInfo::labeled(Role::Button, true, format!("Group {}", self.name)));
                if let Some(g) = pick {
                    self.view.set_group(src, g);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(prim::Button::new("Export CSV").kind(Kind::Quiet).small()).clicked() {
                    let keys = self.view.visible_keys();
                    out.push(TableEvent::Export(export::csv(src, src.columns(), &keys)));
                }
                let n = self.view.item_count();
                let g = cells::fitted(ui, &format!("{n} row{}", if n == 1 { "" } else { "s" }), ui_type::SMALL, color(crate::SHELL.text_3), 200.0);
                let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                ui.painter().galley(r.min, g, color(crate::SHELL.text_3));
            });
        });
    }

    /// Draw the table. Returns what happened.
    pub fn show<S: TableSource<Key = K>>(&mut self, ui: &mut Ui, src: &S) -> Vec<TableEvent<K>> {
        let mut out = Vec::new();
        self.now = ui.input(|i| i.time);
        if !self.built {
            self.update(src, Changed::All);
        }
        if self.toolbar {
            self.show_toolbar(ui, src, &mut out);
        }
        let query = self.query.clone();
        self.view.set_query(src, &query);
        let now = ui.input(|i| i.time);
        if self.tint.live(now) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            self.tint.prune(now);
        }

        // The table keeps keyboard focus between clicks (Tab and the arrows stay with it).
        let area = ui.available_rect_before_wrap();
        let focus = ui.interact(area, self.id, Sense::focusable_noninteractive());
        let focused = focus.has_focus();
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(self.id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })
            });
            if self.edit.is_none() {
                self.keys(ui, src, &mut out);
            }
        }

        if self.view.rows().is_empty() {
            let g = cells::fitted(ui, self.empty, ui_type::BODY, color(crate::SHELL.text_2), ui.available_width());
            let (r, _) = ui.allocate_exact_size(g.size() + vec2(0.0, space::M), Sense::hover());
            ui.painter().galley(r.min + vec2(space::M, space::XS), g, color(crate::SHELL.text_2));
            self.rows_drawn = 0;
            return out;
        }
        // A window's first sizing pass has no real width yet: egui_extras would keep the column
        // widths it squeezes into it. Lay out nothing then; the next pass draws.
        if ui.is_sizing_pass() {
            ui.allocate_space(vec2(ui.available_width(), size::ROW_TABLE * 4.0));
            self.rows_drawn = 0;
            return out;
        }
        self.body(ui, src, focused, now, &mut out);
        out
    }

    fn body<S: TableSource<Key = K>>(&mut self, ui: &mut Ui, src: &S, focused: bool, now: f64, out: &mut Vec<TableEvent<K>>) {
        let s = crate::SHELL;
        let cols = src.columns().to_vec();
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let mut builder = egui_extras::TableBuilder::new(ui)
            .id_salt(self.id)
            .striped(false)
            .resizable(true)
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for c in &cols {
            builder = builder.column(match c.width {
                Width::Fixed(w) => egui_extras::Column::exact(w),
                Width::Initial(w) => egui_extras::Column::initial(w).at_least(space::XXL).resizable(true).clip(true),
                Width::Remainder => egui_extras::Column::remainder().at_least(space::XXL).clip(true),
            });
        }
        if let Some(k) = self.scroll_to.take()
            && let Some(i) = self.view.row_of(k)
        {
            builder = builder.scroll_to_row(i, Some(egui::Align::Center));
        }
        let sort = self.view.sort();
        let mut sort_click = None;
        let table = builder.header(size::ROW_TABLE, |mut header| {
            for c in &cols {
                header.col(|ui| {
                    let rect = ui.max_rect();
                    let r = ui.interact(rect, self.id.with(("head", c.id)), if c.sortable { Sense::click() } else { Sense::hover() });
                    r.widget_info(|| WidgetInfo::labeled(Role::ColumnHeader, true, c.title));
                    let ink = if r.hovered() && c.sortable { s.text } else { s.text_2 };
                    let arrow = sort.filter(|(id, _)| *id == c.id).map(|(_, d)| d);
                    let room = rect.width() - 2.0 * space::M - if arrow.is_some() { size::ICON } else { 0.0 };
                    let g = cells::fitted(ui, c.title, ui_type::CAPTION, color(ink), room);
                    let x = match c.align {
                        Align::Right => rect.right() - space::M - g.size().x - if arrow.is_some() { size::ICON } else { 0.0 },
                        _ => rect.left() + space::M,
                    };
                    let gx = g.size();
                    ui.painter().galley(pos2(x, rect.center().y - gx.y / 2.0), g, color(ink));
                    if let Some(d) = arrow {
                        let icon = if d == Dir::Asc { Icon::ChevronUp } else { Icon::ChevronDown };
                        let at =
                            Rect::from_center_size(pos2(x + gx.x + size::ICON / 2.0 + space::XXS, rect.center().y), vec2(size::ICON, size::ICON));
                        icons::paint(ui.painter(), at, icon, color(s.text_2));
                    }
                    ui.painter().line_segment(
                        [pos2(rect.left(), rect.bottom() - 0.5), pos2(rect.right(), rect.bottom() - 0.5)],
                        Stroke::new(line::HAIRLINE_PX, color(s.line_strong)),
                    );
                    if r.clicked() {
                        sort_click = Some(c.id);
                    }
                });
            }
        });
        let rows = self.view.rows();
        let grouped = self.view.group_by().is_some();
        let mut drawn = 0usize;
        let mut clicked: Option<(K, Option<ColumnId>, bool, Modifiers)> = None;
        let mut toggle: Option<String> = None;
        let mut goto: Option<(K, ColumnId)> = None;
        let mut button: Option<(K, ColumnId, usize)> = None;
        let mut edit_end: Option<(bool, bool, bool)> = None; // (commit, tab, keep focus)
        table.body(|mut body| {
            let widths = body.widths().to_vec();
            // Row hit areas span every column: registered under the body's clip, not the cell's.
            let body_clip = body.ui_mut().clip_rect();
            let total: f32 = widths.iter().sum();
            let draw_row = |mut row: egui_extras::TableRow<'_, '_>| {
                drawn += 1;
                let Some(r) = rows.get(row.index()) else { return };
                match r {
                    Row::Group { label, count, open } => {
                        for (i, _) in cols.iter().enumerate() {
                            row.col(|ui| {
                                let rect = ui.max_rect();
                                ui.painter().rect_filled(rect, 0.0, color(s.raised));
                                if i == 0 {
                                    let full = Rect::from_min_size(rect.min, vec2(total, rect.height()));
                                    let name = if label.is_empty() { "(none)" } else { label.as_str() };
                                    let text = format!("{name} ({count})");
                                    let resp = interact_unclipped(ui, full, body_clip, self.id.with(("group", label.as_str())));
                                    let o = *open;
                                    resp.widget_info(|| WidgetInfo::selected(Role::Button, true, o, &text));
                                    let icon = Rect::from_center_size(
                                        pos2(rect.left() + space::M + size::ICON / 2.0, rect.center().y),
                                        vec2(size::ICON, size::ICON),
                                    );
                                    icons::paint(ui.painter(), icon, if *open { Icon::ChevronDown } else { Icon::ChevronRight }, color(s.text_2));
                                    let g = cells::fitted(ui, &text, ui_type::CAPTION, color(s.text_2), total - size::ICON - 3.0 * space::M);
                                    let gy = g.size().y;
                                    ui.painter().with_clip_rect(full).galley(
                                        pos2(icon.right() + space::S, rect.center().y - gy / 2.0),
                                        g,
                                        color(s.text_2),
                                    );
                                    if resp.clicked() {
                                        toggle = Some(label.clone());
                                    }
                                }
                            });
                        }
                    }
                    Row::Item(k) => {
                        let k = *k;
                        let selected = self.selection.contains(k);
                        let mut row_resp: Option<Response> = None;
                        let mut col_rects: Vec<(ColumnId, Rect)> = Vec::with_capacity(cols.len());
                        for (i, c) in cols.iter().enumerate() {
                            row.col(|ui| {
                                let rect = ui.max_rect();
                                col_rects.push((c.id, rect));
                                if i == 0 {
                                    // Registered before the cells' own widgets, so they take their
                                    // own clicks; anywhere else selects the row.
                                    let full = Rect::from_min_size(rect.min, vec2(total, rect.height()));
                                    let resp = interact_unclipped(ui, full, body_clip, self.id.with(("row", k)));
                                    // The label is made only when accessibility asks for it.
                                    resp.widget_info(|| WidgetInfo::selected(Role::Row, true, selected, src.row_label(k)));
                                    row_resp = Some(resp);
                                }
                                let hovered = row_resp.as_ref().is_some_and(Response::contains_pointer);
                                let p = ui.painter();
                                if selected {
                                    p.rect_filled(rect, 0.0, color(s.accent_soft));
                                } else if hovered {
                                    p.rect_filled(rect, 0.0, color(s.hover));
                                }
                                let tint = self.tint.level(k, c.id, now);
                                if tint > 0.0 {
                                    let a = (f32::from(s.accent_soft.a) * tint).round().clamp(0.0, 255.0) as u8;
                                    p.rect_filled(rect, 0.0, color(ecad_tokens::Color { a, ..s.accent_soft }));
                                }
                                p.line_segment(
                                    [pos2(rect.left(), rect.bottom() - 0.5), pos2(rect.right(), rect.bottom() - 0.5)],
                                    Stroke::new(line::HAIRLINE_PX, color(s.line)),
                                );
                                let editing = self.edit.as_ref().is_some_and(|e| e.key == k && e.col == c.id);
                                if editing {
                                    if let Some(e) = self.edit.as_mut() {
                                        let label = format!("Edit {}", c.title);
                                        let mut child = ui.new_child(
                                            egui::UiBuilder::new()
                                                .max_rect(rect.shrink2(vec2(space::XXS, 0.0)))
                                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                        );
                                        let resp =
                                            Field::new(&mut e.text, FieldKind::Text, &label).width(rect.width() - 2.0 * space::XXS).show(&mut child);
                                        if std::mem::take(&mut e.focus) {
                                            resp.request_focus();
                                        } else if resp.lost_focus() || !resp.has_focus() && !resp.gained_focus() {
                                            let (enter, esc, tab) =
                                                ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape), i.key_pressed(Key::Tab)));
                                            ui.input_mut(|i| {
                                                i.consume_key(Modifiers::NONE, Key::Enter);
                                                i.consume_key(Modifiers::NONE, Key::Escape);
                                                i.consume_key(Modifiers::NONE, Key::Tab);
                                            });
                                            edit_end = Some((!esc, tab, enter || esc || tab));
                                        }
                                    }
                                } else {
                                    let cell = src.cell(k, c.id);
                                    let at_cursor = focused && selected && self.selection.cursor() == Some(k) && self.cursor_col == Some(c.id);
                                    match cells::draw(ui, rect, &cell, c.align, at_cursor) {
                                        Some(cells::Hit::Go) => goto = Some((k, c.id)),
                                        Some(cells::Hit::Button(index)) => button = Some((k, c.id, index)),
                                        None => {}
                                    }
                                }
                                if let Some(f) = self.refusal.as_ref().filter(|f| f.key == k && f.col == c.id) {
                                    refusal_note(ui, rect, &f.message, self.id);
                                }
                            });
                        }
                        if let Some(resp) = row_resp {
                            let col =
                                resp.interact_pointer_pos().and_then(|p| col_rects.iter().find(|(_, r)| r.x_range().contains(p.x)).map(|(c, _)| *c));
                            if resp.double_clicked() {
                                clicked = Some((k, col, true, Modifiers::NONE));
                            } else if resp.clicked() {
                                clicked = Some((k, col, false, resp.ctx.input(|i| i.modifiers)));
                            }
                        }
                    }
                }
            };
            // Without groups every row is 30 px: egui_extras then visits only the visible rows.
            // With groups the heights differ, and it sums one height per row (no cell is read).
            if grouped {
                let heights = rows.iter().map(|r| match r {
                    Row::Group { .. } => size::ROW_GROUP,
                    Row::Item(_) => size::ROW_TABLE,
                });
                body.heterogeneous_rows(heights, draw_row);
            } else {
                body.rows(size::ROW_TABLE, rows.len(), draw_row);
            }
        });
        self.rows_drawn = drawn;

        if let Some(c) = sort_click {
            let next = match sort {
                Some((id, Dir::Asc)) if id == c => Some((c, Dir::Desc)),
                Some((id, Dir::Desc)) if id == c => None,
                _ => Some((c, Dir::Asc)),
            };
            self.view.set_sort(src, next);
        }
        if let Some(g) = toggle {
            self.view.toggle_group(src, &g);
        }
        if let Some((commit, tab, keep)) = edit_end
            && let Some(e) = self.edit.take()
        {
            if commit && e.changed() {
                out.push(TableEvent::Commit { key: e.key, col: e.col, value: e.text.trim().to_string() });
            }
            if tab && let Some((nk, nc)) = self.next_set_cell(src, e.key, e.col) {
                self.selection.click(nk);
                self.scroll_to = Some(nk);
                self.start_edit(src, nk, nc);
            } else if keep {
                ui.memory_mut(|m| m.request_focus(self.id));
            }
        }
        if let Some((key, col, index)) = button {
            out.push(TableEvent::Button { key, col, index });
        } else if let Some((k, col)) = goto {
            self.selection.click(k);
            self.cursor_col = Some(col);
            out.push(TableEvent::Selected(vec![k]));
            out.push(TableEvent::GoTo { key: k, col });
        } else if let Some((k, col, double, m)) = clicked {
            self.refusal = None;
            ui.memory_mut(|mem| mem.request_focus(self.id));
            if double {
                let started = col.is_some_and(|c| self.start_edit(src, k, c));
                if !started {
                    out.push(TableEvent::Open(k));
                }
            } else {
                let shown = self.view.visible_keys();
                if m.shift {
                    self.selection.extend_to(k, &shown);
                } else if m.command {
                    self.selection.toggle(k);
                } else {
                    self.selection.click(k);
                }
                self.cursor_col = col.or(self.cursor_col);
                out.push(TableEvent::Selected(self.selection.in_order(&shown)));
            }
        }
    }
}

/// A click area over `rect` hit-tested within `clip` rather than the cell's own clip (a row's
/// area spans every column but is registered in the first cell).
fn interact_unclipped(ui: &mut Ui, rect: Rect, clip: Rect, id: Id) -> Response {
    let saved = ui.clip_rect();
    ui.set_clip_rect(rect.intersect(clip));
    let r = ui.interact(rect, id, Sense::click());
    ui.set_clip_rect(saved);
    r
}

/// A refusal under its cell (spec §5: where the attempt was made), in the error colour.
fn refusal_note(ui: &Ui, rect: Rect, message: &str, id: Id) {
    let s = crate::SHELL;
    egui::Area::new(id.with("refusal")).order(egui::Order::Foreground).fade_in(false).fixed_pos(rect.left_bottom()).interactable(false).show(
        ui.ctx(),
        |ui| {
            egui::Frame::NONE.fill(color(s.raised)).stroke(Stroke::new(line::HAIRLINE_PX, color(s.error))).inner_margin(space::S).show(ui, |ui| {
                let g = cells::fitted(ui, message, ui_type::SMALL, color(s.error), 420.0);
                let (r, resp) = ui.allocate_exact_size(g.size(), Sense::hover());
                resp.widget_info(|| WidgetInfo::labeled(Role::Label, true, message));
                ui.painter().galley(r.min, g, color(s.error));
            });
        },
    );
}
