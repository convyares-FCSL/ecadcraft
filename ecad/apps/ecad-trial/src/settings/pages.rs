//! Settings ▸ Pages and sheets (brief 054, design 054 §5; UI rev 4 board 5.02): the page type
//! table, the selected type's right panel, and rev 3's project sheet and numbering settings in one
//! folded section under the table.
//!
//! Every control writes one `setstyle {catalogue: "page", …}`: one transaction, one `u`. Read only
//! (a revision open, writes off, an agent writing) disables every control.
//!
//! # Update map (design 054 §10.2)
//! The table is read from the project into [`Snapshot`] on first show and again only when the
//! ledger's head moves (any commit or undo while Settings is open); nothing is read per frame
//! beyond the head.

use ecad_cadcraft::styles::page::{self as ptype, Position};
use ecad_model::sheet::{GridLabels, PRESETS, SheetSpec};
use ecad_model::{PageId, StyleRef};
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Color32, Rect, Sense, pos2, vec2};

use crate::Trial;
use crate::shell::bar;

/// Accessible names (design 054 §5.2).
pub const TABLE: &str = "ecad.settings.pages.table";
pub const ADD: &str = "ecad.settings.pages.add";
pub const PROJECT: &str = "ecad.settings.pages.project";
/// The header line (design 054 §5.1).
pub const ABOUT: &str = "Each page type brings its own sheet. + Page in the Book asks which type and puts it where the type belongs.";
/// Under the table.
pub const LIBRARY_NOTE: &str = "Frames and title blocks are made in Library ▸ Title blocks.";
/// Under Position in the right panel (design 054 §5.3).
pub const NUMBERS_NOTE: &str = "Pages are numbered by folder: set a folder's start number in the Book.";
/// Row height and thumbnail size (design 054 §5.2).
pub const ROW_H: f32 = 58.0;
pub const THUMB: egui::Vec2 = vec2(52.0, 36.0);
/// The right panel's width.
pub const PANEL_W: f32 = 400.0;

/// One page type as the table shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRow {
    pub id: StyleRef,
    pub name: String,
    pub subtitle: &'static str,
    pub own: bool,
    pub overridden: bool,
    /// The type's own sheet, `None` for the project's.
    pub sheet: Option<SheetSpec>,
    /// The sheet its pages are drawn on.
    pub shown: SheetSpec,
    pub title_block: &'static str,
    pub frame: Option<StyleRef>,
    pub zones: bool,
    pub position: Position,
    pub pages: usize,
    /// One page of this type, for Settings' preview (none: the sheet alone).
    pub first: Option<PageId>,
}

/// What the screen shows, read at one ledger head.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub head: u64,
    pub rows: Vec<TypeRow>,
    pub project_sheet: SheetSpec,
    /// The frame styles, for the Frame style drop-down.
    pub frames: Vec<(StyleRef, String)>,
}

/// The screen's own state, kept while Settings is closed.
#[derive(Debug, Default)]
pub struct PagesView {
    pub selected: Option<StyleRef>,
    pub snapshot: Option<Snapshot>,
    /// Rev 3's project sheet and numbering, folded by default (design 054 §5.4).
    pub project_open: bool,
    /// The name field's text and whose it is.
    name: Option<(StyleRef, String)>,
    /// The Other… sheet field.
    other: Option<(StyleRef, String)>,
    pub error: Option<String>,
}

/// `A3 landscape`, `600×400 landscape`.
pub fn sheet_words(s: &SheetSpec) -> String {
    match s.preset_name() {
        Some(n) => format!("{n} landscape"),
        None => format!("{}×{} landscape", s.width, s.height),
    }
}

/// `A–F × 1–8`: the zone grid of a sheet.
pub fn zone_words(s: &SheetSpec) -> String {
    let label = |l: GridLabels, n: u32| l.label(n);
    format!(
        "{}–{} × {}–{}",
        label(s.row_labels, 1),
        label(s.row_labels, s.rows.max(1)),
        label(s.column_labels, 1),
        label(s.column_labels, s.columns.max(1))
    )
}

/// `A3 standard`: a title block layout's name on a sheet.
pub fn title_block_words(s: &SheetSpec) -> String {
    format!("{} standard", s.preset_name().map_or_else(|| format!("{}×{}", s.width, s.height), str::to_string))
}

