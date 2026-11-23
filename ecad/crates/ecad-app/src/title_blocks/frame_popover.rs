//! Frame…: a popover anchored to its button with the frame's sheet, margins,
//! zones and lines, and a thumbnail of the whole sheet. Apply writes into the draft; Save still
//! writes the project.

use std::collections::BTreeMap;

use ecad_model::GridLabels;
use ecad_model::document::sheet::{Margins, PRESETS, SheetSpec, margins_text, parse_frame_sheet, parse_margins};
use ecad_widgets::tokens::space;
use eframe::egui::{self, Color32, Rect, RichText, Stroke, pos2, vec2};

use crate::title_blocks::Editor;

/// Element names.
pub const SHEET: &str = "library.tb.frame.sheet";
pub(crate) const MARGINS: [&str; 4] =
    ["library.tb.frame.margin.left", "library.tb.frame.margin.right", "library.tb.frame.margin.bottom", "library.tb.frame.margin.top"];
pub const COLUMNS: &str = "library.tb.frame.columns";
pub const ROWS: &str = "library.tb.frame.rows";
pub(crate) const COLUMN_LABELS: &str = "library.tb.frame.column_labels";
pub(crate) const ROW_LABELS: &str = "library.tb.frame.row_labels";
pub(crate) const LABELS_ALL: &str = "library.tb.frame.labels_all";
pub(crate) const CENTRE_MARKS: &str = "library.tb.frame.centre_marks";
pub(crate) const BORDER: &str = "library.tb.frame.border";
pub(crate) const INNER: &str = "library.tb.frame.inner";
pub const COLOUR: &str = "library.tb.frame.colour";
pub const APPLY: &str = "library.tb.frame.apply";

/// The popover's values while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameEdit {
    /// `page`: each page's own sheet.
    pub own: bool,
    pub spec: SheetSpec,
    pub margins: Margins,
    pub labels: bool,
    pub centre: bool,
    pub border: f64,
    pub inner: f64,
    pub colour: String,
}

impl FrameEdit {
    /// The popover's values from the draft's fields.
    pub fn of(fields: &BTreeMap<String, String>) -> FrameEdit {
        let f = |k: &str| fields.get(k).map(String::as_str).unwrap_or_default();
        let spec = parse_frame_sheet(f("sheet"));
        FrameEdit {
            own: spec.is_none(),
            spec: spec.unwrap_or_default(),
            margins: parse_margins(f("margins")).unwrap_or_default(),
            labels: ecad_model::naming::words::flag(f("grid_labels")) != Some(false),
            centre: ecad_model::naming::words::flag(f("centre_marks")) != Some(false),
            border: ecad_electrical::input::frame_weight(f("border_weight"), 0.5),
            inner: ecad_electrical::input::frame_weight(f("inner_weight"), 0.25),
            colour: if f("line_colour").is_empty() { "none".into() } else { f("line_colour").into() },
        }
    }

    /// The draft's fields with these values.
    pub fn apply(&self, fields: &mut BTreeMap<String, String>) {
        let n = |v: f64| if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v}") };
        fields.insert("sheet".into(), if self.own { "page".into() } else { self.spec.text() });
        fields.insert("margins".into(), margins_text(&self.margins));
        fields.insert("grid_labels".into(), ecad_model::naming::words::flag_word(self.labels).into());
        fields.insert("centre_marks".into(), ecad_model::naming::words::flag_word(self.centre).into());
        fields.insert("border_weight".into(), n(self.border));
        fields.insert("inner_weight".into(), n(self.inner));
        fields.insert("line_colour".into(), self.colour.clone());
    }
}

fn c32(c: ecad_widgets::tokens::Color) -> Color32 {
    ecad_widgets::color(c)
}

fn labels(ui: &mut egui::Ui, v: &mut GridLabels, name: &str) {
    let mut i = usize::from(*v == GridLabels::Letters);
    let r = ecad_widgets::prim::segmented(ui, &mut i, &["1 2 3", "A B C"]);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, name));
    *v = if i == 1 { GridLabels::Letters } else { GridLabels::Numbers };
}

