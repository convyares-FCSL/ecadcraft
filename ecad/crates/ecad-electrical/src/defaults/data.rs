//! The built-in values as data (brief 066, design 066 §3 and §7).
//!
//! Every registry entry's built-in value, every style field's built-in and every built-in style's
//! field values live in TOML files under `ecad-electrical/defaults/`, one per area, embedded at
//! build time ([`FILES`]). Only values: an entry's kind, range, levels and words stay in its Rust
//! [`Entry`], so no data file can widen what is allowed.
//!
//! # Semantics
//! - [`load`] reads one file (a built-in file now, a company standard file later, stage 3). It
//!   never panics and never gives a value outside its entry's range: a file too large or nested too
//!   deep is refused before parsing, a syntax error refuses the whole file, an unknown key is
//!   listed and ignored, a value of the wrong kind or out of range is listed and left out, so the
//!   layer below is used.
//! - The built-in files are parsed once, on first use. Each entry, field and built-in style keeps
//!   its own value in a [`Slot`], filled on its first read, so resolving is a field read after
//!   that: no hashing by key on the hot path.
//! - A built-in that still fails to load (not reachable in a tested build: tests check every
//!   file) takes its kind's safe value ([`safe_value`]) and is listed by [`builtin_problems`],
//!   which Checks reports as `defaults-damaged`.
//!
//! Excluded: loading a company file from disk (waits for libraries, stage 3), numbered built-in
//! sets (deferred to the v1 format freeze), and `miette` reports (lane 061; the errors already
//! carry the file and the byte span it needs).

use std::ops::Range;
use std::sync::OnceLock;

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::{Entry, Kind, Value};
use crate::styles::{self, Builtin as BuiltinStyle, Catalogue, Field};

/// The largest file [`load`] parses (design 066 §7): a guard, not a preference.
pub const MAX_BYTES: usize = 256 * 1024;
/// The deepest nesting [`load`] parses: brackets, braces and dotted keys.
pub const MAX_DEPTH: usize = 16;
/// The unknown keys listed one by one; the rest are counted in one problem.
pub const MAX_LISTED: usize = 50;

/// The built-in files: one line per area (lanes adding an area each add one line).
#[rustfmt::skip]
pub static FILES: &[(&str, &str)] = &[
    ("text.toml", include_str!("../../defaults/text.toml")),
    ("wires.toml", include_str!("../../defaults/wires.toml")),
    ("print.toml", include_str!("../../defaults/print.toml")),
    ("symbols.toml", include_str!("../../defaults/symbols.toml")),
    ("terminals.toml", include_str!("../../defaults/terminals.toml")),
    ("connectors.toml", include_str!("../../defaults/connectors.toml")),
    ("strip_pages.toml", include_str!("../../defaults/strip_pages.toml")),
    ("bom.toml", include_str!("../../defaults/bom.toml")),
    ("styles.toml", include_str!("../../defaults/styles.toml")),
    ("continuations.toml", include_str!("../../defaults/continuations.toml")),
    ("plc.toml", include_str!("../../defaults/plc.toml")),
    ("pages.toml", include_str!("../../defaults/pages.toml")),
    ("rating.toml", include_str!("../../defaults/rating.toml")),
];

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

/// A slot that holds a built-in value once it has been read from the data ([`Entry::builtin`],
/// [`Field::builtin`], [`BuiltinStyle::values`]).
#[derive(Debug)]
pub struct Slot<T>(OnceLock<T>);

impl<T> Slot<T> {
    /// An empty slot, filled on first read.
    pub const fn new() -> Self {
        Slot(OnceLock::new())
    }

    pub(crate) fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        self.0.get_or_init(f)
    }
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Slot::new()
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

    /// The file's valid value of `entry`, if it sets one.
    pub fn value<'a>(&'a self, entry: &Entry) -> Option<Value<'a>> {
        let v = self.section("values")?.get(entry.key)?;
        convert(entry.kind, v.get_ref(), false).ok()
    }

    /// The file's valid value of field `key` of catalogue `cat`'s built-in style `id`, or, with
    /// `id` `None`, of the field's own built-in.
    pub fn field_value<'a>(&'a self, cat: &Catalogue, id: Option<&str>, key: &str) -> Option<Value<'a>> {
        let f = cat.field(key)?;
        let table = match id {
            Some(id) => sub(sub(self.section("styles")?, cat.word)?, id)?,
            None => sub(self.section("fields")?, cat.word)?,
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

/// Read one defaults file named `file` (design 066 §7). Never fails and never panics; see
/// [`Loaded::problems`].
pub fn load<'i>(file: &str, text: &'i str) -> Loaded<'i> {
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
            out.problems = check(file, &t);
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

/// Every entry with a built-in value: the registry's, and the wire label height, which is the
/// project field of the `wire_label` text style and not listed by [`super::all`].
pub fn entries() -> impl Iterator<Item = &'static Entry> {
    super::all().chain(std::iter::once(&super::text::WIRE_LABEL_HEIGHT))
}

fn find(key: &str) -> Option<&'static Entry> {
    entries().find(|e| e.key == key)
}

