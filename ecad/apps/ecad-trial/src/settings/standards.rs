//! Settings ▸ Standards (brief 042 step 3, screen e; roadmap 1.13(b)): one table of every default
//! with its value and an origin badge ([`crate::lists::standards`]), filtered by group or by
//! "Overridden", and on the right the chosen default's chain, built-in → standard → project →
//! page → object, with the level in use marked. A level is listed only when it holds a value or
//! the entry allows it (push-back 8). The project's value is changed or reset here through
//! `setdefault`; before applying, the detail says how many objects use the value and what
//! changes for them (computed by `defaults::users`, not by applying). Page and object values are
//! set in the inspector.
//!
//! Brief 050a (design 050 §7): the standard line is the **standard header** (make, rename, copy
//! from another project, back to IEC, each saying its consequence first, inline, with no dialog),
//! and the catalogue strip reads one list, [`CATALOGUES`]: Defaults, then one table per style
//! catalogue ([`crate::lists::style_catalogues`]) with the shared style detail on the right. A
//! later catalogue (050b's wire categories) adds one line there.

use ecad_cadcraft::default_cmds::{self, Target};
use ecad_cadcraft::defaults::{self, Entry, Level, Scope, Source};
use ecad_ui::table::{Changed, Table, TableEvent};
use eframe::egui::{self, RichText};

use crate::Trial;
use crate::lists::standards::{DefaultsSource, source_tone, source_word, value_text};
use crate::lists::style_catalogues::{self as sc, CatalogueSource, DetailState};

/// Accessible names.
pub const STANDARD_LINE: &str = "ecad.standards.standard";
pub const CATALOGUE_DEFAULTS: &str = "ecad.standards.defaults";
pub const CATALOGUE_STYLES: &str = "ecad.standards.text_styles";
pub const VALUE: &str = "ecad.standards.value";
pub const APPLY: &str = "ecad.standards.apply";
pub const RESET: &str = "ecad.standards.reset";
pub const CONSEQUENCE: &str = "ecad.standards.consequence";
pub const ERROR: &str = "ecad.standards.error";
pub const IN_USE: &str = "In use";
/// Brief 050a (design 050 §9): the standard header and its parts.
pub const HEADER: &str = "ecad.standards.header";
pub const MAKE: &str = "ecad.standards.header.make";
pub const RENAME: &str = "ecad.standards.header.rename";
pub const COPY_FROM: &str = "ecad.standards.header.copy_from";
pub const BACK_TO_IEC: &str = "ecad.standards.header.back_to_iec";
pub const HEADER_NAME: &str = "ecad.standards.header.name";
pub const HEADER_CONSEQUENCE: &str = "ecad.standards.header.consequence";
pub const HEADER_CONFIRM: &str = "ecad.standards.header.confirm";

/// The style catalogues on the strip after Defaults, one line each (design 050 §7.2): the
/// catalogue word, the tab's accessible name and the table's id. 042's
/// `ecad.standards.text_styles` names the text styles' tab until the phase-end tidy-up.
#[rustfmt::skip]
pub const CATALOGUES: &[(&str, &str, &str)] = &[
    ("text", CATALOGUE_STYLES, "standards.text"),
    ("location", "ecad.standards.catalogue.location", "standards.location"),
    ("frame", "ecad.standards.catalogue.frame", "standards.frame"),
    ("wire", "ecad.standards.catalogue.wire", "standards.wire"),
];

/// What the standard header is doing (design 050 §7.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum HeaderMode {
    #[default]
    Idle,
    /// Make a standard…: its name.
    Make(String),
    Rename(String),
    /// Copy from project…: the file's path, and once read, what it holds (name, values, styles).
    Copy(String, Option<(String, usize, usize)>),
    /// Back to IEC: its consequence, then the confirm.
    Back,
}

