//! Settings ▸ Standards as column sets on U3's table (brief 042, screen e): every default of the
//! registry with its value and an origin badge, and the text styles catalogue (031's style table).
//!
//! **Update map.** The rows show the project's values (resolved at project scope), so only a
//! commit that touched the project (a project property, or the settings, which the change feed
//! reports as "anything") can change them. Then every entry is resolved again (the registry, not
//! the project: a few hundred entries) and only the rows whose value or source changed are
//! patched and tinted. Any other commit: nothing. A full rebuild only on first show or a new
//! project.

use ecad_cadcraft::defaults::{self, Entry, Source};
use ecad_cadcraft::text_style::TextKind;
use ecad_model::Document;
use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Changed, Column, ColumnId, TableSource, Width};

pub const SETTING: ColumnId = ColumnId(0);
pub const GROUP: ColumnId = ColumnId(1);
pub const VALUE: ColumnId = ColumnId(2);
pub const FROM: ColumnId = ColumnId(3);
pub const KEY: ColumnId = ColumnId(4);

/// The words of a source on a badge.
pub fn source_word(s: Source) -> &'static str {
    match s {
        Source::Builtin => "Built-in",
        Source::Standard => "Standard",
        Source::Project => "Project",
        Source::Page => "Page",
        Source::Object => "Object",
        Source::Symbol => "Symbol",
    }
}

/// A badge's tone: the built-in and the standard are quiet, the overrides stand out.
pub fn source_tone(s: Source) -> Tone {
    match s {
        Source::Builtin | Source::Standard => Tone::Neutral,
        Source::Project | Source::Page => Tone::Ok,
        Source::Object | Source::Symbol => Tone::Provisional,
    }
}

/// A value as shown: a length with its unit.
pub fn value_text(e: &Entry, r: &defaults::Resolved<'_>) -> String {
    match r.value {
        defaults::Value::Length(x) => number_text(x, "mm"),
        defaults::Value::Number(x) => number_text(x, ""),
        _ if e.key.ends_with(".prefix") => format!("-{}", r.text().trim_start_matches('-')),
        _ => r.text(),
    }
}

/// `12.5 mm`, `3`: a number with no trailing zeros, and its unit.
pub fn number_text(v: f64, unit: &str) -> String {
    let s = format!("{v:.3}");
    let n = s.trim_end_matches('0').trim_end_matches('.');
    if unit.is_empty() { n.to_string() } else { format!("{n} {unit}") }
}

/// One row: the entry and what the project resolves it to.
#[derive(Debug, Clone)]
struct Row {
    entry: &'static Entry,
    value: String,
    from: Source,
}

impl PartialEq for Row {
    fn eq(&self, o: &Row) -> bool {
        self.entry.key == o.entry.key && self.value == o.value && self.from == o.from
    }
}

fn row(doc: &Document, entry: &'static Entry) -> Row {
    let r = defaults::resolve(doc, entry, defaults::Scope::project());
    Row { entry, value: value_text(entry, &r), from: r.from }
}

/// The defaults as a table source. Keys are the registry's order.
#[derive(Debug)]
pub struct DefaultsSource {
    cols: Vec<Column>,
    keys: Vec<u16>,
    rows: Vec<Row>,
}

impl Default for DefaultsSource {
    fn default() -> Self {
        Self {
            cols: vec![
                Column::read(SETTING.0, "Setting", Width::Initial(220.0)),
                Column::read(GROUP.0, "Group", Width::Initial(130.0)).groupable(),
                Column::read(VALUE.0, "Value", Width::Remainder),
                Column::read(FROM.0, "From", Width::Initial(90.0)).groupable(),
                Column::read(KEY.0, "Key", Width::Initial(210.0)),
            ],
            keys: Vec::new(),
            rows: Vec::new(),
        }
    }
}

impl DefaultsSource {
    /// Every row from `doc` (first show, new project).
    pub fn build(&mut self, doc: &Document) {
        self.rows = defaults::all().map(|e| row(doc, e)).collect();
        self.keys = (0..self.rows.len()).filter_map(|i| u16::try_from(i).ok()).collect();
    }

    /// Resolve every entry again and return the rows whose value or source changed.
    pub fn patch(&mut self, doc: &Document) -> Changed<u16> {
        let mut touched = Vec::new();
        for (i, e) in defaults::all().enumerate() {
            let fresh = row(doc, e);
            match self.rows.get_mut(i) {
                Some(r) if *r == fresh => {}
                Some(r) => {
                    *r = fresh;
                    if let Ok(k) = u16::try_from(i) {
                        touched.push(k);
                    }
                }
                None => return Changed::All,
            }
        }
        if touched.is_empty() { Changed::Nothing } else { Changed::Keys(touched) }
    }

    /// The entry of row `key`.
    pub fn entry(&self, key: u16) -> Option<&'static Entry> {
        self.rows.get(usize::from(key)).map(|r| r.entry)
    }

    /// The row of the entry with this registry key.
    pub fn key_of(&self, registry_key: &str) -> Option<u16> {
        self.rows.iter().position(|r| r.entry.key == registry_key).and_then(|i| u16::try_from(i).ok())
    }

    /// The source of row `key`'s value.
    pub fn from(&self, key: u16) -> Option<Source> {
        self.rows.get(usize::from(key)).map(|r| r.from)
    }
}

