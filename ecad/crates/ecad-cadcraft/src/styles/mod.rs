//! Style catalogues (brief 050a, design 050 §2 and §4): named styles that objects refer to by a
//! stable ID, so changing a style changes every object that uses it.
//!
//! # Semantics
//! - A [`Catalogue`] declares its [`Field`]s and ships built-in styles ([`Builtin`]). Standards
//!   and projects add **own** styles and override fields of built-in ones with style records
//!   (`ecad_model::style`).
//! - **Which style** an object uses is a registry choice entry ([`crate::defaults::Kind::Style`],
//!   `defaults/styles.rs`), resolved like any entry. **What a style says** is [`field`]: the
//!   object's override (for a field that names one), the project's record, the standard's
//!   record, the built-in style, then the field's built-in. A stored value that does not parse
//!   counts as absent, so a hostile file never gives a value outside the field's range.
//! - A reference to a style that does not exist (a hostile or merged file) counts as absent and
//!   falls through to the next level; Checks reports it.
//! - Bounds (`ecad_model::style::MAX_*`): records past them are kept, ignored here and reported
//!   by Checks.
//!
//! # Update map (design 050 §10)
//! [`pages_touched`] says which pages an op changes the look of: a style change marks the pages
//! of its explicit users (through the document's style-choice index) and, when the project's
//! choice resolves to it, every page. Nothing here rescans the project on an ordinary commit;
//! [`users`] (the consequence line, Used by) is asked for one style at a time, on demand.
//!
//! This module draws nothing and records nothing; `style_cmds` builds the ops.

use std::collections::BTreeSet;

use ecad_model::style::{self as ms, MAX_FIELD_CHARS, MAX_FIELDS_PER_STYLE, MAX_STYLES_PER_CATALOGUE, StyleLayer, StyleRecord, StyleRef};
use ecad_model::{Document, ObjectRef, Op, PageId, Properties, Subject};

use crate::defaults::{self, Kind, Resolved, Scope, Source, Value};

pub mod checks;
pub mod frame;
pub mod location;
pub mod text;

/// One field of a catalogue.
#[derive(Debug)]
pub struct Field {
    /// The key in style records: `height`, `line_weight`.
    pub key: &'static str,
    /// Words for tables and messages: `Cap height`.
    pub label: &'static str,
    pub kind: Kind,
    /// The value when no level sets one (step 5 of design 050 §2.4).
    pub builtin: Value<'static>,
    /// The object property that overrides it (050b: a conductor's own `colour`), if any.
    pub over: Option<&'static str>,
}

/// A built-in style: a word ID, never renamed or reused, a name, and the values it sets.
#[derive(Debug)]
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    pub values: &'static [(&'static str, Value<'static>)],
}

impl Builtin {
    /// Its value of field `key`, if it sets one.
    pub fn value(&self, key: &str) -> Option<Value<'static>> {
        self.values.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
}

/// A project-level value read from somewhere other than the style record (the wire label height
/// in `settings.wiring`, design 050 §4.1): `(doc, style, field)` → the value, or `None`.
pub type ProjectField = for<'a> fn(&'a Document, &StyleRef, &str) -> Option<Value<'a>>;

/// One kind of style.
#[derive(Debug)]
pub struct Catalogue {
    /// The catalogue word: `text`, `location`, `frame`.
    pub word: &'static str,
    /// Its table's name: `Text styles`.
    pub plural: &'static str,
    /// One of its styles in words: `Text style`.
    pub one: &'static str,
    pub fields: &'static [Field],
    /// Built-in styles, in table order.
    pub builtins: &'static [Builtin],
    /// The registry's choice entries naming this catalogue's styles.
    pub choices: fn() -> Vec<&'static defaults::Entry>,
    pub project_field: Option<ProjectField>,
}

impl Catalogue {
    /// The field with this key.
    pub fn field(&self, key: &str) -> Option<&'static Field> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// The built-in style with this word.
    pub fn builtin(&self, id: &StyleRef) -> Option<&'static Builtin> {
        match id {
            StyleRef::Builtin(w) => self.builtins.iter().find(|b| b.id == w),
            StyleRef::Own(_) => None,
        }
    }
}

/// Every catalogue: one line each (a later catalogue, 050b's `wire`, 2.6, 2.7, adds a line).
#[rustfmt::skip]
pub static CATALOGUES: &[&Catalogue] = &[
    &text::CATALOGUE,
    &location::CATALOGUE,
    &frame::CATALOGUE,
];

