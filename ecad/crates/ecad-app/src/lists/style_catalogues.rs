//! Settings ▸ Standards' style catalogues (brief 050a, design 050 §7.2) as column sets on U3's
//! table, and the shared style detail on the right.
//!
//! One [`CatalogueSource`] serves every catalogue (Text styles, Location boxes, Frames, and 050b's
//! Wire categories): one row per style, built-in first, then the standard's own, then the
//! project's own; a column per field with the resolved value; the layer as a badge (a dot after a
//! built-in with overrides); and **Used by**, filled for the selected row only.
//!
//! **Update map.** Style records and the standard reach the change feed as the project
//! (`props_changes_since` holds `ObjectRef::Project`), like 042's defaults. Then the catalogue's
//! rows are read again (a style list, not the project) and only the rows that differ are patched;
//! a style added or deleted changes the keys (a patch of every row, never a rebuild). Any other
//! commit: nothing. A full build only on first show or another project. Used by is asked for the
//! selected row once per change count, never per frame.

use ecad_cadcraft::defaults::{self, Source};
use ecad_cadcraft::styles::{self, Catalogue, Layer};
use ecad_model::{Document, StyleRef};
use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Changed, Column, ColumnId, TableSource, Width};
use eframe::egui::{self, RichText};

use crate::lists::standards::{number_text, source_tone, source_word};

/// Accessible names (design 050 §9).
pub const DETAIL: &str = "ecad.standards.style";
pub const CONSEQUENCE: &str = "ecad.standards.style.consequence";
pub const APPLY: &str = "ecad.standards.style.apply";
pub const NEW: &str = "ecad.standards.style.new";
pub const RENAME: &str = "ecad.standards.style.rename";
pub const DELETE: &str = "ecad.standards.style.delete";
pub const REPLACE_WITH: &str = "ecad.standards.style.replace_with";
pub const PROMOTE: &str = "ecad.standards.style.promote";

const NAME: ColumnId = ColumnId(0);
const LAYER: ColumnId = ColumnId(1);
/// Field columns start here, in the catalogue's field order; Used by is the last.
const FIRST_FIELD: u16 = 2;

/// A field's value as a table shows it: a length with its unit.
pub fn value_words(r: &defaults::Resolved<'_>) -> String {
    match r.value {
        defaults::Value::Length(x) => number_text(x, "mm"),
        defaults::Value::Number(x) => number_text(x, ""),
        _ => r.text(),
    }
}

/// One style's row.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRow {
    pub id: StyleRef,
    pub name: String,
    pub layer: Layer,
    pub overridden: bool,
    /// Per field, in the catalogue's order: the value as shown and where it comes from.
    pub values: Vec<(String, Source)>,
}

fn rows_of(doc: &Document, cat: &Catalogue) -> Vec<StyleRow> {
    styles::rows(doc, cat)
        .into_iter()
        .map(|r| {
            let values = cat
                .fields
                .iter()
                .map(|f| {
                    let v = styles::field(doc, cat, &r.id, f, None);
                    // A text style's name of a style field reads better than its ID.
                    let shown = match f.kind {
                        defaults::Kind::Style(c) => StyleRef::parse(&v.text()).map_or_else(|| v.text(), |id| styles::name(doc, c, &id)),
                        _ => value_words(&v),
                    };
                    (shown, v.from)
                })
                .collect();
            StyleRow { id: r.id, name: r.name, layer: r.layer, overridden: r.overridden, values }
        })
        .collect()
}

/// A catalogue as a table source. Keys are row positions.
#[derive(Debug)]
pub struct CatalogueSource {
    pub catalogue: &'static Catalogue,
    cols: Vec<Column>,
    keys: Vec<u16>,
    pub rows: Vec<StyleRow>,
    /// Used by of one row: (row, words).
    pub used: Option<(u16, String)>,
}

impl CatalogueSource {
    /// An empty source of `catalogue`.
    pub fn new(catalogue: &'static Catalogue) -> Self {
        let mut cols = vec![Column::read(NAME.0, "Name", Width::Initial(170.0)), Column::read(LAYER.0, "Layer", Width::Initial(110.0)).groupable()];
        for (i, f) in catalogue.fields.iter().enumerate() {
            let id = FIRST_FIELD.saturating_add(u16::try_from(i).unwrap_or(u16::MAX - FIRST_FIELD));
            cols.push(Column::read(id, f.label, Width::Initial(95.0)));
        }
        cols.push(Column::read(Self::used_col(catalogue).0, "Used by", Width::Remainder));
        Self { catalogue, cols, keys: Vec::new(), rows: Vec::new(), used: None }
    }

    fn used_col(c: &Catalogue) -> ColumnId {
        ColumnId(FIRST_FIELD.saturating_add(u16::try_from(c.fields.len()).unwrap_or(0)))
    }

