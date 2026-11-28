//! The built-in values as data.
//!
//! Every registry entry's built-in value, every style field's built-in and every built-in style's
//! field values live in TOML files, one per area, that the crate declaring them embeds at build time
//! (a [`DataSet`]). Only values: an entry's kind, range, levels and words stay in its Rust [`Entry`],
//! so no data file can widen what is allowed.
//!
//! # Semantics
//! - [`load_in`] reads one file (a built-in file now, a company standard file later). It never panics
//!   and never gives a value outside its entry's range: a file too large or nested too deep is
//!   refused before parsing, a syntax error refuses the whole file, an unknown key is listed and
//!   ignored, a value of the wrong kind or out of range is listed and left out, so the layer below
//!   is used.
//! - A data set's files are parsed once, on first use. Each entry, field and built-in style keeps
//!   its own value in a [`Slot`] that names its data set, filled on its first read, so resolving is
//!   a field read after that: no hashing by key on the hot path.
//! - A built-in that still fails to load (not reachable in a tested build: tests check every
//!   file) takes its kind's safe value ([`safe_value`]) and is listed by [`DataSet::problems`],
//!   which Checks reports as `defaults-damaged`.
//! - The keys [`crate::naming::builtins::KEYS`] names take their built-in from this crate's own data
//!   (`defaults/settings.toml`); no data set names them.
//!
//! Excluded: loading a company file from disk, numbered built-in sets, and `miette` reports (the
//! errors already carry the file and the byte span it needs).

use std::fmt;
use std::ops::Range;
use std::sync::OnceLock;

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::registry::styles::{Builtin as BuiltinStyle, Catalogue, Field};
use crate::registry::{Entry, Kind, Value, parse_value};

/// The largest file [`load_in`] parses: a guard, not a preference.
pub const MAX_BYTES: usize = 256 * 1024;
/// The deepest nesting [`load_in`] parses: brackets, braces and dotted keys.
pub const MAX_DEPTH: usize = 16;
/// The unknown keys listed one by one; the rest are counted in one problem.
pub const MAX_LISTED: usize = 50;

/// Whether this crate holds `key`'s built-in ([`crate::naming::builtins::KEYS`]: what the lowest
/// crate needs lives only there). No data set's files name it.
pub fn in_model(key: &str) -> bool {
    crate::naming::builtins::KEYS.contains(&key)
}

/// A problem with a defaults file. Each names the file and, where it has one, the byte span the
/// TOML parser reports (what a `miette` report underlines).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefaultsError {
    #[error("{file}: {bytes} bytes, more than {MAX_BYTES}")]
    TooLarge { file: String, bytes: usize },
    #[error("{file}: nested deeper than {MAX_DEPTH} at byte {}", span.start)]
    TooDeep { file: String, span: Range<usize> },
    #[error("{file}:{line}:{column}: {message}")]
    Syntax { file: String, line: usize, column: usize, message: String, span: Range<usize> },
    #[error("{file}: unknown key `{key}`")]
    UnknownKey { file: String, key: String, span: Range<usize> },
    #[error("{file}: and {count} more unknown keys")]
    MoreUnknownKeys { file: String, count: usize },
    #[error("{file}: `{key}` should be {expected}")]
    WrongKind { file: String, key: String, expected: &'static str, span: Range<usize> },
    #[error("{file}: `{key}` is out of range or not allowed")]
    OutOfRange { file: String, key: String, span: Range<usize> },
    #[error("{file}: `{key}` has no value")]
    Missing { file: String, key: String },
}

/// One crate's built-in data: its files, and the entries, catalogues and layout values whose
/// built-ins they hold.
pub struct DataSet {
    /// The files, as `(name, text)`: one per area.
    pub files: &'static [(&'static str, &'static str)],
    /// The crate's registry entries, one list per area.
    pub lists: &'static [&'static [&'static Entry]],
    /// Entries with a built-in value in these files that are not listed in the registry (a field
    /// of a style read as an entry).
    pub unlisted: &'static [&'static Entry],
    /// The crate's style catalogues.
    pub catalogues: &'static [&'static Catalogue],
    /// Built-in layout values no level overrides: `[layout]` keys of the files, with the check
    /// each value must pass.
    pub layout: &'static [(&'static str, Kind)],
    /// The file a missing `[layout]` value belongs in, for the message.
    pub layout_file: &'static str,
    /// The file a missing style field belongs in, for the message.
    pub fields_file: &'static str,
    loaded: OnceLock<Vec<Loaded<'static>>>,
    problems: OnceLock<Vec<DefaultsError>>,
}