/// The catalogue with this word.
pub fn catalogue(word: &str) -> Option<&'static Catalogue> {
    CATALOGUES.iter().copied().find(|c| c.word == word)
}

/// The records `layer` holds for `catalogue` in `doc`, at most [`MAX_STYLES_PER_CATALOGUE`] in ID
/// order (the bound's excess is ignored here and reported by Checks).
pub fn records<'a>(doc: &'a Document, layer: StyleLayer, catalogue: &str) -> impl Iterator<Item = (&'a StyleRef, &'a StyleRecord)> + 'a {
    let cat = catalogue.to_string();
    ms::layer_of(doc, layer).and_then(move |c| c.get(&cat)).into_iter().flat_map(|m| m.iter()).take(MAX_STYLES_PER_CATALOGUE)
}

/// The record `layer` holds for `(catalogue, id)`, within the bound.
pub fn record<'a>(doc: &'a Document, layer: StyleLayer, catalogue: &str, id: &StyleRef) -> Option<&'a StyleRecord> {
    let map = ms::layer_of(doc, layer)?.get(catalogue)?;
    let r = map.get(id)?;
    // Past the bound only: the rank costs at most the bound.
    if map.len() > MAX_STYLES_PER_CATALOGUE && map.range(..id).take(MAX_STYLES_PER_CATALOGUE).count() >= MAX_STYLES_PER_CATALOGUE {
        return None;
    }
    Some(r)
}

/// A record's stored value of `key`, within the bounds (at most [`MAX_FIELDS_PER_STYLE`] fields,
/// values of at most [`MAX_FIELD_CHARS`] characters).
fn stored<'a>(r: &'a StyleRecord, key: &str) -> Option<&'a str> {
    let v = r.fields.get(key)?;
    if r.fields.len() > MAX_FIELDS_PER_STYLE
        && r.fields.keys().take_while(|k| k.as_str() < key).take(MAX_FIELDS_PER_STYLE).count() >= MAX_FIELDS_PER_STYLE
    {
        return None;
    }
    (v.chars().count() <= MAX_FIELD_CHARS).then_some(v.as_str())
}

/// Whether `id` names a style of `catalogue` in `doc`: a built-in one, or a record in the project
/// or the standard (within the bound).
pub fn exists(doc: &Document, catalogue: &str, id: &StyleRef) -> bool {
    self::catalogue(catalogue).is_some_and(|c| c.builtin(id).is_some())
        || record(doc, StyleLayer::Project, catalogue, id).is_some()
        || record(doc, StyleLayer::Standard, catalogue, id).is_some()
}

/// The value of field `f` of style `id` (design 050 §2.4): `over`'s property when the field names
/// one, the project's record, the standard's, the built-in style's, the field's built-in. Never
/// fails; allocates nothing.
pub fn field<'a>(doc: &'a Document, cat: &Catalogue, id: &StyleRef, f: &Field, over: Option<&'a Properties>) -> Resolved<'a> {
    let parse = |s: &'a str| defaults::parse_value_in(doc, f.kind, s);
    if let Some(value) = f.over.zip(over).and_then(|(k, p)| p.get(k)).and_then(|v| parse(v.as_str())) {
        return Resolved { value, from: Source::Object };
    }
    if let Some(value) = cat.project_field.and_then(|read| read(doc, id, f.key)).filter(|v| defaults::accepts(f.kind, v)) {
        return Resolved { value, from: Source::Project };
    }
    if let Some(value) = record(doc, StyleLayer::Project, cat.word, id).and_then(|r| stored(r, f.key)).and_then(parse) {
        return Resolved { value, from: Source::Project };
    }
    if let Some(value) = record(doc, StyleLayer::Standard, cat.word, id).and_then(|r| stored(r, f.key)).and_then(parse) {
        return Resolved { value, from: Source::Standard };
    }
    if let Some(value) = cat.builtin(id).and_then(|b| b.value(f.key)) {
        return Resolved { value, from: Source::Builtin };
    }
    Resolved { value: f.builtin, from: Source::Builtin }
}

/// [`field`] by catalogue word and field key; the field's built-in for an unknown one.
pub fn field_of<'a>(doc: &'a Document, catalogue: &str, id: &StyleRef, key: &str, over: Option<&'a Properties>) -> Option<Resolved<'a>> {
    let c = self::catalogue(catalogue)?;
    let f = c.field(key)?;
    Some(field(doc, c, id, f, over))
}