/// Read the table from the project.
pub fn read(t: &mut Trial) -> Option<Snapshot> {
    let p = t.project.clone()?;
    p.read(&mut t.cad.session, |p, _| {
        let doc = p.ledger.document();
        let counts = ptype::counts(doc);
        let project_sheet = if doc.settings.sheet.check().is_ok() { doc.settings.sheet } else { SheetSpec::default() };
        let rows = ptype::ordered(doc)
            .into_iter()
            .map(|r| {
                let look = ptype::look_of(doc, &r.id);
                TypeRow {
                    subtitle: ptype::subtitle(&r.id),
                    own: r.layer != ecad_cadcraft::styles::Layer::Builtin,
                    overridden: r.overridden,
                    shown: look.sheet.unwrap_or(project_sheet),
                    sheet: look.sheet,
                    title_block: look.title_block,
                    frame: look.frame,
                    zones: look.zones,
                    position: look.position,
                    pages: counts.get(&r.id).copied().unwrap_or(0),
                    first: None,
                    name: r.name,
                    id: r.id,
                }
            })
            .collect();
        let frames = ecad_cadcraft::styles::rows(doc, &ecad_cadcraft::styles::frame::CATALOGUE).into_iter().map(|r| (r.id, r.name)).collect();
        Snapshot { head: p.ledger.head(), rows, project_sheet, frames }
    })
    .ok()
}

fn head(t: &mut Trial) -> Option<u64> {
    let p = t.project.clone()?;
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).ok()
}

/// Paint a type's thumbnail glyph (design 054 §5.2): lines for front, the schematic glyph, nodes
/// for network, a tree for single-line, plate and door for panel, a ruled table for report. An own
/// type takes its position's glyph.
pub fn thumbnail(p: &egui::Painter, r: Rect, id: &StyleRef, position: Position) {
    let (paper, ink) = (ecad_ui::color(ecad_ui::tokens::sheet::THUMBNAIL_PAPER), ecad_ui::color(ecad_ui::tokens::sheet::THUMBNAIL_INK));
    p.rect_filled(r, 2.0, paper);
    let st = egui::Stroke::new(1.0, ink);
    let i = r.shrink2(vec2(r.width() * 0.12, r.height() * 0.16));
    let word = match id {
        StyleRef::Builtin(w) => w.as_str(),
        StyleRef::Own(_) => match position {
            Position::Front => "front",
            Position::Back => "report",
            Position::Body => "schematic",
        },
    };
    let lx = |f: f32| i.left() + i.width() * f;
    let ly = |f: f32| i.top() + i.height() * f;
    match word {
        "front" => {
            for (k, w) in [(0.15, 0.7), (0.35, 0.9), (0.5, 0.6), (0.65, 0.8)] {
                p.hline(lx(0.05)..=lx(0.05 + w), ly(k), st);
            }
        }
        "network" => {
            let a = pos2(lx(0.1), ly(0.5));
            let b = pos2(lx(0.45), ly(0.5));
            p.rect_stroke(Rect::from_center_size(a, vec2(5.0, 5.0)), 0.0, st, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_center_size(b, vec2(5.0, 5.0)), 0.0, st, egui::StrokeKind::Middle);
            p.line_segment([a + vec2(2.5, 0.0), b - vec2(2.5, 0.0)], st);
            for y in [0.15, 0.85] {
                let c = pos2(lx(0.85), ly(y));
                p.rect_stroke(Rect::from_center_size(c, vec2(5.0, 5.0)), 0.0, st, egui::StrokeKind::Middle);
                p.line_segment([b + vec2(2.5, 0.0), c - vec2(2.5, 0.0)], st);
            }
        }
        "singleline" => {
            p.vline(lx(0.5), ly(0.0)..=ly(0.4), st);
            p.hline(lx(0.15)..=lx(0.85), ly(0.4), st);
            for x in [0.15, 0.5, 0.85] {
                p.vline(lx(x), ly(0.4)..=ly(0.85), st);
                p.circle_stroke(pos2(lx(x), ly(0.92)), 2.0, st);
            }
        }
        "panel" => {
            p.rect_stroke(Rect::from_min_max(pos2(lx(0.0), ly(0.0)), pos2(lx(0.42), ly(1.0))), 0.0, st, egui::StrokeKind::Middle);
            for k in [0.25, 0.5, 0.75] {
                p.hline(lx(0.05)..=lx(0.37), ly(k), st);
            }
            p.rect_stroke(Rect::from_min_max(pos2(lx(0.55), ly(0.0)), pos2(lx(1.0), ly(1.0))), 0.0, st, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_center_size(pos2(lx(0.72), ly(0.25)), vec2(4.0, 3.0)), 0.0, st, egui::StrokeKind::Middle);
        }
        "report" => {
            p.rect_stroke(i, 0.0, st, egui::StrokeKind::Middle);
            for k in [0.2, 0.4, 0.6, 0.8] {
                p.hline(i.x_range(), ly(k), st);
            }
            p.vline(lx(0.3), i.y_range(), st);
        }
        _ => {
            for x in [0.05, 0.2, 0.35] {
                p.vline(lx(x), i.y_range(), st);
            }
            let accent = ecad_ui::color(ecad_ui::SHELL.accent);
            let sa = egui::Stroke::new(1.0, accent);
            p.hline(lx(0.55)..=lx(0.95), ly(0.1), sa);
            p.vline(lx(0.65), ly(0.1)..=ly(1.0), sa);
            p.vline(lx(0.85), ly(0.1)..=ly(1.0), sa);
        }
    }
}