impl fmt::Debug for DataSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.files.iter().map(|(name, _)| name)).finish()
    }
}

impl DataSet {
    /// A data set; its files are parsed on first use.
    pub const fn new(
        files: &'static [(&'static str, &'static str)], lists: &'static [&'static [&'static Entry]], unlisted: &'static [&'static Entry],
        catalogues: &'static [&'static Catalogue], layout: &'static [(&'static str, Kind)], layout_file: &'static str, fields_file: &'static str,
    ) -> Self {
        DataSet { files, lists, unlisted, catalogues, layout, layout_file, fields_file, loaded: OnceLock::new(), problems: OnceLock::new() }
    }

    /// The registry entries of this set, in list order.
    pub fn entries(&self) -> impl Iterator<Item = &'static Entry> + use<> {
        let lists = self.lists;
        lists.iter().copied().flatten().copied()
    }

    /// Every entry with a built-in value in these files: the listed ones and the unlisted ones.
    pub fn valued(&self) -> impl Iterator<Item = &'static Entry> + use<> {
        let unlisted = self.unlisted;
        self.entries().chain(unlisted.iter().copied())
    }

    /// The listed or unlisted entry with this key.
    pub fn find(&self, key: &str) -> Option<&'static Entry> {
        self.valued().find(|e| e.key == key)
    }

    /// The files, parsed once.
    pub fn loaded(&'static self) -> &'static [Loaded<'static>] {
        self.loaded.get_or_init(|| self.files.iter().map(|(name, text)| load_in(self, name, text)).collect())
    }

    /// `entry`'s built-in value from the data (what [`Entry::builtin`] caches): this crate's own for
    /// its keys, else the set's files.
    pub fn builtin_value(&'static self, entry: &Entry) -> Value<'static> {
        let found = if in_model(entry.key) { model_value(entry) } else { self.loaded().iter().find_map(|f| f.value(entry)) };
        found.unwrap_or_else(|| safe_value(entry.kind))
    }

    /// A `[layout]` value (millimetres for a length), or its kind's safe value when missing or damaged.
    pub fn layout(&'static self, key: &str) -> f64 {
        let kind = self.layout.iter().find(|(l, _)| *l == key).map(|(_, k)| *k);
        let v = self.loaded().iter().find_map(|f| f.layout(self, key)).or_else(|| kind.map(safe_value));
        match v {
            Some(Value::Length(x) | Value::Number(x)) => x,
            _ => 0.0,
        }
    }

    /// The catalogue of this set that declares `f`.
    fn catalogue_of_field(&self, f: &Field) -> Option<&'static Catalogue> {
        self.catalogues.iter().copied().find(|c| c.fields.iter().any(|g| std::ptr::eq(g, f)))
    }

    /// `f`'s built-in value from the data (what [`Field::builtin`] caches).
    pub fn field_builtin(&'static self, f: &Field) -> Value<'static> {
        let Some(cat) = self.catalogue_of_field(f) else { return safe_value(f.kind) };
        if let Some(typed) = cat.typed {
            return (typed.field)(f);
        }
        self.loaded().iter().find_map(|file| file.field_value(cat, None, f.key)).unwrap_or_else(|| safe_value(f.kind))
    }

    /// Built-in style `b`'s field values from the data, in its catalogue's field order (what
    /// [`BuiltinStyle::values`] caches), with the catalogue's own adjustment applied.
    pub fn style_values(&'static self, b: &BuiltinStyle) -> Vec<(&'static str, Value<'static>)> {
        let Some(cat) = self.catalogues.iter().copied().find(|c| c.builtins.iter().any(|x| std::ptr::eq(x, b))) else {
            return Vec::new();
        };
        if let Some(typed) = cat.typed {
            return (typed.values)(b);
        }
        let mut out: Vec<(&'static str, Value<'static>)> = cat
            .fields
            .iter()
            .filter_map(|f| self.loaded().iter().find_map(|file| file.field_value(cat, Some(b.id), f.key)).map(|v| (f.key, v)))
            .collect();
        if let Some(adjust) = cat.adjust {
            adjust(b, &mut out);
        }
        out
    }

    /// The data's problems: each file's, then each entry or style field with no value in any file.
    /// Empty in a tested build; Checks reports any as `defaults-damaged`.
    pub fn problems(&'static self) -> &'static [DefaultsError] {
        self.problems.get_or_init(|| {
            let files = self.loaded();
            let mut out: Vec<DefaultsError> = files.iter().flat_map(|f| f.problems.iter().cloned()).collect();
            for e in self.valued() {
                let found = if in_model(e.key) { model_value(e).is_some() } else { files.iter().any(|f| f.has(e.key)) };
                if !found {
                    let file = if in_model(e.key) { crate::naming::builtins::FILE } else { self.area_file(e.key) };
                    out.push(DefaultsError::Missing { file: file.to_string(), key: e.key.to_string() });
                }
            }
            for (key, _) in self.layout {
                if !files.iter().any(|f| f.layout(self, key).is_some()) {
                    out.push(DefaultsError::Missing { file: self.layout_file.to_string(), key: format!("layout.{key}") });
                }
            }
            // A typed catalogue's missing default is a field its struct leaves `None`, which the
            // field's safe value covers; a damaged typed file fails its own read test.
            for cat in self.catalogues.iter().filter(|c| c.typed.is_none()) {
                for f in cat.fields {
                    if !files.iter().any(|file| file.has_field(crate::Word::word(cat.catalogue), f.key)) {
                        out.push(DefaultsError::Missing { file: self.fields_file.to_string(), key: format!("fields.{}.{}", cat.catalogue, f.key) });
                    }
                }
            }
            out
        })
    }

    /// The file a missing key belongs in, by its area (for the message only).
    fn area_file(&self, key: &str) -> &'static str {
        let area = key.split('.').next().unwrap_or_default();
        let names = self.files.iter().map(|(n, _)| *n);
        names
            .clone()
            .find(|n| n.trim_end_matches(".toml").trim_end_matches('s') == area)
            .or_else(|| self.files.first().map(|(n, _)| *n))
            .unwrap_or("")
    }
}

/// Every crate's data sets together: the whole registry, as the `defaults` and `style` commands,
/// the Standards screen and Checks see it. The crate that knows every set builds it; code that
/// needs one entry or catalogue it declares names that directly.
#[derive(Debug)]
pub struct Registry {
    sets: &'static [&'static DataSet],
}

