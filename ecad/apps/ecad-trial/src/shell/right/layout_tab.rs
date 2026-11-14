//! The Layout tab (UI rev 3, board 1.06): how this symbol is drawn. Placement (position, rotation
//! and mirror read from the drawing; variant and lock not available yet), Text on the sheet (per
//! slot: shown, where, and its size from the standard, with Reset to standard), the contact table
//! under a coil, and Line and colour (folded). For anything else it shows the defaults that apply
//! to what is selected (brief 042), folded the same way.

use ecad_cadcraft::default_cmds;
use ecad_cadcraft::defaults::Source;
use ecad_cadcraft::text_style::TextKind;
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Sense, pos2, vec2};

use super::widgets as w;
use super::{Cx, InspectorAction, fields};

/// Accessible names.
pub const ROTATION: &str = "ecad.inspector.layout.rotation";
pub const MIRROR: &str = "ecad.inspector.layout.mirror";
pub const CONTACT_TABLE: &str = "ecad.inspector.layout.contacts";
pub const CONTACT_ROWS: &str = "ecad.inspector.layout.contact-rows";
pub const RESET_TEXTS: &str = "Reset to standard";

/// A slot's name on the sheet.
fn slot_words(k: TextKind) -> &'static str {
    match k {
        TextKind::Tag => "Tag",
        TextKind::PointLabel => "Pin labels",
        TextKind::Description => "Description",
        TextKind::PartNumber => "Part number",
        _ => "Text",
    }
}