/// The style a choice entry resolves to at `at`: its stored ID when it names a style, else the
/// entry's built-in.
pub fn chosen(doc: &Document, entry: &defaults::Entry, at: Scope) -> StyleRef {
    let r = defaults::resolve(doc, entry, at);
    StyleRef::parse(&r.text()).unwrap_or_else(|| StyleRef::Builtin(builtin_word(entry)))
}

fn builtin_word(entry: &defaults::Entry) -> String {
    match entry.builtin {
        Value::Text(t) => t.to_string(),
        _ => String::new(),
    }
}

/// Where a style lives, for tables: built in, the standard's own, or the project's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Builtin,
    Standard,
    Project,
}

impl Layer {
    /// The word `styles` returns: `builtin`, `standard`, `project`.
    pub fn word(self) -> &'static str {
        match self {
            Layer::Builtin => "builtin",
            Layer::Standard => "standard",
            Layer::Project => "project",
        }
    }

    /// The badge the Standards screen shows.
    pub fn badge(self) -> &'static str {
        match self {
            Layer::Builtin => "Built in",
            Layer::Standard => "Standard",
            Layer::Project => "Project only",
        }
    }
}

/// One style of a catalogue, for tables and `styles`.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: StyleRef,
    pub name: String,
    pub layer: Layer,
    /// A built-in style with records overriding fields (the dot after its badge).
    pub overridden: bool,
}

/// A name for display: at most [`ms::MAX_NAME_CHARS`] characters (a longer stored one is cut and
/// reported by Checks).
fn shown_name(n: &str) -> String {
    n.chars().take(ms::MAX_NAME_CHARS).filter(|c| !c.is_control()).collect()
}

/// The name of style `id` in `catalogue`: the project record's, the standard's, the built-in's,
/// else its ID.
pub fn name(doc: &Document, catalogue: &str, id: &StyleRef) -> String {
    let named = |l| record(doc, l, catalogue, id).map(|r| r.name.as_str()).filter(|n| !n.trim().is_empty());
    if let Some(n) = named(StyleLayer::Project).or_else(|| named(StyleLayer::Standard)) {
        return shown_name(n);
    }
    match self::catalogue(catalogue).and_then(|c| c.builtin(id)) {
        Some(b) => b.name.to_string(),
        None => id.text(),
    }
}

/// The styles of `cat` (design 050 §7.2): built-in first (in the catalogue's order), then the
/// standard's own, then the project's own, each group by name. An own style the standard holds is
/// listed once, as the standard's, also when the project overrides fields of it.
pub fn rows(doc: &Document, cat: &Catalogue) -> Vec<Row> {
    let has = |l, id: &StyleRef| record(doc, l, cat.word, id).is_some();
    let mut out: Vec<Row> = cat
        .builtins
        .iter()
        .map(|b| {
            let id = StyleRef::Builtin(b.id.to_string());
            let overridden = has(StyleLayer::Project, &id) || has(StyleLayer::Standard, &id);
            Row { name: b.name.to_string(), id, layer: Layer::Builtin, overridden }
        })
        .collect();
    let own = |l: StyleLayer, layer: Layer, skip: &dyn Fn(&StyleRef) -> bool| -> Vec<Row> {
        let mut v: Vec<Row> = records(doc, l, cat.word)
            .filter(|(id, _)| cat.builtin(id).is_none() && !skip(id))
            .map(|(id, _)| Row { name: name(doc, cat.word, id), id: id.clone(), layer, overridden: false })
            .collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
        v
    };
    out.extend(own(StyleLayer::Standard, Layer::Standard, &|_| false));
    out.extend(own(StyleLayer::Project, Layer::Project, &|id| has(StyleLayer::Standard, id)));
    out
}

/// The style of `catalogue` that `text` names: an ID (built-in word or `#<hex>`) of an existing
/// style, else a name (without case) that exactly one style has.
///
/// # Errors
/// In words: no such style, or the name is shared and the ID is needed.
pub fn find(doc: &Document, catalogue: &str, text: &str) -> Result<StyleRef, String> {
    let t = text.trim();
    if let Some(id) = StyleRef::parse(t).filter(|id| exists(doc, catalogue, id)) {
        return Ok(id);
    }
    let cat = self::catalogue(catalogue).ok_or_else(|| format!("No catalogue {catalogue:?}; the catalogues are {}.", words()))?;
    let hits: Vec<Row> = rows(doc, cat).into_iter().filter(|r| r.name.eq_ignore_ascii_case(t)).collect();
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(format!("No {} is called {t:?}.", cat.one.to_lowercase())),
        _ => Err(format!("{} {} are called {t:?}; give the ID ({}).", hits.len(), cat.plural.to_lowercase(), ids(&hits))),
    }
}

