//! The editor's right panel: per selection, Value (fields), Style (fields and
//! texts) and Box (every item), folding; with nothing selected, the block itself with Save,
//! Discard, Duplicate for… and Delete….

use ecad_electrical::title_blocks::bind;
use ecad_model::objects::title_block::{Align, Binding, BlockItem, IfEmpty, Ink, LineWeight, Newest, SIZES, TextDraw};
use ecad_widgets::tokens::space;
use eframe::egui::{self, Color32, RichText};
use serde_json::json;

use crate::title_blocks::draft::{Draft, snap};
use crate::title_blocks::list::{Drops, PALETTE};
use crate::title_blocks::{Act, Editor};

/// Element names.
pub const SOURCE: &str = "library.tb.inspector.value.source";
pub const IFEMPTY: &str = "library.tb.inspector.value.ifempty";
pub const LABEL: &str = "library.tb.inspector.draw.label";
pub(crate) const LABEL_TEXT: &str = "library.tb.inspector.draw.label_text";
pub const SIZE: &str = "library.tb.inspector.draw.size";
pub const BOLD: &str = "library.tb.inspector.draw.bold";
pub const ALIGN: &str = "library.tb.inspector.draw.align";
pub const COLOUR: &str = "library.tb.inspector.draw.colour";
pub const TEXT: &str = "library.tb.inspector.draw.text";
/// The "If empty" text (Text…).
pub(crate) const IFEMPTY_TEXT: &str = "library.tb.inspector.value.ifempty_text";
pub const BOX: [&str; 4] = ["library.tb.inspector.box.x", "library.tb.inspector.box.y", "library.tb.inspector.box.w", "library.tb.inspector.box.h"];
pub const ROWS: &str = "library.tb.inspector.box.rows";
pub const HEADER: &str = "library.tb.inspector.box.header";
pub(crate) const NEWEST: &str = "library.tb.inspector.box.newest";
pub(crate) const WEIGHT: &str = "library.tb.inspector.box.weight";
pub(crate) const MAKE_FIELD: &str = "library.tb.inspector.make_field";
pub const NAME: &str = "library.tb.block.name";
pub(crate) const BLOCK_SIZE: &str = "library.tb.block.size";
pub const LAYER: &str = "library.tb.block.layer";
pub const SAVE: &str = "library.tb.block.save";
pub const DISCARD: &str = "library.tb.block.discard";
pub const DUPLICATE: &str = "library.tb.block.duplicate";
pub(crate) const DELETE: &str = "library.tb.block.delete";
pub const CONSEQUENCE: &str = "library.tb.block.consequence";

fn c32(c: ecad_widgets::tokens::Color) -> Color32 {
    ecad_widgets::color(c)
}

fn grey(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).small().color(c32(ecad_widgets::SHELL.text_3)));
}

fn named(r: egui::Response, name: &str) -> egui::Response {
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    r
}

/// A folding section with its one-line summary when folded.
fn section(ui: &mut egui::Ui, title: &str, summary: &str, body: impl FnOnce(&mut egui::Ui)) {
    let id = ui.make_persistent_id(("library.tb.section", title));
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    let open = state.is_open();
    state
        .show_header(ui, |ui| {
            ui.label(RichText::new(title).strong().color(c32(ecad_widgets::SHELL.text)));
            if !open {
                ui.label(RichText::new(summary).small().color(c32(ecad_widgets::SHELL.text_3)));
            }
        })
        .body(body);
}

/// The bindings of the Filled from list, grouped.
fn sources() -> Vec<(&'static str, Vec<(&'static str, String)>)> {
    let mut groups: Vec<(&'static str, Vec<(&'static str, String)>)> = Vec::new();
    for row in PALETTE.iter() {
        if let Drops::Fields(f) = row.drops {
            let group = match row.chip {
                "project" => "Project",
                "page" => "Page",
                "revision" => "Revision",
                "people" => "People",
                _ => "Print",
            };
            for (b, _) in f {
                let name = Binding::parse(b).map(|x| bind::name(&x)).unwrap_or_default();
                match groups.iter_mut().find(|g| g.0 == group) {
                    Some(g) => g.1.push((*b, name)),
                    None => groups.push((group, vec![(*b, name)])),
                }
            }
        }
    }
    groups
}