/// Draw the popover under `button`; Apply writes into the draft.
pub fn show(ctx: &egui::Context, ed: &mut Editor) {
    let Some(edit) = ed.frame.as_mut() else { return };
    let s = ecad_widgets::SHELL;
    let at = ed.canvas.frame_button.map_or(pos2(400.0, 120.0), |r| pos2(r.right() - 380.0, r.bottom() + 6.0));
    let mut apply = false;
    let mut close = false;
    let read_only = ed.draft.as_ref().is_none_or(|d| d.builtin);
    egui::Area::new(egui::Id::unique("library.tb.frame.popover")).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).inner_margin(space::M).show(ui, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Frame").strong());
            ui.add_enabled_ui(!read_only, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Sheet").small().color(c32(s.text_2)));
                    let mut i = if edit.own {
                        0
                    } else {
                        PRESETS.iter().position(|p| p.1 == edit.spec.width && p.2 == edit.spec.height).map_or(5, |i| i + 1)
                    };
                    let r = ecad_widgets::prim::segmented(ui, &mut i, &["Page's own", "A3", "A2", "A1", "A0", "W × H"]);
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, SHEET));
                    edit.own = i == 0;
                    if let Some(p) = PRESETS.get(i.wrapping_sub(1)).filter(|p| (p.1, p.2) != (edit.spec.width, edit.spec.height))
                        && let Some(spec) = SheetSpec::preset(p.0)
                    {
                        edit.spec = spec;
                    }
                });
                if !edit.own {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut edit.spec.width)
                                .range(ecad_model::document::sheet::MIN_WIDTH_MM..=ecad_model::document::sheet::MAX_WIDTH_MM)
                                .suffix(" mm"),
                        );
                        ui.label("×");
                        ui.add(
                            egui::DragValue::new(&mut edit.spec.height)
                                .range(ecad_model::document::sheet::MIN_HEIGHT_MM..=ecad_model::document::sheet::MAX_HEIGHT_MM)
                                .suffix(" mm"),
                        );
                        grey(ui, "landscape");
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Margins").small().color(c32(s.text_2)));
                    let m = &mut edit.margins;
                    for (v, name, word) in [
                        (&mut m.left, MARGINS[0], "L"),
                        (&mut m.right, MARGINS[1], "R"),
                        (&mut m.bottom, MARGINS[2], "B"),
                        (&mut m.top, MARGINS[3], "T"),
                    ] {
                        ui.label(RichText::new(word).small());
                        let r = ui.add(egui::DragValue::new(v).range(0.0..=ecad_model::document::sheet::MAX_MARGIN_MM).speed(0.5));
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, name));
                    }
                });
                if !edit.own {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Zones").small().color(c32(s.text_2)));
                        let c = ui.add(egui::DragValue::new(&mut edit.spec.columns).range(1..=ecad_model::document::sheet::MAX_GRID));
                        c.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, COLUMNS));
                        ui.label("×");
                        let r = ui.add(egui::DragValue::new(&mut edit.spec.rows).range(1..=ecad_model::document::sheet::MAX_GRID));
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, ROWS));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Columns").small().color(c32(s.text_2)));
                        labels(ui, &mut edit.spec.column_labels, COLUMN_LABELS);
                        ui.label(RichText::new("Rows").small().color(c32(s.text_2)));
                        labels(ui, &mut edit.spec.row_labels, ROW_LABELS);
                    });
                } else {
                    grey(ui, "Zones follow each page's sheet (Settings ▸ Project).");
                }
                ui.horizontal(|ui| {
                    let a = ui.checkbox(&mut edit.labels, "Labels on all sides");
                    a.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, edit.labels, LABELS_ALL));
                    let c = ui.checkbox(&mut edit.centre, "Centre marks");
                    c.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, edit.centre, CENTRE_MARKS));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Lines").small().color(c32(s.text_2)));
                    let b = ui.add(egui::DragValue::new(&mut edit.border).range(0.13..=2.11).speed(0.01).prefix("border ").suffix(" mm"));
                    b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, BORDER));
                    let i = ui.add(egui::DragValue::new(&mut edit.inner).range(0.13..=2.11).speed(0.01).prefix("inner ").suffix(" mm"));
                    i.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, INNER));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Colour").small().color(c32(s.text_2)));
                    ecad_widgets::control::TextEntry::new(&mut edit.colour, COLOUR).placeholder("none, RD, #333333").width(90.0).show(ui);
                });
            });
            // The preview: the whole sheet with the block in place.
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), egui::Sense::hover());
            preview(ui, r, edit, ed.draft.as_ref().map(|d| d.now.layout.size));
            ui.horizontal(|ui| {
                let a = ui.add_enabled(!read_only, egui::Button::new("Apply"));
                a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !read_only, APPLY));
                apply = a.clicked();
                close = ui.button("Close").clicked();
            });
        });
    });
    if apply {
        let e = edit.clone();
        if let Some(d) = ed.draft.as_mut() {
            let mut fields = d.now.fields.clone();
            e.apply(&mut fields);
            if fields != d.now.fields {
                d.step();
                d.now.fields = fields;
            }
        }
        ed.frame = None;
    }
    if close {
        ed.frame = None;
    }
}

fn grey(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).small().color(c32(ecad_widgets::SHELL.text_3)));
}

/// A thumbnail of the sheet: paper, frame, zones, the block at the bottom-right.
fn preview(ui: &egui::Ui, r: Rect, e: &FrameEdit, block: Option<(f64, f64)>) {
    let spec = if e.own { SheetSpec::default() } else { e.spec };
    let (w, h) = (f64::from(spec.width), f64::from(spec.height));
    let k = (f64::from(r.width()) / w).min(f64::from(r.height()) / h) as f32;
    let paper = Rect::from_center_size(r.center(), vec2(w as f32 * k, h as f32 * k));
    let p = ui.painter();
    // The thumbnail is drawn on paper, in ink (the sheet tokens).
    let ink = ecad_widgets::color(ecad_model::palette::sheet::PAPER.ink);
    p.rect_filled(paper, 0.0, ecad_widgets::color(ecad_model::palette::sheet::PAPER.sheet));
    let m = e.margins;
    let frame = Rect::from_min_max(paper.min + vec2(m.left as f32 * k, m.top as f32 * k), paper.max - vec2(m.right as f32 * k, m.bottom as f32 * k));
    p.rect_stroke(frame, 0.0, Stroke::new(1.0, ink), egui::StrokeKind::Middle);
    for i in 1..spec.columns {
        let x = frame.left() + frame.width() * i as f32 / spec.columns as f32;
        p.line_segment([pos2(x, frame.top()), pos2(x, frame.top() - 3.0)], Stroke::new(0.6, ink));
    }
    if let Some((bw, bh)) = block {
        let b = Rect::from_min_max(pos2((frame.right() - bw as f32 * k).max(frame.left()), frame.bottom() - bh as f32 * k), frame.right_bottom());
        p.rect_stroke(b, 0.0, Stroke::new(1.0, ink), egui::StrokeKind::Middle);
    }
}
