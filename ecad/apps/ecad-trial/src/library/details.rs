//! The builder's Details (design 048 §3.1, §3.4, §7.2; screen g's right panel, 380 px): name,
//! key, source and version; the symbol's defaults with a note where the project overrides one;
//! its connection points and text slots as rows; what it still needs before it can be placed,
//! with a button for each; Save, Save as new, Cancel.

use ecad_ui::tokens::space;
use eframe::egui::{self, Color32, RichText};

use super::builder::{Builder, Tool};
use super::draft::{self, Missing, Picked, Source};

/// Element names (design 048 §3.1).
pub const DETAILS: &str = "library.details";
pub const NAME: &str = "library.details.name";
pub const KEY: &str = "library.details.key";
pub const PREFIX: &str = "library.details.prefix";
pub const DESCRIPTION: &str = "library.details.description";
pub const RATING: &str = "library.details.rating";
pub const JOIN: &str = "library.details.join";
pub const SOURCE: &str = "library.details.source";
pub const BANNER: &str = "library.details.banner";
pub const SAVE: &str = "library.details.save";
pub const SAVE_AS: &str = "library.details.saveas";
pub const CANCEL: &str = "library.details.cancel";
pub const DISCARD: &str = "library.details.discard";
pub const KEEP: &str = "library.details.keep";
pub const ACCEPT_ALL: &str = "library.details.offers.accept";
pub const REPLACE: &str = "library.details.replace";
pub const REFUSAL: &str = "library.details.refusal";
/// The built-in banner (design 048 §3.4).
pub const BUILTIN_BANNER: &str = "Built-in symbol. Saving makes a project copy; the built-in stays as it is.";
/// Save's reason with writes off.
pub const WRITES_OFF: &str = "Writes are off for this project.";

/// What Details needs from the project, read before it is drawn.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub missing: Vec<Missing>,
    /// The Missing list once every offer is accepted ("Accept all and save to project").
    pub missing_after_offers: Vec<Missing>,
    /// Per default (`prefix`, `description`, `rating`): the project's own value that overrides
    /// the symbol's for its placements (design 048 §7.2).
    pub overrides: Vec<(&'static str, String)>,
    /// `v3 (12 placed) · v2 (1 placed) · v1 (removed)`.
    pub versions: Option<String>,
    /// `Project · from Coil (built-in v1)`.
    pub source: String,
    pub writable: bool,
}

/// What Details asks of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Save,
    SaveAsNew,
    /// Close the builder; `true` forgets the draft.
    Close(bool),
    Standards(String),
}

fn c32(c: ecad_ui::tokens::Color) -> Color32 {
    ecad_ui::color(c)
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(space::M);
    ecad_ui::prim::read_value(ui, title);
}

/// One labelled text field; focused when the Missing list asked for it.
fn field(ui: &mut egui::Ui, b: &mut Builder, label: &str, name: &'static str, which: usize, hint: &str, read_only: bool) {
    ui.label(RichText::new(label).small().color(c32(ecad_ui::SHELL.text_2)));
    let d = &mut b.draft;
    let text = match which {
        0 => &mut d.name,
        1 => &mut d.key,
        2 => &mut d.prefix,
        3 => &mut d.description,
        _ => &mut d.rating,
    };
    let r = ui.add_enabled(!read_only, egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, !read_only, name));
    if b.focus == Some(name) {
        r.request_focus();
        b.focus = None;
    }
}