/// The screen's state: both tables, the row shown in the detail and the value being typed.
#[derive(Debug)]
pub struct StandardsView {
    pub defaults: DefaultsSource,
    pub table: Table<u16>,
    /// Brief 050a: one table per style catalogue, in [`CATALOGUES`] order.
    pub cats: Vec<(CatalogueSource, Table<u16>)>,
    /// 0: Defaults, then 1 + the index into [`CATALOGUES`].
    pub catalogue: usize,
    /// The style row in the detail, per catalogue tab.
    pub style_focus: Option<u16>,
    pub style_detail: DetailState,
    /// The standard header's state and its refusal.
    pub header: HeaderMode,
    pub header_error: Option<String>,
    /// The project and change count the rows were read at.
    seen: Option<(u64, u64)>,
    /// Full builds (first show, another project) and patches: the update map's counters.
    pub rebuilds: u64,
    pub patches: u64,
    /// The row in the detail.
    pub focus: Option<u16>,
    /// A key asked for before the rows were built (an origin badge).
    want: Option<String>,
    /// The project value being typed, for the focused row.
    pub edit: String,
    edit_for: Option<u16>,
    pub error: Option<String>,
    /// `users` of the focused row at a change count: (row, count, users).
    users: Option<(u16, u64, usize)>,
}

impl Default for StandardsView {
    fn default() -> Self {
        let mut table = Table::new("standards", "Defaults");
        table.empty = "No project is open.";
        let cats = CATALOGUES
            .iter()
            .filter_map(|(w, _, id)| ecad_cadcraft::styles::catalogue(w).map(|c| (c, *id)))
            .map(|(c, id)| {
                let mut tb = Table::new(id, c.plural);
                tb.empty = "No project is open.";
                (CatalogueSource::new(c), tb)
            })
            .collect();
        Self {
            defaults: DefaultsSource::default(),
            table,
            cats,
            catalogue: 0,
            style_focus: None,
            style_detail: DetailState::default(),
            header: HeaderMode::Idle,
            header_error: None,
            seen: None,
            rebuilds: 0,
            patches: 0,
            focus: None,
            want: None,
            edit: String::new(),
            edit_for: None,
            error: None,
            users: None,
        }
    }
}

impl StandardsView {
    /// Show the row of registry key `key` in the detail (and select it in the table).
    pub fn focus_key(&mut self, key: &str) {
        self.catalogue = 0;
        match self.defaults.key_of(key) {
            Some(k) => self.focus_row(k),
            None => self.want = Some(key.to_string()),
        }
    }

    fn focus_row(&mut self, k: u16) {
        self.focus = Some(k);
        self.want = None;
        self.table.selection.set([k]);
        self.table.reveal(&[k]);
    }
}

/// Bring the rows up to date (the update map, [`crate::lists::standards`]).
fn sync(t: &mut Trial) {
    let Some(p) = t.project.clone().filter(|p| p.is_attached(&t.cad.session)) else {
        let v = &mut t.settings.standards;
        if v.seen.is_some() {
            v.seen = None;
            v.defaults = DefaultsSource::default();
            v.table.update(&v.defaults, Changed::All);
            for (src, tb) in &mut v.cats {
                *src = CatalogueSource::new(src.catalogue);
                tb.update(src, Changed::All);
            }
        }
        return;
    };
    let serial = t.project_serial;
    let v = &mut t.settings.standards;
    let seen = v.seen;
    let _ = p.read(&mut t.cad.session, |p, _| {
        let count = p.change_count();
        let doc = p.ledger.document();
        match seen {
            Some((s, n)) if s == serial && n == count => {}
            Some((s, n)) if s == serial => {
                let hit = p.props_changes_since(n).is_none_or(|o| o.contains(&ecad_model::ObjectRef::Project));
                if hit {
                    let a = v.defaults.patch(doc);
                    let mut any = a != Changed::Nothing;
                    v.table.update(&v.defaults, a);
                    for (src, tb) in &mut v.cats {
                        let b = src.patch(doc);
                        any |= b != Changed::Nothing;
                        tb.update(src, b);
                    }
                    if any {
                        v.patches += 1;
                    }
                }
                v.seen = Some((serial, count));
            }
            _ => {
                v.defaults.build(doc);
                v.table.reset();
                v.table.update(&v.defaults, Changed::All);
                for (src, tb) in &mut v.cats {
                    src.build(doc);
                    tb.reset();
                    tb.update(src, Changed::All);
                }
                v.rebuilds += 1;
                v.seen = Some((serial, count));
            }
        }
    });
    if let Some(k) = v.want.clone().and_then(|w| v.defaults.key_of(&w)) {
        v.focus_row(k);
    }
}