/// The problems of a parsed file: unknown keys, wrong kinds, values out of range.
fn check(file: &str, t: &DeTable<'_>) -> Vec<DefaultsError> {
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
                    match find(key) {
                        Some(e) => bad(&mut problems, key.to_string(), e.kind, v, false),
                        None => unknown_key(&mut problems, key.to_string(), k.span()),
                    }
                }
            }
            "fields" => {
                for (c, fields) in body.iter() {
                    let Some((cat, DeValue::Table(fields))) = styles::catalogue(c.get_ref()).map(|cat| (cat, fields.get_ref())) else {
                        unknown_key(&mut problems, format!("fields.{}", c.get_ref()), c.span());
                        continue;
                    };
                    for (k, v) in fields.iter() {
                        let key = format!("fields.{}.{}", cat.word, k.get_ref());
                        match cat.field(k.get_ref()) {
                            Some(f) => bad(&mut problems, key, f.kind, v, true),
                            None => unknown_key(&mut problems, key, k.span()),
                        }
                    }
                }
            }
            "styles" => {
                for (c, ids) in body.iter() {
                    let Some((cat, DeValue::Table(ids))) = styles::catalogue(c.get_ref()).map(|cat| (cat, ids.get_ref())) else {
                        unknown_key(&mut problems, format!("styles.{}", c.get_ref()), c.span());
                        continue;
                    };
                    for (id, fields) in ids.iter() {
                        let known = cat.builtins.iter().any(|b| b.id == id.get_ref().as_ref());
                        let (true, DeValue::Table(fields)) = (known, fields.get_ref()) else {
                            unknown_key(&mut problems, format!("styles.{}.{}", cat.word, id.get_ref()), id.span());
                            continue;
                        };
                        for (k, v) in fields.iter() {
                            let key = format!("styles.{}.{}.{}", cat.word, id.get_ref(), k.get_ref());
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
                return Err(Fault::WrongKind(if matches!(kind, Kind::Choice(_)) { "a word" } else { "text" }));
            };
            let s: &'a str = s.as_ref();
            if empty && s.is_empty() && matches!(kind, Kind::Text { .. } | Kind::Parsed { .. }) {
                return Ok(Value::Text(""));
            }
            // A built-in is written exactly; a stored project value is trimmed, a data value is not.
            match super::parse_value(kind, s) {
                Some(Value::Text(t)) if t.len() == s.len() => Ok(Value::Text(t)),
                Some(v @ Value::Choice(_)) if s.trim() == s => Ok(v),
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
        Kind::Style(cat) => Value::Text(styles::catalogue(cat).and_then(|c| c.builtins.first()).map_or("none", |b| b.id)),
        Kind::Text { .. } | Kind::Parsed { .. } => Value::Text(""),
    }
}

/// The built-in files, parsed once.
fn builtin_files() -> &'static [Loaded<'static>] {
    static LOADED: OnceLock<Vec<Loaded<'static>>> = OnceLock::new();
    LOADED.get_or_init(|| FILES.iter().map(|(name, text)| load(name, text)).collect())
}

/// `entry`'s built-in value from the data (what [`Entry::builtin`] caches).
pub(crate) fn builtin_value(entry: &Entry) -> Value<'static> {
    builtin_files().iter().find_map(|f| f.value(entry)).unwrap_or_else(|| safe_value(entry.kind))
}

/// The catalogue that declares `f`.
fn catalogue_of_field(f: &Field) -> Option<&'static Catalogue> {
    styles::CATALOGUES.iter().copied().find(|c| c.fields.iter().any(|g| std::ptr::eq(g, f)))
}

/// `f`'s built-in value from the data (what [`Field::builtin`] caches).
pub(crate) fn field_builtin(f: &Field) -> Value<'static> {
    catalogue_of_field(f)
        .and_then(|cat| builtin_files().iter().find_map(|file| file.field_value(cat, None, f.key)))
        .unwrap_or_else(|| safe_value(f.kind))
}

/// Built-in style `b`'s field values from the data, in its catalogue's field order (what
/// [`BuiltinStyle::values`] caches). The `wire_label` text style's height is the entry
/// `text.wire_label.height` (one value, one place, design 066 §3.3).
pub(crate) fn style_values(b: &BuiltinStyle) -> Vec<(&'static str, Value<'static>)> {
    let Some(cat) = styles::CATALOGUES.iter().copied().find(|c| c.builtins.iter().any(|x| std::ptr::eq(x, b))) else {
        return Vec::new();
    };
    let mut out: Vec<(&'static str, Value<'static>)> = cat
        .fields
        .iter()
        .filter_map(|f| builtin_files().iter().find_map(|file| file.field_value(cat, Some(b.id), f.key)).map(|v| (f.key, v)))
        .collect();
    if cat.word == styles::text::CATALOGUE.word && b.id == "wire_label" {
        out.retain(|(k, _)| *k != "height");
        out.push(("height", super::text::WIRE_LABEL_HEIGHT.builtin()));
    }
    out
}

/// The built-in data's problems: each file's, then each entry or style field with no value in any
/// file. Empty in a tested build; Checks reports any as `defaults-damaged`.
pub fn builtin_problems() -> &'static [DefaultsError] {
    static PROBLEMS: OnceLock<Vec<DefaultsError>> = OnceLock::new();
    PROBLEMS.get_or_init(|| {
        let files = builtin_files();
        let mut out: Vec<DefaultsError> = files.iter().flat_map(|f| f.problems.iter().cloned()).collect();
        for e in entries() {
            if !files.iter().any(|f| f.has(e.key)) {
                out.push(DefaultsError::Missing { file: area_file(e.key).to_string(), key: e.key.to_string() });
            }
        }
        for cat in styles::CATALOGUES {
            for f in cat.fields {
                if !files.iter().any(|file| file.has_field(cat.word, f.key)) {
                    out.push(DefaultsError::Missing { file: "styles.toml".to_string(), key: format!("fields.{}.{}", cat.word, f.key) });
                }
            }
        }
        out
    })
}

/// The file a missing key belongs in, by its area (for the message only).
fn area_file(key: &str) -> &'static str {
    let area = key.split('.').next().unwrap_or_default();
    FILES.iter().map(|(n, _)| *n).find(|n| n.trim_end_matches(".toml").trim_end_matches('s') == area).unwrap_or("text.toml")
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
