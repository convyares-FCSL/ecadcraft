//! The editor's toolbar and left column (design 055 §4.1, §4.3, §4.6): the Library tabs and the
//! tools; the frames with their thumbnails; **+ Title block** and **Import DXF**; the field
//! palette, whose rows are dragged onto the block.

use ecad_cadcraft::styles::Layer;
use ecad_model::title_block::{Align, Binding, BlockItem, IfEmpty, Ink, Newest, Rect as BRect, Source, TextLook, TitleBlockLayout};
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

use super::{Act, DxfAsk, Editor, Tool};
use crate::library::browser::{self, Browser, Listing};
use crate::shell::bar;

/// Element names (design 055 §5).
pub const NEW: &str = "library.tb.list.new";
pub const IMPORT: &str = "library.tb.list.import";
pub const IMPORT_PATH: &str = "library.tb.list.import.path";
pub const IMPORT_GO: &str = "library.tb.list.import.go";
/// The left column's width.
pub const LIST_W: f32 = 300.0;
/// A row's height.
pub const ROW_H: f32 = 56.0;
/// Default sizes of dropped items (mm).
pub const FIELD_SIZE: (f64, f64) = (40.0, 8.0);
pub const TABLE_SIZE: (f64, f64) = (120.0, 25.0);

fn c32(c: ecad_ui::tokens::Color) -> Color32 {
    ecad_ui::color(c)
}

/// What a palette row drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drops {
    /// Fields side by side: (binding, label, red bold).
    Fields(&'static [(&'static str, &'static str)]),
    Table,
    /// Asks for a key.
    ProjectField,
    /// Starts the Logo tool.
    Logo,
}

/// One palette row (design 055 §4.3).
#[derive(Debug, Clone, Copy)]
pub struct PaletteRow {
    pub label: &'static str,
    pub drops: Drops,
    pub chip: &'static str,
}

/// The palette.
pub const PALETTE: [PaletteRow; 15] = [
    PaletteRow { label: "Project name", drops: Drops::Fields(&[("project:title", "Project")]), chip: "project" },
    PaletteRow { label: "Drawing number", drops: Drops::Fields(&[("project:drawing_number", "Drawing no.")]), chip: "project" },
    PaletteRow { label: "Customer", drops: Drops::Fields(&[("project:customer", "Customer")]), chip: "project" },
    PaletteRow { label: "Status", drops: Drops::Fields(&[("project:status", "Status")]), chip: "project" },
    PaletteRow { label: "Page title", drops: Drops::Fields(&[("page:title", "Page title")]), chip: "page" },
    PaletteRow { label: "Page number, of total", drops: Drops::Fields(&[("page:number", "Page"), ("page:total", "of")]), chip: "page" },
    PaletteRow { label: "Next page", drops: Drops::Fields(&[("page:next", "Next")]), chip: "page" },
    PaletteRow { label: "Function, location", drops: Drops::Fields(&[("page:function", "Function"), ("page:location", "Location")]), chip: "page" },
    PaletteRow { label: "Revision and date", drops: Drops::Fields(&[("revision:rev", "Revision"), ("revision:date", "Date")]), chip: "revision" },
    PaletteRow { label: "ECR", drops: Drops::Fields(&[("revision:ecr", "ECR")]), chip: "revision" },
    PaletteRow {
        label: "Drawn, checked, approved",
        drops: Drops::Fields(&[("revision:drawn", "Drawn"), ("revision:checked", "Checked"), ("revision:approved", "Approved")]),
        chip: "people",
    },
    PaletteRow { label: "Revision table, 4 rows", drops: Drops::Table, chip: "revision" },
    PaletteRow { label: "Sheet size", drops: Drops::Fields(&[("print:sheet", "Size")]), chip: "print" },
    PaletteRow { label: "Project field…", drops: Drops::ProjectField, chip: "project" },
    PaletteRow { label: "Logo", drops: Drops::Logo, chip: "image" },
];

impl PaletteRow {
    /// Its element name: `library.tb.palette.project_drawing_number`.
    pub fn name(&self) -> String {
        let key = match self.drops {
            Drops::Fields(f) => f.first().map_or("", |x| x.0).replace(':', "_"),
            Drops::Table => "revision_table".into(),
            Drops::ProjectField => "project_field".into(),
            Drops::Logo => "logo".into(),
        };
        format!("library.tb.palette.{key}")
    }