/// What the detail shows of the focused entry, read in one go.
struct Detail {
    /// (source, value as shown) broadest first.
    chain: Vec<(Source, Option<String>)>,
    from: Source,
    current: String,
}

fn detail(t: &mut Trial, entry: &'static Entry) -> Option<Detail> {
    let p = t.project.clone()?;
    p.read(&mut t.cad.session, |p, _| {
        let doc = p.ledger.document();
        let r = defaults::resolve(doc, entry, Scope::project());
        let chain = defaults::chain(doc, entry, Scope::project())
            .into_iter()
            .map(|(s, v)| (s, v.map(|v| value_text(entry, &defaults::Resolved { value: v, from: s }))))
            .collect();
        Detail { chain, from: r.from, current: value_text(entry, &r) }
    })
    .ok()
}

/// How many objects use the project's value of the focused row, kept per change count.
fn users(t: &mut Trial, row: u16, entry: &'static Entry) -> usize {
    let p = t.project.clone();
    let count = p.as_ref().and_then(|p| p.read(&mut t.cad.session, |p, _| p.change_count()).ok()).unwrap_or(0);
    if let Some((r, n, u)) = t.settings.standards.users
        && r == row
        && n == count
    {
        return u;
    }
    let u = p.and_then(|p| p.read(&mut t.cad.session, |p, _| defaults::users(p.ledger.document(), entry)).ok()).unwrap_or(0);
    t.settings.standards.users = Some((row, count, u));
    u
}

/// The consequence line (push-back 2): what changes, said before applying.
pub fn consequence(entry: &Entry, n: usize, current: &str, new: &str) -> String {
    let noun = defaults::users_noun(entry);
    let (one, many) = match noun {
        "devices" => ("device uses", "devices use"),
        "wires" => ("wire uses", "wires use"),
        _ => ("page uses", "pages use"),
    };
    let who = if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    if entry.key.ends_with(".prefix") {
        return format!("{who} {current}; fixed and released tags keep their names; new devices get {new}.");
    }
    format!("{who} {current}; after the change they use {new}.")
}

/// The words of a level in the chain.
fn level_words(s: Source) -> String {
    match s {
        Source::Standard => format!("Standard: {}", super::STANDARD),
        other => source_word(other).to_string(),
    }
}

/// Draw the screen.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    sync(t);
    let writable = t.project_open() && !t.header().read_only;
    header_ui(t, ui, writable);
    ui.horizontal(|ui| {
        let mut tabs: Vec<(String, &str)> = vec![("Defaults".into(), CATALOGUE_DEFAULTS)];
        for (w, name, _) in CATALOGUES {
            if let Some(c) = ecad_cadcraft::styles::catalogue(w) {
                tabs.push((c.plural.to_string(), name));
            }
        }
        for (i, (label, name)) in tabs.into_iter().enumerate() {
            let on = t.settings.standards.catalogue == i;
            let r = ui.selectable_label(on, label);
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, name));
            if r.clicked() && !on {
                t.settings.standards.catalogue = i;
                t.settings.standards.style_focus = None;
                t.settings.standards.style_detail = DetailState::default();
            }
        }
    });
    let avail = ui.available_rect_before_wrap();
    let split = (avail.width() - 380.0).max(avail.width() * 0.55);
    let left = egui::Rect::from_min_max(avail.min, egui::pos2(avail.left() + split, avail.bottom()));
    let right = egui::Rect::from_min_max(egui::pos2(left.right() + ecad_ui::tokens::space::L, avail.top()), avail.max);
    let catalogue = t.settings.standards.catalogue;
    let events = super::in_rect(ui, left, "ecad.standards.table", |ui| {
        let v = &mut t.settings.standards;
        match catalogue.checked_sub(1).and_then(|i| v.cats.get_mut(i)) {
            None => v.table.show(ui, &v.defaults),
            Some((src, tb)) => tb.show(ui, src),
        }
    });
    for e in events {
        match (catalogue, e) {
            (0, TableEvent::Selected(keys)) => t.settings.standards.focus = keys.first().copied(),
            (0, TableEvent::Open(k) | TableEvent::GoTo { key: k, .. }) => t.settings.standards.focus = Some(k),
            (_, TableEvent::Selected(keys)) => {
                let v = &mut t.settings.standards;
                if v.style_focus != keys.first().copied() {
                    v.style_focus = keys.first().copied();
                    v.style_detail = DetailState::default();
                }
            }
            (_, TableEvent::Export(csv)) => crate::lists::copy_csv(ui, csv),
            _ => {}
        }
    }
    if catalogue > 0 {
        super::in_rect(ui, right, "ecad.standards.detail", |ui| style_detail_ui(t, ui, writable));
        return;
    }
    super::in_rect(ui, right, "ecad.standards.detail", |ui| detail_ui(t, ui, writable));
}