pub fn show(ui: &mut egui::Ui, cx: &mut Cx<'_>, out: &mut Vec<InspectorAction>) {
    let full = cx.full;
    let Some(d) = cx.insp.device.clone() else {
        // Not one device: the defaults for what is selected (the page with nothing selected).
        let rows = cx.insp.rows.len();
        let summary = if rows == 0 { "none".to_string() } else { format!("{rows}") };
        let mut folded = std::mem::take(&mut cx.insp.folded);
        let insp = &mut *cx.insp;
        w::section(ui, &mut folded, full, "defaults", "How it is drawn", &summary, false, |ui| {
            if rows == 0 {
                ui.label(w::rich(ui, "Nothing here is drawn from defaults.", ui_type::SMALL, w::text_3()));
            }
            insp.show_rows(ui, cx.writable, out, |_| true);
        });
        cx.insp.folded = folded;
        return;
    };
    let view = std::mem::take(&mut cx.sh.view);
    let section = view.sections.first().filter(|s| s.kind == "device");
    let mut folded = std::mem::take(&mut cx.insp.folded);
    // Placement.
    w::section(ui, &mut folded, full, "placement", "Placement", &d.place, false, |ui| {
        let why = "Read from the drawing: move, rotate or mirror it on the sheet";
        match d.placed {
            Some(p) => {
                w::read(ui, "Position", &format!("X {:.1}    Y {:.1}   mm", p.x, p.y), "ecad.inspector.layout.position").on_hover_text(why);
                let rot = [0.0, 90.0, 180.0, 270.0].iter().position(|a| (p.rotation - a).abs() < 0.01);
                w::row(ui, "Rotation", |ui| w::segmented(ui, &["0\u{b0}", "90\u{b0}", "180\u{b0}", "270\u{b0}"], rot, false, ROTATION, why));
                w::row(ui, "Mirror", |ui| w::segmented(ui, &["None", "Across", "Up\u{2013}down"], Some(usize::from(p.mirror)), false, MIRROR, why));
            }
            None => {
                w::read(ui, "Position", "not a placed symbol", "ecad.inspector.layout.position");
            }
        }
        w::read(ui, "Variant", &d.symbol_name, "ecad.inspector.layout.variant").on_hover_text("Other variants: not available yet");
        w::row(ui, "Lock", |ui| {
            let (rect, r) = ui.allocate_exact_size(vec2(30.0, 16.0), Sense::hover());
            ui.painter().rect_filled(rect, ecad_ui::style::corner(ecad_ui::tokens::radius::pill(rect.height())), w::raised());
            ui.painter().circle_filled(pos2(rect.left() + 8.0, rect.center().y), 5.5, w::text_3());
            r.on_hover_text(super::device::NOT_YET);
            ui.label(w::rich(ui, "can be moved and joined", ui_type::SMALL, w::text_2()));
        });
    });
    // Text on the sheet.
    let insp = &mut *cx.insp;
    let text_rows: Vec<(TextKind, Option<usize>, Option<usize>)> = super::SYMBOL_KINDS
        .iter()
        .map(|k| {
            let style = insp.rows.iter().position(|r| r.key == k.style_entry().key);
            let show = k.show_entry().and_then(|e| insp.rows.iter().position(|r| r.key == e.key));
            (*k, style, show)
        })
        .collect();
    let here = text_rows.iter().flat_map(|(_, a, b)| [*a, *b]).flatten().filter(|i| insp.rows.get(*i).is_some_and(|r| r.overridden_here())).count();
    let summary = if here == 0 { format!("from {}", d.standard) } else { format!("{here} set here") };
    w::section(ui, &mut folded, full, "texts", "Text on the sheet", &summary, false, |ui| {
        let cols = [86.0, 30.0, 0.0, 70.0];
        ui.horizontal(|ui| {
            for (i, h) in ["Slot", "On", "Where", "Size"].iter().enumerate() {
                let wd = if cols[i] == 0.0 { (ui.available_width() - cols[3] - 4.0).max(40.0) } else { cols[i] };
                let (r, _) = ui.allocate_exact_size(vec2(wd, 18.0), Sense::hover());
                ui.painter().text(pos2(r.left(), r.center().y), egui::Align2::LEFT_CENTER, *h, w::font(ui, ui_type::SMALL), w::text_3());
            }
        });
        for (kind, style, show) in &text_rows {
            let size = d.sizes.iter().find(|(k, _)| k == kind).map(|(_, s)| s.clone()).unwrap_or_default();
            insp.text_slot(ui, *kind, slot_words(*kind), *style, *show, &size, cx.writable, out);
        }
        // The device-wide shown setting of each text (brief 031), for all its symbols.
        if let Some(s) = section {
            let device_texts: Vec<_> = s.rows.iter().filter(|r| r.label.starts_with("Texts:")).collect();
            if !device_texts.is_empty() {
                ui.add_space(space::S);
                let set = device_texts.iter().filter(|r| !r.inherited).count();
                let summary = if set == 0 { "default".to_string() } else { format!("{set} set") };
                let id = egui::Id::unique("ecad.inspector.texts-all");
                let mut inner: std::collections::BTreeSet<String> = ui.data(|m| m.get_temp(id)).unwrap_or_default();
                w::section(ui, &mut inner, full, "texts-all", &format!("Every symbol of {}", d.tag), &summary, true, |ui| {
                    for row in device_texts {
                        fields::prop(ui, cx.sh, &s.target, s.kind, row, &super::device::text_label(&row.label), "");
                    }
                });
                ui.data_mut(|m| m.insert_temp(id, inner));
            }
        }
        ui.add_space(space::S);
        ui.horizontal(|ui| {
            let why = if here == 0 { Some("Every text already follows the standard") } else { None };
            if w::button(ui, RESET_TEXTS, false, why).clicked() {
                for i in text_rows.iter().flat_map(|(_, a, b)| [*a, *b]).flatten() {
                    if let Some(r) = insp.rows.get(i)
                        && r.overridden_here()
                        && let Some((level, target)) = r.here
                    {
                        out.push(InspectorAction::Run(r.key, default_cmds::line(r.key, level, target, None)));
                    }
                }
            }
        });
    });
    // The contact table under a coil.
    if let Some((s, row)) = section.and_then(|s| s.rows.iter().find(|r| r.key == ecad_cadcraft::xref::MIRROR_KEY).map(|r| (s, r))) {
        let v = row.value.to_lowercase();
        let selected = if v.starts_with("below") {
            Some(0)
        } else if v.starts_with("right") {
            Some(1)
        } else if v.starts_with("off") {
            Some(2)
        } else {
            None
        };
        let summary = match selected {
            Some(0) => "under the coil".to_string(),
            Some(1) => "right of the coil".to_string(),
            Some(2) => "hidden".to_string(),
            _ => row.value.to_lowercase(),
        };
        let writable = cx.sh.writable && row.editable;
        w::section(ui, &mut folded, full, "contacts", "Contact table", &summary, false, |ui| {
            let pick = w::row(ui, "Show", |ui| w::segmented(ui, &["Below", "Right", "Hidden"], selected, writable, CONTACT_TABLE, ""));
            if let (Some(i), Some(t)) = (pick, &s.target) {
                let word = ["Below", "Right", "Off"].get(i).copied().unwrap_or("Below");
                cx.sh.edits.push((t.clone(), row.key.clone(), Some(word.to_string())));
            }
            if selected.is_none() {
                w::row(ui, "", |ui| ui.label(w::rich(ui, &row.value, ui_type::SMALL, w::text_3())));
            } else if writable
                && let Some(t) = &s.target
                && w::row(ui, "", |ui| {
                    ui.add(egui::Label::new(w::rich(ui, "Use the project default", ui_type::SMALL, w::text_2())).sense(Sense::click())).clicked()
                })
            {
                cx.sh.edits.push((t.clone(), row.key.clone(), None));
            }
            w::row(ui, "Rows", |ui| w::segmented(ui, &["Used only", "All contacts"], Some(1), false, CONTACT_ROWS, super::device::NOT_YET));
            ui.add(
                egui::Label::new(w::rich(
                    ui,
                    "Lists each contact with its type and where it is drawn, for example 13\u{2013}14 NO 2/B5.",
                    ui_type::SMALL,
                    w::text_3(),
                ))
                .wrap(),
            );
            if let Some(e) = cx.sh.errors.get(&row.key) {
                ui.label(w::rich(ui, e, ui_type::SMALL, ecad_ui::color(ecad_ui::SHELL.error)));
            }
        });
    }
    // Line and colour: from the symbol (folded).
    w::section(ui, &mut folded, full, "line", "Line and colour", "from the symbol", true, |ui| {
        ui.add(egui::Label::new(w::rich(ui, "Drawn in the symbol's own line and colour. Nothing is set here.", ui_type::SMALL, w::text_3())).wrap());
    });
    cx.insp.folded = folded;
    cx.sh.view = view;
}