    /// The items this row drops with their top-left corner at `at` (block mm).
    pub fn items(&self, at: (f64, f64)) -> Vec<BlockItem> {
        match self.drops {
            Drops::Fields(f) => f
                .iter()
                .enumerate()
                .map(|(i, (b, label))| {
                    let x = at.0 + (FIELD_SIZE.0 + 2.0) * i as f64;
                    let bind = Binding::parse(b);
                    let red = bind.source == Source::Project && bind.key == "status";
                    let look = TextLook { size: 2.5, bold: red, align: Align::Left, colour: if red { Ink::Red } else { Ink::Ink } };
                    BlockItem::Field {
                        at: BRect::new(x, at.1 - FIELD_SIZE.1, FIELD_SIZE.0, FIELD_SIZE.1),
                        bind,
                        label: Some((*label).to_string()),
                        look,
                        if_empty: IfEmpty::Blank,
                    }
                })
                .collect(),
            Drops::Table => vec![BlockItem::RevTable {
                at: BRect::new(at.0, at.1 - TABLE_SIZE.1, TABLE_SIZE.0, TABLE_SIZE.1),
                rows: 4,
                header: true,
                newest: Newest::Top,
            }],
            Drops::ProjectField | Drops::Logo => Vec::new(),
        }
    }

    /// Its size when dropped (mm).
    pub fn size(&self) -> (f64, f64) {
        match self.drops {
            Drops::Fields(f) => ((FIELD_SIZE.0 + 2.0) * f.len() as f64 - 2.0, FIELD_SIZE.1),
            Drops::Table => TABLE_SIZE,
            _ => FIELD_SIZE,
        }
    }
}

/// The toolbar: the Library tabs, then the tools, Sample, Snap and Frame….
pub fn head(ui: &mut egui::Ui, main: Rect, b: &mut Browser, list: &Listing, ed: &mut Editor) -> Vec<Act> {
    let s = ecad_ui::SHELL;
    let line = Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, c32(s.line));
    let head = Rect::from_min_size(main.min, vec2(main.width(), browser::TABS_H));
    ui.painter().rect_filled(main, 0.0, c32(s.bg));
    ui.painter().rect_filled(head, 0.0, c32(s.panel));
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, line);
    let builtin = ed.draft.as_ref().is_some_and(|d| d.builtin);
    let acts = Vec::new();
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(space::M, space::S))), |ui| {
        ui.horizontal_centered(|ui| {
            browser::tabs(ui, b, list);
            ui.add_space(space::M);
            bar::vline(ui, ui.cursor().left(), head.top() + 10.0, head.bottom() - 10.0);
            ui.add_space(space::M);
            for t in Tool::ALL {
                let w = bar::text_width(ui, t.label(), ui_type::BODY) + 2.0 * space::M;
                let (r, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
                let enabled = !builtin || t == Tool::Select;
                let resp = bar::hit(ui, r, &t.name(), ed.tool == t, true, enabled);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, t.label(), bar::font(ui, ui_type::BODY), bar::ink(ed.tool == t, true, enabled));
                if resp.clicked() && enabled {
                    ed.tool = t;
                }
            }
            ui.add_space(space::M);
            bar::vline(ui, ui.cursor().left(), head.top() + 10.0, head.bottom() - 10.0);
            ui.add_space(space::M);
            switch(ui, "Sample", super::SAMPLE, &mut ed.sample);
            switch(ui, "Snap", super::SNAP, &mut ed.snap);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let f = ui.add(ecad_ui::prim::Button::new("Frame…").small());
                f.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, super::FRAME));
                if f.clicked() {
                    ed.frame = match ed.frame {
                        Some(_) => None,
                        None => ed.draft.as_ref().map(|d| super::frame_popover::FrameEdit::of(&d.now.fields)),
                    };
                    ed.canvas.frame_button = Some(f.rect);
                }
            });
        });
    });
    acts
}