impl Registry {
    /// The registry of `sets`, in listing order.
    pub const fn new(sets: &'static [&'static DataSet]) -> Self {
        Registry { sets }
    }

    /// Its data sets.
    pub fn sets(&self) -> impl Iterator<Item = &'static DataSet> + use<> {
        self.sets.iter().copied()
    }

    /// Every listed entry, set by set.
    pub fn entries(&self) -> impl Iterator<Item = &'static Entry> + use<> {
        self.sets().flat_map(DataSet::entries)
    }

    /// The listed entry with this key.
    pub fn find(&self, key: &str) -> Option<&'static Entry> {
        self.entries().find(|e| e.key == key)
    }

    /// Every style catalogue, set by set.
    pub fn catalogues(&self) -> impl Iterator<Item = &'static Catalogue> + use<> {
        self.sets().flat_map(|s| s.catalogues.iter().copied())
    }

    /// The catalogue that describes `c`, if a set declares it.
    pub fn catalogue(&self, c: crate::StyleCatalogue) -> Option<&'static Catalogue> {
        self.catalogues().find(|x| x.catalogue == c)
    }

    /// Every set's problems ([`DataSet::problems`]).
    pub fn problems(&self) -> impl Iterator<Item = &'static DefaultsError> + use<> {
        self.sets().flat_map(|s| s.problems().iter())
    }
}

/// A slot that holds a built-in value once it has been read from its data set ([`Entry::builtin`],
/// [`Field::builtin`], [`BuiltinStyle::values`]).
pub struct Slot<T> {
    data: &'static DataSet,
    cell: OnceLock<T>,
}

impl<T: fmt::Debug> fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cell.get().fmt(f)
    }
}

impl<T> Slot<T> {
    /// An empty slot of data set `data`, filled on first read.
    pub const fn new(data: &'static DataSet) -> Self {
        Slot { data, cell: OnceLock::new() }
    }