/// Draw the panel; returns what the window must do.
pub fn show(ui: &mut egui::Ui, ed: &mut Editor, writable: bool) -> Vec<Act> {
    let s = ecad_widgets::SHELL;
    let mut acts = Vec::new();
    let pages = ed.open_row().map_or(0, |r| r.pages);
    let layer = ed.open_row().map(|r| r.layer);
    let sample = ed.sample_data.clone();
    let sample_on = ed.sample;
    let edit_text = ed.canvas.edit_text.take();
    let Some(d) = ed.draft.as_mut() else {
        ui.label(RichText::new("Title blocks").font(ecad_widgets::fonts::font(ui.ctx(), ecad_widgets::tokens::ui_type::TITLE)));
        grey(ui, "Pick a title block on the left.");
        return acts;
    };
    let read_only = d.builtin;
    if d.selected.is_empty() {
        block_panel(ui, d, &mut acts, writable, pages, layer, &ed.notice, &mut ed.ask_discard);
        return acts;
    }
    let sel: Vec<usize> = d.selected.iter().copied().collect();
    let first = sel.first().copied().unwrap_or(0);
    let Some(item) = d.now.layout.items.get(first).cloned() else { return acts };
    // The header: the item's name and a grey line.
    let (title, kind) = match &item {
        BlockItem::Field { bind: b, .. } => (bind::name(b), "Field"),
        BlockItem::Text { .. } => ("Text".to_string(), "Text"),
        BlockItem::Line { .. } => ("Line".to_string(), "Line"),
        BlockItem::RevTable { .. } => ("Revision table".to_string(), "Table"),
        BlockItem::Logo { name, .. } => (if name.is_empty() { "Logo".to_string() } else { name.clone() }, "Logo"),
    };
    let title = if sel.len() > 1 { format!("{} items", sel.len()) } else { title };
    ui.label(RichText::new(title).font(ecad_widgets::fonts::font(ui.ctx(), ecad_widgets::tokens::ui_type::OBJECT)).color(c32(s.text)));
    grey(ui, &format!("{kind} · {}", d.now.name));
    if read_only {
        egui::Frame::new().fill(c32(s.accent_soft)).inner_margin(space::S).corner_radius(4.0).show(ui, |ui| {
            ui.label(RichText::new(crate::title_blocks::BUILTIN_NOTE).small());
        });
    }
    ui.add_enabled_ui(!read_only, |ui| {
        // Value (fields only).
        if let BlockItem::Field { bind: b, if_empty, .. } = &item {
            let shown = sample.as_ref().filter(|_| sample_on).map(|sm| (bind::shown(sm.value(b), if_empty), sm.page.clone()));
            section(ui, "Value", &bind::name(b), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Filled from").small().color(c32(s.text_2)));
                    let mut pick = None;
                    // The bindings, grouped by their source, then Project field… (a free key).
                    let mut binds: Vec<&'static str> = Vec::new();
                    let mut labels: Vec<String> = Vec::new();
                    for (group, items) in sources() {
                        for (bt, name) in items {
                            binds.push(bt);
                            labels.push(format!("{group} ▸ {name}"));
                        }
                    }
                    labels.push("Project field…".to_string());
                    let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                    let current = format!("{} ▸ {}", capital(bind::chip(b)), bind::name(b));
                    let mut i = binds.iter().position(|x| *x == b.text()).unwrap_or(usize::MAX);
                    let r = ecad_widgets::control::Select::new(&mut i, &options, SOURCE).placeholder(&current).show(ui);
                    if r.changed() {
                        // The entry after the palette's bindings asks for a project key of one's own.
                        pick = match binds.get(i) {
                            Some(bt) => Binding::parse(bt).ok().map(Some),
                            None if i == binds.len() => Some(None),
                            None => None,
                        };
                    }
                    if let Some(choice) = pick {
                        if let Some(nb) = choice {
                            d.step();
                            for i in &sel {
                                if let Some(BlockItem::Field { bind: x, .. }) = d.now.layout.items.get_mut(*i) {
                                    *x = nb.clone();
                                }
                            }
                            acts.push(Act::Resample);
                        } else {
                            ed.project_key = Some(String::new());
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Shows").small().color(c32(s.text_2)));
                    match &shown {
                        Some((v, page)) => {
                            ui.label(RichText::new(if v.is_empty() { "(empty)" } else { v }).color(c32(s.text_2)));
                            grey(ui, page);
                        }
                        None => grey(ui, "—"),
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("If empty").small().color(c32(s.text_2)));
                    let mut i = match if_empty {
                        IfEmpty::Blank => 0,
                        IfEmpty::Dash => 1,
                        IfEmpty::Missing => 2,
                        IfEmpty::Text(_) => 3,
                    };
                    let r = ecad_widgets::prim::segmented(ui, &mut i, &["Blank", "Dash", "?", "Text…"]);
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, IFEMPTY));
                    let now = match i {
                        0 => IfEmpty::Blank,
                        1 => IfEmpty::Dash,
                        2 => IfEmpty::Missing,
                        _ => match if_empty {
                            IfEmpty::Text(t) => IfEmpty::Text(t.clone()),
                            _ => IfEmpty::Text(String::new()),
                        },
                    };
                    if now != *if_empty {
                        set_fields(d, &sel, |it| {
                            if let BlockItem::Field { if_empty: x, .. } = it {
                                *x = now.clone();
                            }
                        });
                    }
                });
                if let IfEmpty::Text(t) = if_empty {
                    let mut text = t.clone();
                    let r = ecad_widgets::control::TextEntry::new(&mut text, IFEMPTY_TEXT).placeholder("n/a").show(ui);
                    limit(&mut text, ecad_model::objects::title_block::MAX_IF_EMPTY_TEXT);
                    if r.changed() {
                        set_fields(d, &sel, |it| {
                            if let BlockItem::Field { if_empty: x, .. } = it {
                                *x = IfEmpty::Text(text.clone());
                            }
                        });
                    }
                }
            });
        }
        // Style (fields and texts).
        let draw = match &item {
            BlockItem::Field { draw, .. } | BlockItem::Text { draw, .. } => Some(*draw),
            _ => None,
        };
        if let Some(draw) = draw {
            section(ui, "Style", &format!("{} mm{}", draw.size, if draw.bold { ", bold" } else { "" }), |ui| {
                if let BlockItem::Text { text, .. } = &item {
                    ui.label(RichText::new("Text").small().color(c32(s.text_2)));
                    let mut t = text.clone();
                    let r = ecad_widgets::control::TextEntry::new(&mut t, TEXT).show(ui);
                    limit(&mut t, ecad_model::objects::title_block::MAX_BODY_TEXT);
                    if edit_text == Some(first) {
                        r.request_focus();
                    }
                    if r.changed()
                        && let Some(BlockItem::Text { text: x, .. }) = d.now.layout.items.get_mut(first)
                    {
                        *x = t;
                    }
                    let mf = ui.small_button("Make field");
                    named(mf.clone(), MAKE_FIELD);
                    if mf.clicked() {
                        make_field(d, first);
                        acts.push(Act::Resample);
                    }
                }
                if let BlockItem::Field { label, .. } = &item {
                    ui.horizontal(|ui| {
                        let mut on = label.is_some();
                        let r = ui.checkbox(&mut on, "Label");
                        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, LABEL));
                        let mut text = label.clone().unwrap_or_default();
                        let t = ui.add_enabled_ui(on, |ui| ecad_widgets::control::TextEntry::new(&mut text, LABEL_TEXT).show(ui)).inner;
                        limit(&mut text, ecad_model::objects::title_block::MAX_BODY_TEXT);
                        if r.changed() || t.changed() {
                            let new = on.then(|| text.clone());
                            set_fields(d, &sel, |it| {
                                if let BlockItem::Field { label: x, .. } = it {
                                    *x = new.clone();
                                }
                            });
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Size").small().color(c32(s.text_2)));
                    let labels: Vec<String> = SIZES.iter().map(|v| format!("{v}")).collect();
                    let references: Vec<&str> = labels.iter().map(String::as_str).collect();
                    let mut i = SIZES.iter().position(|v| (v - draw.size).abs() < 1e-9).unwrap_or(usize::MAX);
                    let r = ecad_widgets::prim::segmented(ui, &mut i, &references);
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, SIZE));
                    if let Some(v) = SIZES.get(i).filter(|v| (**v - draw.size).abs() > 1e-9) {
                        set_draw(d, &sel, |l| l.size = *v);
                    }
                });
                ui.horizontal(|ui| {
                    let mut b = draw.bold;
                    let r = ui.checkbox(&mut b, "Bold");
                    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, b, BOLD));
                    if r.changed() {
                        set_draw(d, &sel, |l| l.bold = b);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Align").small().color(c32(s.text_2)));
                    let mut i = match draw.align {
                        Align::Left => 0,
                        Align::Centre => 1,
                        Align::Right => 2,
                    };
                    let r = ecad_widgets::prim::segmented(ui, &mut i, &["Left", "Centre", "Right"]);
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, ALIGN));
                    let a = [Align::Left, Align::Centre, Align::Right].get(i).copied().unwrap_or_default();
                    if a != draw.align {
                        set_draw(d, &sel, |l| l.align = a);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Colour").small().color(c32(s.text_2)));
                    let mut i = usize::from(draw.colour == Ink::Red);
                    let r = ecad_widgets::prim::segmented(ui, &mut i, &["Ink", "Red"]);
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, COLOUR));
                    let c = if i == 1 { Ink::Red } else { Ink::Ink };
                    if c != draw.colour {
                        set_draw(d, &sel, |l| l.colour = c);
                    }
                });
            });
        }
        // Box (every item).
        section(ui, "Box", "mm", |ui| match &item {
            BlockItem::Line { a, b, weight } => {
                let mut v = [a.0, a.1, b.0, b.1];
                ui.horizontal(|ui| {
                    ui.label(RichText::new("From").small().color(c32(s.text_2)));
                    num(ui, &mut v[0], BOX[0]);
                    num(ui, &mut v[1], BOX[1]);
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("To").small().color(c32(s.text_2)));
                    num(ui, &mut v[2], BOX[2]);
                    num(ui, &mut v[3], BOX[3]);
                });
                let mut wi = usize::from(*weight == LineWeight::Thick);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Weight").small().color(c32(s.text_2)));
                    let r = ecad_widgets::prim::segmented(ui, &mut wi, &["Thin", "Thick"]);
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, WEIGHT));
                });
                let nw = if wi == 1 { LineWeight::Thick } else { LineWeight::Thin };
                let now = BlockItem::Line { a: (v[0], v[1]), b: (v[2], v[3]), weight: nw };
                if now != item {
                    d.step();
                    if let Some(x) = d.now.layout.items.get_mut(first) {
                        *x = now;
                    }
                }
            }
            other => {
                let Some(r) = other.bounds() else { return };
                let mut v = [r.x, r.y, r.w, r.h];
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Position").small().color(c32(s.text_2)));
                    num(ui, &mut v[0], BOX[0]);
                    num(ui, &mut v[1], BOX[1]);
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Size").small().color(c32(s.text_2)));
                    num(ui, &mut v[2], BOX[2]);
                    num(ui, &mut v[3], BOX[3]);
                });
                let (bw, bh) = d.now.layout.size;
                let nr = ecad_model::objects::title_block::Rect::new(v[0], v[1], v[2].max(0.5), v[3].max(0.5));
                if nr != r {
                    if nr.x < 0.0 || nr.y < 0.0 || nr.right() > bw + 1e-9 || nr.top() > bh + 1e-9 {
                        ui.label(RichText::new(format!("Keep it inside the block ({} × {} mm).", fmt(bw), fmt(bh))).small().color(c32(s.warning)));
                    } else {
                        d.step();
                        let (dx, dy) = (nr.x - r.x, nr.y - r.y);
                        for i in &sel {
                            if let Some(x) = d.now.layout.items.get_mut(*i) {
                                if *i == first {
                                    set_rect(x, nr);
                                } else {
                                    crate::title_blocks::draft::move_item(x, dx, dy);
                                }
                            }
                        }
                    }
                }
                if let BlockItem::RevTable { rows, header, newest, .. } = other {
                    let (mut rws, mut hd, mut nw) = (*rows, *header, usize::from(*newest == Newest::Bottom));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Rows").small().color(c32(s.text_2)));
                        let r = ui.add(egui::DragValue::new(&mut rws).range(1..=ecad_model::objects::title_block::MAX_TABLE_ROWS));
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, ROWS));
                        let h = ui.checkbox(&mut hd, "Header");
                        h.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, hd, HEADER));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Newest").small().color(c32(s.text_2)));
                        let r = ecad_widgets::prim::segmented(ui, &mut nw, &["Top", "Bottom"]);
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, NEWEST));
                    });
                    let cols = sample
                        .as_ref()
                        .map(|s| s.input.rev_columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>().join(", "))
                        .unwrap_or_default();
                    grey(ui, &format!("Columns from Settings ▸ Revisions: {cols}"));
                    let n = if nw == 1 { Newest::Bottom } else { Newest::Top };
                    if (rws, hd, n) != (*rows, *header, *newest) {
                        d.step();
                        if let Some(BlockItem::RevTable { rows, header, newest, .. }) = d.now.layout.items.get_mut(first) {
                            (*rows, *header, *newest) = (rws, hd, n);
                        }
                    }
                }
                if let BlockItem::Logo { prims, .. } = other {
                    let rp = ui.small_button("Replace…");
                    named(rp.clone(), "library.tb.inspector.logo.replace");
                    if rp.clicked() {
                        ed.dxf = Some(crate::title_blocks::DxfAsk { logo_at: Some(r), ..crate::title_blocks::DxfAsk::default() });
                    }
                    grey(ui, &format!("{} shapes", prims.len()));
                }
            }
        });
    });
    acts
}