/// The checklist row `text` (screen g's "Before it can be placed"): done or to do, with a button.
fn need(ui: &mut egui::Ui, done: bool, text: &str, pill: &str, item: &str, button: Option<&str>) -> bool {
    let s = ecad_ui::SHELL;
    let mut clicked = false;
    let r = ui.horizontal(|ui| {
        // A tick or a warning triangle, drawn (the UI font has no tick).
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        let p = ui.painter();
        if done {
            let st = egui::Stroke::new(1.6, c32(s.ok));
            p.line_segment([rect.left_center() + egui::vec2(1.0, 0.0), rect.center_bottom() + egui::vec2(-1.0, -2.0)], st);
            p.line_segment([rect.center_bottom() + egui::vec2(-1.0, -2.0), rect.right_top() + egui::vec2(-1.0, 2.0)], st);
        } else {
            let st = egui::Stroke::new(1.3, c32(s.warning));
            let (t, l, r) =
                (rect.center_top() + egui::vec2(0.0, 1.0), rect.left_bottom() + egui::vec2(0.5, -1.0), rect.right_bottom() + egui::vec2(-0.5, -1.0));
            p.add(egui::Shape::closed_line(vec![t, r, l], st));
            p.line_segment([rect.center() + egui::vec2(0.0, -1.5), rect.center() + egui::vec2(0.0, 1.5)], st);
        }
        ui.label(RichText::new(text).small().color(if done { c32(s.text) } else { c32(s.warning) }));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(b) = button.filter(|_| !done) {
                let r = ui.add(ecad_ui::prim::Button::new(b).small());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("library.details.missing.{item}")));
                clicked = r.clicked();
            }
            ecad_ui::prim::pill(ui, pill, if done { ecad_ui::prim::Tone::Ok } else { ecad_ui::prim::Tone::Warning });
        });
    });
    r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("library.details.need.{item}")));
    clicked
}