    /// The data set it reads.
    pub fn data(&self) -> &'static DataSet {
        self.data
    }

    pub(crate) fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        self.cell.get_or_init(f)
    }
}

/// One loaded file: its parsed table (none when the file was refused) and its problems.
#[derive(Debug)]
pub struct Loaded<'i> {
    file: String,
    table: Option<DeTable<'i>>,
    problems: Vec<DefaultsError>,
}

impl<'i> Loaded<'i> {
    /// The file's problems, in file order.
    pub fn problems(&self) -> &[DefaultsError] {
        &self.problems
    }

    /// The file's valid value of `entry`, if it sets one (never one of this crate's own keys).
    pub fn value<'a>(&'a self, entry: &Entry) -> Option<Value<'a>> {
        if in_model(entry.key) {
            return None;
        }
        let v = self.section("values")?.get(entry.key)?;
        convert(entry.kind, v.get_ref(), false).ok()
    }

    /// The file's valid `[layout]` value of `key` in `set`, if it sets one.
    pub fn layout<'a>(&'a self, set: &DataSet, key: &str) -> Option<Value<'a>> {
        let (_, kind) = set.layout.iter().find(|(l, _)| *l == key)?;
        convert(*kind, self.section("layout")?.get(key)?.get_ref(), false).ok()
    }

    /// Whether the file names `key` under `[values]` or `[layout]` (a guard test reads it).
    pub fn names(&self, key: &str) -> bool {
        self.has(key) || self.section("layout").is_some_and(|t| t.get(key).is_some())
    }

    /// The file's valid value of field `key` of catalogue `cat`'s built-in style `id`, or, with
    /// `id` `None`, of the field's own built-in.
    pub fn field_value<'a>(&'a self, cat: &Catalogue, id: Option<&str>, key: &str) -> Option<Value<'a>> {
        let f = cat.field(key)?;
        let table = match id {
            Some(id) => sub(sub(self.section("styles")?, crate::Word::word(cat.catalogue))?, id)?,
            None => sub(self.section("fields")?, crate::Word::word(cat.catalogue))?,
        };
        convert(f.kind, table.get(key)?.get_ref(), true).ok()
    }

    /// Whether the file names `key` under `[values]`, valid or not.
    fn has(&self, key: &str) -> bool {
        self.section("values").is_some_and(|t| t.get(key).is_some())
    }

    fn has_field(&self, cat: &str, key: &str) -> bool {
        self.section("fields").and_then(|t| sub(t, cat)).is_some_and(|t| t.get(key).is_some())
    }

    fn section(&self, name: &str) -> Option<&DeTable<'i>> {
        sub(self.table.as_ref()?, name)
    }
}

fn sub<'t, 'i>(t: &'t DeTable<'i>, key: &str) -> Option<&'t DeTable<'i>> {
    match t.get(key)?.get_ref() {
        DeValue::Table(t) => Some(t),
        _ => None,
    }
}

/// Read one defaults file named `file`, checked against the entries, catalogues and layout values
/// of `set`. Never fails and never panics; see [`Loaded::problems`].
pub fn load_in<'i>(set: &DataSet, file: &str, text: &'i str) -> Loaded<'i> {
    let mut out = Loaded { file: file.to_string(), table: None, problems: Vec::new() };
    if text.len() > MAX_BYTES {
        out.problems.push(DefaultsError::TooLarge { file: out.file.clone(), bytes: text.len() });
        return out;
    }
    if let Some(at) = too_deep(text) {
        out.problems.push(DefaultsError::TooDeep { file: out.file.clone(), span: at..at + 1 });
        return out;
    }
    match DeTable::parse(text) {
        Ok(t) => {
            let t = t.into_inner();
            out.problems = check(set, file, &t);
            out.table = Some(t);
        }
        Err(e) => {
            let span = e.span().unwrap_or(0..0);
            let (line, column) = line_column(text, span.start);
            out.problems.push(DefaultsError::Syntax { file: out.file.clone(), line, column, message: e.message().to_string(), span });
        }
    }
    out
}

