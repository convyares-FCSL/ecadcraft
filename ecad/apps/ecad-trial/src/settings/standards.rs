//! Settings ▸ Standards (brief 042 step 3, screen e; roadmap 1.13(b)): one table of every default
//! with its value and an origin badge ([`crate::lists::standards`]), filtered by group or by
//! "Overridden", and on the right the chosen default's chain, built-in → standard → project →
//! page → object, with the level in use marked. A level is listed only when it holds a value or
//! the entry allows it (push-back 8). The project's value is changed or reset here through
//! `setdefault`; before applying, the detail says how many objects use the value and what
//! changes for them (computed by `defaults::users`, not by applying). Page and object values are
//! set in the inspector. The text styles (031) are the first catalogue, a second table.
//!
//! The standard is IEC, built in, read only (owner answer Q1): named standards come later.

use ecad_cadcraft::default_cmds::{self, Target};
use ecad_cadcraft::defaults::{self, Entry, Level, Scope, Source};
use ecad_ui::table::{Changed, Table, TableEvent};
use eframe::egui::{self, RichText};

use crate::Trial;
use crate::lists::standards::{DefaultsSource, StylesSource, source_tone, source_word, value_text};

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

/// The screen's state: both tables, the row shown in the detail and the value being typed.
#[derive(Debug)]
pub struct StandardsView {
    pub defaults: DefaultsSource,
    pub table: Table<u16>,
    pub styles: StylesSource,
    pub styles_table: Table<u16>,
    /// 0: Defaults, 1: Text styles.
    pub catalogue: usize,
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
        let mut styles_table = Table::new("standards.styles", "Text styles");
        styles_table.empty = "No project is open.";
        Self {
            defaults: DefaultsSource::default(),
            table,
            styles: StylesSource::default(),
            styles_table,
            catalogue: 0,
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
            v.styles = StylesSource::default();
            v.table.update(&v.defaults, Changed::All);
            v.styles_table.update(&v.styles, Changed::All);
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
                    let b = v.styles.patch(doc);
                    if a != Changed::Nothing || b != Changed::Nothing {
                        v.patches += 1;
                    }
                    v.table.update(&v.defaults, a);
                    v.styles_table.update(&v.styles, b);
                }
                v.seen = Some((serial, count));
            }
            _ => {
                v.defaults.build(doc);
                v.styles.build(doc);
                v.table.reset();
                v.styles_table.reset();
                v.table.update(&v.defaults, Changed::All);
                v.styles_table.update(&v.styles, Changed::All);
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
    ui.horizontal(|ui| {
        ecad_ui::prim::read_value(ui, &format!("Standard: {} · read only; named standards (IEC, US) come later.", super::STANDARD))
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, STANDARD_LINE));
    });
    ui.horizontal(|ui| {
        for (i, (label, name)) in [("Defaults", CATALOGUE_DEFAULTS), ("Text styles", CATALOGUE_STYLES)].into_iter().enumerate() {
            let on = t.settings.standards.catalogue == i;
            let r = ui.selectable_label(on, label);
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, name));
            if r.clicked() {
                t.settings.standards.catalogue = i;
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
        if catalogue == 0 { v.table.show(ui, &v.defaults) } else { v.styles_table.show(ui, &v.styles) }
    });
    for e in events {
        match (catalogue, e) {
            (0, TableEvent::Selected(keys)) => t.settings.standards.focus = keys.first().copied(),
            (0, TableEvent::Open(k) | TableEvent::GoTo { key: k, .. }) => t.settings.standards.focus = Some(k),
            (_, TableEvent::Selected(keys)) => {
                if let Some(key) = keys.first().and_then(|k| t.settings.standards.styles.height_key(*k)) {
                    t.settings.standards.focus_key(key);
                }
            }
            (_, TableEvent::Export(csv)) => crate::lists::copy_csv(ui, csv),
            _ => {}
        }
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