    /// Every row from `doc` (first show, another project).
    pub fn build(&mut self, doc: &Document) {
        self.rows = rows_of(doc, self.catalogue);
        self.keys = (0..self.rows.len()).filter_map(|i| u16::try_from(i).ok()).collect();
        self.used = None;
    }

    /// Read the rows again and return those that differ (every row when one was added or removed).
    pub fn patch(&mut self, doc: &Document) -> Changed<u16> {
        let fresh = rows_of(doc, self.catalogue);
        if fresh.len() != self.rows.len() || fresh.iter().zip(&self.rows).any(|(a, b)| a.id != b.id) {
            self.rows = fresh;
            self.keys = (0..self.rows.len()).filter_map(|i| u16::try_from(i).ok()).collect();
            self.used = None;
            return Changed::All;
        }
        let mut touched = Vec::new();
        for (i, (new, old)) in fresh.into_iter().zip(self.rows.iter_mut()).enumerate() {
            if new != *old {
                *old = new;
                if let Ok(k) = u16::try_from(i) {
                    touched.push(k);
                }
            }
        }
        if !touched.is_empty() {
            self.used = None;
        }
        if touched.is_empty() { Changed::Nothing } else { Changed::Keys(touched) }
    }

    /// The row of style `id`.
    pub fn key_of(&self, id: &StyleRef) -> Option<u16> {
        self.rows.iter().position(|r| r.id == *id).and_then(|i| u16::try_from(i).ok())
    }

    /// The row at `key`.
    pub fn row(&self, key: u16) -> Option<&StyleRow> {
        self.rows.get(usize::from(key))
    }
}

impl TableSource for CatalogueSource {
    type Key = u16;

    fn columns(&self) -> &[Column] {
        &self.cols
    }

    fn keys(&self) -> &[u16] {
        &self.keys
    }

    fn contains(&self, key: u16) -> bool {
        usize::from(key) < self.rows.len()
    }

    fn cell(&self, key: u16, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(usize::from(key)) else { return Cell::Empty };
        if col == NAME {
            return Cell::Text(r.name.clone());
        }
        if col == LAYER {
            let tone = match r.layer {
                Layer::Builtin if r.overridden => Tone::Ok,
                Layer::Builtin | Layer::Standard => Tone::Neutral,
                Layer::Project => Tone::Ok,
            };
            let dot = if r.overridden { " •" } else { "" };
            return Cell::Badge(format!("{}{dot}", r.layer.badge()), tone);
        }
        if col == Self::used_col(self.catalogue) {
            return match &self.used {
                Some((k, w)) if *k == key => Cell::Text(w.clone()),
                _ => Cell::read("—"),
            };
        }
        let i = usize::from(col.0.saturating_sub(FIRST_FIELD));
        match r.values.get(i) {
            Some((v, Source::Builtin)) => Cell::read(v.clone()),
            Some((v, _)) => Cell::Text(v.clone()),
            None => Cell::Empty,
        }
    }

    fn row_label(&self, key: u16) -> String {
        self.rows.get(usize::from(key)).map(|r| format!("{} {} ({})", self.catalogue.word, r.name, r.layer.badge())).unwrap_or_default()
    }

    fn chips(&self) -> &[&'static str] {
        &["Overridden"]
    }

    fn chip(&self, key: u16, _chip: usize) -> bool {
        self.rows.get(usize::from(key)).is_some_and(|r| r.overridden || r.values.iter().any(|(_, s)| *s != Source::Builtin))
    }
}

/// What the style detail shows of one style, read in one go.
#[derive(Debug, Clone)]
pub struct Detail {
    pub id: StyleRef,
    pub name: String,
    pub layer: Layer,
    /// Per field: label, key, the chain (Built-in, Standard, Project) and the value in use.
    pub fields: Vec<FieldDetail>,
    pub has_standard: bool,
    /// Explicit + implicit users, and the explicit users in words.
    pub users: (usize, String),
}

#[derive(Debug, Clone)]
pub struct FieldDetail {
    pub key: &'static str,
    pub label: &'static str,
    pub builtin: String,
    pub standard: Option<String>,
    pub project: Option<String>,
    pub from: Source,
    pub current: String,
}