/// A toggle switch (rev 4): accent when on. Returns its response; `clicked` flips it.
pub fn switch(ui: &mut egui::Ui, on: bool, enabled: bool, name: &str) -> egui::Response {
    let s = ecad_ui::SHELL;
    let (r, resp) = ui.allocate_exact_size(vec2(30.0, 18.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, name));
    let track = if on { s.accent } else { s.line_strong };
    let mut fill = ecad_ui::color(track);
    if !enabled {
        fill = fill.gamma_multiply(0.5);
    }
    ui.painter().rect_filled(r, 9.0, fill);
    let knob = if on { r.right() - 9.0 } else { r.left() + 9.0 };
    ui.painter().circle_filled(pos2(knob, r.center().y), 6.5, Color32::WHITE);
    resp
}

/// A bordered value box (the board's `A3 landscape` chips): its words, and a response.
fn chip(ui: &egui::Ui, r: Rect, text: &str, id: egui::Id, name: &str, enabled: bool) -> egui::Response {
    let s = ecad_ui::SHELL;
    let resp = ui.interact(r, id, if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, name));
    let fill = if resp.hovered() && enabled { s.hover } else { s.bg };
    ui.painter().rect_filled(r, 4.0, ecad_ui::color(fill));
    ui.painter().rect_stroke(r, 4.0, egui::Stroke::new(1.0, ecad_ui::color(s.line)), egui::StrokeKind::Inside);
    let ink = if enabled { s.text } else { s.text_3 };
    ui.painter().text(pos2(r.left() + space::M, r.center().y), egui::Align2::LEFT_CENTER, text, bar::font(ui, ui_type::BODY), ecad_ui::color(ink));
    resp
}

/// What a control asked for.
enum Do {
    Select(StyleRef),
    /// `setstyle` fields on a type.
    Set(StyleRef, Vec<(&'static str, Option<String>)>),
    Rename(StyleRef, String),
    Add(StyleRef),
    Delete(StyleRef),
    Promote(StyleRef),
    Reset(StyleRef),
}

/// Settings ▸ Pages and sheets, first tab: the type table and its right panel, then the project's
/// own sheet and numbering in one folded section.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    if !t.project_open() {
        ecad_ui::prim::read_value(ui, super::project::NO_PROJECT);
        return;
    }
    let now = head(t);
    if t.settings.pages.snapshot.as_ref().map(|s| s.head) != now
        && let Some(s) = read(t)
    {
        t.settings.pages.snapshot = Some(s);
    }
    let Some(snap) = t.settings.pages.snapshot.clone() else { return };
    let writable = !t.header().read_only;
    if t.settings.pages.selected.as_ref().is_none_or(|sel| !snap.rows.iter().any(|r| r.id == *sel)) {
        t.settings.pages.selected =
            snap.rows.iter().find(|r| r.id == StyleRef::Builtin(ptype::PANEL.into())).or(snap.rows.first()).map(|r| r.id.clone());
    }
    // Below the tabs.
    let full = Rect::from_min_max(pos2(ui.max_rect().left(), ui.cursor().top()), ui.max_rect().max);
    let right = Rect::from_min_max(pos2(full.right() - PANEL_W, full.top()), full.max);
    let left = Rect::from_min_max(full.min, pos2(right.left() - space::XL, full.bottom()));
    let mut todo = Vec::new();
    // The table at the top; the project's own rows under it take what is left (their Save stays
    // at their foot, as in rev 3).
    super::in_rect(ui, left, "settings.pages.left", |ui| {
        if t.settings.pages.project_open {
            // The project's rows open: the table folds to its head, so their Save stays in view.
            let st = ecad_ui::SHELL;
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, false, "ecad.settings.pages.types"));
            ecad_ui::icons::paint(
                ui.painter(),
                Rect::from_center_size(pos2(r.left() + 6.0, r.center().y), vec2(10.0, 10.0)),
                ecad_ui::tokens::Icon::ChevronRight,
                ecad_ui::color(st.text_2),
            );
            ui.painter().text(
                pos2(r.left() + 20.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                "Page types",
                bar::font(ui, ui_type::BODY_EMPHASIS),
                ecad_ui::color(st.text),
            );
            ui.painter().text(
                pos2(r.left() + 240.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                format!("{} types", snap.rows.len()),
                bar::font(ui, ui_type::SMALL),
                ecad_ui::color(st.text_3),
            );
            if resp.clicked() {
                t.settings.pages.project_open = false;
            }
        } else {
            table(t, ui, &snap, writable, &mut todo);
        }
        ui.add_space(space::L);
        let rest = Rect::from_min_max(pos2(left.left(), ui.cursor().top()), left.max);
        super::in_rect(ui, rest, "settings.pages.project", |ui| project_section(t, ui));
    });
    let s = ecad_ui::SHELL;
    ui.painter().rect_filled(right.expand2(vec2(0.0, space::XL)), 0.0, ecad_ui::color(s.panel));
    ui.painter().vline(
        right.left() - 0.5,
        right.expand2(vec2(0.0, space::XL)).y_range(),
        egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)),
    );
    if let Some(row) = t.settings.pages.selected.clone().and_then(|sel| snap.rows.iter().find(|r| r.id == sel).cloned()) {
        super::in_rect(ui, right.shrink2(vec2(space::L, 0.0)), "settings.pages.right", |ui| {
            egui::ScrollArea::vertical().id_salt("settings.pages.right.scroll").auto_shrink([false, false]).show(ui, |ui| {
                panel(t, ui, &snap, &row, writable, &mut todo);
            });
        });
    }
    for d in todo {
        act(t, d);
    }
}

