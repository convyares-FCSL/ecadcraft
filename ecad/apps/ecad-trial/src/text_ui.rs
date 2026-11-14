//! Brief 031 (design 031 §4.1): the Text section of the Project settings popout. A copy of
//! `textstyle {}` taken when the popout opens; OK writes every changed field as one `textstyle`
//! call (one transaction, "Text styles"); Cancel or Esc writes nothing.

use eframe::egui::{self, RichText};
use serde_json::{Map, Value, json};

use ecad_cadcraft::text_style::TextKind;

/// The section's element name.
pub const SECTION_ID: &str = "ecad.projectsettings.text";
/// The height refusal under a field (design 031 §4.1).
pub const BAD_HEIGHT: &str = ecad_cadcraft::text_cmds::BAD_HEIGHT;

/// One row: a kind's fields as shown and as they were.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub kind: &'static str,
    pub label: &'static str,
    /// The kind's own font; empty: the project font (owner, 2026-11-12).
    pub font: String,
    font_orig: String,
    pub height: String,
    height_orig: String,
    builtin_height: String,
    pub bold: bool,
    bold_orig: bool,
    builtin_bold: bool,
    /// `on`, `off` or `first`; `None` for kinds whose visibility this brief does not set.
    pub show: Option<String>,
    show_orig: Option<String>,
    builtin_show: Option<String>,
    /// Lane W's and P's kinds: Shown is fixed here.
    pub show_fixed: bool,
}

/// The Text section's state.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSection {
    pub font: String,
    font_orig: String,
    /// `drawn with …` or why another font is used.
    pub status: String,
    status_for: String,
    /// The font is not drawn as named (the status line is a warning).
    pub warn: bool,
    pub rows: Vec<Row>,
    /// Families offered by the drop-down: those the build and the machine can find.
    pub families: Vec<String>,
}

/// The words of a kind's row.
fn label_of(k: TextKind) -> &'static str {
    match k {
        TextKind::Tag => "Tag",
        TextKind::PointLabel => "Connection point",
        TextKind::Description => "Description",
        TextKind::PartNumber => "Part number",
        TextKind::ContinuationName => "Continuation name",
        TextKind::Reference => "Reference",
        TextKind::WireLabel => "Wire label",
        TextKind::CableText => "Cable text",
        TextKind::LocationLabel => "Location label",
        TextKind::TitleValue => "Title block value",
        TextKind::TitleCaption => "Title block caption",
        TextKind::TitleMain => "Title",
        TextKind::Grid => "Grid label",
        TextKind::Table => "Table text",
        TextKind::TableHeader => "Table header",
        TextKind::Free => "Free text",
        TextKind::TitlePage => "Page number box",
        TextKind::TitleRev => "Revision box",
        TextKind::TableHeading => "Table heading",
        TextKind::Cover => "Cover title",
        TextKind::ChannelText => "PLC channel text",
        TextKind::Address => "PLC address",
    }
}

/// The visibility words a Shown drop-down shows.
fn show_words(w: &str) -> &'static str {
    match w {
        "off" => "Hidden",
        "first" => "Once per run",
        _ => "Shown",
    }
}

/// A number as a field shows it.
fn number(v: &Value) -> String {
    v.as_f64().map(|x| format!("{x}")).unwrap_or_default()
}

/// Whether a height field's text is a valid height.
fn valid_height(t: &str) -> Option<f64> {
    t.trim().parse::<f64>().ok().filter(|h| h.is_finite() && (0.5..=10.0).contains(h))
}

impl TextSection {
    /// The section filled from `textstyle {}`'s result.
    pub fn from_json(v: &Value) -> Self {
        let mut rows = Vec::new();
        for k in v.get("kinds").and_then(Value::as_array).into_iter().flatten() {
            let Some(kind) = k.get("kind").and_then(Value::as_str).and_then(TextKind::from_word) else { continue };
            if kind == TextKind::Free {
                continue;
            }
            let show = k.get("show").and_then(Value::as_str).map(str::to_string);
            let builtin = &k["builtin"];
            // Lane W's wire labels and lane P's kinds keep their visibility rules (design 031 §4.1).
            let show_fixed = kind == TextKind::WireLabel || show.is_none();
            let font = k.get("font").and_then(Value::as_str).unwrap_or_default().to_string();
            rows.push(Row {
                kind: kind.word(),
                label: label_of(kind),
                font_orig: font.clone(),
                font,
                height: number(&k["height"]),
                height_orig: number(&k["height"]),
                builtin_height: number(&builtin["height"]),
                bold: k["weight"] == "bold",
                bold_orig: k["weight"] == "bold",
                builtin_bold: builtin["weight"] == "bold",
                show_orig: show.clone(),
                show,
                builtin_show: builtin["show"].as_str().map(str::to_string),
                show_fixed,
            });
        }
        let font = v.get("font").and_then(Value::as_str).unwrap_or_default().to_string();
        let mut families: Vec<String> = cadcraft_engine::fonts::ttf::installed().into_iter().filter_map(|f| f.family).collect();
        families.sort();
        families.dedup();
        families.truncate(MAX_FAMILIES);
        let warn = v.get("drawn_with").and_then(Value::as_str) != Some(font.as_str());
        TextSection {
            font_orig: font.clone(),
            status: v.get("status").and_then(Value::as_str).unwrap_or_default().to_string(),
            status_for: font.clone(),
            font,
            warn,
            rows,
            families,
        }
    }