/// Draw Details for the builder `b`.
pub fn show(ui: &mut egui::Ui, b: &mut Builder, info: &Info) -> Vec<Act> {
    let s = ecad_ui::SHELL;
    let mut acts = Vec::new();
    let fixed = b.draft.fixed_drawing();
    if matches!(b.draft.source, Source::Edit { builtin: true, .. }) {
        egui::Frame::new().fill(c32(s.accent_soft)).inner_margin(space::S).corner_radius(4.0).show(ui, |ui| {
            ui.label(RichText::new(BUILTIN_BANNER).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, BANNER));
        });
    }
    ui.label(RichText::new(&info.source).small().color(c32(s.text_2))).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, SOURCE));
    if let Some(v) = &info.versions {
        ui.label(RichText::new(v).small().color(c32(s.text_3)));
    }
    ui.add_space(space::S);
    field(ui, b, "Name", NAME, 0, draft::NEW_NAME, false);
    field(ui, b, "Key (what Place and agents call it)", KEY, 1, "for example float1", false);
    if let Some(m) = info.missing.iter().find(|m| m.item == "key" && !b.draft.key.trim().is_empty()) {
        ui.label(RichText::new(&m.text).small().color(c32(s.error)));
    }

    section(ui, "DEFAULTS");
    let mut join = usize::from(b.draft.part);
    let r = ui.horizontal(|ui| ecad_ui::prim::segmented(ui, &mut join, &["A device of its own", "Part of a device"])).inner;
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, JOIN));
    b.draft.part = join == 1;
    for (label, name, which, hint, key) in [
        ("Tag prefix", PREFIX, 2, "for example B", "prefix"),
        ("Description", DESCRIPTION, 3, "Float switch", "description"),
        ("Rating", RATING, 4, "10 A", "rating"),
    ] {
        field(ui, b, label, name, which, hint, false);
        if let Some((_, v)) = info.overrides.iter().find(|(k, _)| *k == key) {
            ui.horizontal(|ui| {
                ecad_ui::prim::badge(ui, format!("Project overrides this: {v}"), ecad_ui::prim::Tone::Neutral);
                if ui.small_button("Go to Standards").clicked() {
                    acts.push(Act::Standards(key.to_string()));
                }
            });
        }
    }

    section(ui, "CONNECTION POINTS");
    if b.draft.shape.points.is_empty() {
        ui.label(RichText::new("None yet. Connection point (C) adds one.").small().color(c32(s.text_3)));
    }
    let mut delete = None;
    for i in 0..b.draft.shape.points.len() {
        let Some(p) = b.draft.shape.points.get(i).cloned() else { continue };
        let name = format!("library.details.points.row.{}", p.key);
        let r = ui.horizontal(|ui| {
            let chosen = b.selected == Some(Picked::Point(i));
            if ui.selectable_label(chosen, RichText::new(&p.key).monospace()).clicked() {
                b.selected = Some(Picked::Point(i));
            }
            let mut label = p.label.clone().unwrap_or_default();
            let e = ui.add_enabled(!fixed, egui::TextEdit::singleline(&mut label).hint_text("label").desired_width(120.0));
            if e.changed() {
                b.draft.set_label(i, &label);
            }
            ui.label(RichText::new(format!("{}, {}", draft::fmt_mm(p.at.x), draft::fmt_mm(p.at.y))).small().color(c32(s.text_3)));
            if !fixed && ui.add(ecad_ui::prim::IconButton::new(ecad_ui::tokens::Icon::Delete, "Remove the point")).clicked() {
                delete = Some(Picked::Point(i));
            }
        });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Row, true, &name));
    }

    section(ui, "TEXT SLOTS");
    if b.draft.shape.slots.is_empty() {
        ui.label(RichText::new("None yet. Text slot (S) adds one.").small().color(c32(s.text_3)));
    }
    for i in 0..b.draft.shape.slots.len() {
        let Some(sl) = b.draft.shape.slots.get(i).cloned() else { continue };
        let name = format!("library.details.slots.row.{}", sl.text);
        let r = ui.horizontal(|ui| {
            let chosen = b.selected == Some(Picked::Slot(i));
            if ui.selectable_label(chosen, draft::slot_label(&sl.text)).clicked() {
                b.selected = Some(Picked::Slot(i));
            }
            let turn = if sl.rotation.abs() > 1e-9 { format!(", {}°", (sl.rotation.to_degrees()).round()) } else { String::new() };
            ui.label(RichText::new(format!("{}, {}{turn}", draft::fmt_mm(sl.at.x), draft::fmt_mm(sl.at.y))).small().color(c32(s.text_3)));
            if ui.add(ecad_ui::prim::IconButton::new(ecad_ui::tokens::Icon::Delete, "Remove the slot")).clicked() {
                delete = Some(Picked::Slot(i));
            }
        });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Row, true, &name));
    }
    if let Some(d) = delete {
        b.draft.delete(d);
        b.selected = None;
    }

    // Make symbol's offers (screen j): one decision per row, nothing applied until accepted.
    if !b.draft.shape.offers.is_empty() {
        section(ui, "OFFERED, NOT APPLIED YET");
        let mut act: Option<(usize, bool)> = None;
        let next = b.draft.next_point_key();
        for (i, o) in b.draft.shape.offers.iter().enumerate() {
            let (what, line) = match o {
                draft::Offer::Point { at } => {
                    ("Line end".to_string(), format!("Offered as connection point {next} at {}, {}", draft::fmt_mm(at.x), draft::fmt_mm(at.y)))
                }
                draft::Offer::Slot { text, .. } => (format!("The text \"{text}\""), "Offered as the tag slot; else kept as lettering".to_string()),
            };
            let r = ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(what).small().color(c32(s.connect)));
                    ui.label(RichText::new(line).small().color(c32(s.text_2)));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(ecad_ui::prim::Button::new("Skip").kind(ecad_ui::prim::Kind::Quiet).small()).clicked() {
                        act = Some((i, false));
                    }
                    if ui.add(ecad_ui::prim::Button::new("Accept").small()).clicked() {
                        act = Some((i, true));
                    }
                });
            });
            r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Row, true, format!("library.details.offers.row.{i}")));
        }
        match act {
            Some((i, true)) => {
                if let Err(e) = b.draft.accept(i) {
                    b.refusal = Some(e);
                }
            }
            Some((i, false)) => b.draft.delete(Picked::Offer(i)),
            None => {}
        }
        let r = ui.add(ecad_ui::prim::Button::new("Accept all").small());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, ACCEPT_ALL));
        if r.clicked()
            && let Err(e) = b.draft.accept_all()
        {
            b.refusal = Some(e);
        }
    }

    // Before it can be placed (screen g): the rules' Missing list as a checklist.
    section(ui, "BEFORE IT CAN BE PLACED");
    let has = |item: &str| info.missing.iter().any(|m| m.item == item);
    let points = b.draft.shape.points.len();
    let mut go: Option<&'static str> = None;
    if need(ui, !has("name") && !has("key"), "A name and a key", if has("name") || has("key") { "To do" } else { "Done" }, "name", Some("Name")) {
        go = Some(if has("name") { "name" } else { "key" });
    }
    let pts = if points == 0 { "To do".to_string() } else { format!("Done, {points} so far") };
    if need(ui, !has("point"), "At least one connection point", &pts, "point", Some("Add a point")) {
        go = Some("point");
    }
    if !b.draft.part && need(ui, !has("prefix"), "A tag prefix", if has("prefix") { "To do" } else { "Done" }, "prefix", Some("Add a prefix")) {
        go = Some("prefix");
    }
    if !b.draft.part
        && need(ui, !has("slot"), "A tag slot, so the tag has a place", if has("slot") { "To do" } else { "Done" }, "slot", Some("Add a tag slot"))
    {
        go = Some("slot");
    }
    for m in info.missing.iter().filter(|m| m.item == "other") {
        ui.label(RichText::new(&m.text).small().color(c32(s.warning)))
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "library.details.missing.other"));
    }
    ui.label(RichText::new("Optional: point labels, a description slot, a 90° turn.").small().color(c32(s.text_3)));
    match go {
        Some("name") => b.focus = Some(NAME),
        Some("key") => b.focus = Some(KEY),
        Some("prefix") => b.focus = Some(PREFIX),
        Some("point") => b.set_tool(Tool::Point),
        Some("slot") => {
            b.set_tool(Tool::Slot);
            b.slot_text = "tag".into();
        }
        _ => {}
    }

    if let Source::Make { page_number, replace, .. } = &mut b.draft.source {
        ui.add_space(space::M);
        let r = ui.checkbox(replace, format!("Replace the drawing on page {page_number} with this symbol"));
        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *replace, REPLACE));
    }
    if let Some(e) = &b.refusal {
        ui.add_space(space::S);
        ui.label(RichText::new(e).small().color(c32(s.error))).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, REFUSAL));
    }

    ui.add_space(space::M);
    if b.ask_discard {
        ui.label(format!("Discard the draft of {}?", b.draft.shown_name()));
        ui.horizontal(|ui| {
            let d = ui.add(ecad_ui::prim::Button::new("Discard").kind(ecad_ui::prim::Kind::Danger).small());
            d.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, DISCARD));
            if d.clicked() {
                acts.push(Act::Close(true));
            }
            let k = ui.add(ecad_ui::prim::Button::new("Keep").kind(ecad_ui::prim::Kind::Quiet).small());
            k.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, KEEP));
            if k.clicked() {
                b.ask_discard = false;
            }
        });
        return acts;
    }
    let offers = !b.draft.shape.offers.is_empty();
    let why: Option<String> = if !info.writable {
        Some(WRITES_OFF.into())
    } else if offers {
        info.missing_after_offers.first().map(|m| m.text.clone())
    } else {
        info.missing.first().map(|m| m.text.clone())
    };
    ui.horizontal(|ui| {
        let edit = matches!(b.draft.source, Source::Edit { .. });
        let label = if edit {
            "Save…"
        } else if offers {
            "Accept all and save to project"
        } else {
            "Save to project"
        };
        let mut save = ecad_ui::prim::Button::new(label).kind(ecad_ui::prim::Kind::Primary);
        if let Some(w) = &why {
            save = save.disabled(w);
        }
        let r = ui.add(save);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, why.is_none(), SAVE));
        if r.clicked() && why.is_none() {
            if offers && let Err(e) = b.draft.accept_all() {
                b.refusal = Some(e);
                return;
            }
            acts.push(Act::Save);
        }
        if edit {
            let mut as_new = ecad_ui::prim::Button::new("Save as new").kind(ecad_ui::prim::Kind::Quiet);
            if let Some(w) = &why {
                as_new = as_new.disabled(w);
            }
            let r = ui.add(as_new);
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, why.is_none(), SAVE_AS));
            if r.clicked() && why.is_none() {
                acts.push(Act::SaveAsNew);
            }
        }
        let c = ui.add(ecad_ui::prim::Button::new("Cancel").kind(ecad_ui::prim::Kind::Quiet));
        c.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CANCEL));
        if c.clicked() {
            if b.draft.changed() {
                b.ask_discard = true;
            } else {
                acts.push(Act::Close(true));
            }
        }
    });
    if why.is_none() {
        ui.label(RichText::new("The draft is kept while you work; Save is the only write.").small().color(c32(s.text_3)));
    }
    acts
}