/// A labelled switch (Sample, Snap).
fn switch(ui: &mut egui::Ui, label: &str, name: &str, on: &mut bool) {
    let s = ecad_ui::SHELL;
    let w = 30.0 + 6.0 + bar::text_width(ui, label, ui_type::BODY) + space::M;
    let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *on, name));
    if resp.clicked() {
        *on = !*on;
    }
    let track = Rect::from_min_size(pos2(r.left(), r.center().y - 8.0), vec2(30.0, 16.0));
    ui.painter().rect_filled(track, 8.0, c32(if *on { s.accent } else { s.line_strong }));
    let knob = if *on { track.right() - 8.0 } else { track.left() + 8.0 };
    ui.painter().circle_filled(pos2(knob, track.center().y), 6.0, Color32::WHITE);
    bar::text_at(ui, track.right() + 6.0, r.center().y, label, ui_type::BODY, c32(s.text));
}

/// The left column: the frames, + Title block and Import DXF, and the palette.
pub fn show(ui: &mut egui::Ui, main: Rect, ed: &mut Editor, drafts_key: &str, writable: bool) -> Vec<Act> {
    let s = ecad_ui::SHELL;
    let line = Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, c32(s.line));
    let mut acts = Vec::new();
    let col = Rect::from_min_max(pos2(main.left(), main.top() + browser::TABS_H), pos2(main.left() + LIST_W, main.bottom()));
    ui.painter().rect_filled(col, 0.0, c32(s.panel));
    ui.painter().vline(col.right() - 0.5, col.y_range(), line);
    ui.scope_builder(egui::UiBuilder::new().max_rect(col.shrink2(vec2(space::S, space::M))), |ui| {
        ui.spacing_mut().item_spacing.y = space::XS;
        ui.label(egui::RichText::new("Title blocks").font(bar::font(ui, ui_type::BODY_EMPHASIS)).color(c32(s.text)));
        let list = ui.scope(|ui| {
            for r in ed.rows.clone() {
                let on = ed.open.as_deref() == Some(r.id.as_str());
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, format!("library.tb.list.row.{}", r.id)));
                if on {
                    ui.painter().rect_filled(rect, 4.0, c32(s.accent_soft));
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 4.0, c32(s.hover));
                }
                let thumb = Rect::from_min_size(rect.min + vec2(space::S, 8.0), vec2(56.0, 40.0));
                paint_thumb(ui, thumb, &r.layout);
                let x = thumb.right() + space::M;
                let dot = !drafts_key.is_empty() && ed.drafts.has(drafts_key, &r.id) && !on;
                let name = if dot { format!("{} •", r.name) } else { r.name.clone() };
                bar::text_at(ui, x, rect.top() + 18.0, &name, ui_type::BODY_EMPHASIS, c32(s.text));
                let galley_font = bar::font(ui, ui_type::SMALL);
                ui.painter().text(pos2(x, rect.top() + 34.0), Align2::LEFT_TOP, &r.line, galley_font, c32(s.text_3));
                if resp.clicked() {
                    acts.push(Act::Open(r.id.clone()));
                }
                resp.context_menu(|ui| row_menu(ui, &r, writable, &mut acts));
            }
        });
        list.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::List, true, super::LIST));
        ui.add_space(space::S);
        ui.horizontal(|ui| {
            let n = ui.add(gate(ecad_ui::prim::Button::new("+  Title block").small(), writable));
            n.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, NEW));
            if n.clicked() {
                ed.new_menu = !ed.new_menu;
            }
            let i = ui.add(gate(ecad_ui::prim::Button::new("Import DXF").small().kind(ecad_ui::prim::Kind::Quiet), writable));
            i.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, IMPORT));
            if i.clicked() {
                ed.dxf = Some(DxfAsk { sheet: "A3".into(), ..DxfAsk::default() });
            }
        });
        if ed.new_menu {
            egui::Frame::new().fill(c32(s.raised)).inner_margin(space::S).corner_radius(4.0).show(ui, |ui| {
                ui.label(egui::RichText::new("From the built-in, for").small().color(c32(s.text_2)));
                ui.horizontal_wrapped(|ui| {
                    for size in ["A3", "A2", "A1", "A0"] {
                        if ui.small_button(size).clicked() {
                            acts.push(Act::Run("titleblock".into(), json!({"new": {"from": "sheet", "sheet": size}})));
                            ed.new_menu = false;
                        }
                    }
                    if ui.small_button("this project's sheet").clicked() {
                        acts.push(Act::Run("titleblock".into(), json!({"new": {"from": "sheet"}})));
                        ed.new_menu = false;
                    }
                });
                ui.label(egui::RichText::new("Blank, for").small().color(c32(s.text_2)));
                ui.horizontal_wrapped(|ui| {
                    for size in ["A3", "A2", "A1", "A0"] {
                        if ui.small_button(format!("{size} ")).clicked() {
                            acts.push(Act::Run("titleblock".into(), json!({"new": {"from": "blank", "sheet": size}})));
                            ed.new_menu = false;
                        }
                    }
                });
            });
        }
        if let Some(ask) = ed.dxf.as_mut().filter(|a| a.logo_at.is_none()) {
            let mut go = false;
            let mut cancel = false;
            egui::Frame::new().fill(c32(s.raised)).inner_margin(space::S).corner_radius(4.0).show(ui, |ui| {
                ui.label(egui::RichText::new("Import a title block from a DXF, at 1:1").small().color(c32(s.text_2)));
                let f = ui.add(egui::TextEdit::singleline(&mut ask.path).hint_text("C:\\drawings\\block.dxf").desired_width(f32::INFINITY));
                f.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, IMPORT_PATH));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Sheet").small());
                    for size in ["A3", "A2", "A1", "A0"] {
                        if ui.selectable_label(ask.sheet == size, size).clicked() {
                            ask.sheet = size.into();
                        }
                    }
                });
                if let Some(e) = &ask.error {
                    ui.label(egui::RichText::new(e).small().color(c32(s.error)));
                }
                ui.horizontal(|ui| {
                    let b = ui.add(ecad_ui::prim::Button::new("Import").small().kind(ecad_ui::prim::Kind::Primary));
                    b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, IMPORT_GO));
                    go = b.clicked();
                    cancel = ui.small_button("Cancel").clicked();
                });
            });
            if go {
                acts.push(Act::Run("titleblockimport".into(), json!({"path": ask.path.trim(), "sheet": ask.sheet})));
            }
            if cancel || go {
                ed.dxf = None;
            }
        }
        ui.add_space(space::M);
        ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), line);
        ui.add_space(space::S);
        ui.label(egui::RichText::new("Fields · drag onto the block").font(bar::font(ui, ui_type::SMALL)).color(c32(s.text_3)));
        let read_only = ed.draft.as_ref().is_none_or(|d| d.builtin);
        for (i, row) in PALETTE.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click_and_drag());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !read_only, row.name()));
            if resp.hovered() && !read_only {
                ui.painter().rect_filled(rect, 4.0, c32(s.hover));
            }
            bar::text_at(ui, rect.left() + space::S, rect.center().y, row.label, ui_type::BODY, c32(if read_only { s.text_3 } else { s.text }));
            let chip_w = bar::text_width(ui, row.chip, ui_type::SMALL) + 12.0;
            let chip = Rect::from_center_size(pos2(rect.right() - space::S - chip_w / 2.0, rect.center().y), vec2(chip_w, 18.0));
            ui.painter().rect(chip, 4.0, c32(s.raised), Stroke::new(1.0, c32(s.line)), egui::StrokeKind::Inside);
            ui.painter().text(chip.center(), Align2::CENTER_CENTER, row.chip, bar::font(ui, ui_type::SMALL), c32(s.text_3));
            if read_only {
                continue;
            }
            if resp.drag_started() {
                ed.canvas.palette = Some(i);
            }
            if resp.double_clicked() || (resp.clicked() && matches!(row.drops, Drops::ProjectField | Drops::Logo)) {
                ed.canvas.drop_free = Some(i);
            }
        }
        if let Some(key) = ed.project_key.as_mut() {
            let mut ok = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                let f = ui.add(egui::TextEdit::singleline(key).hint_text("job_no").desired_width(140.0));
                f.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "library.tb.palette.project_field.key"));
                ok = ui.small_button("Add").clicked() || (f.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                cancel = ui.small_button("Cancel").clicked();
            });
            let valid = ecad_cadcraft::title_block::bind::is_project_key(key.trim());
            if !valid && !key.is_empty() {
                ui.label(egui::RichText::new("a-z, 0-9 and _, 1 to 32 characters").small().color(c32(s.warning)));
            }
            if ok && valid {
                let k = key.trim().to_string();
                if let Some(d) = ed.draft.as_mut() {
                    let at = super::canvas::free_place(&d.now.layout, FIELD_SIZE);
                    d.add(BlockItem::Field {
                        at: BRect::new(at.0, at.1 - FIELD_SIZE.1, FIELD_SIZE.0, FIELD_SIZE.1),
                        bind: Binding::new(Source::Project, &k),
                        label: Some(k.replace('_', " ")),
                        look: TextLook::default(),
                        if_empty: IfEmpty::Blank,
                    });
                    acts.push(Act::Resample);
                }
                ed.project_key = None;
            }
            if cancel {
                ed.project_key = None;
            }
        }
        if let Some(n) = &ed.notice {
            ui.add_space(space::S);
            ui.label(egui::RichText::new(n).small().color(c32(s.warning)));
        }
    });
    acts
}