impl super::Inspector {
    /// One row of Text on the sheet: the slot (a click opens Standards at its style), whether it
    /// shows, where (from the symbol), its size (a list of the styles), and Reset when set here.
    #[allow(clippy::too_many_arguments)]
    fn text_slot(
        &mut self, ui: &mut egui::Ui, kind: TextKind, words: &str, style: Option<usize>, show: Option<usize>, size: &str, writable: bool,
        out: &mut Vec<InspectorAction>,
    ) {
        let style_row = style.and_then(|i| self.rows.get(i)).cloned();
        let show_row = show.and_then(|i| self.rows.get(i)).cloned();
        ui.horizontal(|ui| {
            ui.set_min_height(w::FIELD_H + 2.0);
            // Slot: its name; a click opens Standards where its style is set.
            let (r, resp) = ui.allocate_exact_size(vec2(86.0, w::FIELD_H), Sense::click());
            ui.painter().text(pos2(r.left(), r.center().y), egui::Align2::LEFT_CENTER, words, w::font(ui, ui_type::BODY), w::text());
            if let Some(s) = &style_row {
                let name = format!("ecad.origin.{}", s.key);
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
                let from = crate::lists::standards::source_word(s.from);
                if resp.on_hover_text(format!("Style from {from}: open Standards")).clicked() {
                    out.push(InspectorAction::Standards(s.key));
                }
            }
            // On: the shown setting at this symbol.
            let (r, resp) = ui.allocate_exact_size(vec2(30.0, w::FIELD_H), Sense::click());
            let on = show_row.as_ref().is_none_or(|s| s.value != "Hidden");
            let can = writable && show_row.as_ref().is_some_and(|s| s.here.is_some());
            let b = egui::Rect::from_center_size(pos2(r.left() + 9.0, r.center().y), vec2(16.0, 16.0));
            let shell = ecad_ui::SHELL;
            if on {
                ui.painter().rect_filled(
                    b,
                    ecad_ui::style::corner(ecad_ui::tokens::radius::CHECK),
                    ecad_ui::color(if can { shell.accent } else { shell.text_3 }),
                );
                let c = egui::Color32::WHITE;
                ui.painter().line_segment([b.center() + vec2(-4.0, 0.0), b.center() + vec2(-1.0, 3.0)], egui::Stroke::new(1.6, c));
                ui.painter().line_segment([b.center() + vec2(-1.0, 3.0), b.center() + vec2(4.0, -3.0)], egui::Stroke::new(1.6, c));
            } else {
                ui.painter().rect_stroke(
                    b,
                    ecad_ui::style::corner(ecad_ui::tokens::radius::CHECK),
                    egui::Stroke::new(1.2, w::text_2()),
                    egui::StrokeKind::Inside,
                );
            }
            if let Some(s) = &show_row {
                let name = format!("ecad.inspector.{}", s.key);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, can, on, &name));
                let resp = resp.on_hover_text(format!("{}: {}", s.label, s.value));
                if can
                    && resp.clicked()
                    && let Some((level, target)) = s.here
                {
                    out.push(InspectorAction::Run(s.key, default_cmds::line(s.key, level, target, Some(if on { "off" } else { "on" }))));
                }
            }
            // Where: as the symbol places it (grey: read from the drawing).
            let where_w = (ui.available_width() - 70.0 - 4.0).max(40.0);
            let (r, _) = ui.allocate_exact_size(vec2(where_w, w::FIELD_H), Sense::hover());
            let words = if !on {
                "Hidden"
            } else if kind == TextKind::PointLabel {
                "At pins"
            } else {
                "As drawn"
            };
            ui.painter().rect_filled(r.shrink2(vec2(2.0, 0.0)), ecad_ui::style::corner(ecad_ui::tokens::radius::SEGMENT), w::raised());
            ui.painter().text(
                pos2(r.left() + 10.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                w::ellipsis(words, 14),
                w::font(ui, ui_type::BODY),
                if on { w::text_2() } else { w::text_3() },
            );
            // Size: from the style; set here, a list of the catalogue's styles.
            if let Some(s) = &style_row {
                let set = s.from != Source::Builtin && s.from != Source::Standard;
                let colour = if set { w::text() } else { w::text_2() };
                match s.here.filter(|_| writable && !s.styles.is_empty()) {
                    Some((level, target)) => {
                        let name = format!("ecad.inspector.style.{}", s.key);
                        let mut pick = None;
                        let combo = egui::ComboBox::from_id_salt(&name).selected_text(w::rich(ui, size, ui_type::BODY, colour)).width(52.0).show_ui(
                            ui,
                            |ui| {
                                for (id, n) in &s.styles {
                                    if ui.selectable_label(*n == s.value, n).clicked() {
                                        pick = Some(id.clone());
                                    }
                                }
                            },
                        );
                        combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &name));
                        combo.response.on_hover_text(format!("Style: {}", s.value));
                        if let Some(id) = pick {
                            out.push(InspectorAction::Run(s.key, default_cmds::line(s.key, level, target, Some(&id))));
                        }
                    }
                    None => {
                        ui.label(w::rich(ui, size, ui_type::BODY, colour)).on_hover_text(format!("Style: {}", s.value));
                    }
                }
            } else {
                ui.label(w::rich(ui, size, ui_type::BODY, w::text_2()));
            }
        });
        // Set here: Reset beside it, and any refusal under it.
        for r in [&style_row, &show_row].into_iter().flatten() {
            if writable
                && r.overridden_here()
                && let Some((level, target)) = r.here
            {
                ui.horizontal(|ui| {
                    ui.add_space(86.0);
                    let words = if r.key.ends_with(".show") { "Reset shown" } else { "Reset size" };
                    let b = ui.small_button(words);
                    b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("ecad.inspector.reset.{}", r.key)));
                    if b.clicked() {
                        out.push(InspectorAction::Run(r.key, default_cmds::line(r.key, level, target, None)));
                    }
                });
            }
            if let Some(d) = &r.dangling {
                ui.label(w::rich(ui, d, ui_type::SMALL, ecad_ui::color(ecad_ui::SHELL.warning)));
            }
            if let Some(e) = self.errors.get(r.key) {
                ui.label(w::rich(ui, e, ui_type::SMALL, ecad_ui::color(ecad_ui::SHELL.error)));
            }
        }
    }
}