fn ids(rows: &[Row]) -> String {
    rows.iter().map(|r| r.id.text()).collect::<Vec<_>>().join(", ")
}

/// The catalogue words, for messages.
pub fn words() -> String {
    CATALOGUES.iter().map(|c| c.word).collect::<Vec<_>>().join(", ")
}

/// How many objects use a style (design 050 §2.5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Count {
    /// Objects whose own choice names it.
    pub explicit: usize,
    /// Objects with no own choice whose page, project, standard or built-in choice resolves to it.
    pub implicit: usize,
}

/// Who uses style `id` of `catalogue`. Explicit users come from the document's style-choice index;
/// implicit ones are counted from the resolved choices, one pass over the kind of object the
/// catalogue styles. Asked for one style at a time (a selected row, a consequence line), never per
/// frame.
pub fn users(doc: &Document, catalogue: &str, id: &StyleRef) -> Count {
    let Some(cat) = self::catalogue(catalogue) else { return Count::default() };
    let value = id.text();
    let mut c = Count::default();
    for entry in (cat.choices)() {
        c.explicit += doc.style_users(entry.key, &value).count();
        c.implicit += implicit(doc, cat, entry, id);
    }
    c
}

/// The objects of `entry`'s kind with no own choice that resolve to `id` through the page,
/// project, standard or built-in.
fn implicit(doc: &Document, cat: &Catalogue, entry: &defaults::Entry, id: &StyleRef) -> usize {
    let project = chosen(doc, entry, Scope::project());
    let own = |p: &Properties| p.get(entry.key).is_some_and(|v| entry.parse_in(doc, v).is_some());
    match cat.word {
        "frame" => doc.pages.iter().filter(|(_, p)| !own(&p.properties)).filter(|_| project == *id).count(),
        "location" if project == *id => doc.components.iter().filter(|(c, x)| crate::locations::is_box(doc, **c) && !own(&x.properties)).count(),
        "text" if project == *id => match text::noun(entry.key) {
            text::Noun::Devices => doc.components.values().filter(|x| !own(&x.properties)).count(),
            text::Noun::Wires => doc.conductors.values().filter(|x| !own(&x.properties)).count(),
            text::Noun::Pages => doc.pages.len(),
        },
        _ => 0,
    }
}

/// The explicit users of `id` (any choice key of `catalogue`), through the index.
pub fn explicit_users(doc: &Document, catalogue: &str, id: &StyleRef) -> Vec<(&'static str, ObjectRef)> {
    let Some(cat) = self::catalogue(catalogue) else { return Vec::new() };
    let value = id.text();
    let mut out = Vec::new();
    for entry in (cat.choices)() {
        out.extend(doc.style_users(entry.key, &value).map(|o| (entry.key, o)));
    }
    out
}

/// The pages an object is drawn on.
fn pages_of(doc: &Document, o: ObjectRef, out: &mut BTreeSet<PageId>) {
    match o {
        ObjectRef::Component(c) => out.extend(doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page)),
        ObjectRef::Conductor(c) => out.extend(doc.representations_of(Subject::Conductor(c)).map(|(_, r)| r.page)),
        ObjectRef::Cable(c) => out.extend(doc.representations_of(Subject::Cable(c)).map(|(_, r)| r.page)),
        ObjectRef::Page(p) => {
            out.insert(p);
        }
        ObjectRef::Project | ObjectRef::Standard => {}
    }
}

/// The pages a style op changes the look of (design 050 §10), or `None` for every page. `None`
/// for an op this module does not cover.
pub fn pages_touched(doc: &Document, op: &Op) -> Option<BTreeSet<PageId>> {
    match op {
        // The header shows on the Standards screen only.
        Op::SetStandard { .. } => Some(BTreeSet::new()),
        Op::SetStyle { catalogue, id, .. } => {
            let Some(cat) = self::catalogue(catalogue) else { return Some(BTreeSet::new()) };
            // A style the project (or standard, or built-in) choice resolves to is on every page.
            if (cat.choices)().iter().any(|e| chosen(doc, e, Scope::project()) == *id) {
                return None;
            }
            // A text style a location style labels with reaches every box.
            if cat.word == "text" && location::labels_with(doc, id) {
                return None;
            }
            let mut out = BTreeSet::new();
            for (_, o) in explicit_users(doc, catalogue, id) {
                pages_of(doc, o, &mut out);
            }
            Some(out)
        }
        _ => None,
    }
}