fn detail_ui(t: &mut Trial, ui: &mut egui::Ui, writable: bool) {
    let Some(row) = t.settings.standards.focus else {
        ecad_ui::prim::read_value(ui, "Choose a default to see where its value comes from.");
        return;
    };
    let Some(entry) = t.settings.standards.defaults.entry(row) else { return };
    let Some(d) = detail(t, entry) else { return };
    let v = &mut t.settings.standards;
    if v.edit_for != Some(row) {
        v.edit = d.current.clone();
        v.edit_for = Some(row);
        v.error = None;
    }
    ui.label(RichText::new(entry.label).strong());
    ui.label(RichText::new(entry.key).monospace().small());
    ui.label(RichText::new(format!("{} · brief {}", entry.group, entry.brief)).small());
    ui.add_space(ecad_ui::tokens::space::M);
    // The chain, broadest first, the level in use marked.
    egui::Grid::new("ecad.standards.chain").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
        for (s, val) in &d.chain {
            let used = *s == d.from;
            let words = level_words(*s);
            let name = format!("ecad.standards.chain.{}", s.as_str());
            let label = if used { RichText::new(&words).strong() } else { RichText::new(&words) };
            ui.label(label).widget_info(|| egui::WidgetInfo::selected(egui::Role::Label, true, used, &name));
            let shown = match (s, val) {
                (_, Some(x)) => x.clone(),
                (Source::Standard, None) => "no value of its own".into(),
                (Source::Page, None) => "set on a page in the inspector".into(),
                (Source::Object | Source::Symbol, None) => "set on a device in the inspector".into(),
                _ => "—".into(),
            };
            ui.label(if val.is_some() { RichText::new(shown) } else { RichText::new(shown).weak() });
            if used {
                ecad_ui::prim::badge(ui, IN_USE, source_tone(*s));
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    });
    ui.add_space(ecad_ui::tokens::space::M);
    if !entry.allows(Level::Project) {
        ecad_ui::prim::read_value(ui, "This default is not set for the project.");
        return;
    }
    ui.label(RichText::new("Project value").small().strong());
    let mut apply = false;
    let mut reset = false;
    let v = &mut t.settings.standards;
    ui.horizontal(|ui| {
        let r = ui.add_enabled(writable, egui::TextEdit::singleline(&mut v.edit).desired_width(160.0));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, writable, VALUE));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        apply = ui.add_enabled(writable, egui::Button::new("Apply")).widget_info_named(APPLY).clicked() || (enter && writable);
        if d.from == Source::Project {
            reset = ui.add_enabled(writable, egui::Button::new("Reset")).widget_info_named(RESET).clicked();
        }
    });
    if !writable {
        ui.label(RichText::new("The project is read-only.").small().weak());
    }
    let typed = v.edit.trim().to_string();
    let parsed = default_cmds::stored_text(entry, &serde_json::Value::String(typed.clone()));
    let changes = match &parsed {
        Ok(s) => entry.parse(s).map(|x| value_text(entry, &defaults::Resolved { value: x, from: Source::Project })).filter(|n| *n != d.current),
        Err(_) => None,
    };
    if let (Err(e), false) = (&parsed, typed.is_empty() || typed == d.current) {
        ui.label(RichText::new(e).small().color(ui.visuals().warn_fg_color));
    }
    if let Some(new) = &changes {
        let n = users(t, row, entry);
        let line = consequence(entry, n, &d.current, new);
        ui.label(RichText::new(&line).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, CONSEQUENCE));
    }
    if let Some(e) = &t.settings.standards.error {
        ui.label(RichText::new(e).small().color(ui.visuals().error_fg_color))
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, ERROR));
    }
    let line = if reset {
        Some(default_cmds::line(entry.key, Level::Project, Target::Project, None))
    } else if apply && changes.is_some() {
        parsed.ok().map(|s| default_cmds::line(entry.key, Level::Project, Target::Project, Some(&s)))
    } else {
        None
    };
    if let Some(line) = line {
        let r = t.run_line(&line);
        let v = &mut t.settings.standards;
        v.error = r;
        if v.error.is_none() {
            v.edit_for = None;
        }
    }
}