fn run(t: &mut Trial, cmd: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    match t.run_value(cmd, params) {
        Ok(v) => {
            t.settings.pages.error = None;
            Some(v)
        }
        Err(e) => {
            t.settings.pages.error = Some(e.clone());
            t.cad.session.echo(e);
            None
        }
    }
}

fn act(t: &mut Trial, d: Do) {
    use serde_json::json;
    match d {
        Do::Select(id) => t.settings.pages.selected = Some(id),
        Do::Set(id, fields) => {
            let f: serde_json::Map<String, serde_json::Value> =
                fields.into_iter().map(|(k, v)| (k.to_string(), v.map_or(serde_json::Value::Null, serde_json::Value::String))).collect();
            run(t, "setstyle", json!({"catalogue": ptype::WORD, "id": id.text(), "fields": f}));
        }
        Do::Rename(id, name) => {
            run(t, "setstyle", json!({"catalogue": ptype::WORD, "id": id.text(), "rename": name}));
        }
        Do::Add(from) => {
            let name = t
                .settings
                .pages
                .snapshot
                .as_ref()
                .and_then(|s| s.rows.iter().find(|r| r.id == from))
                .map(|r| format!("{} 2", r.name))
                .unwrap_or_default();
            if let Some(v) = run(t, "setstyle", json!({"catalogue": ptype::WORD, "new": true, "copy_of": from.text(), "name": name})) {
                let id = ["id", "style"].iter().filter_map(|k| v.get(*k).and_then(serde_json::Value::as_str)).find_map(StyleRef::parse);
                if let Some(id) = id {
                    t.settings.pages.selected = Some(id);
                }
            }
        }
        Do::Delete(id) => {
            run(t, "deletestyle", json!({"catalogue": ptype::WORD, "id": id.text(), "replace_with": ptype::SCHEMATIC}));
        }
        Do::Promote(id) => {
            run(t, "promote", json!({"catalogue": ptype::WORD, "id": id.text()}));
        }
        Do::Reset(id) => {
            let keys = ptype::CATALOGUE.fields.iter().map(|f| (f.key.to_string(), serde_json::Value::Null)).collect::<serde_json::Map<_, _>>();
            run(t, "setstyle", json!({"catalogue": ptype::WORD, "id": id.text(), "fields": keys}));
        }
    }
}

/// The table's column lefts, as shares of its width after the thumbnail (board 5.02 without its
/// Numbers column).
const COLS: [(&str, f32); 7] =
    [("Type", 0.0), ("Sheet", 0.33), ("Frame", 0.46), ("Title block", 0.56), ("Zones", 0.69), ("Position", 0.76), ("Pages", 0.94)];

fn col(r: Rect, i: usize) -> f32 {
    let x0 = r.left() + THUMB.x + 2.0 * space::XL;
    x0 + (r.right() - x0) * COLS.get(i).map_or(0.0, |c| c.1)
}