/// Whether record `r` may be written as `(layer, catalogue, id)` (design 050 §3.2): a name that is
/// none or 1–64 characters, at most [`MAX_FIELDS_PER_STYLE`] fields of at most
/// [`MAX_FIELD_CHARS`] characters, and room in the layer for a new style.
///
/// # Errors
/// The bound it breaks, in words.
pub fn check_record(doc: &Document, cat: &Catalogue, id: &StyleRef, layer: StyleLayer, r: &StyleRecord) -> Result<(), String> {
    if !r.name.is_empty() && !ms::is_name(&r.name) {
        return Err("A style needs a name (1 to 64 characters, no control characters).".into());
    }
    if r.fields.len() > MAX_FIELDS_PER_STYLE {
        return Err(format!("A style holds at most {MAX_FIELDS_PER_STYLE} fields."));
    }
    if r.fields.values().any(|v| v.chars().count() > MAX_FIELD_CHARS) {
        return Err(format!("A field value is at most {MAX_FIELD_CHARS} characters."));
    }
    let held = ms::layer_of(doc, layer).and_then(|c| c.get(cat.word));
    if held.is_some_and(|m| !m.contains_key(id) && m.len() >= MAX_STYLES_PER_CATALOGUE) {
        return Err(format!("{} holds at most {MAX_STYLES_PER_CATALOGUE} styles per layer.", cat.plural));
    }
    Ok(())
}

/// The record a `style` command writes for `fields` set or cleared on `before` (design 050 §6.2).
pub fn with_fields(before: Option<&StyleRecord>, name: Option<&str>, fields: &[(String, Option<String>)]) -> StyleRecord {
    let mut r = before.cloned().unwrap_or_default();
    if let Some(n) = name {
        r.name = n.to_string();
    }
    for (k, v) in fields {
        match v {
            Some(v) => r.fields.insert(k.clone(), v.clone()),
            None => r.fields.remove(k),
        };
    }
    r
}

/// Whether `s` is a colour a style field takes (design 050 §4.1): `none`, one IEC 60757 code
/// (`RD`, `BLU`), or `#rrggbb`.
pub fn is_colour(s: &str) -> bool {
    let t = s.trim();
    t.eq_ignore_ascii_case("none") || hex_rgb(t).is_some() || crate::look::colour_code(t).is_some_and(|(_, two)| two.is_none())
}

fn hex_rgb(t: &str) -> Option<(u8, u8, u8)> {
    let h = t.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// A colour field's value as drawn on `sheet`: `None` for `none` (ink) and anything unknown.
pub fn colour_rgb(value: &str, sheet: &ecad_tokens::Sheet) -> Option<cadcraft_engine::doc::color::Rgb> {
    let t = value.trim();
    if t.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some((r, g, b)) = hex_rgb(t) {
        return Some(cadcraft_engine::doc::color::Rgb(r, g, b));
    }
    let w = crate::look::wire_colours(t, sheet)?;
    Some(crate::look::rgb(w.solid))
}

/// A text value of a resolved field (`""` for a number).
pub fn text_of(r: &Resolved<'_>) -> String {
    r.text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_value_is_valid_for_its_field() {
        for c in CATALOGUES {
            assert!(ms::is_word(c.word), "{}", c.word);
            for f in c.fields {
                assert!(defaults::accepts(f.kind, &f.builtin), "{}.{} built-in", c.word, f.key);
            }
            for b in c.builtins {
                assert!(ms::is_word(b.id), "{}", b.id);
                for (k, v) in b.values {
                    let f = c.field(k);
                    assert!(f.is_some(), "{}.{} names no field {k}", c.word, b.id);
                    assert!(f.is_some_and(|f| defaults::accepts(f.kind, v)), "{}.{}.{k}", c.word, b.id);
                }
            }
            for e in (c.choices)() {
                assert!(matches!(e.kind, Kind::Style(w) if w == c.word), "{}", e.key);
                let b = StyleRef::parse(&builtin_word(e));
                assert!(b.as_ref().is_some_and(|b| c.builtin(b).is_some()), "{}: built-in names no built-in style", e.key);
            }
        }
    }

    #[test]
    fn colours_are_none_one_code_or_hex() {
        for ok in ["none", "NONE", "RD", "blu", "#c62828"] {
            assert!(is_colour(ok), "{ok}");
        }
        for bad in ["", "GNYE", "#12345", "#gggggg", "purple"] {
            assert!(!is_colour(bad), "{bad}");
        }
    }
}