    /// Why OK cannot write, if a field is invalid.
    pub fn error(&self) -> Option<&'static str> {
        if self.rows.iter().any(|r| valid_height(&r.height).is_none()) {
            return Some(BAD_HEIGHT);
        }
        if self.rows.iter().any(|r| !r.font.trim().is_empty() && ecad_cadcraft::defaults::text::FONT.parse(&r.font).is_none()) {
            return Some(ecad_cadcraft::text_cmds::BAD_FONT);
        }
        if ecad_cadcraft::defaults::text::FONT.parse(&self.font).is_none() {
            return Some(ecad_cadcraft::text_cmds::BAD_FONT);
        }
        None
    }

    /// Every row back to the built-in defaults (fields only; nothing is written until OK).
    pub fn reset(&mut self) {
        for r in &mut self.rows {
            r.height.clone_from(&r.builtin_height);
            r.bold = r.builtin_bold;
            r.font.clear();
            if !r.show_fixed {
                r.show.clone_from(&r.builtin_show);
            }
        }
    }

    /// The `textstyle` line writing what changed; `None` when nothing did. A field set back to its
    /// default is written as `null` (the project keeps no key for it).
    pub fn line(&self) -> Result<Option<String>, String> {
        if let Some(e) = self.error() {
            return Err(e.to_string());
        }
        let mut m = Map::new();
        if self.font.trim() != self.font_orig {
            m.insert("font".into(), json!(self.font.trim()));
        }
        let mut kinds = Vec::new();
        for r in &self.rows {
            let mut k = Map::new();
            if r.font.trim() != r.font_orig {
                k.insert("font".into(), if r.font.trim().is_empty() { Value::Null } else { json!(r.font.trim()) });
            }
            if r.height.trim() != r.height_orig {
                let h = valid_height(&r.height).ok_or(BAD_HEIGHT)?;
                k.insert("height".into(), if r.height.trim() == r.builtin_height { Value::Null } else { json!(h) });
            }
            if r.bold != r.bold_orig {
                k.insert("weight".into(), if r.bold == r.builtin_bold { Value::Null } else { json!(if r.bold { "bold" } else { "regular" }) });
            }
            if !r.show_fixed && r.show != r.show_orig {
                k.insert("show".into(), if r.show == r.builtin_show { Value::Null } else { json!(r.show) });
            }
            if !k.is_empty() {
                k.insert("kind".into(), json!(r.kind));
                kinds.push(Value::Object(k));
            }
        }
        if !kinds.is_empty() {
            m.insert("kinds".into(), Value::Array(kinds));
        }
        Ok((!m.is_empty()).then(|| format!("textstyle {}", Value::Object(m))))
    }

    /// Keep the status line in step with the typed font name.
    fn refresh_status(&mut self) {
        if self.status_for == self.font {
            return;
        }
        let mut doc = ecad_model::Document::default();
        doc.properties.insert(ecad_cadcraft::defaults::text::FONT.key.into(), self.font.trim().to_string());
        let f = ecad_cadcraft::text_style::Fonts::resolve(&doc);
        self.status = f.status();
        self.warn = !f.is_wanted();
        self.status_for.clone_from(&self.font);
    }

    /// Draw the section.
    pub fn show(&mut self, ui: &mut egui::Ui, read_only: bool, warn: egui::Color32, error: egui::Color32) {
        self.refresh_status();
        ui.label(RichText::new("Text").small().strong()).on_hover_text(SECTION_ID);
        ui.add_enabled_ui(!read_only, |ui| {
            ui.horizontal(|ui| {
                ui.label("Font");
                ui.add(egui::TextEdit::singleline(&mut self.font).desired_width(200.0)).accessible_name("ecad.projectsettings.text.font");
                ui.menu_button("v", |ui| {
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for f in &self.families {
                            if ui.button(f).clicked() {
                                self.font.clone_from(f);
                                ui.close();
                            }
                        }
                    });
                });
            });
        });
        let status = RichText::new(&self.status).small();
        ui.label(if self.warn { status.color(warn) } else { status }).on_hover_text("ecad.projectsettings.text.font.status");
        ui.add_enabled_ui(!read_only, |ui| {
            egui::Grid::new(SECTION_ID).num_columns(5).show(ui, |ui| {
                ui.label(RichText::new("Kind").small());
                ui.label(RichText::new("Font").small());
                ui.label(RichText::new("Height").small());
                ui.label(RichText::new("Bold").small());
                ui.label(RichText::new("Shown").small());
                ui.end_row();
                for r in &mut self.rows {
                    ui.label(r.label);
                    ui.add(egui::TextEdit::singleline(&mut r.font).hint_text("project font").desired_width(110.0))
                        .accessible_name(format!("ecad.projectsettings.text.{}.font", r.kind));
                    ui.add(egui::TextEdit::singleline(&mut r.height).desired_width(36.0))
                        .accessible_name(format!("ecad.projectsettings.text.{}.height", r.kind));
                    ui.checkbox(&mut r.bold, "").accessible_name(format!("ecad.projectsettings.text.{}.bold", r.kind));
                    let current = r.show.clone().unwrap_or_else(|| "on".into());
                    ui.add_enabled_ui(!r.show_fixed, |ui| {
                        egui::ComboBox::from_id_salt(format!("ecad.projectsettings.text.{}.show", r.kind))
                            .selected_text(show_words(&current))
                            .show_ui(ui, |ui| {
                                let words: &[&str] = if r.kind == "tag" { &["on", "off", "first"] } else { &["on", "off"] };
                                for w in words {
                                    if ui.selectable_label(current == *w, show_words(w)).clicked() {
                                        r.show = Some((*w).to_string());
                                    }
                                }
                            });
                    });
                    ui.end_row();
                }
            });
            if ui.button("Defaults").accessible_name("ecad.projectsettings.text.defaults").clicked() {
                self.reset();
            }
        });
        if let Some(e) = self.error() {
            ui.label(RichText::new(e).small().color(error));
        }
    }
}