fn table(t: &mut Trial, ui: &mut egui::Ui, snap: &Snapshot, writable: bool, todo: &mut Vec<Do>) {
    let s = ecad_ui::SHELL;
    let w = ui.available_width();
    let (head, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Grid, true, TABLE));
    let p = ui.painter().clone();
    for (i, (label, _)) in COLS.iter().enumerate() {
        p.text(pos2(col(head, i), head.center().y), egui::Align2::LEFT_CENTER, *label, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_2));
    }
    p.hline(head.x_range(), head.bottom() - 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let selected = t.settings.pages.selected.clone();
    for row in &snap.rows {
        let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
        let on = selected.as_ref() == Some(&row.id);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Row, true, on, format!("ecad.settings.pages.row.{}", row.id.text())));
        if on {
            p.rect_filled(r, 0.0, ecad_ui::color(s.accent_soft));
        } else if resp.hovered() {
            p.rect_filled(r, 0.0, ecad_ui::color(s.hover));
        }
        if resp.clicked() {
            todo.push(Do::Select(row.id.clone()));
        }
        resp.context_menu(|ui| {
            if ui.add_enabled(writable, egui::Button::new("Duplicate")).clicked() {
                todo.push(Do::Add(row.id.clone()));
                ui.close();
            }
            if ui.add_enabled(writable && row.own, egui::Button::new("Delete…")).on_hover_text("Its pages become Schematic pages").clicked() {
                todo.push(Do::Delete(row.id.clone()));
                ui.close();
            }
            if ui.add_enabled(writable && row.own, egui::Button::new("Promote to Standard")).clicked() {
                todo.push(Do::Promote(row.id.clone()));
                ui.close();
            }
        });
        let y = r.center().y;
        let th = Rect::from_min_size(pos2(r.left() + space::XL, y - THUMB.y / 2.0), THUMB);
        thumbnail(&p, th, &row.id, row.position);
        p.rect_stroke(th, 2.0, egui::Stroke::new(1.0, ecad_ui::color(s.line)), egui::StrokeKind::Outside);
        // Type: the name (with 050's override dot) over the subtitle.
        let nx = col(r, 0);
        let has_sub = !row.subtitle.is_empty();
        let ny = if has_sub { y - 8.0 } else { y };
        let g = p.text(pos2(nx, ny), egui::Align2::LEFT_CENTER, &row.name, bar::font(ui, ui_type::BODY_EMPHASIS), ecad_ui::color(s.text));
        if row.overridden {
            p.circle_filled(pos2(g.right() + 6.0, ny), 3.0, ecad_ui::color(s.accent));
        }
        if has_sub {
            p.text(pos2(nx, y + 9.0), egui::Align2::LEFT_CENTER, row.subtitle, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_3));
        }
        // Sheet: a drop-down chip.
        let sheet_r = Rect::from_min_size(pos2(col(r, 1), y - 14.0), vec2(((col(r, 2) - col(r, 1)) - space::L).min(132.0), 28.0));
        let id = row.id.text();
        let sheet_resp = chip(
            ui,
            sheet_r,
            &sheet_words(&row.shown),
            egui::Id::unique(("ecad.settings.pages.sheet", &id)),
            &format!("ecad.settings.pages.sheet.{id}"),
            writable,
        );
        egui::Popup::menu(&sheet_resp).show(|ui| sheet_menu(ui, row, snap, todo));
        // Frame: the zone grid, or Plain.
        let frame = if row.zones { zone_words(&row.shown) } else { "Plain".into() };
        let fr = p.text(pos2(col(r, 2), y), egui::Align2::LEFT_CENTER, frame, bar::font(ui, ui_type::BODY), ecad_ui::color(s.text_2));
        ui.interact(fr, egui::Id::unique(("ecad.settings.pages.frame", &id)), Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("ecad.settings.pages.frame.{id}")));
        // Title block.
        let tb_r = Rect::from_min_size(pos2(col(r, 3), y - 14.0), vec2(((col(r, 4) - col(r, 3)) - space::L).min(120.0), 28.0));
        let tb = chip(
            ui,
            tb_r,
            &title_block_words(&row.shown),
            egui::Id::unique(("ecad.settings.pages.titleblock", &id)),
            &format!("ecad.settings.pages.titleblock.{id}"),
            writable,
        );
        egui::Popup::menu(&tb).show(|ui| title_block_menu(ui, row, todo));
        // Zones.
        let z = ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(col(r, 4), y - 9.0), vec2(40.0, 18.0))), |ui| {
            switch(ui, row.zones, writable, &format!("ecad.settings.pages.zones.{id}"))
        });
        if z.inner.clicked() {
            todo.push(Do::Set(row.id.clone(), vec![("zones", Some(if row.zones { "off" } else { "on" }.into()))]));
        }
        // Position.
        let pos_r = Rect::from_min_size(pos2(col(r, 5), y - 15.0), vec2(col(r, 6) - col(r, 5) - space::L, 30.0));
        let mut i = row.position as usize;
        let changed = ui
            .scope_builder(egui::UiBuilder::new().max_rect(pos_r).id_salt(("ecad.settings.pages.position", &id)), |ui| {
                ui.add_enabled_ui(writable, |ui| ecad_ui::prim::segmented(ui, &mut i, &["Front", "Body", "Back"]).changed()).inner
            })
            .inner;
        if changed {
            let w = ["front", "body", "back"].get(i).copied().unwrap_or("body");
            todo.push(Do::Set(row.id.clone(), vec![("position", Some(w.into()))]));
        }
        // Pages.
        p.text(pos2(col(r, 6), y), egui::Align2::LEFT_CENTER, row.pages.to_string(), bar::font(ui, ui_type::BODY), ecad_ui::color(s.text_2));
        p.hline(r.x_range(), r.bottom() - 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    }
    ui.add_space(space::L);
    ui.horizontal(|ui| {
        ui.add_space(THUMB.x + 2.0 * space::XL);
        let b = ui.add_enabled(writable, ecad_ui::prim::Button::new("+ Page type").small());
        b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, ADD));
        if b.clicked()
            && let Some(sel) = selected.clone()
        {
            todo.push(Do::Add(sel));
        }
        ui.add_space(space::M);
        ui.label(egui::RichText::new(LIBRARY_NOTE).color(ecad_ui::color(s.text_3)));
    });
    if let Some(e) = &t.settings.pages.error {
        ui.label(egui::RichText::new(e).small().color(ecad_ui::color(s.error)));
    }
}