/// The standard header (design 050 §7.1): the standard's name, what it holds, and its buttons;
/// each action states its consequence inline before anything is written. Esc writes nothing.
fn header_ui(t: &mut Trial, ui: &mut egui::Ui, writable: bool) {
    if !t.project_open() {
        return;
    }
    let read = t.project.clone().and_then(|p| {
        p.read(&mut t.cad.session, |p, _| {
            let doc = p.ledger.document();
            let (v, n) = ecad_cadcraft::style_cmds::standard_counts(doc);
            (doc.standard.as_ref().map(|s| s.header.name.clone()), v, n)
        })
        .ok()
    });
    let Some((name, values, styles)) = read else { return };
    let mut line: Option<String> = None;
    let mode = t.settings.standards.header.clone();
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    ui.horizontal(|ui| {
        ui.label(RichText::new("STANDARD").small().strong()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, HEADER));
        let words = match &name {
            None => super::STANDARD.to_string(),
            Some(n) => format!("{n} · based on IEC · {values} values · {styles} styles"),
        };
        ecad_ui::prim::read_value(ui, &words).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, STANDARD_LINE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let h = &mut t.settings.standards.header;
            match name {
                None => {
                    if ui.add_enabled(writable, egui::Button::new("Make a standard…")).widget_info_named(MAKE).clicked() {
                        *h = HeaderMode::Make(String::new());
                    }
                }
                Some(ref n) => {
                    if ui.add_enabled(writable, egui::Button::new("Back to IEC")).widget_info_named(BACK_TO_IEC).clicked() {
                        *h = HeaderMode::Back;
                    }
                    if ui.add_enabled(writable, egui::Button::new("Copy from project…")).widget_info_named(COPY_FROM).clicked() {
                        *h = HeaderMode::Copy(String::new(), None);
                    }
                    if ui.add_enabled(writable, egui::Button::new("Rename")).widget_info_named(RENAME).clicked() {
                        *h = HeaderMode::Rename(n.clone());
                    }
                }
            }
        });
    });
    if !writable {
        if mode != HeaderMode::Idle {
            t.settings.standards.header = HeaderMode::Idle;
        }
        return;
    }
    match mode {
        HeaderMode::Idle => {}
        HeaderMode::Make(mut text) | HeaderMode::Rename(mut text) => {
            let making = matches!(t.settings.standards.header, HeaderMode::Make(_));
            let mut go = false;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(200.0).hint_text("Standard name"));
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, HEADER_NAME));
                go = ui.button(if making { "Make" } else { "Rename" }).widget_info_named(HEADER_CONFIRM).clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if ui.button("Cancel").clicked() {
                    t.settings.standards.header = HeaderMode::Idle;
                }
            });
            if esc {
                t.settings.standards.header = HeaderMode::Idle;
            } else if go && text.trim().is_empty() {
                t.settings.standards.header_error = Some("A standard needs a name.".into());
            } else if go {
                let key = if making { "new" } else { "name" };
                line = Some(format!("standard {{\"{key}\":{}}}", serde_json::Value::String(text.trim().to_string())));
            } else if t.settings.standards.header != HeaderMode::Idle {
                t.settings.standards.header = if making { HeaderMode::Make(text) } else { HeaderMode::Rename(text) };
            }
        }
        HeaderMode::Copy(mut path, read) => {
            let mut look = false;
            let mut copy = false;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut path).desired_width(320.0).hint_text("Path of a .ecad project"));
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, HEADER_NAME));
                look = ui.button("Read").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if read.is_some() {
                    copy = ui.button("Copy").widget_info_named(HEADER_CONFIRM).clicked();
                }
                if ui.button("Cancel").clicked() {
                    t.settings.standards.header = HeaderMode::Idle;
                }
            });
            if let Some((other, v, n)) = &read {
                let current = name.clone().unwrap_or_else(|| super::STANDARD.to_string());
                let words = format!("Replace {current} with {other} ({v} values, {n} styles)? Your project values stay.");
                ui.label(RichText::new(words).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, HEADER_CONSEQUENCE));
            }
            if esc {
                t.settings.standards.header = HeaderMode::Idle;
            } else if copy {
                line = Some(format!("standard {{\"from\":{}}}", serde_json::Value::String(path.trim().to_string())));
            } else if t.settings.standards.header != HeaderMode::Idle {
                let fresh = if look {
                    match ecad_cadcraft::style_cmds::read_standard(path.trim()) {
                        Ok(s) => Some((s.header.name.clone(), s.values.len(), s.styles.values().map(|m| m.len()).sum())),
                        Err(e) => {
                            t.settings.standards.header_error = Some(e);
                            None
                        }
                    }
                } else {
                    read
                };
                t.settings.standards.header = HeaderMode::Copy(path, fresh);
            }
        }
        HeaderMode::Back => {
            let words = format!("{values} values and {styles} styles go back to IEC; project values stay.");
            let mut go = false;
            ui.horizontal(|ui| {
                ui.label(RichText::new(words).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, HEADER_CONSEQUENCE));
                go = ui.button("Back to IEC").widget_info_named(HEADER_CONFIRM).clicked();
                if ui.button("Cancel").clicked() || esc {
                    t.settings.standards.header = HeaderMode::Idle;
                }
            });
            if go {
                line = Some("standard {\"remove\":true}".to_string());
            }
        }
    }
    if let Some(e) = &t.settings.standards.header_error {
        ui.label(RichText::new(e).small().color(ui.visuals().error_fg_color));
    }
    if let Some(line) = line {
        let r = t.run_line(&line);
        let v = &mut t.settings.standards;
        if r.is_none() {
            v.header = HeaderMode::Idle;
        }
        v.header_error = r;
    }
}