/// The problems of a parsed file: unknown keys, wrong kinds, values out of range.
fn check(set: &DataSet, file: &str, t: &DeTable<'_>) -> Vec<DefaultsError> {
    let mut problems = Vec::new();
    let mut unknown = 0usize;
    let mut unknown_key = |problems: &mut Vec<DefaultsError>, key: String, span: Range<usize>| {
        unknown += 1;
        if unknown <= MAX_LISTED {
            problems.push(DefaultsError::UnknownKey { file: file.to_string(), key, span });
        }
    };
    let bad =
        |problems: &mut Vec<DefaultsError>, key: String, kind: Kind, v: &Spanned<DeValue<'_>>, empty: bool| match convert(kind, v.get_ref(), empty) {
            Ok(_) => {}
            Err(Fault::WrongKind(expected)) => problems.push(DefaultsError::WrongKind { file: file.to_string(), key, expected, span: v.span() }),
            Err(Fault::OutOfRange) => problems.push(DefaultsError::OutOfRange { file: file.to_string(), key, span: v.span() }),
        };
    let catalogue =
        |word: &str| <crate::StyleCatalogue as crate::Word>::parse(word).and_then(|c| set.catalogues.iter().copied().find(|x| x.catalogue == c));
    for (k, v) in t.iter() {
        let section = k.get_ref().as_ref();
        let DeValue::Table(body) = v.get_ref() else {
            problems.push(DefaultsError::WrongKind { file: file.to_string(), key: section.to_string(), expected: "a table", span: v.span() });
            continue;
        };
        match section {
            "values" => {
                for (k, v) in body.iter() {
                    let key = k.get_ref().as_ref();
                    match set.find(key).filter(|e| !in_model(e.key)) {
                        Some(e) => bad(&mut problems, key.to_string(), e.kind, v, false),
                        None => unknown_key(&mut problems, key.to_string(), k.span()),
                    }
                }
            }
            "layout" => {
                for (k, v) in body.iter() {
                    let key = k.get_ref().as_ref();
                    match set.layout.iter().find(|(l, _)| *l == key) {
                        Some((_, kind)) => bad(&mut problems, format!("layout.{key}"), *kind, v, false),
                        None => unknown_key(&mut problems, format!("layout.{key}"), k.span()),
                    }
                }
            }
            "fields" => {
                for (c, fields) in body.iter() {
                    let Some((cat, DeValue::Table(fields))) = catalogue(c.get_ref()).map(|cat| (cat, fields.get_ref())) else {
                        unknown_key(&mut problems, format!("fields.{}", c.get_ref()), c.span());
                        continue;
                    };
                    for (k, v) in fields.iter() {
                        let key = format!("fields.{}.{}", cat.catalogue, k.get_ref());
                        match cat.field(k.get_ref()) {
                            Some(f) => bad(&mut problems, key, f.kind, v, true),
                            None => unknown_key(&mut problems, key, k.span()),
                        }
                    }
                }
            }
            "styles" => {
                for (c, ids) in body.iter() {
                    let Some((cat, DeValue::Table(ids))) = catalogue(c.get_ref()).map(|cat| (cat, ids.get_ref())) else {
                        unknown_key(&mut problems, format!("styles.{}", c.get_ref()), c.span());
                        continue;
                    };
                    for (id, fields) in ids.iter() {
                        let known = cat.builtins.iter().any(|b| b.id == id.get_ref().as_ref());
                        let (true, DeValue::Table(fields)) = (known, fields.get_ref()) else {
                            unknown_key(&mut problems, format!("styles.{}.{}", cat.catalogue, id.get_ref()), id.span());
                            continue;
                        };
                        for (k, v) in fields.iter() {
                            let key = format!("styles.{}.{}.{}", cat.catalogue, id.get_ref(), k.get_ref());
                            match cat.field(k.get_ref()) {
                                Some(f) => bad(&mut problems, key, f.kind, v, true),
                                None => unknown_key(&mut problems, key, k.span()),
                            }
                        }
                    }
                }
            }
            other => unknown_key(&mut problems, other.to_string(), k.span()),
        }
    }
    if unknown > MAX_LISTED {
        problems.push(DefaultsError::MoreUnknownKeys { file: file.to_string(), count: unknown - MAX_LISTED });
    }
    problems
}