fn row_menu(ui: &mut egui::Ui, r: &super::Row, writable: bool, acts: &mut Vec<Act>) {
    let own = r.layer != Layer::Builtin;
    ui.menu_button("Duplicate for…", |ui| {
        for size in ["A3", "A2", "A1", "A0"] {
            if ui.add_enabled(writable, egui::Button::new(size)).clicked() {
                acts.push(Act::Run("titleblock".into(), json!({"new": {"from": r.id, "sheet": size}})));
                ui.close();
            }
        }
    });
    if ui.add_enabled(writable && own && r.layer == Layer::Project, egui::Button::new("Promote to standard")).clicked() {
        acts.push(Act::Run("promote".into(), json!({"catalogue": "frame", "id": r.id})));
        ui.close();
    }
    if ui.add_enabled(writable, egui::Button::new("Use for this project")).clicked() {
        acts.push(Act::Run("setdefault".into(), json!({"key": "print.frame.style", "level": "project", "value": r.id})));
        ui.close();
    }
    if ui.add_enabled(writable && own, egui::Button::new("Delete… (pages go back to the built-in)")).clicked() {
        acts.push(Act::Run("deletestyle".into(), json!({"catalogue": "frame", "id": r.id, "replace_with": "sheet"})));
        ui.close();
    }
}