/// Read the detail of style `id` of `cat`.
pub fn detail(doc: &Document, cat: &Catalogue, id: &StyleRef) -> Detail {
    use ecad_model::StyleLayer;
    let fields = cat
        .fields
        .iter()
        .map(|f| {
            let r = styles::field(doc, cat, id, f, None);
            let at = |l| styles::record(doc, l, cat.word, id).and_then(|rec| rec.fields.get(f.key)).cloned();
            let builtin = cat.builtin(id).and_then(|b| b.value(f.key)).unwrap_or(f.builtin);
            FieldDetail {
                key: f.key,
                label: f.label,
                builtin: value_words(&defaults::Resolved { value: builtin, from: Source::Builtin }),
                standard: at(StyleLayer::Standard),
                project: at(StyleLayer::Project),
                from: r.from,
                current: r.text(),
            }
        })
        .collect();
    let c = styles::users(doc, cat.word, id);
    let explicit = ecad_cadcraft::style_cmds::users_words(&styles::explicit_users(doc, cat.word, id));
    let layer = styles::rows(doc, cat).into_iter().find(|r| r.id == *id).map_or(Layer::Project, |r| r.layer);
    Detail {
        id: id.clone(),
        name: styles::name(doc, cat.word, id),
        layer,
        fields,
        has_standard: doc.standard.is_some(),
        users: (c.explicit + c.implicit, explicit),
    }
}

/// `31 objects use 24 V DC. Colour BU → RD.`: what a change does, said first (design 050 §7.2).
pub fn consequence(cat: &Catalogue, d: &Detail, field: &str, from: &str, to: &str) -> String {
    let noun = match cat.word {
        "text" => "texts' owners",
        "location" => "location boxes",
        "frame" => "pages",
        "wire" => "wires",
        _ => "objects",
    };
    let label = cat.field(field).map_or(field, |f| f.label);
    let who = match d.users.0 {
        1 => format!("1 of the {noun} uses {}", d.name),
        n => format!("{n} {noun} use {}", d.name),
    };
    format!("{who}. {label} {from} → {to}.")
}

/// What the detail asks the screen for.
#[derive(Debug, Clone, PartialEq)]
pub enum DetailAction {
    /// Run this command line.
    Run(String),
}

/// The detail's typing state.
#[derive(Debug, Default)]
pub struct DetailState {
    /// The field being edited and its text.
    pub field: Option<&'static str>,
    pub edit: String,
    /// Edit at the standard level (when there is a standard), else the project.
    pub at_standard: bool,
    /// New style / Rename name, Delete's replacement.
    pub name: Option<(bool, String)>,
    pub replace: Option<String>,
    pub error: Option<String>,
}