fn sheet_menu(ui: &mut egui::Ui, row: &TypeRow, snap: &Snapshot, todo: &mut Vec<Do>) {
    if ui.button(format!("Project's ({})", sheet_words(&snap.project_sheet))).clicked() {
        todo.push(Do::Set(row.id.clone(), vec![("sheet", Some("project".into()))]));
    }
    for (name, ..) in PRESETS {
        if ui.button(format!("{name} landscape")).clicked() {
            todo.push(Do::Set(row.id.clone(), vec![("sheet", Some(name.to_string()))]));
        }
    }
    if ui.button("Other…").on_hover_text("Type a size in the right panel").clicked() {
        todo.push(Do::Select(row.id.clone()));
    }
}

fn title_block_menu(ui: &mut egui::Ui, row: &TypeRow, todo: &mut Vec<Do>) {
    let words = title_block_words(&row.shown);
    if ui.button(format!("Frame's ({words})")).clicked() {
        todo.push(Do::Set(row.id.clone(), vec![("title_block", Some("frame".into()))]));
    }
    if ui.button(words).clicked() {
        todo.push(Do::Set(row.id.clone(), vec![("title_block", Some("standard".into()))]));
    }
}

/// A folding section's head: a chevron, the title, and the folded summary.
fn section(ui: &mut egui::Ui, id: &str, title: &str, summary: &str, add: impl FnOnce(&mut egui::Ui)) {
    let s = ecad_ui::SHELL;
    let key = egui::Id::unique(("ecad.settings.pages.section", id));
    let mut open = ui.ctx().data_mut(|d| *d.get_temp_mut_or(key, true));
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, format!("ecad.settings.pages.panel.{id}")));
    let icon = if open { ecad_ui::tokens::Icon::ChevronDown } else { ecad_ui::tokens::Icon::ChevronRight };
    ecad_ui::icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 6.0, r.center().y), vec2(10.0, 10.0)), icon, ecad_ui::color(s.text_2));
    ui.painter().text(
        pos2(r.left() + 20.0, r.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        bar::font(ui, ui_type::BODY_EMPHASIS),
        ecad_ui::color(s.text),
    );
    if !open {
        ui.painter().text(
            pos2(r.right(), r.center().y),
            egui::Align2::RIGHT_CENTER,
            summary,
            bar::font(ui, ui_type::SMALL),
            ecad_ui::color(s.text_3),
        );
    }
    if resp.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(key, open));
    }
    if open {
        add(ui);
    }
    ui.add_space(space::S);
    let y = ui.cursor().top();
    ui.painter().hline(ui.max_rect().x_range(), y, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    ui.add_space(space::S);
}

/// One labelled row of the right panel.
fn prow<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let s = ecad_ui::SHELL;
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(104.0, 34.0), Sense::hover());
        ui.painter().text(pos2(r.left(), r.center().y), egui::Align2::LEFT_CENTER, label, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_2));
        add(ui)
    })
    .inner
}