/// Keep `text` to at most `max` characters (the bounds the entries kept before the shared components).
fn limit(text: &mut String, max: usize) {
    if text.chars().count() > max {
        *text = text.chars().take(max).collect();
    }
}

fn capital(w: &str) -> String {
    let mut c = w.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// A millimetre box: typed values snap to 0.1 mm.
fn num(ui: &mut egui::Ui, v: &mut f64, name: &str) {
    let r = ui.add(egui::DragValue::new(v).speed(0.5).range(0.0..=2400.0).suffix(" mm"));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, name));
    *v = snap(*v, false);
}

fn set_rect(item: &mut BlockItem, r: ecad_model::objects::title_block::Rect) {
    match item {
        BlockItem::Text { at, .. } | BlockItem::Field { at, .. } | BlockItem::RevTable { at, .. } | BlockItem::Logo { at, .. } => *at = r,
        BlockItem::Line { .. } => {}
    }
}

/// Apply `f` to every selected field (one undo step).
fn set_fields(d: &mut Draft, sel: &[usize], f: impl Fn(&mut BlockItem)) {
    d.step();
    for i in sel {
        if let Some(x) = d.now.layout.items.get_mut(*i) {
            f(x);
        }
    }
}

/// Apply `f` to the style of every selected field and text (one undo step).
fn set_draw(d: &mut Draft, sel: &[usize], f: impl Fn(&mut TextDraw)) {
    set_fields(d, sel, |it| {
        if let BlockItem::Field { draw, .. } | BlockItem::Text { draw, .. } = it {
            f(draw);
        }
    });
}