impl TableSource for DefaultsSource {
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
        match col {
            SETTING => Cell::Text(r.entry.label.to_string()),
            GROUP => Cell::read(r.entry.group),
            VALUE => Cell::Text(r.value.clone()),
            FROM => Cell::Badge(source_word(r.from).to_string(), source_tone(r.from)),
            KEY => Cell::read(r.entry.key),
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: u16) -> String {
        self.rows.get(usize::from(key)).map(|r| format!("{}: {} ({})", r.entry.key, r.value, source_word(r.from))).unwrap_or_default()
    }

    fn chips(&self) -> &[&'static str] {
        &["Overridden"]
    }

    fn chip(&self, key: u16, _chip: usize) -> bool {
        self.rows.get(usize::from(key)).is_some_and(|r| r.from != Source::Builtin)
    }
}

pub const KIND: ColumnId = ColumnId(0);
pub const HEIGHT: ColumnId = ColumnId(1);
pub const WEIGHT: ColumnId = ColumnId(2);
pub const FONT: ColumnId = ColumnId(3);
pub const SHOWN: ColumnId = ColumnId(4);

/// One text kind's style row.
#[derive(Debug, Clone, PartialEq)]
struct StyleRow {
    kind: TextKind,
    height: (String, Source),
    weight: (String, Source),
    font: String,
    shown: Option<(String, Source)>,
}

fn style_row(doc: &Document, k: TextKind) -> StyleRow {
    let at = defaults::Scope::project();
    let h = defaults::resolve(doc, k.height_entry(), at);
    let w = defaults::resolve(doc, k.weight_entry(), at);
    StyleRow {
        kind: k,
        height: (number_text(h.number(), "mm"), h.from),
        weight: (w.word().to_string(), w.from),
        font: match ecad_cadcraft::text_style::kind_font(doc, k) {
            Some(f) => f.to_string(),
            None => defaults::resolve(doc, &defaults::text::FONT, at).text(),
        },
        shown: k.show_entry().map(|e| {
            let r = defaults::resolve(doc, e, at);
            (r.word().to_string(), r.from)
        }),
    }
}

/// The text styles catalogue (031): one row per kind. Same update map as the defaults.
#[derive(Debug)]
pub struct StylesSource {
    cols: Vec<Column>,
    keys: Vec<u16>,
    rows: Vec<StyleRow>,
}

impl Default for StylesSource {
    fn default() -> Self {
        Self {
            cols: vec![
                Column::read(KIND.0, "Text", Width::Initial(180.0)),
                Column::read(HEIGHT.0, "Height", Width::Initial(90.0)),
                Column::read(WEIGHT.0, "Weight", Width::Initial(90.0)),
                Column::read(FONT.0, "Font", Width::Remainder),
                Column::read(SHOWN.0, "Shown", Width::Initial(90.0)),
            ],
            keys: Vec::new(),
            rows: Vec::new(),
        }
    }
}

impl StylesSource {
    pub fn build(&mut self, doc: &Document) {
        self.rows = TextKind::ALL.iter().map(|k| style_row(doc, *k)).collect();
        self.keys = (0..self.rows.len()).filter_map(|i| u16::try_from(i).ok()).collect();
    }

    pub fn patch(&mut self, doc: &Document) -> Changed<u16> {
        let mut touched = Vec::new();
        for (i, k) in TextKind::ALL.iter().enumerate() {
            let fresh = style_row(doc, *k);
            if let Some(r) = self.rows.get_mut(i)
                && *r != fresh
            {
                *r = fresh;
                if let Ok(k) = u16::try_from(i) {
                    touched.push(k);
                }
            }
        }
        if touched.is_empty() { Changed::Nothing } else { Changed::Keys(touched) }
    }

    /// The height entry's key of row `key` (where Go to on a row leads).
    pub fn height_key(&self, key: u16) -> Option<&'static str> {
        self.rows.get(usize::from(key)).map(|r| r.kind.height_entry().key)
    }
}

/// A kind's name: its height entry's label without " height" (`Tag`).
pub fn kind_label(k: TextKind) -> &'static str {
    let l = k.height_entry().label;
    l.strip_suffix(" height").unwrap_or(l)
}

/// A value with its source when it is not the built-in: `3 mm (Project)`.
fn with_source(v: &(String, Source)) -> String {
    if v.1 == Source::Builtin { v.0.clone() } else { format!("{} ({})", v.0, source_word(v.1)) }
}

impl TableSource for StylesSource {
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
        match col {
            KIND => Cell::Text(kind_label(r.kind).to_string()),
            HEIGHT => Cell::Text(with_source(&r.height)),
            WEIGHT => Cell::Text(with_source(&r.weight)),
            FONT => Cell::read(r.font.clone()),
            SHOWN => r.shown.as_ref().map_or(Cell::Empty, |s| Cell::Text(with_source(s))),
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: u16) -> String {
        self.rows.get(usize::from(key)).map(|r| format!("text.{}: {}", r.kind.word(), with_source(&r.height))).unwrap_or_default()
    }
}