fn panel(t: &mut Trial, ui: &mut egui::Ui, snap: &Snapshot, row: &TypeRow, writable: bool, todo: &mut Vec<Do>) {
    let s = ecad_ui::SHELL;
    ui.add_space(space::S);
    ui.label(egui::RichText::new(&row.name).strong().size(ui_type::OBJECT.size).color(ecad_ui::color(s.text)));
    let n = row.pages;
    ui.label(egui::RichText::new(format!("Page type · {n} page{}", if n == 1 { "" } else { "s" })).small().color(ecad_ui::color(s.text_2)));
    ui.add_space(space::L);
    // The sheet's preview: the frame, the title block in place, zones when on.
    let w = ui.available_width();
    let h = w * row.shown.height as f32 / row.shown.width.max(1) as f32;
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "ecad.settings.pages.preview"));
    preview(ui.painter(), r, row);
    ui.add_space(space::L);
    let id = row.id.text();
    let tb = title_block_words(&row.shown);
    let summary = format!("{} · {tb} · zones {}", sheet_words(&row.shown), if row.zones { "on" } else { "off" });
    section(ui, "sheet", "Sheet", &summary, |ui| {
        prow(ui, "Size", |ui| {
            let r = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover()).0;
            let c = chip(ui, r, &sheet_words(&row.shown), egui::Id::unique(("ecad.settings.pages.size", &id)), "ecad.settings.pages.size", writable);
            egui::Popup::menu(&c).show(|ui| sheet_menu(ui, row, snap, todo));
        });
        // Other…: any size the sheet field takes (019's `A2 12x8`).
        prow(ui, "", |ui| {
            let v = &mut t.settings.pages.other;
            if v.as_ref().is_none_or(|(o, _)| *o != row.id) {
                *v = Some((row.id.clone(), row.sheet.map(|x| x.text()).unwrap_or_default()));
            }
            if let Some((_, text)) = v {
                let r =
                    ui.add_enabled(writable, egui::TextEdit::singleline(text).hint_text("Other: 594x420 12x8").desired_width(ui.available_width()));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
                    todo.push(Do::Set(row.id.clone(), vec![("sheet", Some(text.trim().to_string()))]));
                }
            }
        });
        prow(ui, "Title block", |ui| {
            let r = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover()).0;
            let c = chip(ui, r, &tb, egui::Id::unique(("ecad.settings.pages.tb", &id)), "ecad.settings.pages.panel.titleblock", writable);
            egui::Popup::menu(&c).show(|ui| title_block_menu(ui, row, todo));
        });
        prow(ui, "Frame style", |ui| {
            let r = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover()).0;
            let shown =
                row.frame.as_ref().and_then(|f| snap.frames.iter().find(|(x, _)| x == f)).map_or_else(|| "Project's".to_string(), |(_, n)| n.clone());
            let c = chip(ui, r, &shown, egui::Id::unique(("ecad.settings.pages.framestyle", &id)), "ecad.settings.pages.panel.frame", writable);
            egui::Popup::menu(&c).show(|ui| {
                if ui.button("Project's").clicked() {
                    todo.push(Do::Set(row.id.clone(), vec![("frame", Some("project".into()))]));
                }
                for (f, name) in &snap.frames {
                    if ui.button(name).clicked() {
                        todo.push(Do::Set(row.id.clone(), vec![("frame", Some(f.text()))]));
                    }
                }
            });
        });
        prow(ui, "Zones", |ui| {
            if switch(ui, row.zones, writable, "ecad.settings.pages.panel.zones").clicked() {
                todo.push(Do::Set(row.id.clone(), vec![("zones", Some(if row.zones { "off" } else { "on" }.into()))]));
            }
        });
    });
    section(ui, "book", "In the Book", row.position.label(), |ui| {
        prow(ui, "Position", |ui| {
            let mut i = row.position as usize;
            let ch = ui.add_enabled_ui(writable, |ui| ecad_ui::prim::segmented(ui, &mut i, &["Front", "Body", "Back"]).changed()).inner;
            if ch {
                let w = ["front", "body", "back"].get(i).copied().unwrap_or("body");
                todo.push(Do::Set(row.id.clone(), vec![("position", Some(w.into()))]));
            }
        });
        ui.label(egui::RichText::new(NUMBERS_NOTE).small().color(ecad_ui::color(s.text_3)));
    });
    if row.own || row.overridden {
        section(ui, "name", "Name", &row.name, |ui| {
            if row.own {
                prow(ui, "Name", |ui| {
                    let v = &mut t.settings.pages.name;
                    if v.as_ref().is_none_or(|(o, _)| *o != row.id) {
                        *v = Some((row.id.clone(), row.name.clone()));
                    }
                    if let Some((_, text)) = v {
                        let r = ui.add_enabled(writable, egui::TextEdit::singleline(text).desired_width(ui.available_width()));
                        if r.lost_focus() && text.trim() != row.name && !text.trim().is_empty() {
                            todo.push(Do::Rename(row.id.clone(), text.trim().to_string()));
                        }
                    }
                });
            }
            if row.overridden && ui.add_enabled(writable, ecad_ui::prim::Button::new("Reset").small()).clicked() {
                todo.push(Do::Reset(row.id.clone()));
            }
        });
    }
}