/// Most font families the drop-down lists (a machine may have thousands).
const MAX_FAMILIES: usize = 400;

#[cfg(test)]
mod tests {
    use super::*;

    fn read() -> TextSection {
        let doc = ecad_model::Document::default();
        TextSection::from_json(&ecad_cadcraft::text_cmds::textstyle_json(&doc, false))
    }

    #[test]
    fn nothing_changed_writes_nothing_and_a_change_is_one_line() {
        let mut t = read();
        assert_eq!(t.line(), Ok(None));
        assert!(t.rows.iter().all(|r| r.kind != "free"));
        let tag = t.rows.iter_mut().find(|r| r.kind == "tag").unwrap();
        tag.height = "3".into();
        tag.bold = false;
        let pl = t.rows.iter_mut().find(|r| r.kind == "point_label").unwrap();
        pl.show = Some("off".into());
        let line = t.line().unwrap().unwrap();
        let v: Value = serde_json::from_str(line.strip_prefix("textstyle ").unwrap()).unwrap();
        assert_eq!(v, json!({"kinds": [{"kind": "tag", "height": 3.0, "weight": "regular"}, {"kind": "point_label", "show": "off"}]}));
        // A kind's own font, then back to the project font.
        let tag = t.rows.iter_mut().find(|r| r.kind == "tag").unwrap();
        tag.font = "Arial".into();
        let v: Value = serde_json::from_str(t.line().unwrap().unwrap().strip_prefix("textstyle ").unwrap()).unwrap();
        assert_eq!(v["kinds"][0]["font"], json!("Arial"), "{v}");
        // Defaults puts every field back: nothing to write.
        t.reset();
        assert_eq!(t.line(), Ok(None));
    }

    #[test]
    fn an_out_of_range_height_blocks_ok() {
        let mut t = read();
        t.rows[0].height = "11".into();
        assert_eq!(t.error(), Some(BAD_HEIGHT));
        assert!(t.line().is_err());
    }

    #[test]
    fn lane_kinds_keep_their_shown_rule() {
        let t = read();
        for k in ["wire_label", "grid", "title_value", "table"] {
            assert!(t.rows.iter().find(|r| r.kind == k).unwrap().show_fixed, "{k}");
        }
        assert!(!t.rows.iter().find(|r| r.kind == "tag").unwrap().show_fixed);
    }
}