fn quote(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

/// Draw the style detail of `d` (design 050 §7.2): the consequence line first, then each field's
/// chain with its editor and Reset per level, then New style from this, Rename, Delete… and
/// Promote to standard. Returns the command lines to run.
pub fn show_detail(
    ui: &mut egui::Ui, cat: &Catalogue, d: &Detail, st: &mut DetailState, writable: bool, others: &[(String, String)],
) -> Vec<DetailAction> {
    let mut out = Vec::new();
    let id = quote(&d.id.text());
    let level = if st.at_standard && d.has_standard { "standard" } else { "project" };
    // The consequence line comes first.
    if let Some(field) = st.field
        && let Some(f) = d.fields.iter().find(|f| f.key == field)
        && st.edit.trim() != f.current
        && !st.edit.trim().is_empty()
    {
        let line = consequence(cat, d, field, &f.current, st.edit.trim());
        ui.label(RichText::new(line).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, CONSEQUENCE));
    }
    ui.label(RichText::new(&d.name).strong()).on_hover_text(d.id.text());
    ui.label(RichText::new(format!("{} · {}", cat.one, d.layer.badge())).small());
    ui.add_space(ecad_ui::tokens::space::S);
    if d.has_standard {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Change at").small());
            for (on, word) in [(false, "Project"), (true, "Standard")] {
                let name = format!("{DETAIL}.level.{}", word.to_lowercase());
                let r = ui.selectable_label(st.at_standard == on, word);
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, writable, st.at_standard == on, &name));
                if r.clicked() {
                    st.at_standard = on;
                }
            }
        });
    } else {
        ui.label(RichText::new("Standard level: make a standard to change it.").small().weak());
    }
    egui::Grid::new(("ecad.standards.style.fields", cat.word)).num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
        for f in &d.fields {
            ui.label(RichText::new(f.label).small());
            let chain = format!(
                "Built-in {}{}{}",
                f.builtin,
                f.standard.as_ref().map_or(String::new(), |v| format!(" → Standard {v}")),
                f.project.as_ref().map_or(String::new(), |v| format!(" → Project {v}"))
            );
            ui.label(RichText::new(chain).small().weak());
            let name = format!("{DETAIL}.field.{}", f.key);
            let editing = st.field == Some(f.key);
            let mut text = if editing { st.edit.clone() } else { f.current.clone() };
            let r = ui.add_enabled(writable, egui::TextEdit::singleline(&mut text).desired_width(110.0));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, writable, &name));
            if r.changed() {
                st.field = Some(f.key);
                st.edit = text;
            }
            let at_level = if level == "standard" { &f.standard } else { &f.project };
            ui.horizontal(|ui| {
                let apply = editing && st.edit.trim() != f.current;
                if ui.add_enabled(writable && apply, egui::Button::new("Apply").small()).widget_info_named(APPLY).clicked() {
                    let v = st.edit.trim();
                    let value = if v.parse::<f64>().is_ok() { v.to_string() } else { quote(v) };
                    out.push(DetailAction::Run(format!(
                        "setstyle {{\"catalogue\":\"{}\",\"id\":{id},\"layer\":\"{level}\",\"fields\":{{\"{}\":{value}}}}}",
                        cat.word, f.key
                    )));
                    st.field = None;
                }
                if at_level.is_some() {
                    let reset = format!("{DETAIL}.reset.{level}.{}", f.key);
                    let r = ui.add_enabled(writable, egui::Button::new("Reset").small());
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, &reset));
                    if r.clicked() {
                        out.push(DetailAction::Run(format!(
                            "setstyle {{\"catalogue\":\"{}\",\"id\":{id},\"layer\":\"{level}\",\"fields\":{{\"{}\":null}}}}",
                            cat.word, f.key
                        )));
                    }
                }
            });
            ui.end_row();
        }
    });
    ui.add_space(ecad_ui::tokens::space::S);
    ui.label(RichText::new(format!("Used by {} ({})", d.users.0, if d.users.1.is_empty() { "none of their own" } else { &d.users.1 })).small());
    // New style from this, Rename, Delete…, Promote.
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(writable, egui::Button::new("New style from this").small()).widget_info_named(NEW).clicked() {
            st.name = Some((true, format!("{} copy", d.name)));
        }
        let own = !d.id.is_builtin();
        if own && ui.add_enabled(writable, egui::Button::new("Rename").small()).widget_info_named(RENAME).clicked() {
            st.name = Some((false, d.name.clone()));
        }
        if own && ui.add_enabled(writable, egui::Button::new("Delete…").small()).widget_info_named(DELETE).clicked() {
            st.replace = Some(others.first().map(|o| o.0.clone()).unwrap_or_default());
        }
        if own
            && d.has_standard
            && d.layer == Layer::Project
            && ui.add_enabled(writable, egui::Button::new("Promote to standard").small()).widget_info_named(PROMOTE).clicked()
        {
            out.push(DetailAction::Run(format!("promote {{\"catalogue\":\"{}\",\"id\":{id}}}", cat.word)));
        }
    });
    if let Some((new, text)) = &mut st.name {
        let is_new = *new;
        let mut go = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(text).desired_width(160.0));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, format!("{DETAIL}.name")));
            go =
                ui.small_button(if is_new { "Add" } else { "Rename" }).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            cancel = ui.small_button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
        });
        if go {
            let n = quote(text.trim());
            out.push(DetailAction::Run(if is_new {
                format!("setstyle {{\"catalogue\":\"{}\",\"new\":true,\"name\":{n},\"copy_of\":{id},\"layer\":\"{level}\"}}", cat.word)
            } else {
                format!("setstyle {{\"catalogue\":\"{}\",\"id\":{id},\"rename\":{n}}}", cat.word)
            }));
        }
        if go || cancel {
            st.name = None;
        }
    }
    if let Some(pick) = &mut st.replace {
        let users = d.users.1.clone();
        ui.label(
            RichText::new(if users.is_empty() { format!("Delete {}?", d.name) } else { format!("{users} use {}. Replace with:", d.name) }).small(),
        );
        let mut go = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            let shown = others.iter().find(|o| o.0 == *pick).map_or(pick.clone(), |o| o.1.clone());
            let combo = egui::ComboBox::from_id_salt(REPLACE_WITH).selected_text(shown).show_ui(ui, |ui| {
                for (oid, oname) in others {
                    if ui.selectable_label(*pick == *oid, oname).clicked() {
                        *pick = oid.clone();
                    }
                }
            });
            combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, REPLACE_WITH));
            go = ui.small_button("Delete").clicked();
            cancel = ui.small_button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
        });
        if go {
            let with = if users.is_empty() || pick.is_empty() { String::new() } else { format!(",\"replace_with\":{}", quote(pick)) };
            out.push(DetailAction::Run(format!("deletesetstyle {{\"catalogue\":\"{}\",\"id\":{id}{with}}}", cat.word)));
        }
        if go || cancel {
            st.replace = None;
        }
    }
    if let Some(e) = &st.error {
        ui.label(RichText::new(e).small().color(ui.visuals().error_fg_color));
    }
    out
}

/// The tone of a field's source on a badge.
pub fn tone(s: Source) -> Tone {
    source_tone(s)
}

/// The words of a field's source.
pub fn word(s: Source) -> &'static str {
    source_word(s)
}

/// `widget_info` with a label, chained.
trait Named {
    fn widget_info_named(self, name: &str) -> Self;
}

impl Named for egui::Response {
    fn widget_info_named(self, name: &str) -> Self {
        self.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, self.enabled(), name));
        self
    }
}