/// A thumbnail of `l`: the block's lines and boxes, fitted.
pub fn paint_thumb(ui: &egui::Ui, rect: Rect, l: &TitleBlockLayout) {
    let s = ecad_ui::SHELL;
    let p = ui.painter();
    p.rect(rect, 2.0, Color32::from_rgb(0xf6, 0xf4, 0xee), Stroke::new(1.0, c32(s.line)), egui::StrokeKind::Inside);
    let (w, h) = l.size;
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return;
    }
    // The block sits at the bottom-right of a sheet-shaped thumbnail.
    let inner = rect.shrink(3.0);
    let k = (f64::from(inner.width()) / w).min(f64::from(inner.height()) * 0.45 / h);
    let bw = (w * k) as f32;
    let bh = (h * k) as f32;
    let o = pos2(inner.right() - bw, inner.bottom());
    let ink = Color32::from_rgb(0x33, 0x33, 0x33);
    p.rect_stroke(Rect::from_min_max(inner.min, inner.max), 0.0, Stroke::new(0.8, ink), egui::StrokeKind::Inside);
    p.rect_stroke(Rect::from_min_max(pos2(o.x, o.y - bh), pos2(o.x + bw, o.y)), 0.0, Stroke::new(0.8, ink), egui::StrokeKind::Inside);
    for item in l.items.iter().take(200) {
        if let BlockItem::Line { a, b, .. } = item {
            let q = |x: f64, y: f64| pos2(o.x + (x * k) as f32, o.y - (y * k) as f32);
            p.line_segment([q(a.0, a.1), q(b.0, b.1)], Stroke::new(0.6, ink));
        }
    }
}

/// `b`, disabled with the writes-off reason unless `on`.
pub fn gate(b: ecad_ui::prim::Button<'_>, on: bool) -> ecad_ui::prim::Button<'_> {
    if on { b } else { b.disabled(super::WRITES_OFF) }
}