/// Make field: a text becomes a field that keeps its box and style; its text
/// becomes the label and the binding a project field named after it.
pub(crate) fn make_field(d: &mut Draft, i: usize) {
    let Some(BlockItem::Text { at, text, draw }) = d.now.layout.items.get(i).cloned() else { return };
    d.step();
    let key: String = text
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(bind::MAX_KEY_CHARS)
        .collect();
    let key = if key.is_empty() { "field".to_string() } else { key };
    let Some(key) = ecad_model::objects::title_block::ProjectKey::new(&key) else { return };
    if let Some(x) = d.now.layout.items.get_mut(i) {
        *x = BlockItem::Field { at, bind: Binding::Project(key), label: Some(text), draw, if_empty: IfEmpty::Blank };
    }
}

/// The block's panel, with nothing selected.
#[allow(clippy::too_many_arguments)]
fn block_panel(
    ui: &mut egui::Ui, d: &mut Draft, acts: &mut Vec<Act>, writable: bool, pages: usize, layer: Option<ecad_model::registry::styles::Layer>,
    notice: &Option<String>, ask_discard: &mut bool,
) {
    let s = ecad_widgets::SHELL;
    ui.label(RichText::new(&d.now.name).font(ecad_widgets::fonts::font(ui.ctx(), ecad_widgets::tokens::ui_type::OBJECT)).color(c32(s.text)));
    let outside = d.outside().len();
    let line = if outside > 0 {
        format!("{outside} item{} outside the block", if outside == 1 { " is" } else { "s are" })
    } else {
        format!("Title block · {} items", d.now.layout.items.len())
    };
    ui.label(RichText::new(line).small().color(if outside > 0 { c32(s.warning) } else { c32(s.text_3) }));
    if d.builtin {
        egui::Frame::new().fill(c32(s.accent_soft)).inner_margin(space::S).corner_radius(4.0).show(ui, |ui| {
            ui.label(RichText::new(crate::title_blocks::BUILTIN_NOTE).small());
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Duplicate for").small());
            for size in ["A3", "A2", "A1", "A0"] {
                let b = ui.add_enabled(writable, egui::Button::new(size).small());
                b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, format!("{DUPLICATE}.{}", size.to_lowercase())));
                if b.clicked() {
                    acts.push(Act::Run("titleblock".into(), json!({"new": {"from": d.id, "sheet": size}})));
                }
            }
        });
        return;
    }
    // Save first: the consequence line, then the buttons.
    let changed = d.changed();
    let consequence = match pages {
        0 => "Saving changes no pages yet.".to_string(),
        1 => "Saving changes 1 page.".to_string(),
        n => format!("Saving changes {n} pages."),
    };
    if changed {
        ui.label(RichText::new(&consequence).color(c32(s.text))).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, CONSEQUENCE));
    }
    ui.horizontal(|ui| {
        let save = ui.add(crate::title_blocks::list::gate(
            ecad_widgets::prim::Button::new("Save").kind(ecad_widgets::prim::Kind::Primary).small(),
            writable && changed,
        ));
        save.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable && changed, SAVE));
        if save.clicked() && writable && changed {
            acts.push(Act::Save);
        }
        let discard = ui.add_enabled(changed, egui::Button::new("Discard").small());
        discard.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, changed, DISCARD));
        if discard.clicked() {
            *ask_discard = true;
        }
    });
    if !writable {
        ui.label(RichText::new(crate::title_blocks::WRITES_OFF).small().color(c32(s.text_3)));
    }
    if *ask_discard {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Discard the changes to {}?", d.now.name)).small());
            if ui.small_button("Discard").clicked() {
                acts.push(Act::Discard);
            }
            if ui.small_button("Keep").clicked() {
                *ask_discard = false;
            }
        });
    }
    if let Some(n) = notice {
        ui.label(RichText::new(n).small().color(c32(s.warning)));
    }
    ui.add_space(space::M);
    ui.label(RichText::new("Name").small().color(c32(s.text_2)));
    let mut name = d.now.name.clone();
    let r = ecad_widgets::control::TextEntry::new(&mut name, NAME).show(ui);
    limit(&mut name, ecad_model::objects::styles::MAX_NAME_CHARS);
    if r.changed() && ecad_model::objects::styles::is_name(&name) {
        d.now.name = name;
    }
    let sheet = d.now.fields.get("sheet").cloned().unwrap_or_default();
    grey(ui, &format!("Sheet {} (from Frame…)", if sheet == "page" { "of each page".to_string() } else { sheet }));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Size").small().color(c32(s.text_2)));
        let (mut w, mut h) = d.now.layout.size;
        let a = ui.add(egui::DragValue::new(&mut w).range(1.0..=2400.0).suffix(" mm"));
        ui.label("×");
        let b = ui.add(egui::DragValue::new(&mut h).range(1.0..=400.0).suffix(" mm"));
        a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, BLOCK_SIZE));
        if (a.changed() || b.changed()) && (w, h) != d.now.layout.size {
            d.step();
            d.now.layout.size = (snap(w, false), snap(h, false));
        }
    });
    let layer_word = match layer {
        Some(ecad_model::registry::styles::Layer::Standard) => "Standard",
        _ => "Project",
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("Layer").small().color(c32(s.text_2)));
        ui.label(layer_word).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, LAYER));
        if layer_word == "Project" {
            let p = ui.add_enabled(writable, egui::Button::new("Promote to standard").small());
            if p.clicked() {
                acts.push(Act::Run("promote".into(), json!({"catalogue": "frame", "id": d.id})));
            }
        }
    });
    grey(ui, &format!("Used by {pages} page{}", ecad_widgets::plural(pages)));
    ui.add_space(space::M);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Duplicate for").small());
        for size in ["A3", "A2", "A1", "A0"] {
            let b = ui.add_enabled(writable, egui::Button::new(size).small());
            b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, format!("{DUPLICATE}.{}", size.to_lowercase())));
            if b.clicked() {
                acts.push(Act::Run("titleblock".into(), json!({"new": {"from": d.id, "sheet": size}})));
            }
        }
    });
    let del = ui.add_enabled(writable, egui::Button::new("Delete… (its pages go back to the built-in)").small());
    del.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, DELETE));
    if del.clicked() {
        acts.push(Act::Run("deletestyle".into(), json!({"catalogue": "frame", "id": d.id, "replace_with": "sheet"})));
    }
}