/// The selected type's sheet in miniature: paper, frame, zone ticks when on, the title block at
/// the bottom right; a panel layout shows its plate and door.
fn preview(p: &egui::Painter, r: Rect, row: &TypeRow) {
    let sheet = crate::canvas::Sheet::Paper.tokens();
    let (paper, ink, dim) = (ecad_ui::color(sheet.sheet), ecad_ui::color(sheet.ink), ecad_ui::color(sheet.ink_2));
    p.rect_filled(r, 2.0, paper);
    let k = r.width() / row.shown.width.max(1) as f32;
    let frame = Rect::from_min_max(pos2(r.left() + 10.0 * k, r.top() + 7.0 * k), pos2(r.right() - 10.0 * k, r.bottom() - 10.0 * k));
    let st = egui::Stroke::new(1.0, ink);
    p.rect_stroke(frame, 0.0, st, egui::StrokeKind::Middle);
    if row.zones {
        let (c, rr) = (row.shown.columns.max(1), row.shown.rows.max(1));
        for i in 1..c {
            let x = frame.left() + frame.width() * i as f32 / c as f32;
            p.vline(x, frame.top() - 3.0..=frame.top(), st);
            p.vline(x, frame.bottom()..=frame.bottom() + 3.0, st);
        }
        for i in 1..rr {
            let y = frame.top() + frame.height() * i as f32 / rr as f32;
            p.hline(frame.left() - 3.0..=frame.left(), y, st);
            p.hline(frame.right()..=frame.right() + 3.0, y, st);
        }
    }
    // The standard title block: 230 mm wide, 40 mm high, at the bottom right (design 019 §4).
    let tb = Rect::from_min_max(pos2(frame.right() - 230.0 * k, frame.bottom() - 40.0 * k), frame.right_bottom());
    p.rect_stroke(tb, 0.0, st, egui::StrokeKind::Middle);
    p.hline(tb.x_range(), tb.center().y, st);
    for f in [0.52, 0.86] {
        p.vline(tb.left() + tb.width() * f, tb.y_range(), st);
    }
    if matches!(&row.id, StyleRef::Builtin(w) if w == ptype::PANEL) {
        let dashed = |a: Rect, label: &str| {
            let pts = [a.left_top(), a.right_top(), a.right_bottom(), a.left_bottom(), a.left_top()];
            p.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0, dim), 4.0, 3.0));
            p.text(a.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), dim);
        };
        let top = frame.top() + 20.0 * k;
        let bottom = tb.top() - 14.0 * k;
        dashed(Rect::from_min_max(pos2(frame.left() + 15.0 * k, top), pos2(frame.center().x - 15.0 * k, bottom)), "plate");
        dashed(Rect::from_min_max(pos2(frame.center().x + 15.0 * k, top), pos2(frame.right() - 15.0 * k, bottom)), "door");
    }
}

/// Rev 3's project sheet and numbering rows, in one folded section (design 054 §5.4).
fn project_section(t: &mut Trial, ui: &mut egui::Ui) {
    let s = ecad_ui::SHELL;
    let open = t.settings.pages.project_open;
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, PROJECT));
    let icon = if open { ecad_ui::tokens::Icon::ChevronDown } else { ecad_ui::tokens::Icon::ChevronRight };
    ecad_ui::icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 6.0, r.center().y), vec2(10.0, 10.0)), icon, ecad_ui::color(s.text_2));
    ui.painter().text(
        pos2(r.left() + 20.0, r.center().y),
        egui::Align2::LEFT_CENTER,
        "Project sheet and numbering",
        bar::font(ui, ui_type::BODY_EMPHASIS),
        ecad_ui::color(s.text),
    );
    if !open {
        let summary = t.settings.pages.snapshot.as_ref().map(|x| sheet_words(&x.project_sheet)).unwrap_or_default();
        ui.painter().text(
            pos2(r.left() + 240.0, r.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{summary} · numbering, references, tags and revisions"),
            bar::font(ui, ui_type::SMALL),
            ecad_ui::color(s.text_3),
        );
    }
    if resp.clicked() {
        t.settings.pages.project_open = !open;
    }
    if open {
        ui.set_max_width(900.0);
        super::project::body(t, ui, crate::sheets_ui::Popout::Settings, crate::sheets_ui::Part::Settings);
    }
}