/// The shared style detail of the focused style row (design 050 §7.2).
fn style_detail_ui(t: &mut Trial, ui: &mut egui::Ui, writable: bool) {
    let catalogue = t.settings.standards.catalogue;
    let Some(i) = catalogue.checked_sub(1) else { return };
    let (Some(cat), Some(row)) = (t.settings.standards.cats.get(i).map(|c| c.0.catalogue), t.settings.standards.style_focus) else {
        ecad_ui::prim::read_value(ui, "Choose a style to see its fields and who uses it.");
        return;
    };
    let Some(id) = t.settings.standards.cats.get(i).and_then(|c| c.0.row(row)).map(|r| r.id.clone()) else { return };
    let Some(p) = t.project.clone() else { return };
    let read = p.read(&mut t.cad.session, |p, _| {
        let doc = p.ledger.document();
        let d = sc::detail(doc, cat, &id);
        let others: Vec<(String, String)> =
            ecad_cadcraft::styles::rows(doc, cat).into_iter().filter(|r| r.id != id).map(|r| (r.id.text(), r.name)).collect();
        (d, others)
    });
    let Ok((d, others)) = read else { return };
    // Used by in the table, for this row only.
    if let Some((src, tb)) = t.settings.standards.cats.get_mut(i)
        && src.used.as_ref().is_none_or(|(k, _)| *k != row)
    {
        src.used = Some((row, d.users.0.to_string()));
        tb.update(src, Changed::Keys(vec![row]));
    }
    let actions = sc::show_detail(ui, cat, &d, &mut t.settings.standards.style_detail, writable, &others);
    for a in actions {
        let sc::DetailAction::Run(line) = a;
        let r = t.run_line(&line);
        t.settings.standards.style_detail.error = r;
    }
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