enum Fault {
    WrongKind(&'static str),
    OutOfRange,
}

/// A TOML value as a value of `kind`. `empty` allows `""` for a text kind (a style field with no
/// text, such as a wire category's maker).
fn convert<'a>(kind: Kind, v: &'a DeValue<'_>, empty: bool) -> Result<Value<'a>, Fault> {
    match kind {
        Kind::Length { min, max } | Kind::Number { min, max } => {
            let x = match v {
                DeValue::Float(f) => f.as_str().replace('_', "").parse::<f64>().map_err(|_| Fault::OutOfRange)?,
                DeValue::Integer(i) => i64::from_str_radix(&i.as_str().replace('_', ""), i.radix()).map_err(|_| Fault::OutOfRange)? as f64,
                _ => return Err(Fault::WrongKind("a number")),
            };
            if !(x.is_finite() && (min..=max).contains(&x)) {
                return Err(Fault::OutOfRange);
            }
            Ok(if matches!(kind, Kind::Length { .. }) { Value::Length(x) } else { Value::Number(x) })
        }
        _ => {
            let DeValue::String(s) = v else {
                return Err(Fault::WrongKind(match kind {
                    Kind::Choice(_) => "a word",
                    Kind::Flag => "yes or no",
                    _ => "text",
                }));
            };
            let s: &'a str = s.as_ref();
            if empty && s.is_empty() && matches!(kind, Kind::Text { .. } | Kind::Parsed { .. } | Kind::Choice(_)) {
                return Ok(Value::Text(""));
            }
            // A built-in is written exactly; a stored project value is trimmed, a data value is not.
            match parse_value(kind, s) {
                Some(Value::Text(t)) if t.len() == s.len() => Ok(Value::Text(t)),
                Some(v @ (Value::Choice(_) | Value::Flag(_))) if s.trim() == s => Ok(v),
                _ => Err(Fault::OutOfRange),
            }
        }
    }
}

/// The value an entry or field of `kind` takes when its built-in is missing or damaged: the low
/// end of its range, its first word, the first built-in style of its catalogue (`none` for a
/// catalogue with none built in), or empty text.
pub fn safe_value(kind: Kind) -> Value<'static> {
    match kind {
        Kind::Length { min, .. } => Value::Length(min),
        Kind::Number { min, .. } => Value::Number(min),
        Kind::Choice(words) => words.first().map_or(Value::Text(""), |w| Value::Choice(w)),
        Kind::Flag => Value::Flag(false),
        Kind::Style(cat) => Value::Text(cat.builtins.first().map_or("none", |b| b.id)),
        Kind::Text { .. } | Kind::Parsed { .. } => Value::Text(""),
    }
}

/// This crate's built-in of `entry`, valid for its kind.
fn model_value(entry: &Entry) -> Option<Value<'static>> {
    use crate::naming::builtins::BuiltinValue;
    let v = match (entry.kind, crate::naming::builtins::get(entry.key)?) {
        (Kind::Number { .. }, BuiltinValue::Number(x)) => Value::Number(x),
        (Kind::Length { .. }, BuiltinValue::Number(x)) => Value::Length(x),
        (Kind::Choice(words), BuiltinValue::Word(w)) => Value::Choice(words.iter().copied().find(|x| *x == w)?),
        (Kind::Flag, BuiltinValue::Word(w)) => parse_value(Kind::Flag, w)?,
        _ => return None,
    };
    entry.accepts(&v).then_some(v)
}

/// 1-based line and column of byte `at`.
fn line_column(text: &str, at: usize) -> (usize, usize) {
    let before = text.get(..at.min(text.len())).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// The byte where nesting first goes past [`MAX_DEPTH`], if it does: brackets and braces outside
/// strings and comments, plus the dots of the key being read. A cheap scan before parsing.
fn too_deep(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut dots = 0usize;
    let mut in_key = true;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut comment = false;
    for (i, c) in text.char_indices() {
        if comment {
            if c == '\n' {
                comment = false;
                in_key = true;
                dots = 0;
            }
            continue;
        }
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' && q == '"' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '#' => comment = true,
            '[' | '{' => {
                depth += 1;
                if c == '{' {
                    in_key = true;
                    dots = 0;
                }
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            '.' if in_key => dots += 1,
            '=' => {
                in_key = false;
                dots = 0;
            }
            ',' if depth > 0 => {
                in_key = true;
                dots = 0;
            }
            '\n' => {
                in_key = true;
                dots = 0;
            }
            _ => {}
        }
        if depth + dots > MAX_DEPTH {
            return Some(i);
        }
    }
    None
}
